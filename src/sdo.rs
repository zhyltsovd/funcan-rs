
use core::ops::Not;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    UnknownTransferType(u8)
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferType {
    NormalWithNoData,
    Normal,
    ExpeditedWithSize(u8),
    ExpeditedNoSize
}

impl Into<u8> for TransferType {
    fn into(self: Self) -> u8 {
        match self {
            TransferType::NormalWithNoData => 0x00,
            TransferType::Normal => 0x01,
            TransferType::ExpeditedWithSize(_) => 0x02,
            TransferType::ExpeditedNoSize => 0x03,
        }
    }
}

impl From<u8> for TransferType {
    fn from(x: u8) -> Self {
        let code = x & 0x03;
        
        match code {
            0x00 =>  TransferType::NormalWithNoData,
            0x01 =>  TransferType::Normal,
            0x02 =>  {
                let n = (x & 0x0c) >> 2;
                TransferType::ExpeditedWithSize(n)
            },
            0x03 =>  TransferType::ExpeditedNoSize,
            _   => unreachable!()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToggleBit(bool);

impl Not for ToggleBit {
    type Output = Self;

    fn not(self) -> Self::Output {
        ToggleBit(!self.0)
    }
}

impl Into<u8> for ToggleBit {
    fn into(self: Self) -> u8 {
        (self.0 as u8) << 7
    }
}
    
impl From<u8> for ToggleBit {
    fn from(x: u8) -> Self {
        ToggleBit((x & 0x10) > 0x00)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientCommandSpecifier {
    InitiateDownload,
    DownloadSegment,
    InitiateUpload,
    UploadSegment,
    AbortTransfer,
}

impl Into<u8> for ClientCommandSpecifier {
    fn into(self: Self) -> u8 {
        ClientCommandSpecifier::InitiateDownload => (1 << 6),
        ClientCommandSpecifier::DownloadSegment => (0 << 6),
        ClientCommandSpecifier::InitiateUpload => (2 << 6),
        ClientCommandSpecifier::UploadSegment => (3 << 6),
        ClientCommandSpecifier::AbortTransfer => (4 << 6),
    }
} 

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerCommandSpecifier {
    InitiateDownload,
    DownloadSegment,
    InitiateUpload,
    UploadSegment
}

impl Into<u8> for ServerCommandSpecifier {
    fn into(self: Self) -> u8 {
        ClientCommandSpecifier::InitiateDownload => (3 << 6),
        ClientCommandSpecifier::DownloadSegment => (1 << 6),
        ServerCommandSpecifier::InitiateUpload => (2 << 6),
        ServerCommandSpecifier::UploadSegment => (0 << 6),
    }
} 
