use super::*;

use crate::io::{
    error, read_exact_error_kind, ErrorKind, Read, ReadExactResult, ReadSelfExactResult, Result,
};

pub trait RangeSource: Read {
    fn next_byte(&mut self) -> ReadSelfExactResult<u8>;
    fn next_u32(&mut self) -> ReadSelfExactResult<u32>;
    fn read_u8(&mut self) -> ReadSelfExactResult<u8>;
    fn read_u16_be(&mut self) -> ReadSelfExactResult<u16>;
    fn read_u16_le(&mut self) -> ReadSelfExactResult<u16>;
    fn read_u32_be(&mut self) -> ReadSelfExactResult<u32>;
    fn read_u32_le(&mut self) -> ReadSelfExactResult<u32>;

    fn read_u64_be(&mut self) -> ReadSelfExactResult<u64>;

    fn read_u64_le(&mut self) -> ReadSelfExactResult<u64>;
}
impl<R: Read> RangeSource for R {
    fn read_u8(&mut self) -> ReadExactResult<u8> {
        let mut buf = [0u8; 1];
        self.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    fn read_u16_be(&mut self) -> ReadExactResult<u16> {
        let mut buf = [0u8; 2];
        self.read_exact(&mut buf)?;
        Ok(u16::from_be_bytes(buf))
    }

    fn read_u16_le(&mut self) -> ReadExactResult<u16> {
        let mut buf = [0u8; 2];
        self.read_exact(&mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }

    fn read_u32_be(&mut self) -> ReadExactResult<u32> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }

    fn read_u32_le(&mut self) -> ReadExactResult<u32> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    fn read_u64_be(&mut self) -> ReadExactResult<u64> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(u64::from_be_bytes(buf))
    }

    fn read_u64_le(&mut self) -> ReadExactResult<u64> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    fn next_byte(&mut self) -> ReadExactResult<u8> {
        self.read_u8()
    }
    fn next_u32(&mut self) -> ReadExactResult<u32> {
        self.read_u32_be()
    }
}

pub struct RangeDecoder<R> {
    inner: R,
    range: u32,
    code: u32,
}
impl RangeDecoder<RangeDecoderBuffer> {
    pub fn new_buffer(len: usize) -> Self {
        Self {
            inner: RangeDecoderBuffer::new(len - 5),
            code: 0,
            range: 0,
        }
    }
}

impl<R: RangeSource> RangeDecoder<R> {
    pub fn new_stream(mut inner: R) -> crate::io::read_exact_result!(Self, Self) {
        let b = inner.next_byte()?;
        if b != 0x00 {
            return error!(
                read_exact_error_kind!(R, ErrorKind::InvalidInput),
                "range decoder first byte is 0"
            );
        }
        let code = inner.next_u32()?;
        Ok(Self {
            inner,
            code,
            range: (0xFFFFFFFFu32),
        })
    }

    pub fn is_stream_finished(&self) -> bool {
        self.code == 0
    }
}

impl<R: RangeSource> RangeDecoder<R> {
    pub fn normalize(&mut self) -> ReadExactResult<()> {
        if self.range < 0x0100_0000 {
            let b = self.inner.next_byte()? as u32;
            let code = ((self.code) << SHIFT_BITS) | b;
            self.code = code;
            let range = (self.range) << SHIFT_BITS;
            self.range = range;
        }
        Ok(())
    }

    pub fn decode_bit(&mut self, prob: &mut u16) -> ReadExactResult<i32> {
        self.normalize()?;
        let bound = (self.range >> (BIT_MODEL_TOTAL_BITS as i32)) * (*prob as u32);
        // let mask = 0x80000000u32;
        // let cm = self.code ^ mask;
        // let bm = bound ^ mask;
        if self.code < bound {
            self.range = bound;
            *prob += (BIT_MODEL_TOTAL as u16 - *prob) >> (MOVE_BITS as u16);
            Ok(0)
        } else {
            self.range -= bound;
            self.code -= bound;
            *prob -= *prob >> (MOVE_BITS as u16);
            Ok(1)
        }
    }

    pub fn decode_bit_tree(&mut self, probs: &mut [u16]) -> ReadExactResult<i32> {
        let mut symbol = 1;
        loop {
            symbol = (symbol << 1) | self.decode_bit(&mut probs[symbol as usize])?;
            if symbol >= probs.len() as i32 {
                break;
            }
        }
        Ok(symbol - probs.len() as i32)
    }

    pub fn decode_reverse_bit_tree(&mut self, probs: &mut [u16]) -> ReadExactResult<i32> {
        let mut symbol = 1;
        let mut i = 0;
        let mut result = 0;
        loop {
            let bit = self.decode_bit(&mut probs[symbol as usize])?;
            symbol = (symbol << 1) | bit;
            result |= bit << i;
            i += 1;
            if symbol >= probs.len() as i32 {
                break;
            }
        }
        Ok(result)
    }

    pub fn decode_direct_bits(&mut self, count: u32) -> ReadExactResult<i32> {
        let mut result = 0;
        for _ in 0..count {
            // }
            // loop {
            self.normalize()?;
            self.range >>= 1;
            let t = (self.code.wrapping_sub(self.range)) >> 31;
            self.code -= self.range & (t.wrapping_sub(1));
            result = (result << 1) | (1u32.wrapping_sub(t));
            // count -= 1;
            // if count == 0 {
            //     break;
            // }
        }
        Ok(result as _)
    }
}

pub struct RangeDecoderBuffer {
    buf: crate::Vec<u8>,
    pos: usize,
}
impl RangeDecoder<RangeDecoderBuffer> {
    pub fn prepare<R: Read>(&mut self, mut reader: R, len: usize) -> ReadExactResult<()> {
        if len < 5 {
            return error!(
                read_exact_error_kind!(R, ErrorKind::InvalidInput),
                "buffer len must >= 5"
            );
        }

        let b = reader.read_u8()?;
        if b != 0x00 {
            return error!(
                read_exact_error_kind!(R, ErrorKind::InvalidInput),
                "range decoder first byte is 0"
            );
        }
        self.code = reader.read_u32_be()?;

        self.range = 0xFFFFFFFFu32;
        let len = len - 5;
        let pos = self.inner.buf.len() - len;
        let end = pos + len;
        self.inner.pos = pos;
        reader.read_exact(&mut self.inner.buf[pos..end])
    }

    #[inline]
    pub fn is_finished(&self) -> bool {
        self.inner.pos == self.inner.buf.len() && self.code == 0
    }
}

impl RangeDecoderBuffer {
    pub fn new(len: usize) -> Self {
        Self {
            buf: vec![0; len],
            pos: len,
        }
    }
}

#[cfg(feature = "no_std")]
impl embedded_io::ErrorType for RangeDecoderBuffer {
    type Error = embedded_io::ErrorKind;
}

impl Read for RangeDecoderBuffer {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let len = buf.len();
        let pos = self.pos;
        let end = pos + len;

        if end > self.buf.len() {
            return error!(
                ErrorKind::InvalidInput,
                "Attempted to read past end of RangeEncoderBuffer"
            );
        }

        buf.copy_from_slice(&self.buf[pos..end]);
        self.pos = end;
        Ok(len)
    }
}
