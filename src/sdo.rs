
use core::ops::Not;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SDOError {
    UnknownTransferType(u8)
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SDOTransferType {
    NormalWithNoData,
    Normal,
    ExpeditedWithSize(u8),
    ExpeditedNoSize
}

impl Into<u8> for SDOTransferType {
    fn into(self: Self) -> u8 {
        match self {
            SDOTransferType::NormalWithNoData => 0x00,
            SDOTransferType::Normal => 0x01,
            SDOTransferType::ExpeditedWithSize(_) => 0x02,
            SDOTransferType::ExpeditedNoSize => 0x03,
        }
    }
}

impl From<u8> for SDOTransferType {
    fn from(x: u8) -> Self {
        let code = x & 0x03;
        
        match code {
            0x00 =>  SDOTransferType::NormalWithNoData,
            0x01 =>  SDOTransferType::Normal,
            0x02 =>  {
                let n = (x & 0x0c) >> 2;
                SDOTransferType::ExpeditedWithSize(n)
            },
            0x03 =>  SDOTransferType::ExpeditedNoSize,
            _   => unreachable!()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SDOToggleBit(bool);

impl Not for SDOToggleBit {
    type Output = Self;

    fn not(self) -> Self::Output {
        SDOToggleBit(!self.0)
    }
}

impl Into<u8> for SDOToggleBit {
    fn into(self: Self) -> u8 {
        (self.0 as u8) << 7
    }
}
    
impl From<u8> for SDOToggleBit {
    fn from(x: u8) -> Self {
        SDOToggleBit((x & 0x10) > 0x00)
    }
}

