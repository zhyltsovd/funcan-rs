//! # Raw Module
//!
//! The `raw` module provides an abstract interface for working with raw CAN frames.

use crate::machine::*;

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

/// Represents the possible states within a CAN frame processing sequence.
enum State {
    Init,
    Id0,
    Id1,
    Id2,
    Id3,
    Len,
    Skip0,
    Skip1,
    Skip2,
    Data,
    Final,
}

/// A state machine designed to process and construct raw CAN frames.
pub struct CANFrameMachine {
    state: State,
    can_frame: CANFrame,
    len: usize,
    index: usize,
}

impl Default for CANFrameMachine {
    fn default() -> Self {
        Self {
            state: State::Init,
            can_frame: CANFrame::default(),
            len: 0,
            index: 0,
        }
    }
}

impl CANFrameMachine {
    /// Processes an incoming data byte, storing it in the CAN frame's data array.
    ///
    /// This method updates the state and manages the index where the byte is stored.
    /// Depending on the remaining length, it sets the next state appropriately.
    fn get_data_byte(self: &mut Self, x: u8) {
        if self.len > 1 {
            self.len = self.len - 1;
            self.state = State::Data;
            self.can_frame.can_data[self.index] = x;
        } else if self.len == 1 {
            self.len = self.len - 1;
            self.state = State::Final;
            self.can_frame.can_data[self.index] = x;
        } else {
            self.state = State::Final;
        }

        self.index = self.index + 1;
    }
}

impl MachineTrans<u8> for CANFrameMachine {
    type Observation = Option<CANFrame>;

    /// Resets the machine's state and the CAN frame data to their initial conditions.
    fn initial(self: &mut Self) {
        self.can_frame.can_cobid = 0;
        self.can_frame.can_data.fill(0);
        self.can_frame.can_len = 0;
        self.len = 0;
        self.index = 0;
        self.state = State::Init;
    }

    /// Consumes an input byte and transitions the state machine according to the current state.
    ///
    /// Processes the input byte `x` and transitions the state machine to the next state
    /// as part of building a CAN frame.
    fn transit(self: &mut Self, x: u8) {
        match &self.state {
            State::Init => {
                self.state = State::Id0;
                self.can_frame.can_cobid = x.into();
            }

            State::Id0 => {
                self.state = State::Id1;
                self.can_frame.can_cobid = self.can_frame.can_cobid | ((x as u32) << 8);
            }

            State::Id1 => {
                self.state = State::Id2;
                self.can_frame.can_cobid = self.can_frame.can_cobid | ((x as u32) << 16);
            }

            State::Id2 => {
                self.state = State::Id3;
                self.can_frame.can_cobid = self.can_frame.can_cobid | ((x as u32) << 24);
            }

            State::Id3 => {
                self.state = State::Len;
                let len: usize = x.into();
                self.len = len;
                self.can_frame.can_len = len;
            }

            State::Len => {
                self.state = State::Skip0;
            }

            State::Skip0 => {
                self.state = State::Skip1;
            }

            State::Skip1 => {
                self.state = State::Skip2;
            }

            State::Skip2 => {
                self.get_data_byte(x);
            }

            State::Data => {
                self.get_data_byte(x);
            }

            State::Final => {
                self.index = self.index + 1;
            }
        }
    }

    /// Observes the current machine state to check for a completed CAN frame.
    ///
    /// Returns `Some(CANFrame)` if in a final state with a valid frame, otherwise `None`.
    fn observe(self: &mut Self) -> Self::Observation {
        match self.state {
            State::Final => {
                // should consume all input
                if self.index == 8 {
                    Some(self.can_frame)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl Final for Option<CANFrame> {
    type FinalValue = CANFrame;

    /// Determines if an `Option<CANFrame>` contains a final frame.
    ///
    /// # Returns
    ///
    /// - `Some(CANFrame)` if the option contains a valid frame.
    /// - `None` if the option is empty.
    fn is_final(self: Self) -> Option<Self::FinalValue> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_can_frame_parsing() {
        let frame = [
            0x02, 0x07, 0x00, 0x00, // cobid
            0x01, 0x00, 0x00, 0x00, // length with padding
            0x7f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // data
        ];

        let mut parser = CANFrameMachine::default();

        for x in frame {
            parser.transit(x);
        }

        let result = parser.observe().is_final().unwrap();

        assert_eq!(result.can_cobid, 0x702);
        assert_eq!(result.can_len, 1);
        assert_eq!(result.can_data[0], 0x7f);
    }

    #[test]
    fn test_raw_can_frame_decode_encode() {
        let frame0: [u8; 16] = [
            0x02, 0x07, 0x00, 0x00, // cobid
            0x08, 0x00, 0x00, 0x00, // length with padding
            0x7f, 0x7e, 0x7d, 0x7c, 0x00, 0x01, 0x02, 0x03, // data
        ];

        let mut frame1: [u8; 16] = [0; 16];

        let mut parser = CANFrameMachine::default();

        for x in frame0 {
            parser.transit(x);
        }

        let can_frame = parser.observe().is_final().unwrap();

        can_frame.write_to_slice(&mut frame1);

        assert_eq!(frame0, frame1);
    }

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
