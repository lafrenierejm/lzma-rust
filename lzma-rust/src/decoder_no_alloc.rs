use crate::range_dec::RangeSource;

use super::lz::LZDecoder;
use super::range_dec::RangeDecoder;
use super::*;
use core::ops::{Deref, DerefMut};

pub struct LZMADecoder<
    const LC: u32,
    const LP: u32,
    const PB: u32,
    const NUM_SUBDECODERS: usize,
    const DICT_SIZE: usize,
> {
    coder: LZMACoder<PB>,
    literal_decoder: LiteralDecoder<LC, LP, PB, NUM_SUBDECODERS, DICT_SIZE>,
    match_len_decoder: LengthCoder,
    rep_len_decoder: LengthCoder,
}

impl<
        const LC: u32,
        const LP: u32,
        const PB: u32,
        const NUM_SUBDECODERS: usize,
        const DICT_SIZE: usize,
    > Deref for LZMADecoder<LC, LP, PB, NUM_SUBDECODERS, DICT_SIZE>
{
    type Target = LZMACoder<PB>;

    fn deref(&self) -> &Self::Target {
        &self.coder
    }
}
impl<
        const LC: u32,
        const LP: u32,
        const PB: u32,
        const NUM_SUBDECODERS: usize,
        const DICT_SIZE: usize,
    > DerefMut for LZMADecoder<LC, LP, PB, NUM_SUBDECODERS, DICT_SIZE>
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.coder
    }
}

pub const fn get_num_sub_decoders<const LC: u32, const LP: u32>() -> usize {
    (1 << (LC + LP)) as usize
}

impl<
        const LC: u32,
        const LP: u32,
        const PB: u32,
        const NUM_SUBDECODERS: usize,
        const DICT_SIZE: usize,
    > LZMADecoder<LC, LP, PB, NUM_SUBDECODERS, DICT_SIZE>
{
    pub const fn new() -> Self {
        let mut literal_decoder = LiteralDecoder::<LC, LP, PB, NUM_SUBDECODERS, DICT_SIZE>::new();
        literal_decoder.reset();
        let match_len_decoder = {
            let mut l = LengthCoder::new();
            l.reset();
            l
        };
        let rep_len_decoder = {
            let mut l = LengthCoder::new();
            l.reset();
            l
        };
        Self {
            coder: LZMACoder::<PB>::new(),
            literal_decoder,
            match_len_decoder,
            rep_len_decoder,
        }
    }

    pub fn reset(&mut self) {
        self.coder.reset();
        self.literal_decoder.reset();
        self.match_len_decoder.reset();
        self.rep_len_decoder.reset();
    }

    pub fn end_marker_detected(&self) -> bool {
        self.reps[0] == -1
    }

    pub fn decode<R: RangeSource>(
        &mut self,
        lz: &mut LZDecoder<DICT_SIZE>,
        rc: &mut RangeDecoder<R>,
    ) -> () {
        lz.repeat_pending().unwrap();
        while lz.has_space() {
            let pos_state = lz.get_pos() as u32 & LZMACoder::<PB>::POS_MASK;
            let i = self.state.get() as usize;
            let probs = &mut self.is_match[i];
            let bit = rc.decode_bit(&mut probs[pos_state as usize]);
            if bit == 0 {
                self.literal_decoder.decode(&mut self.coder, lz, rc);
            } else {
                let index = self.state.get() as usize;
                let len = if rc.decode_bit(&mut self.is_rep[index]) == 0 {
                    self.decode_match(pos_state, rc)
                } else {
                    self.decode_rep_match(pos_state, rc)
                };
                lz.repeat(self.reps[0] as _, len as _).unwrap();
            }
        }
        rc.normalize();
        ()
    }

    fn decode_match<R: RangeSource>(&mut self, pos_state: u32, rc: &mut RangeDecoder<R>) -> u32 {
        self.state.update_match();
        self.reps[3] = self.reps[2];
        self.reps[2] = self.reps[1];
        self.reps[1] = self.reps[0];

        let len = self.match_len_decoder.decode(pos_state as _, rc);
        let dist_slot = rc.decode_bit_tree(&mut self.dist_slots[coder_get_dict_size(len as _)]);

        if dist_slot < DIST_MODEL_START as i32 {
            self.reps[0] = dist_slot as _;
        } else {
            let limit = (dist_slot >> 1) - 1;
            self.reps[0] = (2 | (dist_slot & 1)) << limit;
            if dist_slot < DIST_MODEL_END as i32 {
                let probs = self.get_dist_special((dist_slot - DIST_MODEL_START as i32) as usize);
                self.reps[0] |= rc.decode_reverse_bit_tree(probs);
            } else {
                let r0 = rc.decode_direct_bits(limit as u32 - ALIGN_BITS as u32) << ALIGN_BITS;
                self.reps[0] = self.reps[0] | r0;
                self.reps[0] |= rc.decode_reverse_bit_tree(&mut self.dist_align);
            }
        }

        len as _
    }

    fn decode_rep_match<R: RangeSource>(
        &mut self,
        pos_state: u32,
        rc: &mut RangeDecoder<R>,
    ) -> u32 {
        let index = self.state.get() as usize;
        if rc.decode_bit(&mut self.is_rep0[index]) == 0 {
            let index: usize = self.state.get() as usize;
            if rc.decode_bit(&mut self.is_rep0_long[index][pos_state as usize]) == 0 {
                self.state.update_short_rep();
                return 1;
            }
        } else {
            let tmp;
            let s = self.state.get() as usize;
            if rc.decode_bit(&mut self.is_rep1[s]) == 0 {
                tmp = self.reps[1];
            } else {
                if rc.decode_bit(&mut self.is_rep2[s]) == 0 {
                    tmp = self.reps[2];
                } else {
                    tmp = self.reps[3];
                    self.reps[3] = self.reps[2];
                }
                self.reps[2] = self.reps[1];
            }
            self.reps[1] = self.reps[0];
            self.reps[0] = tmp;
        }

        self.state.update_long_rep();
        self.rep_len_decoder.decode(pos_state as _, rc) as u32
    }
}
pub struct LiteralDecoder<
    const LC: u32,
    const LP: u32,
    const PB: u32,
    const NUM_SUBDECODERS: usize,
    const DICT_SIZE: usize,
> {
    coder: LiteralCoder<LC, LP>,
    sub_decoders: [LiteralSubdecoder<DICT_SIZE, PB>; NUM_SUBDECODERS],
}

impl<
        const LC: u32,
        const LP: u32,
        const PB: u32,
        const NUM_SUBDECODERS: usize,
        const DICT_SIZE: usize,
    > LiteralDecoder<LC, LP, PB, NUM_SUBDECODERS, DICT_SIZE>
{
    const fn new() -> Self {
        let coder = LiteralCoder::<LC, LP>::new();
        let sub_decoders = [LiteralSubdecoder::<DICT_SIZE, PB>::new(); NUM_SUBDECODERS];

        Self {
            coder,
            sub_decoders,
        }
    }

    const fn reset(&mut self) {
        self.sub_decoders = [LiteralSubdecoder {
            coder: LiteralSubcoder {
                probs: [crate::PROB_INIT; 0x300],
            },
        }; NUM_SUBDECODERS];
    }

    fn decode<R: RangeSource>(
        &mut self,
        coder: &mut LZMACoder<PB>,
        lz: &mut LZDecoder<DICT_SIZE>,
        rc: &mut RangeDecoder<R>,
    ) -> () {
        let i = self
            .coder
            .get_sub_coder_index(lz.get_byte(0) as _, lz.get_pos() as _);
        let d = &mut self.sub_decoders[i as usize];
        d.decode(coder, lz, rc)
    }
}

#[derive(Clone, Copy)]
struct LiteralSubdecoder<const DICT_SIZE: usize, const PB: u32> {
    coder: LiteralSubcoder,
}

impl<const DICT_SIZE: usize, const PB: u32> LiteralSubdecoder<DICT_SIZE, PB> {
    const fn new() -> Self {
        Self {
            coder: LiteralSubcoder::new(),
        }
    }
    pub fn decode<R: RangeSource>(
        &mut self,
        coder: &mut LZMACoder<PB>,
        lz: &mut LZDecoder<DICT_SIZE>,
        rc: &mut RangeDecoder<R>,
    ) -> () {
        let mut symbol: u32 = 1;
        let liter = coder.state.is_literal();
        if liter {
            loop {
                let b = rc.decode_bit(&mut self.coder.probs[symbol as usize]) as u32;
                symbol = (symbol << 1) | b;
                if symbol >= 0x100 {
                    break;
                }
            }
        } else {
            let r = coder.reps[0];
            let mut match_byte = lz.get_byte(r as usize) as u32;
            let mut offset = 0x100;
            let mut match_bit;
            let mut bit;

            loop {
                match_byte = match_byte << 1;
                match_bit = match_byte & offset;
                bit = rc.decode_bit(&mut self.coder.probs[(offset + match_bit + symbol) as usize])
                    as u32;
                symbol = (symbol << 1) | bit;
                offset &= (0u32.wrapping_sub(bit)) ^ !match_bit;
                if symbol >= 0x100 {
                    break;
                }
            }
        }
        lz.put_byte(symbol as u8);
        coder.state.update_literal();
        ()
    }
}

impl LengthCoder {
    fn decode<R: RangeSource>(&mut self, pos_state: usize, rc: &mut RangeDecoder<R>) -> i32 {
        if rc.decode_bit(&mut self.choice[0]) == 0 {
            return rc
                .decode_bit_tree(&mut self.low[pos_state])
                .wrapping_add(MATCH_LEN_MIN as _);
        }

        if rc.decode_bit(&mut self.choice[1]) == 0 {
            return rc
                .decode_bit_tree(&mut self.mid[pos_state])
                .wrapping_add(MATCH_LEN_MIN as _)
                .wrapping_add(LOW_SYMBOLS as _);
        }

        let r = rc
            .decode_bit_tree(&mut self.high)
            .wrapping_add(MATCH_LEN_MIN as _)
            .wrapping_add(LOW_SYMBOLS as _)
            .wrapping_add(MID_SYMBOLS as _);
        r
    }
}
