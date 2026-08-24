pub mod abort;
pub mod client;
pub mod machines;
pub mod server;

use core::ops::Not;

use crate::dictionary::*;
use crate::sdo::abort::*;

//---------------------------------------------------------------------------------------------------
// CRC-16/CCITT as used by the SDO block transfer protocol (CiA 301 7.2.4.7 / 7.2.4.8).
//
// Polynomial x^16 + x^12 + x^5 + 1 (0x1021), initial value 0x0000 ("zero for XMODEM"),
// no bit reflection, no final XOR. The CRC covers the actual data bytes of a transfer,
// i.e. the padding bytes of the last segment (signalled by the `n` field of the
// block end frame) are excluded.

const fn crc16_ccitt_table() -> [u16; 256] {
    let mut table = [0u16; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc: u16 = 0;
        let mut c: u16 = (i as u16) << 8;
        let mut j = 0;
        while j < 8 {
            if (crc ^ c) & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
            c <<= 1;
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

static CRC16_CCITT_TABLE: [u16; 256] = crc16_ccitt_table();

/// Updates a CRC-16/CCITT checksum with the given data block.
///
/// For a fresh checksum the initial value must be `0`. For data split into several
/// chunks, pass the previously calculated value as `crc`.
pub fn crc16_ccitt(data: &[u8], crc: u16) -> u16 {
    let mut crc = crc;
    for &b in data {
        let tmp = ((crc >> 8) as u8) ^ b;
        crc = (crc << 8) ^ CRC16_CCITT_TABLE[tmp as usize];
    }
    crc
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    UnsupportedTransferType(u8),
    UnknownClientCommandSpecifier(u8),
    UnknownServerCommandSpecifier(u8),
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
            TransferType::ExpeditedWithSize(n) => 0x03 | ((4 - n) << 2),
            TransferType::NormalUnspecifiedSize => 0x00,
        }
    }
}

impl TryFrom<u8> for TransferType {
    type Error = Error;
    fn try_from(x: u8) -> Result<Self, Self::Error> {
        let code = x & 0x03;

        match code {
            0x01 => Ok(TransferType::Normal),
            0x02 => {
                //e == 1, s == 0 - expedited transfer without size specified
                //let n = 4- ((x & 0x0c) >> 2);
                Ok(TransferType::ExpeditedWithSize(0))
            }
            0x03 => {
                let n = 4 - ((x & 0x0c) >> 2);
                Ok(TransferType::ExpeditedWithSize(n))
            }
            0x00 => Ok(TransferType::NormalUnspecifiedSize),
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
            code => Err(Error::UnknownClientCommandSpecifier(code >> 5)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerCommandSpecifier {
    InitDownloadAck,
    DownloadSegmentAck,
    InitUpload,
    UploadSegment,
    Abort,
}

impl Into<u8> for ServerCommandSpecifier {
    fn into(self: Self) -> u8 {
        match self {
            ServerCommandSpecifier::InitDownloadAck => 3 << 5,
            ServerCommandSpecifier::DownloadSegmentAck => 1 << 5,
            ServerCommandSpecifier::InitUpload => 2 << 5,
            ServerCommandSpecifier::UploadSegment => 0 << 5,
            ServerCommandSpecifier::Abort => 4 << 5,
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
            0x80 => Ok(ServerCommandSpecifier::Abort),
            code => Err(Error::UnknownServerCommandSpecifier(code >> 5)),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum ClientRequest {
    InitUpload(CanIndex),
    UploadSegment(ToggleBit),
    InitSingleSegmentDownload(CanIndex, u8, [u8; 4]), // index, length, data
    InitMultipleDownload(CanIndex, u32),              // index and length,
    DownloadSegment(ToggleBit, bool, u8, [u8; 7]),    // toogle bit, end bit, length, data
    AbortTransfer(CanIndex, AbortCode),
    // Block transfer (CiA 301 7.2.4.7 - block download, 7.2.4.8 - block upload)
    /// Block download initiate. Carries the index, the data size (0 = not
    /// indicated) and whether CRC checking is requested.
    BlockDownloadInitiate(CanIndex, u32, bool),
    /// Block download segment. Carries the sequence number (1..block size),
    /// the "last segment of the whole transfer" flag and up to 7 data bytes.
    /// Note: this frame has no command specifier, so it can only be decoded
    /// contextually (by a machine in a block download state).
    BlockDownloadSegment(u8, bool, [u8; 7]),
    /// Block download end request. Carries the CRC of the transferred data and
    /// the number of bytes in the last segment that do not contain data.
    BlockDownloadEnd(u16, u8),
    /// Block upload initiate. Carries the index, the requested number of
    /// segments per block (1..127) and the protocol switch threshold.
    BlockUploadInitiate(CanIndex, u8, u8),
    /// Block upload "start" frame, sent by the client after it received the
    /// server's initiate response. Signals the server to start sending segments.
    BlockUploadStart,
    /// Block upload response, sent after each sub-block. Carries the sequence
    /// number of the last correctly received segment and the requested block
    /// size for the next sub-block.
    BlockUploadResponse(u8, u8),
    /// Block upload end acknowledgement.
    BlockUploadEndAck,
}

impl Into<[u8; 8]> for ClientRequest {
    fn into(self: Self) -> [u8; 8] {
        let mut req = [0; 8];

        match self {
            ClientRequest::InitUpload(ix) => {
                req[0] = ClientCommandSpecifier::InitUpload.into();
                ix.write_to_slice(&mut req[1..4]);
            }

            ClientRequest::UploadSegment(t) => {
                let u: u8 = t.into();
                let c: u8 = ClientCommandSpecifier::UploadSegment.into();
                req[0] = c | u;
            }

            ClientRequest::InitSingleSegmentDownload(ix, len, data) => {
                req[0] = ClientCommandSpecifier::InitDownload.into();
                //len is n field in first byte, represented 4-len value (len must be from 0 to 4, other is mistake)
                let len = if len <= 4 {
                    4 - len
                } else {
                    //TODO: как-то обработать ситуацию, когда len > 4. Возможно ли это?
                    panic!("Requested single segment download length out of range!");
                };

                req[0] |= len << 2;

                //Single segment download meant expedited download, so we must set e bit
                req[0] |= 1 << 1;
                //Besides we specify size, so s bit must be set too
                req[0] |= 1 << 0;

                ix.write_to_slice(&mut req[1..4]);

                req[4..8].copy_from_slice(&data[0..4]); // Set the first 4 bytes of data
                                                        // Note: data[3] is not used due to byte constraints
            }

            ClientRequest::InitMultipleDownload(ix, len) => {
                req[0] = ClientCommandSpecifier::InitDownload.into();

                //setup sized bit if we specify len
                if len > 0 {
                    req[0] |= 1 << 0;
                }

                ix.write_to_slice(&mut req[1..4]);
                req[4..8].copy_from_slice(&len.to_le_bytes()); // Set length as u32 in little endian
            }

            ClientRequest::DownloadSegment(t, c, n, data) => {
                let code: u8 = ClientCommandSpecifier::DownloadSegment.into();
                let toogle: u8 = t.into();

                let len = if n <= 7 {
                    7 - n
                } else {
                    //TODO: как-то обработать ситуацию, когда len >= 8
                    panic!("Requested segment download length out of range!");
                };

                req[0] = code | toogle | (len << 1) | (c as u8);
                req[1..8].copy_from_slice(&data[..7]);
            }

            ClientRequest::AbortTransfer(index, code) => {
                req[0] = ClientCommandSpecifier::AbortTransfer.into();
                index.write_to_slice(&mut req[1..4]);
                req[4..8].copy_from_slice(&(code as u32).to_le_bytes()); // Set abort code as u32 in little endian
            }

            ClientRequest::BlockDownloadInitiate(index, size, crc) => {
                req[0] = 0xC0; // cs = 6, block download initiate
                if crc {
                    req[0] |= 0x04; // crc support requested
                }
                if size > 0 {
                    req[0] |= 0x02; // size indicated
                    req[4..8].copy_from_slice(&size.to_le_bytes());
                }
                index.write_to_slice(&mut req[1..4]);
            }

            ClientRequest::BlockDownloadSegment(seqno, end, data) => {
                req[0] = seqno | ((end as u8) << 7);
                req[1..8].copy_from_slice(&data);
            }

            ClientRequest::BlockDownloadEnd(crc, no_data) => {
                req[0] = 0xC1 | ((no_data & 0x07) << 2); // cs = 6, sub = 1
                req[1] = crc as u8;
                req[2] = (crc >> 8) as u8;
            }

            ClientRequest::BlockUploadInitiate(index, block_size, pst) => {
                req[0] = 0xA4; // cs = 5, block upload initiate
                index.write_to_slice(&mut req[1..4]);
                req[4] = block_size;
                req[5] = pst;
            }

            ClientRequest::BlockUploadStart => {
                req[0] = 0xA3;
            }

            ClientRequest::BlockUploadResponse(ackseq, block_size) => {
                req[0] = 0xA2;
                req[1] = ackseq;
                req[2] = block_size;
            }

            ClientRequest::BlockUploadEndAck => {
                req[0] = 0xA1;
            }
        };

        req
    }
}

impl TryFrom<[u8; 8]> for ClientRequest {
    type Error = Error;

    fn try_from(req: [u8; 8]) -> Result<Self, Self::Error> {
        // Block transfer command specifiers (cs = 5, 6) do not fit the
        // segmented command specifier space, handle them first.
        match req[0] & 0xE0 {
            0xA0 => {
                // cs = 5 - block upload frames (client -> server)
                match req[0] {
                    0xA1 => Ok(ClientRequest::BlockUploadEndAck),
                    0xA2 => Ok(ClientRequest::BlockUploadResponse(req[1], req[2])),
                    0xA3 => Ok(ClientRequest::BlockUploadStart),
                    0xA4 => Ok(ClientRequest::BlockUploadInitiate(
                        CanIndex::read_from_slice(&req[1..4]),
                        req[4],
                        req[5],
                    )),
                    _ => Err(Error::UnknownClientCommandSpecifier(req[0] >> 5)),
                }
            }
            0xC0 => {
                // cs = 6 - block download initiate / end request
                if (req[0] & 0xE3) == 0xC1 {
                    // sub-command = 1: end request
                    let no_data = (req[0] >> 2) & 0x07;
                    let crc = u16::from_le_bytes([req[1], req[2]]);
                    Ok(ClientRequest::BlockDownloadEnd(crc, no_data))
                } else if (req[0] & 0xE1) == 0xC0 {
                    // sub-command = 0: initiate
                    let crc = (req[0] & 0x04) != 0;
                    let size = if (req[0] & 0x02) != 0 {
                        u32::from_le_bytes([req[4], req[5], req[6], req[7]])
                    } else {
                        0
                    };
                    Ok(ClientRequest::BlockDownloadInitiate(
                        CanIndex::read_from_slice(&req[1..4]),
                        size,
                        crc,
                    ))
                } else {
                    Err(Error::UnknownClientCommandSpecifier(req[0] >> 5))
                }
            }
            _ => {
                let code = ClientCommandSpecifier::try_from(req[0])?;

                match code {
            ClientCommandSpecifier::InitUpload => {
                let ix = CanIndex::read_from_slice(&req[1..4]);
                Ok(ClientRequest::InitUpload(ix))
            }

            ClientCommandSpecifier::UploadSegment => {
                let t = ToggleBit::from(req[0]);
                Ok(ClientRequest::UploadSegment(t))
            }

            ClientCommandSpecifier::InitDownload => {
                let ix = CanIndex::read_from_slice(&req[1..4]);
                // Determine if it's a single or multiple segment download based on s and e bits in first byte
                let is_expedited = (req[0] >> 1) & 1 > 0;
                let is_sized = (req[0] >> 0) & 1 > 0;

                match (is_expedited, is_sized) {
                    (false, false) => {
                        //Unspecified len download request
                        Ok(ClientRequest::InitMultipleDownload(ix, 0))
                    }
                    (false, true) => {
                        let len = u32::from_le_bytes([req[4], req[5], req[6], req[7]]);
                        Ok(ClientRequest::InitMultipleDownload(ix, len))
                    }
                    (true, true) => {
                        //Expedited download request
                        let len = 4 - ((req[0] >> 2) & 0x3);
                        let mut data = [0u8; 4];
                        data[..4].copy_from_slice(&req[4..8]);
                        Ok(ClientRequest::InitSingleSegmentDownload(ix, len, data))
                    }
                    (true, false) => {
                        //Expedited, but len not specified. Not sure is this used in real and have any meaning
                        let mut data = [0u8; 4];
                        data[..3].copy_from_slice(&req[4..8]);
                        Ok(ClientRequest::InitSingleSegmentDownload(ix, 0, data))
                    }
                }
            }

            ClientCommandSpecifier::DownloadSegment => {
                let toggle_bit = ToggleBit::from(req[0]);
                let end = req[0] & 0x01 > 0;
                let n = 7 - ((req[0] >> 1) & 0x07);
                let data = [req[1], req[2], req[3], req[4], req[5], req[6], req[7]];
                Ok(ClientRequest::DownloadSegment(toggle_bit, end, n, data))
            }

            ClientCommandSpecifier::AbortTransfer => {
                let ix = CanIndex::read_from_slice(&req[1..4]);
                let code = (u32::from_le_bytes([req[4], req[5], req[6], req[7]])).into();
                Ok(ClientRequest::AbortTransfer(ix, code))
            }
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServerResponse {
    UploadSingleSegment(CanIndex, u8, [u8; 4]),
    UploadInitMultiples(CanIndex, u32),
    UploadMultiples(ToggleBit, bool, u8, [u8; 7]),
    DownloadInitAck(CanIndex),
    DownloadSegmentAck(ToggleBit),
    Abort(CanIndex, AbortCode),
    // Block transfer (CiA 301 7.2.4.7 - block download, 7.2.4.8 - block upload)
    /// Block download initiate acknowledgement. Carries the number of segments
    /// per block (1..127) chosen by the server.
    BlockDownloadInitiateAck(u8),
    /// Block download response, sent after each sub-block. Carries the sequence
    /// number of the last correctly received segment and the block size for the
    /// next sub-block.
    BlockDownloadResponse(u8, u8),
    /// Block download end acknowledgement.
    BlockDownloadEndAck,
    /// Block upload initiate acknowledgement. Carries the index, the size of
    /// the data to upload (0 = not indicated) and whether CRC checking is
    /// supported by the server.
    BlockUploadInitiateAck(CanIndex, u32, bool),
    /// Block upload segment. Carries the sequence number (1..block size), the
    /// "last segment of the whole transfer" flag and up to 7 data bytes.
    /// Note: this frame has no command specifier, so it can only be decoded
    /// contextually (by a machine in a block upload state).
    BlockUploadSegment(u8, bool, [u8; 7]),
    /// Block upload end frame. Carries the number of bytes in the last segment
    /// that do not contain data and the CRC of the transferred data.
    BlockUploadEnd(u8, u16),
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
                ix.write_to_slice(&mut req[1..4]);
                req[4..8].copy_from_slice(&data);
            }

            ServerResponse::UploadInitMultiples(ix, len) => {
                let cs: u8 = ServerCommandSpecifier::InitUpload.into();

                let ty: u8 = if len == 0 {
                    TransferType::NormalUnspecifiedSize.into()
                } else {
                    TransferType::Normal.into()
                };

                let code = cs | ty;
                req[0] = code;
                ix.write_to_slice(&mut req[1..4]);
                req[4..8].copy_from_slice(&len.to_le_bytes());
            }

            ServerResponse::UploadMultiples(tb, is_end, len, data) => {
                let cs: u8 = ServerCommandSpecifier::UploadSegment.into();
                let t: u8 = tb.into();
                let len = if len <= 7 {
                    7 - len
                } else {
                    panic!("Upload segment len out of range!");
                };

                let code = cs | (len << 1) | (is_end as u8) | t;
                req[0] = code;
                req[1..8].copy_from_slice(&data);
            }

            ServerResponse::DownloadInitAck(ix) => {
                let cs: u8 = ServerCommandSpecifier::InitDownloadAck.into(); // ?

                // Set the command specifier for single segment download acknowledgment
                req[0] = cs;
                ix.write_to_slice(&mut req[1..4]);
            }

            ServerResponse::DownloadSegmentAck(toggle_bit) => {
                let cs: u8 = ServerCommandSpecifier::DownloadSegmentAck.into();
                let t: u8 = toggle_bit.into();

                // Command specifier includes the toggle bit
                let code = cs | t;
                req[0] = code;
            }

            ServerResponse::Abort(ix, code) => {
                let cs: u8 = ServerCommandSpecifier::DownloadSegmentAck.into();
                let code_u32: u32 = code.into();
                
                req[0] = cs;
                ix.write_to_slice(&mut req[1..4]);
                req[4..8].copy_from_slice(&code_u32.to_le_bytes());
            }

            ServerResponse::BlockDownloadInitiateAck(block_size) => {
                req[0] = 0xA4; // cs = 5, crc supported
                req[4] = block_size;
            }

            ServerResponse::BlockDownloadResponse(ackseq, block_size) => {
                req[0] = 0xA2;
                req[1] = ackseq;
                req[2] = block_size;
            }

            ServerResponse::BlockDownloadEndAck => {
                req[0] = 0xA1;
            }

            ServerResponse::BlockUploadInitiateAck(index, size, crc) => {
                req[0] = 0xC0; // cs = 6, block upload initiate response
                if crc {
                    req[0] |= 0x04;
                }
                if size > 0 {
                    req[0] |= 0x02; // size indicated
                    req[4..8].copy_from_slice(&size.to_le_bytes());
                }
                index.write_to_slice(&mut req[1..4]);
            }

            ServerResponse::BlockUploadSegment(seqno, end, data) => {
                req[0] = seqno | ((end as u8) << 7);
                req[1..8].copy_from_slice(&data);
            }

            ServerResponse::BlockUploadEnd(no_data, crc) => {
                req[0] = 0xC1 | ((no_data & 0x07) << 2); // cs = 6, sub = 1
                req[1] = crc as u8;
                req[2] = (crc >> 8) as u8;
            }
        };

        req
    }
}

impl TryFrom<[u8; 8]> for ServerResponse {
    type Error = Error;

    fn try_from(req: [u8; 8]) -> Result<Self, Self::Error> {
        // Block transfer command specifiers (cs = 5, 6) do not fit the
        // segmented command specifier space, handle them first.
        match req[0] & 0xE0 {
            0xA0 => {
                // cs = 5 - block download frames (server -> client)
                match req[0] {
                    0xA1 => Ok(ServerResponse::BlockDownloadEndAck),
                    0xA2 => Ok(ServerResponse::BlockDownloadResponse(req[1], req[2])),
                    0xA4 => Ok(ServerResponse::BlockDownloadInitiateAck(req[4])),
                    _ => Err(Error::UnknownServerCommandSpecifier(req[0] >> 5)),
                }
            }
            0xC0 => {
                // cs = 6 - block upload initiate response / end frame
                if (req[0] & 0xE3) == 0xC1 {
                    // sub-command = 1: end frame
                    let no_data = (req[0] >> 2) & 0x07;
                    let crc = u16::from_le_bytes([req[1], req[2]]);
                    Ok(ServerResponse::BlockUploadEnd(no_data, crc))
                } else if (req[0] & 0xE1) == 0xC0 {
                    // sub-command = 0: initiate response
                    let crc = (req[0] & 0x04) != 0;
                    let size = if (req[0] & 0x02) != 0 {
                        u32::from_le_bytes([req[4], req[5], req[6], req[7]])
                    } else {
                        0
                    };
                    Ok(ServerResponse::BlockUploadInitiateAck(
                        CanIndex::read_from_slice(&req[1..4]),
                        size,
                        crc,
                    ))
                } else {
                    Err(Error::UnknownServerCommandSpecifier(req[0] >> 5))
                }
            }
            _ => {
                let code = ServerCommandSpecifier::try_from(req[0])?;

                match code {
            ServerCommandSpecifier::InitUpload => {
                let ix = CanIndex::read_from_slice(&req[1..4]);
                let ty = TransferType::try_from(req[0])?;

                match ty {
                    TransferType::Normal => {
                        let n = u32::from_le_bytes(req[4..8].try_into().unwrap());
                        Ok(ServerResponse::UploadInitMultiples(ix, n))
                    }

                    TransferType::ExpeditedWithSize(n) => {
                        let mut data = [0; 4];
                        data.copy_from_slice(&req[4..8]);

                        Ok(ServerResponse::UploadSingleSegment(ix, n, data))
                    }
                    TransferType::NormalUnspecifiedSize => {
                        Ok(ServerResponse::UploadInitMultiples(ix, 0))
                    }
                }
            }

            ServerCommandSpecifier::UploadSegment => {
                let mut data = [0; 7];
                data.copy_from_slice(&req[1..8]);
                let toggle: ToggleBit = req[0].into();
                let last = (req[0] & 1) == 1;
                let len = 7 - ((req[0] >> 1) & 0x07);

                Ok(ServerResponse::UploadMultiples(toggle, last, len, data))
            }

            ServerCommandSpecifier::InitDownloadAck => {
                let ix = CanIndex::read_from_slice(&req[1..4]);
                Ok(ServerResponse::DownloadInitAck(ix))
            }

            ServerCommandSpecifier::DownloadSegmentAck => {
                Ok(ServerResponse::DownloadSegmentAck(req[0].into()))
            }
            
            ServerCommandSpecifier::Abort => {
                let abort_code = u32::from_le_bytes(req[4..8].try_into().unwrap());
                let ix = CanIndex::read_from_slice(&req[1..4]);
                Ok(ServerResponse::Abort(ix, abort_code.into()))
            }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdo;
    use sdo::ClientRequest;

    //----------------------------Client side tests------------------------------------------------//
    #[test]
    fn client_upload_init() {
        let req = ClientRequest::InitUpload(CanIndex::new(0x1000, 0x01));
        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.6 - SDO upload initiate
        let expected_buf: [u8; 8] = [0x40, 0x00, 0x10, 0x01, 0x00, 0x00, 0x00, 0x00];

        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from(req_buf).unwrap();

        assert_eq!(req, req_dec);
    }

    #[test]
    fn client_upload_segment() {
        let req = ClientRequest::UploadSegment(ToggleBit(false));
        let req_buf: [u8; 8] = req.clone().into();

        let expected_buf: [u8; 8] = [0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from(req_buf).unwrap();
        assert_eq!(req, req_dec);
    }

    #[test]
    fn client_download_single_init() {
        let index = CanIndex::new(0x1000, 0x01);

        let req = ClientRequest::InitSingleSegmentDownload(index, 4, [0x01, 0x02, 0x03, 0x04]);

        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.3 - SDO download initiate, e = 1, s = 1
        let expected_buf: [u8; 8] = [0x23, 0x00, 0x10, 0x01, 0x01, 0x02, 0x03, 0x04];

        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from(req_buf).unwrap();

        assert_eq!(req, req_dec);
    }

    #[test]
    fn client_download_last_segment() {
        let req = ClientRequest::DownloadSegment(ToggleBit(true), true, 7, [1, 2, 3, 4, 5, 6, 7]);

        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.4 - SDO download last segment with toggle bit == 1
        let expected_buf: [u8; 8] = [0x11, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];
        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from(req_buf).unwrap();
        assert_eq!(req, req_dec);
    }

    #[test]
    fn client_download_intermidiate_segment() {
        let req = ClientRequest::DownloadSegment(ToggleBit(false), false, 3, [1, 2, 3, 4, 5, 6, 7]);

        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.4 - SDO download last segment with toggle bit == 1
        let expected_buf: [u8; 8] = [0x08, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];
        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from(req_buf).unwrap();
        assert_eq!(req, req_dec);
    }

    #[test]
    fn client_download_segment_init() {
        let index = CanIndex::new(0x1000, 0x01);
        let req = ClientRequest::InitMultipleDownload(index, 10);

        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.3 - SDO download initiate, e = 0, s = 1
        let expected_buf: [u8; 8] = [0x21, 0x00, 0x10, 0x01, 0x0A, 0x00, 0x00, 0x00];

        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from(req_buf).unwrap();

        assert_eq!(req, req_dec);
    }

    #[test]
    fn client_download_segment_init_unspecified_len() {
        let index = CanIndex::new(0x1000, 0x01);
        let req = ClientRequest::InitMultipleDownload(index, 0);

        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.3 - SDO download initiate, e = 0, s = 0
        let expected_buf: [u8; 8] = [0x20, 0x00, 0x10, 0x01, 0x00, 0x00, 0x00, 0x00];

        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from(req_buf).unwrap();

        assert_eq!(req, req_dec);
    }

    #[test]
    fn client_abort_transfer() {
        let index = CanIndex::new(0x1000, 0x01);
        let req = ClientRequest::AbortTransfer(index, AbortCode::SDOProtocolTimedOut);

        let req_buf: [u8; 8] = req.clone().into();

        //CiA301 7.2.4.3.17 Protocol SDO abort transfer
        let expected_buf: [u8; 8] = [0x80, 0x00, 0x10, 0x01, 0x00, 0x00, 0x04, 0x05];
        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from(req_buf).unwrap();
        assert_eq!(req, req_dec);
    }

    //----------------------------Server side tests------------------------------------------------//
    #[test]
    fn server_resp_upload_single() {
        let index = CanIndex::new(0x1000, 0x01);
        let resp = ServerResponse::UploadSingleSegment(index, 2, [1, 2, 3, 4]);

        let resp_buf: [u8; 8] = resp.clone().into();
        //CiA 301 7.2.4.3.6 SDO protocol upload initiate response section
        let expected_buf: [u8; 8] = [0x4B, 0x00, 0x10, 0x01, 1, 2, 3, 4];
        assert_eq!(resp_buf, expected_buf);

        let resp_dec = ServerResponse::try_from(resp_buf).unwrap();
        assert_eq!(resp, resp_dec);
    }

    #[test]
    fn server_resp_initiate_multiply_segments_with_specified_len() {
        let index = CanIndex::new(0x1000, 0x01);
        let resp = ServerResponse::UploadInitMultiples(index, 20);

        let resp_buf: [u8; 8] = resp.clone().into();
        //CiA 301 7.2.4.3.6 SDO protocol upload initiate response section
        let expected_buf: [u8; 8] = [0x41, 0x00, 0x10, 0x01, 20, 0, 0, 0];
        assert_eq!(resp_buf, expected_buf);

        let resp_dec = ServerResponse::try_from(resp_buf).unwrap();
        assert_eq!(resp, resp_dec);
    }

    #[test]
    fn server_resp_initiate_multiply_segments_with_unspecified_len() {
        let index = CanIndex::new(0x1000, 0x01);
        let resp = ServerResponse::UploadInitMultiples(index, 0);

        let resp_buf: [u8; 8] = resp.clone().into();
        //CiA 301 7.2.4.3.6 SDO protocol upload initiate response section
        let expected_buf: [u8; 8] = [0x40, 0x00, 0x10, 0x01, 0, 0, 0, 0];
        assert_eq!(resp_buf, expected_buf);

        let resp_dec = ServerResponse::try_from(resp_buf).unwrap();
        assert_eq!(resp, resp_dec);
    }

    #[test]
    fn server_resp_upload_last_segment() {
        let resp = ServerResponse::UploadMultiples(ToggleBit(true), true, 5, [1, 2, 3, 4, 5, 6, 7]);

        let resp_buf: [u8; 8] = resp.clone().into();
        //CiA 301 7.2.4.3.7 SDO protocol upload segment response section
        let expected_buf: [u8; 8] = [0x15, 1, 2, 3, 4, 5, 6, 7];
        assert_eq!(resp_buf, expected_buf);

        let resp_dec = ServerResponse::try_from(resp_buf).unwrap();
        assert_eq!(resp, resp_dec);
    }

    #[test]
    fn server_resp_upload_intermidiate_segment() {
        let resp =
            ServerResponse::UploadMultiples(ToggleBit(false), false, 7, [1, 2, 3, 4, 5, 6, 7]);

        let resp_buf: [u8; 8] = resp.clone().into();
        //CiA 301 7.2.4.3.7 SDO protocol upload segment response section
        let expected_buf: [u8; 8] = [0x00, 1, 2, 3, 4, 5, 6, 7];
        assert_eq!(resp_buf, expected_buf);

        let resp_dec = ServerResponse::try_from(resp_buf).unwrap();
        assert_eq!(resp, resp_dec);
    }

    #[test]
    fn server_resp_download_init_ack() {
        let index = CanIndex::new(0x1000, 0x01);
        let resp = ServerResponse::DownloadInitAck(index);

        let resp_buf: [u8; 8] = resp.clone().into();
        //CiA 301 7.2.4.3.7 SDO protocol upload segment response section
        let expected_buf: [u8; 8] = [0x60, 0x00, 0x10, 0x01, 0, 0, 0, 0];
        assert_eq!(resp_buf, expected_buf);

        let resp_dec = ServerResponse::try_from(resp_buf).unwrap();
        assert_eq!(resp, resp_dec);
    }

    #[test]
    fn server_resp_download_segment_ack() {
        let resp = ServerResponse::DownloadSegmentAck(ToggleBit(true));

        let resp_buf: [u8; 8] = resp.clone().into();
        //CiA 301 7.2.4.3.7 SDO protocol upload segment response section
        let expected_buf: [u8; 8] = [0x30, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(resp_buf, expected_buf);

        let resp_dec = ServerResponse::try_from(resp_buf).unwrap();
        assert_eq!(resp, resp_dec);
    }

    //----------------------------Block transfer codec tests----------------------------//

    #[test]
    fn crc16_ccitt_check_value() {
        // Check value of CRC-16/XMODEM (polynomial 0x1021, initial value 0)
        // for the ASCII string "123456789".
        assert_eq!(crc16_ccitt(b"123456789", 0), 0x31C3);
    }

    #[test]
    fn crc16_ccitt_incremental() {
        let one_shot = crc16_ccitt(b"123456789", 0);
        let mut crc = crc16_ccitt(b"12345", 0);
        crc = crc16_ccitt(b"6789", crc);
        assert_eq!(crc, one_shot);
    }

    #[test]
    fn block_download_initiate() {
        let req = ClientRequest::BlockDownloadInitiate(CanIndex::new(0x1010, 0x01), 16, true);
        let req_buf: [u8; 8] = req.clone().into();

        // cs = 6 (0xC0) | crc support (0x04) | size indicated (0x02)
        let expected_buf: [u8; 8] = [0xC6, 0x10, 0x10, 0x01, 16, 0, 0, 0];
        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from(req_buf).unwrap();
        assert_eq!(req, req_dec);
    }

    #[test]
    fn block_download_initiate_unspecified_size() {
        let req = ClientRequest::BlockDownloadInitiate(CanIndex::new(0x1010, 0x01), 0, false);
        let req_buf: [u8; 8] = req.clone().into();
        assert_eq!(req_buf[0], 0xC0);

        let req_dec = ClientRequest::try_from(req_buf).unwrap();
        assert_eq!(req, req_dec);
    }

    #[test]
    fn block_download_segment_is_ambiguous_for_static_decoder() {
        // A block download segment carries no command specifier: with a small
        // sequence number it is indistinguishable from a segmented
        // DownloadSegment frame, and with the end flag it looks like an abort.
        let req = ClientRequest::BlockDownloadSegment(3, false, [1, 2, 3, 4, 5, 6, 7]);
        let req_buf: [u8; 8] = req.clone().into();
        assert_eq!(req_buf[0], 0x03);
        assert!(matches!(
            ClientRequest::try_from(req_buf).unwrap(),
            ClientRequest::DownloadSegment(..)
        ));

        let req = ClientRequest::BlockDownloadSegment(3, true, [1, 2, 3, 4, 5, 6, 7]);
        let req_buf: [u8; 8] = req.clone().into();
        assert_eq!(req_buf[0], 0x83);
        assert!(matches!(
            ClientRequest::try_from(req_buf).unwrap(),
            ClientRequest::AbortTransfer(..)
        ));
    }

    #[test]
    fn block_download_end() {
        // CiA 301 7.2.4.3.10: block download end request.
        // n = 7 (all bytes of the last segment invalid) -> byte0 = 0xDD.
        let req = ClientRequest::BlockDownloadEnd(0x1234, 7);
        let req_buf: [u8; 8] = req.clone().into();

        let expected_buf: [u8; 8] = [0xC1 | (7 << 2), 0x34, 0x12, 0, 0, 0, 0, 0];
        assert_eq!(req_buf, expected_buf);
        assert_eq!(req_buf[0], 0xDD);

        let req_dec = ClientRequest::try_from(req_buf).unwrap();
        assert_eq!(req, req_dec);
    }

    #[test]
    fn block_upload_initiate() {
        let req = ClientRequest::BlockUploadInitiate(CanIndex::new(0x1010, 0x02), 7, 0);
        let req_buf: [u8; 8] = req.clone().into();

        // cs = 5, sub-command = 4 (0xA4): block upload initiate
        let expected_buf: [u8; 8] = [0xA4, 0x10, 0x10, 0x02, 7, 0, 0, 0];
        assert_eq!(req_buf, expected_buf);

        let req_dec = ClientRequest::try_from(req_buf).unwrap();
        assert_eq!(req, req_dec);
    }

    #[test]
    fn block_upload_start_response_end_ack() {
        let start_buf: [u8; 8] = ClientRequest::BlockUploadStart.into();
        assert_eq!(start_buf[0], 0xA3);
        assert!(matches!(
            ClientRequest::try_from(start_buf).unwrap(),
            ClientRequest::BlockUploadStart
        ));

        let resp = ClientRequest::BlockUploadResponse(5, 7);
        let resp_buf: [u8; 8] = resp.clone().into();
        assert_eq!(&resp_buf[0..3], &[0xA2, 5, 7]);
        assert_eq!(ClientRequest::try_from(resp_buf).unwrap(), resp);

        let end_ack_buf: [u8; 8] = ClientRequest::BlockUploadEndAck.into();
        assert_eq!(end_ack_buf[0], 0xA1);
        assert!(matches!(
            ClientRequest::try_from(end_ack_buf).unwrap(),
            ClientRequest::BlockUploadEndAck
        ));
    }

    #[test]
    fn block_download_initiate_ack() {
        let resp = ServerResponse::BlockDownloadInitiateAck(7);
        let resp_buf: [u8; 8] = resp.clone().into();

        // cs = 5, sub-command = 4 (0xA4): block size in byte 4
        let expected_buf: [u8; 8] = [0xA4, 0, 0, 0, 7, 0, 0, 0];
        assert_eq!(resp_buf, expected_buf);

        let resp_dec = ServerResponse::try_from(resp_buf).unwrap();
        assert_eq!(resp, resp_dec);
    }

    #[test]
    fn block_download_response() {
        let resp = ServerResponse::BlockDownloadResponse(7, 7);
        let resp_buf: [u8; 8] = resp.clone().into();
        assert_eq!(&resp_buf[0..3], &[0xA2, 7, 7]);
        assert_eq!(ServerResponse::try_from(resp_buf).unwrap(), resp);
    }

    #[test]
    fn block_upload_initiate_ack() {
        let resp = ServerResponse::BlockUploadInitiateAck(CanIndex::new(0x1010, 0x02), 16, true);
        let resp_buf: [u8; 8] = resp.clone().into();

        // cs = 6 (0xC0) | crc support (0x04) | size indicated (0x02)
        let expected_buf: [u8; 8] = [0xC6, 0x10, 0x10, 0x02, 16, 0, 0, 0];
        assert_eq!(resp_buf, expected_buf);

        let resp_dec = ServerResponse::try_from(resp_buf).unwrap();
        assert_eq!(resp, resp_dec);
    }

    #[test]
    fn block_upload_segment_and_end() {
        let seg = ServerResponse::BlockUploadSegment(1, true, [9, 8, 7, 6, 5, 4, 3]);
        let seg_buf: [u8; 8] = seg.clone().into();
        assert_eq!(seg_buf[0], 0x81);
        assert_eq!(&seg_buf[1..8], &[9, 8, 7, 6, 5, 4, 3]);

        // the end frame has the same layout in both directions
        let end = ServerResponse::BlockUploadEnd(4, 0xBEFF);
        let end_buf: [u8; 8] = end.clone().into();
        assert_eq!(&end_buf[0..3], &[0xC1 | (4 << 2), 0xFF, 0xBE]);
        assert_eq!(ServerResponse::try_from(end_buf).unwrap(), end);
    }
}
