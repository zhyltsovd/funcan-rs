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
    NormalUnspecifiedSize,
    ExpeditedWithSize(u8),
}

impl Into<u8> for TransferType {
    fn into(self: Self) -> u8 {
        match self {
            TransferType::Normal => 0x01,
            TransferType::ExpeditedWithSize(n)  => 0x03 | ((4-n) << 2),
            TransferType::NormalUnspecifiedSize => 0x00
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
                //e == 1, s == 0 - expedited transfer without size specified
                //let n = 4- ((x & 0x0c) >> 2);
                Ok(TransferType::ExpeditedWithSize(0))
            },
            0x03 => {
                let n = 4 - (( x & 0x0c ) >> 2);
                Ok(TransferType::ExpeditedWithSize(n))
            },
            0x00 => {
                Ok(TransferType::NormalUnspecifiedSize)
            }
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
    InitDownload,
    DownloadSegment,
    InitUpload,
    UploadSegment,
    AbortTransfer,
}

impl Into<u8> for ClientCommandSpecifier {
    fn into(self: Self) -> u8 {
        match self {
            ClientCommandSpecifier::InitDownload => 1 << 5,
            ClientCommandSpecifier::DownloadSegment => 0 << 5,
            ClientCommandSpecifier::InitUpload => 2 << 5,
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
            0x20 => Ok(ClientCommandSpecifier::InitDownload),
            0x40 => Ok(ClientCommandSpecifier::InitUpload),
            0x60 => Ok(ClientCommandSpecifier::UploadSegment),
            0x80 => Ok(ClientCommandSpecifier::AbortTransfer),
            code => Err(Error::UnknownClientCommandSpecifier(code >> 5))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerCommandSpecifier {
    InitDownloadAck,
    DownloadSegmentAck,
    InitUpload,
    UploadSegment
}

impl Into<u8> for ServerCommandSpecifier {
    fn into(self: Self) -> u8 {
        match self {
            ServerCommandSpecifier::InitDownloadAck => 3 << 5,
            ServerCommandSpecifier::DownloadSegmentAck => 1 << 5,
            ServerCommandSpecifier::InitUpload => 2 << 5,
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
            0x20 => Ok(ServerCommandSpecifier::DownloadSegmentAck),
            0x40 => Ok(ServerCommandSpecifier::InitUpload),
            0x60 => Ok(ServerCommandSpecifier::InitDownloadAck),
            code => Err(Error::UnknownServerCommandSpecifier(code >> 5))
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum ClientRequest {
    InitUpload(Index),
    UploadSegment(ToggleBit),
    InitSingleSegmentDownload(Index, u8, [u8; 4]), // index, length, data
    InitMultipleDownload(Index, u32), // index and length,
    DownloadSegment(ToggleBit, bool, u8, [u8; 7]), // toogle bit, end bit, length, data
    AbortTransfer(Index, AbortCode)
}


impl Into<[u8; 8]> for ClientRequest {
    fn into(self: Self) -> [u8; 8] {
        let mut req = [0; 8];

        match self {
            ClientRequest::InitUpload(ix) => {            
                req[0] = ClientCommandSpecifier::InitUpload.into();
                ix.write_to_slice(&mut req[1 .. 4]);
            },
            
            ClientRequest::UploadSegment(t) => {
                let u: u8 = t.into();
                let c: u8 = ClientCommandSpecifier::UploadSegment.into();
                req[0] = c | u;
            },
            
            ClientRequest::InitSingleSegmentDownload(ix, len, data) => {
                req[0] = ClientCommandSpecifier::InitDownload.into();
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

            ClientRequest::InitMultipleDownload(ix, len) => {
                req[0] = ClientCommandSpecifier::InitDownload.into();

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

                let len = if n <= 7 {
                    7 - n
                }else{
                    //TODO: как-то обработать ситуацию, когда len >= 8
                    panic!("Requested segment download length out of range!");
                };

                req[0] = code | toogle | (len << 1) | (c as u8);
                req[1..8].copy_from_slice(&data[..7]);
            }

            ClientRequest::AbortTransfer(index, code) => {
                req[0] = ClientCommandSpecifier::AbortTransfer.into();
                index.write_to_slice(&mut req[1 .. 4]);
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
            ClientCommandSpecifier::InitUpload => {
                let ix = Index::read_from_slice(&req[1 .. 4]);
                Ok(ClientRequest::InitUpload(ix))
            },
            
            ClientCommandSpecifier::UploadSegment => {
                let t = ToggleBit::from(req[0]);
                Ok(ClientRequest::UploadSegment(t))
            },

            ClientCommandSpecifier::InitDownload => {
                let ix = Index::read_from_slice(&req[1 .. 4]);
                // Determine if it's a single or multiple segment download based on s and e bits in first byte
                let is_expedited = (req[0] >> 1 ) & 1 > 0;
                let is_sized = (req[0] >> 0 ) & 1 > 0;

                match (is_expedited, is_sized) {
                    (false, false) => {
                        //Unspecified len download request
                        Ok(ClientRequest::InitMultipleDownload(ix, 0))
                    },
                    (false, true) => {
                        let len = u32::from_le_bytes([req[4], req[5], req[6], req[7]]);
                        Ok(ClientRequest::InitMultipleDownload(ix, len))
                    },
                    (true, true) => {
                        //Expedited download request
                        let len= 4 - (( req[0] >> 2 ) & 0x3);
                        let mut data = [0u8;4];
                        data[..4].copy_from_slice(&req[4..8]);
                        Ok(ClientRequest::InitSingleSegmentDownload(ix, len, data))
                    },
                    (true, false) => {
                        //Expedited, but len not specified. Not sure is this used in real and have any meaning
                        let mut data = [0u8;4];
                        data[..3].copy_from_slice(&req[4..8]);
                        Ok(ClientRequest::InitSingleSegmentDownload(ix, 0, data))
                    }
                }
            },

            ClientCommandSpecifier::DownloadSegment => {
                let toggle_bit = ToggleBit::from(req[0]);
                let end = req[0] & 0x01 > 0;
                let n = 7 - ((req[0] >> 1) & 0x07);
                let data = [req[1], req[2], req[3], req[4], req[5], req[6], req[7]];
                Ok(ClientRequest::DownloadSegment(toggle_bit, end, n, data))
            }

            ClientCommandSpecifier::AbortTransfer => {
                let ix = Index::read_from_slice(&req[1 .. 4]);
                let code = (u32::from_le_bytes([req[4], req[5], req[6], req[7]])).into();
                Ok(ClientRequest::AbortTransfer(ix, code))
            },
            
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServerResponse {
    UploadSingleSegment( Index, u8, [u8; 4] ),
    UploadInitMultiples(Index, u32),
    UploadMultiples(ToggleBit, bool, u8, [u8; 7]),
    DownloadInitAck(Index),
    DownloadSegmentAck(ToggleBit)
}

impl Into<[u8; 8]> for ServerResponse {
    fn into(self: Self) -> [u8; 8] {
        let mut req = [0; 8];

        match self {
            ServerResponse::UploadSingleSegment(ix, len, data) => {
                let cs: u8 = ServerCommandSpecifier::InitUpload.into();

                if len > 4 {
                    panic!("Expedited upload segment len out of range!");
                }

                let ty: u8 = TransferType::ExpeditedWithSize(len & 0x3).into();

                let code = cs | ty;
                req[0] = code;
                ix.write_to_slice(&mut req[1 .. 4]);
                req[4 .. 8].copy_from_slice(&data);
            },
            
            ServerResponse::UploadInitMultiples(ix, len) => {
                let cs: u8 = ServerCommandSpecifier::InitUpload.into();

                let ty: u8 = if len == 0 {
                    TransferType::NormalUnspecifiedSize.into()
                }else{
                    TransferType::Normal.into()
                };
                
                let code = cs | ty;
                req[0] = code;
                ix.write_to_slice(&mut req[1 .. 4]);
                req[4 .. 8].copy_from_slice(&len.to_le_bytes());
            }
            
            ServerResponse::UploadMultiples(tb, is_end, len, data) => {
                let cs: u8 = ServerCommandSpecifier::UploadSegment.into();
                let t: u8 = tb.into();
                let len = if len <= 7 {
                    7 - len
                }else{
                    panic!("Upload segment len out of range!");

                };

                let code = cs | (len << 1) |(is_end as u8) | t;
                req[0] = code;
                req[1 .. 8].copy_from_slice(&data);
            }

            ServerResponse::DownloadInitAck(ix) => {
                let cs: u8 = ServerCommandSpecifier::InitDownloadAck.into(); // ?
                
                // Set the command specifier for single segment download acknowledgment
                req[0] = cs;
                ix.write_to_slice(&mut req[1 .. 4]);
            },
            
            ServerResponse::DownloadSegmentAck(toggle_bit) => {
                let cs: u8 = ServerCommandSpecifier::DownloadSegmentAck.into();
                let t: u8 = toggle_bit.into();
                
                // Command specifier includes the toggle bit
                let code = cs | t;
                req[0] = code;
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
            ServerCommandSpecifier::InitUpload => {
                let ix = Index::read_from_slice(&req[1 .. 4]);
                let ty = TransferType::try_from(req[0])?;
                
                match ty {
                    TransferType::Normal => {
                        let n = u32::from_le_bytes(req[4 .. 8].try_into().unwrap());
                        Ok(ServerResponse::UploadInitMultiples(ix, n))
                    },
                    
                    TransferType::ExpeditedWithSize(n) => {
                        let mut data = [0; 4];
                        data.copy_from_slice(&req[4 .. 8]);
                        
                        Ok(ServerResponse::UploadSingleSegment(ix, n, data))
                    },
                    TransferType::NormalUnspecifiedSize => {
                        Ok(ServerResponse::UploadInitMultiples(ix, 0))
                    },
                }
            },
            
            ServerCommandSpecifier::UploadSegment => {
                let mut data = [0;7];
                data.copy_from_slice(&req[1..8]);
                let toggle: ToggleBit = req[0].into();
                let last = (req[0] & 1) == 1;
                let len = 7 - ((req[0] >> 1) & 0x07);

                Ok(ServerResponse::UploadMultiples(toggle, last, len, data))
            },

            ServerCommandSpecifier::InitDownloadAck => {
                let ix = Index::read_from_slice(&req[1 .. 4]);
                Ok(ServerResponse::DownloadInitAck(ix))
            },

            ServerCommandSpecifier::DownloadSegmentAck => {
                Ok(ServerResponse::DownloadSegmentAck(req[0].into()))
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdo::ClientRequest;
    use crate::sdo;

    //----------------------------Client side tests------------------------------------------------//
    #[test]
    fn client_upload_init() {
        let req = ClientRequest::InitUpload(Index::new(0x1000, 0x01));
        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.6 - SDO upload initiate
        let expected_buf: [u8; 8] = [ 0x40, 0x00, 0x10, 0x01, 0x00, 0x00, 0x00, 0x00 ];

        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from( req_buf ).unwrap();

        assert_eq!( req, req_dec );
    }

    #[test]
    fn client_upload_segment() {
        let req = ClientRequest::UploadSegment;
        let req_buf: [u8;8] = req.clone().into();

        let expected_buf: [u8;8] = [0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00 ];

        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from( req_buf ).unwrap();
        assert_eq!( req, req_dec );
    }

    #[test]
    fn client_download_single_init() {
        let index = Index::new(0x1000, 0x01);

        let req = ClientRequest::InitSingleSegmentDownload( index, 4, [0x01, 0x02, 0x03, 0x04] );

        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.3 - SDO download initiate, e = 1, s = 1
        let expected_buf: [u8; 8] = [ 0x23, 0x00, 0x10, 0x01, 0x01, 0x02, 0x03, 0x04 ];

        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from( req_buf ).unwrap();

        assert_eq!( req, req_dec );
    }

    #[test]
    fn client_download_last_segment(){
        let req = ClientRequest::DownloadSegment(ToggleBit(true), true, 7, [1,2,3,4,5,6,7]);

        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.4 - SDO download last segment with toggle bit == 1
        let expected_buf: [u8; 8] = [ 0x11, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07 ];
        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from( req_buf ).unwrap();
        assert_eq!(req, req_dec);
    }

    #[test]
    fn client_download_intermidiate_segment(){
        let req = ClientRequest::DownloadSegment(ToggleBit(false), false, 3, [1,2,3,4,5,6,7]);

        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.4 - SDO download last segment with toggle bit == 1
        let expected_buf: [u8; 8] = [ 0x08, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07 ];
        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from( req_buf ).unwrap();
        assert_eq!(req, req_dec);
    }

    #[test]
    fn client_download_segment_init() {
        let index = Index::new(0x1000, 0x01);
        let req = ClientRequest::InitMultipleDownload( index, 10 );

        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.3 - SDO download initiate, e = 0, s = 1
        let expected_buf: [u8; 8] = [ 0x21, 0x00, 0x10, 0x01, 0x0A, 0x00, 0x00, 0x00 ];

        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from( req_buf ).unwrap();

        assert_eq!( req, req_dec );
    }

    #[test]
    fn client_download_segment_init_unspecified_len() {
        let index = Index::new(0x1000, 0x01);
        let req = ClientRequest::InitMultipleDownload( index, 0 );

        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.3 - SDO download initiate, e = 0, s = 0
        let expected_buf: [u8; 8] = [ 0x20, 0x00, 0x10, 0x01, 0x00, 0x00, 0x00, 0x00 ];

        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from( req_buf ).unwrap();

        assert_eq!( req, req_dec );
    }

    #[test]
    fn client_abort_transfer(){
        let index = Index::new( 0x1000, 0x01 );
        let req = ClientRequest::AbortTransfer(index, AbortCode::SDOProtocolTimedOut);

        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.17 Protocol SDO abort transfer
        let expected_buf:[u8; 8] = [ 0x80, 0x00, 0x10, 0x01, 0x00, 0x00, 0x04, 0x05 ];
        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from( req_buf ).unwrap();
        assert_eq!( req, req_dec );
    }

    //----------------------------Server side tests------------------------------------------------//
    #[test]
    fn server_resp_upload_single(){
        let index = Index::new( 0x1000, 0x01 );
        let resp = ServerResponse::UploadSingleSegment( index, 2, [1,2,3,4] );

        let resp_buf:[u8; 8] = resp.clone().into();
        //CiA 301 7.2.4.3.6 SDO protocol upload initiate response section
        let expected_buf:[u8;8] = [0x4B, 0x00, 0x10, 0x01, 1, 2, 3, 4];
        assert_eq!(resp_buf, expected_buf);

        let resp_dec = ServerResponse::try_from( resp_buf ).unwrap();
        assert_eq!(resp, resp_dec);
    }

    #[test]
    fn server_resp_initiate_multiply_segments_with_specified_len(){
        let index = Index::new( 0x1000, 0x01 );
        let resp = ServerResponse::UploadInitMultiples( index, 20 );

        let resp_buf:[u8; 8] = resp.clone().into();
        //CiA 301 7.2.4.3.6 SDO protocol upload initiate response section
        let expected_buf:[u8;8] = [0x41, 0x00, 0x10, 0x01, 20, 0, 0, 0];
        assert_eq!(resp_buf, expected_buf);

        let resp_dec = ServerResponse::try_from( resp_buf ).unwrap();
        assert_eq!(resp, resp_dec);
    }

    #[test]
    fn server_resp_initiate_multiply_segments_with_unspecified_len(){
        let index = Index::new( 0x1000, 0x01 );
        let resp = ServerResponse::UploadInitMultiples( index, 0 );

        let resp_buf:[u8; 8] = resp.clone().into();
        //CiA 301 7.2.4.3.6 SDO protocol upload initiate response section
        let expected_buf:[u8;8] = [0x40, 0x00, 0x10, 0x01, 0, 0, 0, 0];
        assert_eq!(resp_buf, expected_buf);

        let resp_dec = ServerResponse::try_from( resp_buf ).unwrap();
        assert_eq!(resp, resp_dec);
    }

    #[test]
    fn server_resp_upload_last_segment(){
        let resp = ServerResponse::UploadMultiples(ToggleBit(true), true, 5, [1, 2, 3, 4, 5, 6, 7]);

        let resp_buf:[u8; 8] = resp.clone().into();
        //CiA 301 7.2.4.3.7 SDO protocol upload segment response section
        let expected_buf:[u8;8] = [0x15, 1, 2, 3, 4, 5, 6, 7];
        assert_eq!(resp_buf, expected_buf);

        let resp_dec = ServerResponse::try_from( resp_buf ).unwrap();
        assert_eq!(resp, resp_dec);
    }

    #[test]
    fn server_resp_upload_intermidiate_segment(){
        let resp = ServerResponse::UploadMultiples(ToggleBit(false), false, 7, [1, 2, 3, 4, 5, 6, 7]);

        let resp_buf:[u8; 8] = resp.clone().into();
        //CiA 301 7.2.4.3.7 SDO protocol upload segment response section
        let expected_buf:[u8;8] = [0x00, 1, 2, 3, 4, 5, 6, 7];
        assert_eq!(resp_buf, expected_buf);

        let resp_dec = ServerResponse::try_from( resp_buf ).unwrap();
        assert_eq!(resp, resp_dec);
    }

    #[test]
    fn server_resp_download_init_ack(){
        let index = Index::new( 0x1000, 0x01 );
        let resp = ServerResponse::DownloadInitAck(index);

        let resp_buf:[u8; 8] = resp.clone().into();
        //CiA 301 7.2.4.3.7 SDO protocol upload segment response section
        let expected_buf:[u8;8] = [0x60, 0x00, 0x10, 0x01, 0, 0, 0, 0];
        assert_eq!(resp_buf, expected_buf);

        let resp_dec = ServerResponse::try_from( resp_buf ).unwrap();
        assert_eq!(resp, resp_dec);
    }

    #[test]
    fn server_resp_download_segment_ack(){
        let resp = ServerResponse::DownloadSegmentAck(ToggleBit(true));

        let resp_buf:[u8; 8] = resp.clone().into();
        //CiA 301 7.2.4.3.7 SDO protocol upload segment response section
        let expected_buf:[u8;8] = [0x30, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(resp_buf, expected_buf);

        let resp_dec = ServerResponse::try_from( resp_buf ).unwrap();
        assert_eq!(resp, resp_dec);
    }
}
