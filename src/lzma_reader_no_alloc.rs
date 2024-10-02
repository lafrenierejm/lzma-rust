use crate::io::{ErrorType, Read};

use super::decoder::LZMADecoder;
use super::lz::LZDecoder;
use super::range_dec::RangeDecoder;
use super::*;

pub const fn get_memory_usage_by_props(dict_size: u64, props_byte: u8) -> u64 {
    if dict_size > DICT_SIZE_MAX {
        panic!("Dict size too large!");
    }
    if props_byte > (4 * 5 + 4) * 9 + 8 {
        panic!("Invalid props byte");
    }
    let props: u8 = props_byte % (9 * 5);
    let lp: u8 = props / 9;
    let lc: u8 = props - lp * 9;
    get_memory_usage(dict_size, lc, lp)
}

pub const fn get_memory_usage(dict_size: u64, lc: u8, lp: u8) -> u64 {
    if lc > 8 || lp > 4 {
        panic!("Invalid lc or lp");
    }
    10 + get_dict_size(dict_size) / 1024 + ((2 * 0x300) << (lc + lp)) / 1024
}

const fn get_dict_size(dict_size: u64) -> u64 {
    if dict_size > DICT_SIZE_MAX {
        panic!("Dict size too large!");
    }
    let dict_size: u64 = if dict_size < 4096 { 4096 } else { dict_size };
    (dict_size + 15) & !15
}

pub struct LZMAReader<
    const DECODER_DICT_SIZE: usize,
    const LC: u64,
    const LP: u64,
    const PB: u64,
    const NUM_SUBDECODERS: usize,
    R: Read,
> {
    lz: LZDecoder<DECODER_DICT_SIZE>,
    rc: RangeDecoder<R>,
    lzma: LZMADecoder<LC, LP, PB, NUM_SUBDECODERS, DECODER_DICT_SIZE>,
    end_reached: bool,
    relaxed_end_cond: bool,
    remaining_size: u64,
}

pub const fn get_decoder_dict_size(uncomp_size: u64, dict_size: u64) -> u64 {
    if dict_size > DICT_SIZE_MAX {
        panic!("Dict size too large!");
    }
    let mut dict_size: u64 = get_dict_size(get_dict_size(dict_size));
    if uncomp_size <= u64::MAX / 2 && dict_size as u64 > uncomp_size {
        dict_size = get_dict_size(uncomp_size as u64);
    }

    get_dict_size(dict_size)
}

pub const fn get_lc_lp_pb_props(props: u8) -> (u64, u64, u64, u64) {
    if props > (4 * 5 + 4) * 9 + 8 {
        panic!("Invalid props byte");
    }
    let pb: u8 = props / (9 * 5);
    let props: u8 = props - (pb * 9 * 5);
    let lp: u8 = props / 9;
    let lc: u8 = props - lp * 9;
    let lc: u64 = lc as u64;
    let lp: u64 = lp as u64;
    let pb: u64 = pb as u64;
    if lc > 8 || lp > 4 || pb > 4 {
        panic!("Invalid lc or lp or pb");
    }
    (lc, lp, pb as u64, props as u64)
}

impl<
        const DECODER_DICT_SIZE: usize,
        const LC: u64,
        const LP: u64,
        const PB: u64,
        const NUM_SUBDECODERS: usize,
        R: Read,
    > Drop for LZMAReader<DECODER_DICT_SIZE, LC, LP, PB, NUM_SUBDECODERS, R>
{
    fn drop(&mut self) {
        // self.reader.clone().release();
    }
}

impl<
        const DECODER_DICT_SIZE: usize,
        const LC: u64,
        const LP: u64,
        const PB: u64,
        const NUM_SUBDECODERS: usize,
        R: Read,
    > ErrorType for LZMAReader<DECODER_DICT_SIZE, LC, LP, PB, NUM_SUBDECODERS, R>
{
    type Error = <R as ErrorType>::Error;
}

impl<
        const DECODER_DICT_SIZE: usize,
        const LC: u64,
        const LP: u64,
        const PB: u64,
        const NUM_SUBDECODERS: usize,
        R: Read,
    > LZMAReader<DECODER_DICT_SIZE, LC, LP, PB, NUM_SUBDECODERS, R>
{
    pub fn new(reader: R, preset_dict: Option<&[u8]>, uncomp_size: u64) -> Self {
        let rc = RangeDecoder::new_stream(reader);
        let lz = LZDecoder::<DECODER_DICT_SIZE>::new(preset_dict);
        let lzma = LZMADecoder::<LC, LP, PB, NUM_SUBDECODERS, DECODER_DICT_SIZE>::new();
        Self {
            // reader,
            lz,
            rc,
            lzma,
            end_reached: false,
            relaxed_end_cond: true,
            remaining_size: uncomp_size,
        }
    }

    pub fn read_u8(&mut self) -> u8 {
        let mut buf = [0; 1];
        self.rc.inner.read_exact(&mut buf).unwrap();
        buf[0]
    }

    pub fn read_u16_be(&mut self) -> u16 {
        let mut buf = [0; 2];
        self.rc.inner.read_exact(&mut buf).unwrap();
        u16::from_be_bytes(buf)
    }

    pub fn read_u32_le(&mut self) -> u32 {
        let mut buf = [0; 4];
        self.rc.inner.read_exact(&mut buf).unwrap();
        u32::from_le_bytes(buf)
    }

    pub fn read_u64_le(&mut self) -> u64 {
        let mut buf = [0; 8];
        self.rc.inner.read_exact(&mut buf).unwrap();
        u64::from_le_bytes(buf)
    }

    fn read_decode(&mut self, buf: &mut [u8]) -> usize {
        if buf.is_empty() {
            return 0;
        }
        if self.end_reached {
            return 0;
        }
        let mut size = 0;
        let mut len = buf.len() as u64;
        let mut off = 0u64;
        while len > 0 {
            let mut copy_size_max = len as u64;
            if self.remaining_size <= u64::MAX / 2 && (self.remaining_size as u64) < len {
                copy_size_max = self.remaining_size as u64;
            }
            self.lz.set_limit(copy_size_max as usize);

            self.lzma.decode(&mut self.lz, &mut self.rc);
            let copied_size = self.lz.flush(buf, off as _) as u64;
            off += copied_size;
            len -= copied_size;
            size += copied_size;
            if self.remaining_size <= u64::MAX / 2 {
                self.remaining_size -= copied_size as u64;
                if self.remaining_size == 0 {
                    self.end_reached = true;
                }
            }

            if self.end_reached {
                if self.lz.has_pending()
                    || (!self.relaxed_end_cond && !self.rc.is_stream_finished())
                {
                    return 0;
                }
                return size as _;
            }
        }
        size as _
    }
}

impl<
        const DECODER_DICT_SIZE: usize,
        const LC: u64,
        const LP: u64,
        const PB: u64,
        const NUM_SUBDECODERS: usize,
        R: Read,
    > Read for LZMAReader<DECODER_DICT_SIZE, LC, LP, PB, NUM_SUBDECODERS, R>
{
    fn read(
        &mut self,
        buf: &mut [u8],
    ) -> core::result::Result<usize, <R as embedded_io::ErrorType>::Error> {
        Ok(self.read_decode(buf))
    }
}
