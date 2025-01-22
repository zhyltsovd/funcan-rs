pub mod machines; 
pub mod abort; 

use core::ops::Not;

use crate::dictionary::*;
use crate::sdo::abort::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    UnsupportedTransferType(u8),
    UnknownClientCommandSpecifier(u8),
    UnknownServerCommandSpecifier(u8)
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferType {
    Normal,
    ExpeditedWithSize(u8),
}

impl Into<u8> for TransferType {
    fn into(self: Self) -> u8 {
        match self {
            TransferType::Normal => 0x01,
            TransferType::ExpeditedWithSize(n) => 0x02 | (n << 2),
        }
    }
}

impl TryFrom<u8> for TransferType {
    type Error = Error;
    fn try_from(x: u8) -> Result<Self, Self::Error> {
        let code = x & 0x03;
        
        match code {
            0x01 => Ok(TransferType::Normal),
            0x02 =>  {
                let n = (x & 0x0c) >> 2;
                Ok(TransferType::ExpeditedWithSize(n))
            },
            t => Err(Error::UnsupportedTransferType(t)),
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

#[derive(Debug, PartialEq, Clone)]
pub enum ClientRequest {
    InitiateUpload(Index),
    UploadSegment,
    InitiateSingleSegmentDownload(Index, u8, [u8; 4]), // index, length, data
    InitiateMultipleSegmentDownload(Index, u32), // index and length,
    DownloadSegment(ToggleBit, bool, u8, [u8; 7]), // toogle bit, end bit, length, data
    AbortTransfer(AbortCode) 
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
            
            ClientRequest::InitiateSingleSegmentDownload(ix, len, data) => {
                req[0] = ClientCommandSpecifier::InitiateDownload.into();
                //len is n field in first byte, represented 4-len value (len must be from 0 to 4, other is mistake)
                let len = if len <= 4 {
                    4 - len
                }else{
                    //TODO: как-то обработать ситуацию, когда len > 4. Возможно ли это?
                    panic!("Requested single segment download length out of range!");
                };

                req[0] |= len << 2;

                //Single segment download meant expedited download, so we must set e bit
                req[0] |= 1 << 1;
                //Besides we specify size, so s bit must be set too
                req[0] |= 1 << 0;

                ix.write_to_slice(&mut req[1 .. 4]);

                req[4..8].copy_from_slice(&data[0..4]); // Set the first 4 bytes of data
                // Note: data[3] is not used due to byte constraints
            },

            ClientRequest::InitiateMultipleSegmentDownload(ix, len) => {
                req[0] = ClientCommandSpecifier::InitiateDownload.into();

                //setup sized bit if we specify len
                if len > 0 {
                    req[0] |= 1 << 0;
                }

                ix.write_to_slice(&mut req[1 .. 4]);
                req[4..8].copy_from_slice(&len.to_le_bytes()); // Set length as u32 in little endian
            },

            ClientRequest::DownloadSegment(t, c, n, data) => {
                let code: u8 = ClientCommandSpecifier::DownloadSegment.into();
                let toogle: u8 = t.into();
                req[0] = code | toogle | ((n & 0x07) << 1) | (c as u8); 
                req[1..8].copy_from_slice(&data[..7]);
            }

            ClientRequest::AbortTransfer(code) => {
                req[0] = ClientCommandSpecifier::AbortTransfer.into();
                req[4..8].copy_from_slice(&(code as u32).to_le_bytes()); // Set abort code as u32 in little endian
            }
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

            ClientCommandSpecifier::InitiateDownload => {
                let ix = Index::read_from_slice(&req[1 .. 4]);
                // Determine if it's a single or multiple segment download based on s and e bits in first byte
                let is_expedited = (req[0] >> 1 ) & 1 > 0;
                let is_sized = (req[0] >> 0 ) & 1 > 0;

                match (is_expedited, is_sized) {
                    (false, false) => {
                        //Unspecified len download request
                        Ok(ClientRequest::InitiateMultipleSegmentDownload(ix, 0))
                    },
                    (false, true) => {
                        let len = u32::from_le_bytes([req[4], req[5], req[6], req[7]]);
                        Ok(ClientRequest::InitiateMultipleSegmentDownload(ix, len))
                    },
                    (true, true) => {
                        //Expedited download request
                        let len= 4 - (( req[0] >> 2 ) & 0x3);
                        let mut data = [0u8;4];
                        data[..4].copy_from_slice(&req[4..8]);
                        Ok(ClientRequest::InitiateSingleSegmentDownload(ix, len, data))
                    },
                    (true, false) => {
                        //Expedited, but len not specified. Not sure is this used in real and have any meaning
                        let mut data = [0u8;4];
                        data[..3].copy_from_slice(&req[4..8]);
                        Ok(ClientRequest::InitiateSingleSegmentDownload(ix, 0, data))
                    }
                }
            },

            ClientCommandSpecifier::DownloadSegment => {
                let toggle_bit = ToggleBit::from(req[0]);
                let end = req[0] & 0x01 > 0;
                let n = (req[0] >> 1) & 0x07; // Extract n assuming it's 3 bits
                let data = [req[1], req[2], req[3], req[4], req[5], req[6], req[7]];
                Ok(ClientRequest::DownloadSegment(toggle_bit, end, n, data))
            }

            ClientCommandSpecifier::AbortTransfer => {
                let code = (u32::from_le_bytes([req[4], req[5], req[6], req[7]])).into();
                Ok(ClientRequest::AbortTransfer(code))
            },
            
        }
    }
}

pub enum ServerResponse {
    UploadSingleSegment( Index, u8, [u8; 4] ),
    InitMupltipleSegments(Index, u32),
    UploadMupltipleSegments(ToggleBit, [u8; 7], u8, bool),
    DownloadSingleSegmentAck(Index),
    DownloadSegmentAck(ToggleBit, u8)
}

impl Into<[u8; 8]> for ServerResponse {
    fn into(self: Self) -> [u8; 8] {
        let mut req = [0; 8];

        match self {
            ServerResponse::UploadSingleSegment(ix, len, data) => {
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

            ServerResponse::DownloadSingleSegmentAck(ix) => {
                let cs: u8 = ServerCommandSpecifier::InitiateDownload.into(); // ?
                
                // Set the command specifier for single segment download acknowledgment
                req[0] = cs;
                ix.write_to_slice(&mut req[1 .. 4]);
                
                // The remaining bytes can be set to zero or used for additional flags if necessary
                req[4..8].copy_from_slice(&[0, 0, 0, 0]);
            },
            
            ServerResponse::DownloadSegmentAck(toggle_bit, len) => {
                let cs: u8 = ServerCommandSpecifier::DownloadSegment.into();
                let t: u8 = toggle_bit.into();
                
                // Command specifier includes the toggle bit
                let code = cs | t;
                req[0] = code;
                
                // The second byte can represent the number of bytes not used in the last segment (or other flags)
                req[1] = len;
                
                // The remaining bytes can be set to zero or used for additional flags if necessary
                req[2..8].copy_from_slice(&[0, 0, 0, 0, 0, 0]);
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
                let ty = TransferType::try_from(req[0])?;
                
                match ty {
                    TransferType::Normal => {
                        let n = u32::from_le_bytes(req[4 .. 8].try_into().unwrap());
                        Ok(ServerResponse::InitMupltipleSegments(ix, n))
                    },
                    
                    TransferType::ExpeditedWithSize(n) => {
                        let mut data = [0; 4];
                        data.copy_from_slice(&req[4 .. (8 - n as usize)]);
                        
                        Ok(ServerResponse::UploadSingleSegment(ix, n, data))
                    },   
                }
            },
            
            ServerCommandSpecifier::UploadSegment => {
                todo!()
            },

            _ => todo!()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdo::ClientRequest;
    use crate::sdo;

    #[test]
    fn client_upload_init() {
        let req = ClientRequest::InitiateUpload(Index::new(0x1000, 0x01));
        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.6 - SDO upload initiate
        let expected_buf: [u8; 8] = [ 0x40, 0x00, 0x10, 0x01, 0x00, 0x00, 0x00, 0x00 ];

        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from( req_buf ).unwrap();

        assert_eq!( req, req_dec );
    }

    #[test]
    fn client_download_single_init() {
        let index = Index::new(0x1000, 0x01);

        let req = ClientRequest::InitiateSingleSegmentDownload( index, 4, [0x01, 0x02, 0x03, 0x04] );

        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.3 - SDO download initiate, e = 1, s = 1
        let expected_buf: [u8; 8] = [ 0x23, 0x00, 0x10, 0x01, 0x01, 0x02, 0x03, 0x04 ];

        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from( req_buf ).unwrap();

        assert_eq!( req, req_dec );
    }

    #[test]
    fn client_download_segment_init() {
        let index = Index::new(0x1000, 0x01);
        let req = ClientRequest::InitiateMultipleSegmentDownload( index, 10 );

        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.3 - SDO download initiate, e = 0, s = 1
        let expected_buf: [u8; 8] = [ 0x21, 0x00, 0x10, 0x01, 0x0A, 0x00, 0x00, 0x00 ];

        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from( req_buf ).unwrap();

        assert_eq!( req, req_dec );
    }

    #[test]
    fn client_download_segment_unspecified_len() {
        let index = Index::new(0x1000, 0x01);
        let req = ClientRequest::InitiateMultipleSegmentDownload( index, 0 );

        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.3 - SDO download initiate, e = 0, s = 1
        let expected_buf: [u8; 8] = [ 0x20, 0x00, 0x10, 0x01, 0x00, 0x00, 0x00, 0x00 ];

        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from( req_buf ).unwrap();

        assert_eq!( req, req_dec );
    }
}
