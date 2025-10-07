//! # Raw Module
//!
//! The `raw` module provides an abstract interface for working with raw CAN frames.

use core::fmt;

/// A structure representing RAW CAN frames.
///
/// # Fields
///
/// * `can_cobid` - The CAN identifier (COB-ID) of the frame. This is a 32-bit value that uniquely identifies the frame in the CAN network.
/// * `can_len` - The length of the CAN frame. Number of valid bytes in `can_data`
/// * `can_data` - The data of the CAN frame. This is an array of 8 bytes containing the payload of the frame.
///
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CANFrame {
    /// The CAN identifier (COB-ID) of the frame.
    ///
    /// This is a 32-bit value that uniquely identifies the frame in the CAN network.
    pub can_cobid: u32,

    /// The length of the CAN frame
    pub can_len: usize,

    /// The data of the CAN frame.
    ///
    /// This is an array of 8 bytes containing the payload of the frame.
    pub can_data: [u8; 8],
}

impl fmt::Debug for CANFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#X}: [", self.can_cobid)?;
        for i in 0..self.can_len {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{:02X}", self.can_data[i])?;
        }

        write!(f, "]")
    }
}

impl Default for CANFrame {
    fn default() -> Self {
        Self {
            can_cobid: 0,
            can_len: 0,
            can_data: [0; 8],
        }
    }
}

impl CANFrame {
    /// Serializes the CAN frame into a byte slice.
    ///
    /// # Panics
    ///
    /// Panics if the provided buffer is less than 16 bytes long.
    pub fn write_to_slice(self: &Self, buffer: &mut [u8]) {
        assert!(buffer.len() >= 16, "Buffer must be at least 16 bytes long");

        // Write COB-ID as little endian
        buffer[0..4].copy_from_slice(&self.can_cobid.to_le_bytes());

        // Write length
        buffer[4] = self.can_len as u8;

        // Fill 3 bytes with zero (padding)
        buffer[5..8].fill(0);

        // Write CAN data
        buffer[8..16].copy_from_slice(&self.can_data);
    }

    /// Deserializes a `CANFrame` from a byte slice.
    ///
    /// # Arguments
    ///
    /// * `buffer` - A byte slice containing the serialized CAN frame. Must be at least 16 bytes long.
    ///
    /// # Panics
    ///
    /// Panics if the provided buffer is less than 16 bytes long.
    pub fn read_from_slice(buffer: &[u8]) -> Self {
        assert!(buffer.len() >= 16, "Buffer must be at least 16 bytes long");

        // Read COB-ID from little endian bytes
        let can_cobid = u32::from_le_bytes(buffer[0..4].try_into().unwrap());

        // Read length
        let can_len = buffer[4] as usize;

        // Read CAN data
        let can_data = buffer[8..16].try_into().unwrap();

        CANFrame {
            can_cobid,
            can_len,
            can_data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialization_deserialization() {
        let frame = CANFrame {
            can_cobid: 0x12345678,
            can_len: 8,
            can_data: [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11],
        };

        let mut buffer = [0u8; 16];
        frame.write_to_slice(&mut buffer);

        let deserialized_frame = CANFrame::read_from_slice(&buffer);

        assert_eq!(frame, deserialized_frame);
    }
}
