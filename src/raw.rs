//! # Raw Module
//!
//! The `raw` module provides an abstract interface for working with raw CAN frames.

use crate::machine::*;

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
    
    /// The length of the CAN frame
    pub can_len: usize,
    
    /// The data of the CAN frame.
    ///
    /// This is an array of 8 bytes containing the payload of the frame.
    pub can_data: [u8; 8],
}

impl Default for CANFrame {
    fn default() -> Self {
        Self {
            can_cobid: 0,
            can_len: 0,
            can_data: [0; 8]
        }
    }
}

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
    Final
}

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

    fn initial(self: &mut Self) {
        self.can_frame.can_cobid = 0;
        self.can_frame.can_data.fill(0);
        self.can_frame.can_len = 0;
        self.len = 0;
        self.index = 0;
        self.state = State::Init;   
    }

    fn transit(self: &mut Self, x: u8) {
        
        match &self.state {
            State::Init => {
                self.state = State::Id0;
                self.can_frame.can_cobid = x.into();
            }

            State::Id0 => {
                self.state = State::Id1;
                self.can_frame.can_cobid = self.can_frame.can_cobid | ((x as u32) <<  8);
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
    
    fn observe(self: &Self) -> Self::Observation {
        match self.state {
            State::Final => {
                // should consume all input
                if self.index == 8 {
                    Some(self.can_frame)
                } else {
                    None
                }
            },
            _ => None
        }
    }
}

impl Final for Option<CANFrame> {
    type FinalValue = CANFrame;
    fn is_final(self: Self) -> Option<Self::FinalValue> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_can_frame_parsing() {

        let frame = [0x02, 0x07, 0x00, 0x00, // cobid
                     0x01, 0x00, 0x00, 0x00, // length with padding
                     0x7f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00 // data
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

}
