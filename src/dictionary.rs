//use core::marker::PhantomData;

pub trait DictionaryValue<D: Dictionary>: TryFrom<D::Object> {
    fn index() -> D::Index;
}

pub trait IntoBuf {
    fn into_buf<'a>(self: &'a Self, buf: &'a mut [u8]) -> usize;
}

impl IntoBuf for u32 {
    fn into_buf<'a>(self: &'a Self, buf: &'a mut [u8]) -> usize {
        let data = self.to_le_bytes();
        let n = data.len();
        assert!(buf.len() >= n);
        buf[0..n].copy_from_slice(&data);
        n
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Index {
    pub index: u16,
    pub sub: u8,
}

impl Index {
    pub fn new(index: u16, sub: u8) -> Self {
        Index { index, sub }
    }
    /// Writes the Index to a mutable byte slice.
    ///
    /// # Panics
    ///
    /// Panics if the buffer length is less than 3 bytes.
    pub fn write_to_slice(&self, buf: &mut [u8]) {
        assert!(
            buf.len() >= 3,
            "Buffer must be at least 3 bytes long, got {} bytes.",
            buf.len()
        );

        // Little-endian: Least Significant Byte first
        buf[0] = (self.index & 0xFF) as u8; // Lower byte of index
        buf[1] = ((self.index >> 8) & 0xFF) as u8; // Higher byte of index
        buf[2] = self.sub; // Sub-index
    }

    /// Reads the Index from a byte slice.
    ///
    /// # Panics
    ///
    /// Panics if the buffer length is less than 3 bytes.
    pub fn read_from_slice(buf: &[u8]) -> Self {
        assert!(
            buf.len() >= 3,
            "Buffer must be at least 3 bytes long, got {} bytes.",
            buf.len()
        );

        // Little-endian: Least Significant Byte first
        let index = ((buf[1] as u16) << 8) | (buf[0] as u16);
        let sub = buf[2];

        Index { index, sub }
    }
}

pub trait Dictionary {
    type Index: Sized;
    type Object: Sized;

    fn set(self: &mut Self, x: Self::Object);
    fn get(self: &Self, ix: &Self::Index) -> Self::Object;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_write_to_slice() {
        let index = Index {
            index: 0x1234,
            sub: 0x56,
        };
        let mut buf = [0u8; 3];
        index.write_to_slice(&mut buf);
        assert_eq!(buf, [0x34, 0x12, 0x56]);
    }

    #[test]
    fn test_index_read_from_slice() {
        let buf = [0x34, 0x12, 0x56];
        let index = Index::read_from_slice(&buf);
        assert_eq!(
            index,
            Index {
                index: 0x1234,
                sub: 0x56
            }
        );
    }

    #[test]
    fn test_index_write_read_inverse() {
        let test_index_cases = [
            Index {
                index: 0x0000,
                sub: 0x00,
            },
            Index {
                index: 0xFFFF,
                sub: 0xFF,
            },
            Index {
                index: 0x1234,
                sub: 0x56,
            },
            Index {
                index: 0xABCD,
                sub: 0xEF,
            },
        ];

        for &original in &test_index_cases {
            let mut buf = [0u8; 3];
            original.write_to_slice(&mut buf);
            let read_back = Index::read_from_slice(&buf);
            assert_eq!(
                original, read_back,
                "Original: {:?}, Read Back: {:?}",
                original, read_back
            );
        }
    }
}
