pub mod machines; 

use core::ops::Not;

use crate::dictionary::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    UnsupportedTransferType(TransferType),
    UnknownClientCommandSpecifier(u8),
    UnknownServerCommandSpecifier(u8)
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
            0x00 => TransferType::NormalWithNoData,
            0x01 => TransferType::Normal,
            0x02 =>  {
                let n = (x & 0x0c) >> 2;
                TransferType::ExpeditedWithSize(n)
            },
            0x03 => TransferType::ExpeditedNoSize,
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
        (self.0 as u8) << 4
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
        match self {
            ClientCommandSpecifier::InitiateDownload => 1 << 5,
            ClientCommandSpecifier::DownloadSegment => 0 << 5,
            ClientCommandSpecifier::InitiateUpload => 2 << 5,
            ClientCommandSpecifier::UploadSegment => 3 << 5,
            ClientCommandSpecifier::AbortTransfer => 4 << 5,
        }
    }
} 

impl TryFrom<u8> for ClientCommandSpecifier {
    type Error = Error;
    fn try_from(x: u8) -> Result<Self, Self::Error> {
        let cs = x & 0xe0;
        match cs {
            0x00 => Ok(ClientCommandSpecifier::DownloadSegment),
            0x20 => Ok(ClientCommandSpecifier::InitiateDownload),
            0x40 => Ok(ClientCommandSpecifier::InitiateUpload),
            0x50 => Ok(ClientCommandSpecifier::UploadSegment),
            0x80 => Ok(ClientCommandSpecifier::AbortTransfer),
            code => Err(Error::UnknownClientCommandSpecifier(code >> 5))
        }
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
        match self {
            ServerCommandSpecifier::InitiateDownload => 3 << 5,
            ServerCommandSpecifier::DownloadSegment => 1 << 5,
            ServerCommandSpecifier::InitiateUpload => 2 << 5,
            ServerCommandSpecifier::UploadSegment => 0 << 5,
        }
    }
} 

impl TryFrom<u8> for ServerCommandSpecifier {
    type Error = Error;
    fn try_from(x: u8) -> Result<Self, Self::Error> {
        let cs = x & 0xe0;
        match cs {
            0x00 => Ok(ServerCommandSpecifier::UploadSegment),
            0x20 => Ok(ServerCommandSpecifier::DownloadSegment),
            0x40 => Ok(ServerCommandSpecifier::InitiateUpload),
            0x50 => Ok(ServerCommandSpecifier::InitiateDownload),
            code => Err(Error::UnknownServerCommandSpecifier(code >> 5))
        }
    }
}

pub enum ClientRequest {
    InitiateUpload(Index),
    UploadSegment
}

impl Into<[u8; 8]> for ClientRequest {
    fn into(self: Self) -> [u8; 8] {
        let mut req = [0; 8];

        match self {
            ClientRequest::InitiateUpload(ix) => {            
                req[0] = ClientCommandSpecifier::InitiateUpload.into();
                ix.write_to_slice(&mut req[1 .. 4]);
            },
            ClientRequest::UploadSegment => {
                req[0] = ClientCommandSpecifier::UploadSegment.into();
            },            
        };

        req
    }
}

impl TryFrom<[u8; 8]> for ClientRequest {
    type Error = Error;

    fn try_from(req: [u8; 8]) -> Result<Self, Self::Error> {
        let code = ClientCommandSpecifier::try_from(req[0])?;

        match code {
            ClientCommandSpecifier::InitiateUpload => {
                let ix = Index::read_from_slice(&req[1 .. 4]);
                Ok(ClientRequest::InitiateUpload(ix))
            },
            
            ClientCommandSpecifier::UploadSegment => {
                Ok(ClientRequest::UploadSegment)
            },

            _ => todo!()
        }
    }
}

// ---

pub enum ServerResponse {
    UploadSingleSegment(Index, [u8; 4], u8),
    InitMupltipleSegments(Index, u32),
    UploadMupltipleSegments(ToggleBit, [u8; 7], u8, bool),
}

impl Into<[u8; 8]> for ServerResponse {
    fn into(self: Self) -> [u8; 8] {
        let mut req = [0; 8];

        match self {
            ServerResponse::UploadSingleSegment(ix, data, len) => {            
                let cs: u8 = ServerCommandSpecifier::InitiateUpload.into();
                let ty: u8 = TransferType::ExpeditedWithSize(0x3 & len).into();
                
                let code = cs | ty | ((0x3 & len) << 2);
                req[0] = code;
                ix.write_to_slice(&mut req[1 .. 4]);
                req[4 .. 8].copy_from_slice(&data);
            },
            
            ServerResponse::InitMupltipleSegments(ix, len) => {
                let cs: u8 = ServerCommandSpecifier::InitiateUpload.into();
                let ty: u8 = TransferType::Normal.into();
                
                let code = cs | ty;
                req[0] = code;
                ix.write_to_slice(&mut req[1 .. 4]);
                req[4 .. 8].copy_from_slice(&len.to_le_bytes());
            }
            
            ServerResponse::UploadMupltipleSegments(tb, data, len, is_end) => {
                let cs: u8 = ServerCommandSpecifier::UploadSegment.into();
                let t: u8 = tb.into(); 
                let code = cs | ((0x3 & len) << 1) | (is_end as u8) | t;
                req[0] = code;
                req[1 .. 8].copy_from_slice(&data);
            }
        };

        req
    }

}

impl TryFrom<[u8; 8]> for ServerResponse {
    type Error = Error;

    fn try_from(req: [u8; 8]) -> Result<Self, Self::Error> {
        let code = ServerCommandSpecifier::try_from(req[0])?;

        match code {
            ServerCommandSpecifier::InitiateUpload => {
                let ix = Index::read_from_slice(&req[1 .. 4]);
                let ty = TransferType::from(req[0]);
                
                match ty {
                    TransferType::Normal => {
                        let n = u32::from_le_bytes(req[4 .. 8].try_into().unwrap());
                        Ok(ServerResponse::InitMupltipleSegments(ix, n))
                    },
                    
                    TransferType::ExpeditedWithSize(n) => {
                        let mut data = [0; 4];
                        data.copy_from_slice(&req[4 .. (8 - n as usize)]);
                        
                        Ok(ServerResponse::UploadSingleSegment(ix, data, n))
                    },
                    
                    _ => Err(Error::UnsupportedTransferType(ty))
                }
                
            },
            
            ServerCommandSpecifier::UploadSegment => {
                todo!()
            },

            _ => todo!()
        }
    }
}
