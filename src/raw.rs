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

    /// The data of the CAN frame.
    ///
    /// This is an array of 8 bytes containing the payload of the frame.
    pub can_data: [u8; 8],
}

impl Default for CANFrame {
    fn default() -> Self {
        Self {
            can_cobid: 0,
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

impl CANFrameMachine {
    fn get_data_byte(self: &mut Self, x: u8) {                     
        if self.len > 1 {
            self.len = self.len - 1;
            self.state = State::Data;
            self.can_frame.can_data[self.index] = x;
            self.index = self.index + 1;
        } else if self.len == 1 {
            self.len = self.len - 1;
            self.state = State::Final;
            self.can_frame.can_data[self.index] = x;
        } else {
            self.state = State::Final;
        }
    }
}

impl MachineTrans<u8> for CANFrameMachine {
    type Observation = Option<CANFrame>;

    fn initial(self: &mut Self) {
        self.can_frame.can_cobid = 0;
        self.can_frame.can_data.fill(0);
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
                self.len = x.into();
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
                // do nothing
            }          
        }
    }
    
    fn observe(self: &Self) -> Self::Observation {
        match self.state {
            State::Final => Some(self.can_frame),
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
    fn parse() {
    }

}
