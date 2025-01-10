//! # Raw Module
//!
//! The `raw` module provides an abstract interface for working with raw CAN frames.


/// A structure representing RAW CAN frames.
///
/// # Fields
///
/// * `can_cobid` - The CAN identifier (COB-ID) of the frame. This is a 32-bit value that uniquely identifies the frame in the CAN network.
/// * `can_data` - The data of the CAN frame. This is an array of 8 bytes containing the payload of the frame.
///
#[derive(Debug, Clone, Copy)]
pub struct CANFrame {
    /// The CAN identifier (COB-ID) of the frame.
    ///
    /// This is a 32-bit value that uniquely identifies the frame in the CAN network.
    pub can_cobid: u32,

    /// The data of the CAN frame.
    ///
    /// This is an array of 8 bytes containing the payload of the frame.
    pub can_data: [u8; 8],
}
