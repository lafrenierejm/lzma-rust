use super::{
    decoder::LZMADecoder,
    lz::LZDecoder,
    range_dec::{RangeDecoder, RangeDecoderBuffer},
};
use crate::io::{ErrorType, Read};
pub const COMPRESSED_SIZE_MAX: usize = 1 << 16;

/// Decompresses a raw LZMA2 stream (no XZ headers).
/// # Examples
/// ```
/// use io::Read;
/// use lzma_rust::LZMA2Reader;
/// use lzma_rust::LZMA2Options;
/// let compressed = [1, 0, 12, 72, 101, 108, 108, 111, 44, 32, 119, 111, 114, 108, 100, 33, 0];
/// let mut reader = LZMA2Reader::new(compressed, LZMA2Options::DICT_SIZE_DEFAULT, None);
/// let mut decompressed = crate::Vec::new();
/// reader.read_to_end(&mut decompressed);
/// assert_eq!(&decompressed[..], b"Hello, world!");
///
/// ```
///
impl<
        const DICT_SIZE: usize,
        const LC: u32,
        const LP: u32,
        const PB: u32,
        const NUM_SUBDECODERS: usize,
        R: Read,
    > ErrorType for LZMA2Reader<DICT_SIZE, LC, LP, PB, NUM_SUBDECODERS, R>
where
    [(); (DICT_SIZE + 15) & !15]:,
{
    type Error = <R as embedded_io::ErrorType>::Error;
}
pub struct LZMA2Reader<
    const DICT_SIZE: usize,
    const LC: u32,
    const LP: u32,
    const PB: u32,
    const NUM_SUBDECODERS: usize,
    R: Read,
> where
    [(); COMPRESSED_SIZE_MAX as usize - 5]:,
    [(); (DICT_SIZE + 15) & !15]:,
{
    pub inner: R,
    use_lzma: bool,
    lz: LZDecoder<
        {
            {
                (DICT_SIZE + 15) & !15
            }
        },
    >,
    rc: RangeDecoder<RangeDecoderBuffer<{ COMPRESSED_SIZE_MAX as usize - 5 }>>,
    lzma: LZMADecoder<
        LC,
        LP,
        PB,
        NUM_SUBDECODERS,
        {
            {
                (DICT_SIZE + 15) & !15
            }
        },
    >,
    uncompressed_size: usize,
    is_lzma_chunk: bool,
    need_dict_reset: bool,
    need_props: bool,
    end_reached: bool,
}
#[inline]
pub const fn get_memory_usage<const DICT_SIZE: usize>() -> usize {
    40 + COMPRESSED_SIZE_MAX / 1024 + get_dict_size::<DICT_SIZE>() / 1024
}

#[inline]
pub const fn get_dict_size<const DICT_SIZE: usize>() -> usize {
    DICT_SIZE + 15 & !15
}

#[inline]
pub const fn get_props_pb_lp_lc_num_subdecoders(props: u8) -> (u32, u32, u32, u32, u32) {
    if props > (4 * 5 + 4) * 9 + 8 {
        unreachable!()
    }
    let pb = props / (9 * 5);
    let props = props - pb * 9 * 5;
    let lp = props / 9;
    let lc = props - lp * 9;
    if lc + lp > 4 {
        unreachable!()
    }

    let num_subdecoders = 1 << (lc + lp);

    (
        props as u32,
        pb as u32,
        lp as u32,
        lc as u32,
        num_subdecoders as u32,
    )
}

impl<
        const DICT_SIZE: usize,
        const LC: u32,
        const LP: u32,
        const PB: u32,
        const NUM_SUBDECODERS: usize,
        R: Read + ErrorType,
    > LZMA2Reader<DICT_SIZE, LC, LP, PB, NUM_SUBDECODERS, R>
where
    [(); COMPRESSED_SIZE_MAX as usize - 5]:,
    [(); (DICT_SIZE + 15) & !15]:,
{
    /// Create a new LZMA2 reader.
    /// `inner` is the reader to read compressed data from.
    /// `dict_size` is the dictionary size in bytes.
    pub const fn new(inner: R, preset_dict: Option<&[u8]>) -> Self {
        let has_preset = if let Some(ref preset_dict) = preset_dict {
            preset_dict.len() > 0
        } else {
            false
        };
        let lz = LZDecoder::<{ (DICT_SIZE + 15) & !15 }>::new(preset_dict);
        const COMPRESSED_SIZE_MAX_MINUS_FIVE: usize = COMPRESSED_SIZE_MAX as usize - 5;
        let rc = RangeDecoder::<RangeDecoderBuffer<COMPRESSED_SIZE_MAX_MINUS_FIVE>>::new_buffer();
        Self {
            inner,
            lz,
            rc,
            use_lzma: false,
            lzma: LZMADecoder::<LC, LP, PB, NUM_SUBDECODERS, { (DICT_SIZE + 15) & !15 }>::new(),
            uncompressed_size: 0,
            is_lzma_chunk: false,
            need_dict_reset: !has_preset,
            need_props: true,
            end_reached: false,
        }
    }

    pub fn into_inner(self) -> R {
        self.inner
    }

    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    pub fn read_u8(&mut self) -> u8 {
        let mut buf = [0; 1];
        self.inner.read_exact(&mut buf).unwrap();
        buf[0]
    }

    pub fn read_u16_be(&mut self) -> u16 {
        let mut buf = [0; 2];
        self.inner.read_exact(&mut buf).unwrap();
        u16::from_be_bytes(buf)
    }

    fn decode_chunk_header(&mut self) {
        let control = self.read_u8();
        if control == 0x00 {
            self.end_reached = true;
            return ();
        }

        if control >= 0xE0 || control == 0x01 {
            self.need_props = true;
            self.need_dict_reset = false;
            self.lz.reset();
        } else if self.need_dict_reset {
            unreachable!();
        }
        if control >= 0x80 {
            self.is_lzma_chunk = true;
            self.uncompressed_size = ((control & 0x1F) as usize) << 16;
            self.uncompressed_size += self.read_u16_be() as usize + 1;
            let compressed_size = self.read_u16_be() as usize + 1;
            if control >= 0xC0 {
                self.need_props = false;
                let _ = self.read_u8();
                self.use_lzma = true;
                self.lzma.reset();
            } else if self.need_props {
                return;
            } else if control >= 0xA0 && self.use_lzma {
                self.lzma.reset();
            }
            self.rc.prepare(&mut self.inner, compressed_size);
        } else if control > 0x02 {
            return;
        } else {
            self.is_lzma_chunk = false;
            self.uncompressed_size = (self.read_u16_be() + 1) as _;
        }
    }

    fn read_decode(&mut self, buf: &mut [u8]) -> usize {
        if buf.is_empty() {
            return 0;
        }

        if self.end_reached {
            return 0;
        }
        let mut size = 0;
        let mut len = buf.len();
        let mut off = 0;
        while len > 0 {
            if self.uncompressed_size == 0 {
                self.decode_chunk_header();
                if self.end_reached {
                    return size;
                }
            }

            let copy_size_max = self.uncompressed_size.min(len);
            if !self.is_lzma_chunk {
                self.lz
                    .copy_uncompressed(&mut self.inner, copy_size_max)
                    .unwrap();
            } else {
                self.lz.set_limit(copy_size_max);
                if self.use_lzma {
                    self.lzma.decode(&mut self.lz, &mut self.rc);
                }
            }

            {
                let copied_size = self.lz.flush(buf, off);
                off += copied_size;
                len -= copied_size;
                size += copied_size;
                self.uncompressed_size -= copied_size;
                if self.uncompressed_size == 0 && (!self.rc.is_finished() || self.lz.has_pending())
                {
                    return 0;
                }
            }
        }
        size
    }
}

impl<
        const DICT_SIZE: usize,
        const LC: u32,
        const LP: u32,
        const PB: u32,
        const NUM_SUBDECODERS: usize,
        R: Read,
    > Read for LZMA2Reader<DICT_SIZE, LC, LP, PB, NUM_SUBDECODERS, R>
where
    [(); COMPRESSED_SIZE_MAX as usize - 5]:,
    [(); (DICT_SIZE + 15) & !15]:,
{
    fn read(
        &mut self,
        buf: &mut [u8],
    ) -> core::result::Result<usize, <R as embedded_io::ErrorType>::Error> {
        Ok(self.read_decode(buf))
    }
}
