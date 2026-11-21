//! Little-endian bytes and LEB128 varints.

#[derive(Default)]
pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn i8(&mut self, v: i8) {
        self.buf.push(v as u8);
    }

    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn i16(&mut self, v: i16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn var(&mut self, mut v: u32) {
        while v >= 0x80 {
            self.buf.push(v as u8 | 0x80);
            v >>= 7;
        }
        self.buf.push(v as u8);
    }

    pub fn svar(&mut self, v: i32) {
        self.var(((v << 1) ^ (v >> 31)) as u32);
    }

    pub fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
}

/// Every read is None past the end of the buffer.
pub struct Reader<'a> {
    buf: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, at: 0 }
    }

    pub fn done(&self) -> bool {
        self.at >= self.buf.len()
    }

    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let b = self.buf.get(self.at..self.at + N)?;
        self.at += N;
        b.try_into().ok()
    }

    pub fn u8(&mut self) -> Option<u8> {
        Some(self.take::<1>()?[0])
    }

    pub fn i8(&mut self) -> Option<i8> {
        Some(self.u8()? as i8)
    }

    pub fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take()?))
    }

    pub fn i16(&mut self) -> Option<i16> {
        Some(i16::from_le_bytes(self.take()?))
    }

    pub fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take()?))
    }

    pub fn var(&mut self) -> Option<u32> {
        let mut v = 0u32;
        for shift in (0..35).step_by(7) {
            let b = self.u8()?;
            v |= ((b & 0x7f) as u32).checked_shl(shift)?;
            if b & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    }

    pub fn svar(&mut self) -> Option<i32> {
        let v = self.var()?;
        Some((v >> 1) as i32 ^ -((v & 1) as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_of_number_reads_back() {
        let mut w = Writer::new();
        w.u8(7);
        w.i8(-3);
        w.u16(65000);
        w.i16(-30000);
        w.u32(0xDEAD_BEEF);
        for v in [0, 1, 127, 128, 16_383, 16_384, u32::MAX] {
            w.var(v);
        }
        for v in [0, -1, 1, -64, 64, i32::MIN, i32::MAX] {
            w.svar(v);
        }
        let mut r = Reader::new(&w.buf);
        assert_eq!((r.u8(), r.i8(), r.u16(), r.i16(), r.u32()), (Some(7), Some(-3), Some(65000), Some(-30000), Some(0xDEAD_BEEF)));
        for v in [0, 1, 127, 128, 16_383, 16_384, u32::MAX] {
            assert_eq!(r.var(), Some(v));
        }
        for v in [0, -1, 1, -64, 64, i32::MIN, i32::MAX] {
            assert_eq!(r.svar(), Some(v));
        }
        assert!(r.done());
        assert_eq!(r.u8(), None);
    }
}
