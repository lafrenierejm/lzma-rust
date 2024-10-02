use super::*;
use crate::io::{ErrorKind, ErrorType, Read};

pub trait RangeSource: Read {
    fn next_byte(&mut self) -> u8;
    fn next_u64(&mut self) -> u64;
    fn read_u8(&mut self) -> u8;
    fn read_u16_be(&mut self) -> u16;
    fn read_u16_le(&mut self) -> u16;
    fn read_u32_be(&mut self) -> u32;
    fn read_u32_le(&mut self) -> u32;

    fn read_u64_be(&mut self) -> u64;

    fn read_u64_le(&mut self) -> u64;
}
impl<T: Read> RangeSource for T {
    fn read_u8(&mut self) -> u8 {
        let mut buf = [0u8; 1];
        self.read_exact(&mut buf).unwrap();
        buf[0]
    }

    fn read_u16_be(&mut self) -> u16 {
        let mut buf = [0u8; 2];
        self.read_exact(&mut buf).unwrap();
        u16::from_be_bytes(buf)
    }

    fn read_u16_le(&mut self) -> u16 {
        let mut buf = [0u8; 2];
        self.read_exact(&mut buf).unwrap();
        u16::from_le_bytes(buf)
    }

    fn read_u32_be(&mut self) -> u32 {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf).unwrap();
        u32::from_be_bytes(buf)
    }

    fn read_u32_le(&mut self) -> u32 {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf).unwrap();
        u32::from_le_bytes(buf)
    }

    fn read_u64_be(&mut self) -> u64 {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf).unwrap();
        u64::from_be_bytes(buf)
    }

    fn read_u64_le(&mut self) -> u64 {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf).unwrap();
        u64::from_le_bytes(buf)
    }

    fn next_byte(&mut self) -> u8 {
        self.read_u8()
    }
    fn next_u64(&mut self) -> u64 {
        self.read_u64_be()
    }
}

impl<R: RangeSource> ErrorType for RangeDecoder<R> {
    type Error = ErrorKind;
}

pub struct RangeDecoder<R: RangeSource> {
    pub(crate) inner: R,
    range: u64,
    code: u64,
}
impl<const DICT_SIZE_MINUS_FIVE: usize> RangeDecoder<RangeDecoderBuffer<DICT_SIZE_MINUS_FIVE>> {
    pub const fn new_buffer() -> Self {
        Self {
            inner: RangeDecoderBuffer::<DICT_SIZE_MINUS_FIVE>::new(),
            code: 0,
            range: 0,
        }
    }
}

impl<R: RangeSource> RangeDecoder<R> {
    pub fn new_stream(mut inner: R) -> Self {
        let b = inner.next_byte();
        if b != 0x00 {
            unreachable!()
        }
        let code = inner.next_u64();
        Self {
            inner,
            code,
            range: (0xFFFFFFFFu64),
        }
    }

    pub const fn is_stream_finished(&self) -> bool {
        self.code == 0
    }
}

impl<R: RangeSource> RangeDecoder<R> {
    pub fn normalize(&mut self) -> () {
        if self.range < 0x0100_0000 {
            let b = self.inner.next_byte() as u64;
            let code = ((self.code) << SHIFT_BITS) | b;
            self.code = code;
            let range = (self.range) << SHIFT_BITS;
            self.range = range;
        }
        ()
    }

    pub fn decode_bit(&mut self, prob: &mut u16) -> i64 {
        self.normalize();
        let bound = (self.range >> (BIT_MODEL_TOTAL_BITS as i64)) * (*prob as u64);
        // let mask = 0x80000000u64;
        // let cm = self.code ^ mask;
        // let bm = bound ^ mask;
        if self.code < bound {
            self.range = bound;
            *prob += (BIT_MODEL_TOTAL as u16 - *prob) >> (MOVE_BITS as u16);
            0
        } else {
            self.range -= bound;
            self.code -= bound;
            *prob -= *prob >> (MOVE_BITS as u16);
            1
        }
    }

    pub fn decode_bit_tree(&mut self, probs: &mut [u16]) -> i64 {
        let mut symbol = 1;
        loop {
            symbol = (symbol << 1) | self.decode_bit(&mut probs[symbol as usize]);
            if symbol >= probs.len() as i64 {
                break;
            }
        }
        symbol - probs.len() as i64
    }

    pub fn decode_reverse_bit_tree(&mut self, probs: &mut [u16]) -> i64 {
        let mut symbol = 1;
        let mut i = 0;
        let mut result = 0;
        loop {
            let bit = self.decode_bit(&mut probs[symbol as usize]);
            symbol = (symbol << 1) | bit;
            result |= bit << i;
            i += 1;
            if symbol >= probs.len() as i64 {
                break;
            }
        }
        result as i64
    }

    pub fn decode_direct_bits(&mut self, count: u64) -> i64 {
        let mut result = 0;
        for _ in 0..count {
            // }
            // loop {
            self.normalize();
            self.range = self.range >> 1;
            let t = (self.code.wrapping_sub(self.range)) >> 31;
            self.code -= self.range & (t.wrapping_sub(1));
            result = (result << 1) | (1u64.wrapping_sub(t));
            // count -= 1;
            // if count == 0 {
            //     break;
            // }
        }
        result as i64
    }
}

pub struct RangeDecoderBuffer<const DICT_SIZE: usize> {
    buf: [u8; DICT_SIZE],
    pos: usize,
}

impl<const DICT_SIZE: usize> ErrorType for RangeDecoderBuffer<DICT_SIZE> {
    type Error = ErrorKind;
}

impl<const DICT_SIZE: usize> Read for RangeDecoderBuffer<DICT_SIZE> {
    fn read(&mut self, buf: &mut [u8]) -> core::result::Result<usize, embedded_io::ErrorKind> {
        let len = buf.len();
        let pos = self.pos;
        let end = pos + len;

        if end > DICT_SIZE {
            return Err(ErrorKind::InvalidInput);
        }

        buf.copy_from_slice(&self.buf[pos..end]);
        self.pos = end;
        Ok(len)
    }
}

impl<const DICT_SIZE: usize> RangeDecoder<RangeDecoderBuffer<DICT_SIZE>> {
    pub fn prepare<R: RangeSource>(&mut self, mut reader: R, len: usize) -> () {
        if len < 5 {
            unreachable!()
        }

        let b = reader.read_u8();
        if b != 0x00 {
            unreachable!()
        }
        self.code = reader.read_u64_be();

        self.range = 0xFFFFFFFFu64;
        let len = len - 5;
        let pos = DICT_SIZE - len;
        let end = pos + len;
        self.inner.pos = pos;
        reader.read_exact(&mut self.inner.buf[pos..end]).unwrap()
    }

    #[inline]
    pub fn is_finished(&self) -> bool {
        self.inner.pos == DICT_SIZE && self.code == 0
    }
}

impl<const DICT_SIZE: usize> RangeDecoderBuffer<DICT_SIZE> {
    pub const fn new() -> Self {
        Self {
            buf: [0; DICT_SIZE],
            pos: DICT_SIZE,
        }
    }
}
