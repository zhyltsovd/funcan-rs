use heapless::vec::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanBaseIndex(u16);
    
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanType {
    Base,
    Array(Vec<u8, 254>),
    Struct(Vec<u8, 254>),
}

impl CanType {
    pub fn is_compound(self: &Self) -> Vec<u8, 254> {
        match self {
            CanType::Base => Vec::new(),
            CanType::Array(n) => n.clone(),
            CanType::Struct(n) => n.clone(),
        }
    }
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanIndices {
    pub base_index: CanBaseIndex,
    pub can_type: CanType,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanIndex {
    pub base: u16,
    pub sub: u8,
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
