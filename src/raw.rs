//! # Raw Module
//!
//! The `raw` module provides an abstract interface for working with raw CAN frames.

use core::fmt;


/// Enum for every CANopen‐defined COB-ID
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CobId {
    /// NMT management service (always uses COB-ID 0x000).  Data[0] = cmd, Data[1] = node.
    NmtService,

    /// Synchronization object (COB-ID = 0x080).  Data may carry SYNC counter (optional).
    Sync,

    /// Time stamp object (COB-ID = 0x100).
    TimeStamp,

    /// Emergency message (COB-ID = 0x080 + nodeid)
    Emergency(u8),

    /// PDO Tx: 1..4
    PdoTx {
        pdo_number: u8, /*1..4*/
        node: u8,
    },

    /// PDO Rx: 1..4
    PdoRx {
        pdo_number: u8, /*1..4*/
        node: u8,
    },

    /// SDO Response  (COB-ID = 0x580 + node)
    SdoResponse(u8),

    /// SDO Request   (COB-ID = 0x600 + node)
    SdoRequest(u8),

    /// Heartbeat or Node-Guard (COB-ID = 0x700 + node)
    Heartbeat(u8),

    /// Everything else: manufacturer-specific or reserved
    ManufacturerSpecific(u16),
}

pub const NODE_MASK: u32 = 0x7F; // lower 7 bits
const FUNC_MASK: u32 = 0x780; // next 4 bits << 7

impl From<CobId> for u32 {
    fn from(c: CobId) -> u32 {
        match c {
            CobId::NmtService => 0x000,
            CobId::Sync => 0x080,
            CobId::TimeStamp => 0x100,
            CobId::Emergency(n) => 0x080 | (n as u32),
            CobId::PdoTx { pdo_number, node } => {
                let base = 0x100 * pdo_number as u32 + 0x080;
                base | (node as u32)
            }
            CobId::PdoRx { pdo_number, node } => {
                let base = 0x100 * pdo_number as u32 + 0x100;
                base | (node as u32)
            }
            CobId::SdoResponse(n) => 0x580 | (n as u32),
            CobId::SdoRequest(n) => 0x600 | (n as u32),
            CobId::Heartbeat(n) => 0x700 | (n as u32),
            CobId::ManufacturerSpecific(id) => id as u32,
        }
    }
}

impl From<u32> for CobId {
    fn from(raw: u32) -> CobId {
        let func = raw & FUNC_MASK;
        let node = (raw & NODE_MASK) as u8;
        match (func, node) {
            // NMT service is always COB-ID = 0x000
            (0x000, _) => {
                // data[0] and data[1] must be examined by caller
                // to figure out the actual NmtCommand and target node,
                // so we just return a placeholder here.
                // Application code can then decode actual bytes.
                CobId::NmtService
            }

            // Sync object
            (0x080, 0x00) => CobId::Sync,

            // timestamp
            (0x100, 0x00) => CobId::TimeStamp,

            // Emergency
            (0x080, n) if n != 0 => CobId::Emergency(n),

            // PDO Tx [1..4]
            (fp, n) if (0x180..=0x480).contains(&fp) && (fp - 0x080) % 0x100 == 0 => {
                let pdo = ((fp - 0x080) / 0x100) as u8;
                CobId::PdoTx {
                    pdo_number: pdo,
                    node: n,
                }
            }
            // PDO Rx [1..4]
            (fp, n) if (0x200..=0x500).contains(&fp) && (fp - 0x100) % 0x100 == 0 => {
                let pdo = ((fp - 0x100) / 0x100) as u8;
                CobId::PdoRx {
                    pdo_number: pdo,
                    node: n,
                }
            }

            // SDO Resp
            (0x580, n) => CobId::SdoResponse(n),
            // SDO Req
            (0x600, n) => CobId::SdoRequest(n),
            // Heartbeat / Guard
            (0x700, n) => CobId::Heartbeat(n),

            // Anything else
            _ => CobId::ManufacturerSpecific(raw as u16),
        }
    }
}

/// A structure representing RAW CAN frames.
///
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CanFrame {
    /// The CAN identifier (COB-ID) of the frame.
    ///
    /// CAN identifier of the frame in the CAN network.
    pub can_cobid: CobId,

    /// The length of the CAN frame
    pub can_len: usize,

    /// The data of the CAN frame.
    ///
    /// This is an array of 8 bytes containing the payload of the frame.
    pub can_data: [u8; 8],
}

impl fmt::Debug for CanFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cobid: u32 = self.can_cobid.into();
        write!(f, "{:#X}: [", cobid)?;
        for i in 0..self.can_len {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{:02X}", self.can_data[i])?;
        }

        write!(f, "]")
    }
}

impl Default for CanFrame {
    fn default() -> Self {
        Self {
            can_cobid: CobId::ManufacturerSpecific(0xffff),
            can_len: 0,
            can_data: [0; 8],
        }
    }
}

impl CanFrame {
    /// Serializes the CAN frame into a byte slice.
    ///
    /// # Panics
    ///
    /// Panics if the provided buffer is less than 16 bytes long.
    pub fn write_to_slice(self: &Self, buffer: &mut [u8]) {
        assert!(buffer.len() >= 16, "Buffer must be at least 16 bytes long");

        // Write COB-ID as little endian
        let cobid: u32 = self.can_cobid.into();
        buffer[0..4].copy_from_slice(&cobid.to_le_bytes());

        // Write length
        buffer[4] = self.can_len as u8;

        // Fill 3 bytes with zero (padding)
        buffer[5..8].fill(0);

        // Write CAN data
        buffer[8..16].copy_from_slice(&self.can_data);
    }

    /// Deserializes a `CanFrame` from a byte slice.
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
        let can_cobid = u32::from_le_bytes(buffer[0..4].try_into().unwrap()).into();

        // Read length
        let can_len = buffer[4] as usize;

        // Read CAN data
        let can_data = buffer[8..16].try_into().unwrap();

        CanFrame {
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
        let frame = CanFrame {
            can_cobid: CobId::SdoRequest(2),
            can_len: 8,
            can_data: [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11],
        };

        let mut buffer = [0u8; 16];
        frame.write_to_slice(&mut buffer);

        let deserialized_frame = CanFrame::read_from_slice(&buffer);

        assert_eq!(frame, deserialized_frame);
    }
}
