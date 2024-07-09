use crate::io::{Error, ErrorKind, Read, Result};

use super::decoder::LZMADecoder;
use super::lz::LZDecoder;
use super::range_dec::RangeDecoder;
use super::*;

pub trait DictSize {
    const DICT_SIZE: u32;
}

pub trait PropsByte {
    const PROPS_BYTE: u8;
}

pub trait UncompSize {
    const UNCOMP_SIZE: u64;
}

pub trait Lc {
    const LC: u32;
}

pub trait Lp {
    const LP: u32;
}

pub trait Pb {
    const PB: u32;
}

pub const fn get_memory_usage_by_props<T: DictSize + PropsByte>() -> Result<u32> {
    const DICT_SIZE: u32 = T::DICT_SIZE;
    const PROPS_BYTE: u8 = T::PROPS_BYTE;
    if DICT_SIZE > DICT_SIZE_MAX {
        return Err(Error::new(ErrorKind::InvalidInput, "dict size too large"));
    }
    if PROPS_BYTE > (4 * 5 + 4) * 9 + 8 {
        return Err(Error::new(ErrorKind::InvalidInput, "Invalid props byte"));
    }
    const PROPS: u8 = PROPS_BYTE % (9 * 5);
    const LP: u8 = PROPS / 9;
    const LC: u8 = PROPS - LP * 9;
    get_memory_usage::<DICT_SIZE, LC, LP>()
}
pub const fn get_memory_usage<T: DictSize + Lp + Lc>() -> Result<u32> {
    const DICT_SIZE: u32 = T::DICT_SIZE;
    const LC: u32 = T::LC;
    const LP: u32 = T::LP;

    if LC > 8 || LP > 4 {
        return Err(Error::new(ErrorKind::InvalidInput, "Invalid lc or lp"));
    }
    return Ok(10 + get_dict_size::<DICT_SIZE>()? / 1024 + ((2 * 0x300) << (LC + LP)) / 1024);
}

const fn get_dict_size<T: DictSize>() -> Result<u32> {
    const DICT_SIZE: u32 = T::DICT_SIZE;
    if DICT_SIZE > DICT_SIZE_MAX {
        return Err(Error::new(ErrorKind::InvalidInput, "dict size too large"));
    }
    const DICT_SIZE: u32 = DICT_SIZE.max(4096);
    Ok((DICT_SIZE + 15) & !15)
}

pub struct LZMAReader<const DECODER_DICT_SIZE: u32> {
    lz: LZDecoder<DECODER_DICT_SIZE>,
    rc: RangeDecoder<R>,
    lzma: LZMADecoder,
    end_reached: bool,
    relaxed_end_cond: bool,
    remaining_size: u64,
}

pub const fn get_decoder_dict_size<T: UncompSize + DictSize>() -> Result<u32> {
    const UNCOMP_SIZE: u64 = T::UNCOMP_SIZE;
    const DICT_SIZE: u32 = T::DICT_SIZE;
    if DICT_SIZE > DICT_SIZE_MAX {
        return Err(Error::new(ErrorKind::InvalidInput, "dict size too large"));
    }
    const DICT_SIZE_TWO: u32 = get_dict_size::<DICT_SIZE>()?;
    const DICT_SIZE_THREE: u32 = get_dict_size::<DICT_SIZE_TWO>()?;
    const DICT_SIZE_FOUR: u32 =
        if UNCOMP_SIZE <= u64::MAX / 2 && DICT_SIZE_THREE as u64 > UNCOMP_SIZE {
            const UNCOMP_SIZE_THIRTY_TWO: u32 = UNCOMP_SIZE as u32;
            get_dict_size::<UNCOMP_SIZE_THIRTY_TWO>()?
        } else {
            DICT_SIZE_THREE
        };

    get_dict_size::<DICT_SIZE_FOUR>()
}

pub const fn get_lc_lp_pb<T: PropsByte>() -> Result<(u32, u32, u32)> {
    const PROPS: u8 = T::PROPS_BYTE;
    if PROPS > (4 * 5 + 4) * 9 + 8 {
        return Err(Error::new(ErrorKind::InvalidInput, "Invalid props byte"));
    }
    const PB: u8 = PROPS / (9 * 5);
    const PROPS_TWO: u8 = PROPS - (PB * 9 * 5);
    const LP: u8 = PROPS_TWO / 9;
    const LC: u8 = PROPS_TWO - LP * 9;
    const LC_TWO: u32 = LC as u32;
    const LP_TWO: u32 = LP as u32;
    const PB_TWO: u32 = PB as u32;
    if LC_TWO > 8 || LP_TWO > 4 || PB_TWO > 4 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "Invalid lc or lp or pb",
        ));
    }
    Ok((LC_TWO, LP_TWO, PB_TWO))
}

pub fn new_reader<T: UncompSize + DictSize + PropsByte, R>(
    reader: R,
) -> Result<LzmaReader<DECODER_DICT_SIZE, R>> {
    const UNCOMP_SIZE: u64 = T::UNCOMP_SIZE;
    const DICT_SIZE: u32 = T::DICT_SIZE;
    const PROPS_BYTE: u8 = T::PROPS_BYTE;
    const LC_LP_PB: (u32, u32, u32) = get_lc_lp_pb::<PROPS>()?;
    const LC: u32 = LC_LP_PB.0;
    const LP: u32 = LC_LP_PB.1;
    const PB: u32 = LC_LP_PB.2;
    const DECODER_DICT_SIZE: u32 = get_decoder_dict_size::<UNCOMP_SIZE, DICT_SIZE>()?;

    Ok(LzmaReader::<DECODER_DICT_SIZE, R>::new(
        UNCOMP_SIZE,
        LC,
        LP,
        PB,
        reader,
        None,
    )?)
}

impl<R> Drop for LZMAReader<R> {
    fn drop(&mut self) {
        // self.reader.clone().release();
    }
}

pub fn read_u8<R: Read>(reader: &mut R) -> Result<u8> {
    let mut buf = [0; 1];
    reader.inner.read_exact(&mut buf)?;
    Ok(buf[0])
}

pub fn read_u16_be<R: Read>(reader: &mut R) -> Result<u16> {
    let mut buf = [0; 2];
    reader.inner.read_exact(&mut buf)?;
    Ok(u16::from_be_bytes(buf))
}

pub fn read_u32_le<R: Read>(reader: &mut R) -> Result<u32> {
    let mut buf = [0; 4];
    reader.inner.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

pub fn read_u64_le<R: Read>(reader: &mut R) -> Result<u64> {
    let mut buf = [0; 8];
    reader.inner.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

impl<const DECODER_DICT_SIZE: u32, R: Read> LZMAReader<DECODER_DICT_SIZE, R> {
    pub fn new(
        reader: R,
        preset_dict: Option<&[u8]>,
        uncomp_size: u64,
        lc: u32,
        lp: u32,
        pb: u32,
    ) -> Result<Self> {
        let rc = RangeDecoder::new_stream(reader);
        let rc = match rc {
            Ok(r) => r,
            Err(e) => {
                return Err(e);
            }
        };

        let lz = LZDecoder::<DECODER_DICT_SIZE>::new(preset_dict);
        let lzma = LZMADecoder::new(LC, LP, PB);
        Ok(Self {
            // reader,
            lz,
            rc,
            lzma,
            end_reached: false,
            relaxed_end_cond: true,
            remaining_size: UNCOMP_SIZE,
        })
    }

    fn read_decode(&mut self, buf: &mut [u8]) -> Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.end_reached {
            return Ok(0);
        }
        let mut size = 0;
        let mut len = buf.len() as u32;
        let mut off = 0u32;
        while len > 0 {
            let mut copy_size_max = len as u32;
            if self.remaining_size <= u64::MAX / 2 && (self.remaining_size as u32) < len {
                copy_size_max = self.remaining_size as u32;
            }
            self.lz.set_limit(copy_size_max as usize);

            match self.lzma.decode(&mut self.lz, &mut self.rc) {
                Ok(_) => {}
                Err(e) => {
                    if self.remaining_size != u64::MAX || !self.lzma.end_marker_detected() {
                        return Err(e);
                    }
                    self.end_reached = true;
                    self.rc.normalize()?;
                }
            }

            let copied_size = self.lz.flush(buf, off as _) as u32;
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
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "end reached but not decoder finished",
                    ));
                }
                return Ok(size as _);
            }
        }
        Ok(size as _)
    }
}

impl<R: Read> Read for LZMAReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.read_decode(buf)
    }
}
