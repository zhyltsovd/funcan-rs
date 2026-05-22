//! # Raw Module
//!
//! The `raw` module provides an abstract interface for working with raw CAN frames.

use core::fmt;

/// Enum for every CANopen‐defined COB-ID
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CobId {
    /// NMT management service (always uses COB-ID 0x000).  Data[0] = cmd, Data[1] = node.
    NmtService(u8, u8),

    /// Synchronization object (COB-ID = 0x080).  Data may carry SYNC counter (optional).
    Sync,

    /// Time stamp object (COB-ID = 0x100).
    TimeStamp,

    /// Emergency message (COB-ID = 0x080 + nodeid)
    Emergency(u8),

    /// PDO Tx: 1..4
    PdoTx {
        pdo_id: u8, /*1..4*/
        node_id: u8,
    },

    /// PDO Rx: 1..4
    PdoRx {
        pdo_id: u8, /*1..4*/
        node_id: u8,
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
            CobId::NmtService(d0, d1) => 0x000, // ((d0 as u32) << 8) | d1 as u32,
            CobId::Sync => 0x080,
            CobId::TimeStamp => 0x100,
            CobId::Emergency(n) => 0x080 | (n as u32),
            CobId::PdoTx { pdo_id, node_id } => {
                let base = 0x100 * pdo_id as u32 + 0x080;
                base | (node_id as u32)
            }
            CobId::PdoRx { pdo_id, node_id } => {
                let base = 0x100 * pdo_id as u32 + 0x100;
                base | (node_id as u32)
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
            (0x000, node) => {
                // data[0] and data[1] must be examined by caller
                // to figure out the actual NmtCommand and target node,
                // so we just return a placeholder here.
                // Application code can then decode actual bytes.
                CobId::NmtService((raw >> 8) as u8, node)
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
                    pdo_id: pdo,
                    node_id: n,
                }
            }
            // PDO Rx [1..4]
            (fp, n) if (0x200..=0x500).contains(&fp) && (fp - 0x100) % 0x100 == 0 => {
                let pdo = ((fp - 0x100) / 0x100) as u8;
                CobId::PdoRx {
                    pdo_id: pdo,
                    node_id: n,
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
    pub cobid: CobId,

    /// The length of the CAN frame
    pub len: usize,

    /// The data of the CAN frame.
    ///
    /// This is an array of 8 bytes containing the payload of the frame.
    pub data: [u8; 8],
}

impl fmt::Debug for CanFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: [", self.cobid)?;
        for i in 0..self.len {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{:02X}", self.data[i])?;
        }

        write!(f, "]")
    }
}

impl Default for CanFrame {
    fn default() -> Self {
        Self {
            cobid: CobId::ManufacturerSpecific(0xffff),
            len: 0,
            data: [0; 8],
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
        assert!(buffer.len() >= 13, "Buffer must be at least 13 bytes long");

        // Write COB-ID as little endian
        let cobid: u32 = self.cobid.into();
        buffer[1..5].copy_from_slice(&cobid.to_be_bytes());

        // Write length
        buffer[0] = self.len as u8;

        // Write CAN data
        buffer[5..13].copy_from_slice(&self.data);
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
        assert!(buffer.len() >= 13, "Buffer must be at least 13 bytes long");

        // Read COB-ID from little endian bytes
        let cobid = u32::from_be_bytes(buffer[1..5].try_into().unwrap()).into();

        // Read length
        let len = buffer[0] as usize;

        // Read CAN data
        let data = buffer[5..13].try_into().unwrap();

        CanFrame { cobid, len, data }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialization_deserialization() {
        let frame = CanFrame {
            cobid: CobId::SdoRequest(2),
            len: 8,
            data: [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11],
        };

        let mut buffer = [0u8; 13];
        frame.write_to_slice(&mut buffer);

        let deserialized_frame = CanFrame::read_from_slice(&buffer);

        assert_eq!(frame, deserialized_frame);
    }
}
