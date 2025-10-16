use core::fmt::*;

use heapless::vec::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanType {
    Base(usize),
    Field(usize, u8),
    Array(Vec<u8, 254>),
    Struct(Vec<u8, 254>),
}

impl CanType {
    pub fn is_compound(self: &Self) -> Vec<u8, 254> {
        match self {
            CanType::Base(_) => Vec::new(),
            CanType::Field(_, _) => Vec::new(),
            CanType::Array(ns) => ns.clone(),
            CanType::Struct(ns) => ns.clone(),
        }
    }

    pub fn size(self: &Self) -> usize {
        match self {
            CanType::Base(s) => *s,
            CanType::Field(s, _) => *s,
            CanType::Array(ns) => ns.iter().fold(0, |s, x| { s + *x as usize } ),
            CanType::Struct(ns) => ns.iter().fold(0, |s, x| { s + *x as usize } ),
        }
    }
    
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanDesc {
    pub base_index: u16,
    pub can_type: CanType,
}

impl Default for CanDesc {
    fn default() -> Self {
        Self { base_index: 0, can_type: CanType::Base(0) }
    }
}


impl CanDesc {
    pub fn initial_index(self: &Self) -> CanIndex {
        match &self.can_type {
            CanType::Base(_) => self.base_index.into(),
            CanType::Field(_, sub) => CanIndex { base: self.base_index, sub: *sub }, 
            CanType::Array(_) => self.base_index.into(),
            CanType::Struct(_) => self.base_index.into(),
        }
    }

    pub fn is_field(self: &Self) -> Option<u8> {
        match &self.can_type {
            CanType::Field(_, sub) => Some(*sub), 
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CanIndex {
    pub base: u16,
    pub sub: u8,
}

impl Into<CanIndex> for u16 {
    fn into(self: Self) -> CanIndex {
        CanIndex { base: self, sub: 0 }
    } 
}

impl Debug for CanIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "[{:x}:{:x}]", self.base, self.sub)
    }
}

impl CanIndex {
    pub fn inc_sub(self: &mut Self) {
        self.sub = self.sub + 1;
    }
}

impl CanIndex {
    pub fn new(base: u16, sub: u8) -> Self {
        Self { base, sub }
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
        buf[0] = (self.base & 0xFF) as u8; // Lower byte of index
        buf[1] = ((self.base >> 8) & 0xFF) as u8; // Higher byte of index
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
        let base = ((buf[1] as u16) << 8) | (buf[0] as u16);
        let sub = buf[2];

        Self { base, sub }
    }
}

//---------------------------------------------------------------------------------------------------

pub trait Dictionary {
    type Index: Sized;
    type Object: Sized;

    fn set(self: &mut Self, x: Self::Object);
    fn get(self: &Self, ix: &Self::Index) -> Self::Object;
}

pub trait DictionaryValue<D: Dictionary>: TryFrom<D::Object> {
    fn index() -> D::Index;
}

//---------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_write_to_slice() {
        let index = CanIndex {
            base: 0x1234,
            sub: 0x56,
        };
        let mut buf = [0u8; 3];
        index.write_to_slice(&mut buf);
        assert_eq!(buf, [0x34, 0x12, 0x56]);
    }

    #[test]
    fn test_index_read_from_slice() {
        let buf = [0x34, 0x12, 0x56];
        let index = CanIndex::read_from_slice(&buf);
        assert_eq!(
            index,
            CanIndex {
                base: 0x1234,
                sub: 0x56
            }
        );
    }

    #[test]
    fn test_index_write_read_inverse() {
        let test_index_cases = [
            CanIndex {
                base: 0x0000,
                sub: 0x00,
            },
            CanIndex {
                base: 0xFFFF,
                sub: 0xFF,
            },
            CanIndex {
                base: 0x1234,
                sub: 0x56,
            },
            CanIndex {
                base: 0xABCD,
                sub: 0xEF,
            },
        ];

        for &original in &test_index_cases {
            let mut buf = [0u8; 3];
            original.write_to_slice(&mut buf);
            let read_back = CanIndex::read_from_slice(&buf);
            assert_eq!(
                original, read_back,
                "Original: {:?}, Read Back: {:?}",
                original, read_back
            );
        }
    }
}
