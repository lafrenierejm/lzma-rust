use crate::io::{ErrorKind, ErrorType, Read, ReadExactError, Result};

#[derive(Copy, Clone)]
pub struct LZDecoder<const DICT_SIZE: usize> {
    buf: [u8; DICT_SIZE],
    start: usize,
    pos: usize,
    full: usize,
    limit: usize,
    pending_len: usize,
    pending_dist: usize,
}

impl<const DICT_SIZE: usize> Default for LZDecoder<DICT_SIZE> {
    fn default() -> Self {
        Self {
            buf: [0; DICT_SIZE],
            start: 0,
            pos: 0,
            full: 0,
            limit: 0,
            pending_len: 0,
            pending_dist: 0,
        }
    }
}

impl<const DICT_SIZE: usize> LZDecoder<DICT_SIZE> {
    pub const fn new(preset_dict: Option<&[u8]>) -> Self {
        let mut buf = [0; DICT_SIZE];
        let mut pos = 0;
        let mut full = 0;
        let mut start = 0;
        if let Some(preset) = preset_dict {
            pos = if preset.len() < DICT_SIZE {
                preset.len()
            } else {
                DICT_SIZE
            };
            full = pos;
            start = pos;
            let ps = preset.len() - pos;

            let out_arr = unsafe {
                let ptr = buf.as_mut_ptr();
                core::slice::from_raw_parts_mut(ptr, pos)
            };

            let in_arr = unsafe {
                let ptr = preset.as_ptr().add(ps);
                core::slice::from_raw_parts(ptr, pos)
            };

            crate::copy_from_slice(out_arr, in_arr);
        }
        Self {
            buf,
            pos,
            full,
            start,
            limit: DICT_SIZE,
            pending_len: 0,
            pending_dist: 0,
        }
    }

    pub fn reset(&mut self) {
        self.start = 0;
        self.pos = 0;
        self.full = 0;
        self.limit = 0;
        self.buf[DICT_SIZE - 1] = 0;
    }

    pub fn set_limit(&mut self, out_max: usize) {
        self.limit = (out_max + self.pos).min(DICT_SIZE);
    }

    pub fn has_space(&self) -> bool {
        self.pos < self.limit
    }

    pub fn has_pending(&self) -> bool {
        self.pending_len > 0
    }

    pub fn get_pos(&self) -> usize {
        self.pos
    }

    pub fn get_byte(&self, dist: usize) -> u8 {
        let offset = if dist >= self.pos {
            DICT_SIZE + self.pos - dist - 1
        } else {
            self.pos - dist - 1
        };
        self.buf[offset]
    }

    pub fn put_byte(&mut self, b: u8) {
        self.buf[self.pos] = b;
        self.pos += 1;
        if self.full < self.pos {
            self.full = self.pos;
        }
    }

    pub fn repeat(&mut self, dist: usize, len: usize) -> Result<()> {
        if dist >= self.full {
            return Err(ErrorKind::InvalidInput);
        }
        let mut left = usize::min(self.limit - self.pos, len);
        self.pending_len = len - left;
        self.pending_dist = dist;

        let back = if self.pos < dist + 1 {
            // The distance wraps around to the end of the cyclic dictionary
            // buffer. We cannot get here if the dictionary isn't full.
            assert!(self.full == DICT_SIZE);
            let mut back = DICT_SIZE + self.pos - dist - 1;

            // Here we will never copy more than dist + 1 bytes and
            // so the copying won't repeat from its own output.
            // Thus, we can always use core::ptr::copy safely.
            let copy_size = usize::min(DICT_SIZE - back, left);
            assert!(copy_size <= dist + 1);
            unsafe {
                let buf_ptr = self.buf.as_mut_ptr();
                let src = buf_ptr.add(back);
                let dest = buf_ptr.add(self.pos);
                core::ptr::copy_nonoverlapping(src, dest, copy_size);
            }
            self.pos += copy_size;
            back = 0;
            left -= copy_size;

            if left == 0 {
                return Ok(());
            }
            back
        } else {
            self.pos - dist - 1
        };

        assert!(back < self.pos);
        assert!(left > 0);

        loop {
            let copy_size = left.min(self.pos - back);
            let pos = self.pos;
            unsafe {
                let buf_ptr = self.buf.as_mut_ptr();
                let src = buf_ptr.add(back);
                let dest = buf_ptr.add(pos);
                core::ptr::copy_nonoverlapping(src, dest, copy_size);
            }

            self.pos += copy_size;
            left -= copy_size;
            if left == 0 {
                break;
            }
        }

        if self.full < self.pos {
            self.full = self.pos;
        }
        Ok(())
    }

    pub fn repeat_pending(&mut self) -> Result<()> {
        if self.pending_len > 0 {
            self.repeat(self.pending_dist, self.pending_len)?;
        }
        Ok(())
    }

    pub fn copy_uncompressed<R: Read>(
        &mut self,
        mut in_data: R,
        len: usize,
    ) -> core::result::Result<(), ReadExactError<<R as ErrorType>::Error>> {
        let copy_size = (DICT_SIZE - self.pos).min(len);
        let buf = &mut self.buf[self.pos..(self.pos + copy_size)];
        in_data.read_exact(buf)?;
        self.pos += copy_size;
        if self.full < self.pos {
            self.full = self.pos;
        }
        Ok(())
    }

    pub fn flush(&mut self, out: &mut [u8], out_off: usize) -> usize {
        let copy_size = self.pos - self.start;
        if self.pos == DICT_SIZE {
            self.pos = 0;
        }
        out[out_off..(out_off + copy_size)]
            .copy_from_slice(&self.buf[self.start..(self.start + copy_size)]);

        self.start = self.pos;
        copy_size
    }
}

#[cfg(test)]
mod tests {}
