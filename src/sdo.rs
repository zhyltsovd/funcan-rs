
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

impl TryFrom<(u8, u8)> for SDOTransferType {
    type Error = SDOError;
    
    fn try_from((n, code): (u8, u8)) -> Result<Self, Self::Error> {
        match code {
            0x00 => Ok(SDOTransferType::NormalWithNoData),
            0x01 => Ok(SDOTransferType::Normal),
            0x02 => Ok(SDOTransferType::ExpeditedWithSize(n)),
            0x03 => Ok(SDOTransferType::ExpeditedNoSize),
            ty   => Err(SDOError::UnknownTransferType(ty))
        }
    }
        
}
