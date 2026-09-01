use crate::interfaces::*;
use crate::machine::*;
use crate::sdo::abort::*;
use crate::sdo::{Error as SdoDecodingError};
use crate::sdo::*;

/// Possible errors during SDO communications
#[derive(Clone)]
pub enum SdoError {
    ClientStateResponseMismatch(ClientState, ServerResponse),
    ServerStateResponseMismatch(ServerState, ClientRequest),
    CanIndexMismatch(CanIndex, CanIndex),
    TransferAborted(CanIndex, AbortCode),  
    ToggleMismatch,
    BufferOverflow,
    Busy,
    DictionaryUnsupportedIndex(CanIndex),
    DictionaryDecodingFailure(CanIndex),
    DecodingFailure(SdoDecodingError),
    NoResponder
}

impl core::fmt::Debug for SdoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use SdoError::*;
        match self {
            ClientStateResponseMismatch(cl, sv) => {
                f.debug_tuple("ClientStateResponseMismatch")
                 .field(cl)
                 .field(sv)
                 .finish()
            }

            ServerStateResponseMismatch(st, rq) => {
                f.debug_tuple("ServerStateResponseMismatch")
                 .field(st)
                 .field(rq)
                 .finish()
            }

            CanIndexMismatch(a, b) => {
                write!(f,
                       "CanIndexMismatch({:?}, {:?})",
                       a,
                       b)
            }

            TransferAborted(idx, code) => {
                write!(f,
                       "TransferAborted({:?}, {:?})",
                       idx,
                       code)
            }

            ToggleMismatch     => f.write_str("ToggleMismatch"),
            BufferOverflow     => f.write_str("BufferOverflow"),
            Busy               => f.write_str("Busy"),
            NoResponder        => f.write_str("NoResponder"),

            DictionaryUnsupportedIndex(idx) => {
                write!(f, "DictionaryUnsupportedIndex({:?})", idx)
            }

            DictionaryDecodingFailure(idx) => {
                write!(f, "DictionaryDecodingFailure({:?})", idx)
            }

            DecodingFailure(err) => {
                f.debug_tuple("DecodingFailure")
                 .field(err)
                 .finish()
            }
        }
    }
}

/// Client states
#[derive(Debug, Clone, Copy)]
pub enum ClientState {
    Idle,
    InitUploading,
    //    SingleSegmentUploaded,
    UploadingMultiples(ToggleBit),
    //    MultiplesUploaded,
    InitiateSingleDownload(usize),
    InitiateMultipleDownload(usize),
    DownloadingSegments(ToggleBit, usize),
    //    DownloadCompleted,
    // Block transfer states (CiA 301 7.2.4.7 / 7.2.4.8)
    /// Block download initiate was sent, awaiting the server's initiate ack.
    BlockDownloadInitiated,
    /// Client is emitting the segments of the current sub-block. `block_start`
    /// is the data offset where the sub-block began (used for retransmission),
    /// `position` the offset of the next segment, `seqno` the number of
    /// segments already emitted in this sub-block.
    BlockDownloadSending {
        blocksize: u8,
        block_start: usize,
        position: usize,
        seqno: u8,
    },
    /// A sub-block was fully sent, awaiting the server's block response.
    BlockDownloadAwaitingResponse {
        block_start: usize,
        sent: u8,
        finished: bool,
        no_data: u8,
    },
    /// Block download end request was sent, awaiting the end ack.
    BlockDownloadEnding,
    /// Block upload initiate was sent, awaiting the server's initiate ack.
    BlockUploadInitiated,
    /// Client is receiving the segments of a sub-block. `blocksize` is the
    /// negotiated number of segments per block, `position` the received data
    /// offset, `seqno` the sequence number of the last received segment.
    BlockUploadReceiving {
        blocksize: u8,
        position: usize,
        seqno: u8,
    },
    /// The last segment of the transfer was received (stashed), the block
    /// response was sent, awaiting the block upload end frame.
    BlockUploadAwaitingEnd { position: usize },
}

/// Default number of segments per block requested by the client, derived from
/// the size of its data buffer (CiA 301: 1..127).
fn client_block_size<const N: usize>() -> u8 {
    ((N / 7).min(127)).max(1) as u8
}

/// Client context
pub struct ClientMachine<const N: usize, RR, RW> {
    current_index: CanIndex,
    state: ClientState,
    data_index: usize,
    read_responder: Option<RR>,
    write_responder: Option<RW>,
    data: [u8; N],
    /// Last data segment received during a block upload. Its valid length is
    /// only known when the block upload end frame arrives.
    upload_last: [u8; 7],
    /// Size of the data to upload, as indicated by the server in the block
    /// upload initiate response (0 = not indicated).
    block_upload_size: u32,
    /// Whether the server supports CRC checking for the block upload.
    block_upload_crc: bool,
}

/// Possible final result that machine produces
#[derive(Debug)]
pub enum ClientResult<const N: usize, RR, RW> {
    UploadCompleted(CanIndex, [u8; N], usize, Option<RR>),
    DownloadCompleted(Option<RW>),
}

/// All possible observations of client machine
#[derive(Debug)]
pub enum ClientOutput<const N: usize, RR, RW> {
    Output(ClientRequest),
    Done(ClientResult<N, RR, RW>),
    /// The machine produced a final frame (the block upload end
    /// acknowledgement) together with the transfer result. The caller must
    /// transmit the frame, then handle the result exactly like `Done`.
    FinalOutput(ClientRequest, ClientResult<N, RR, RW>),
    TransferCompleted,
    Error(SdoError),
    /// The machine processed an input but has nothing to send. Used by block
    /// transfer for frames that are acknowledged internally (e.g. duplicate
    /// segments) or that only advance the transfer state.
    NoFrame,
}

impl<const N: usize, RR, RW> ClientOutput<N, RR, RW> {
    pub fn is_ready(self: &Self) -> bool {
        match self {
            ClientOutput::Output(_) => false,
            _ => true,
        }
    }
}

impl<const N: usize, RR, RW> Default for ClientMachine<N, RR, RW> {
    fn default() -> Self {
        ClientMachine {
            read_responder: None,
            write_responder: None,
            current_index: CanIndex::new(0, 0),
            state: ClientState::Idle,
            data_index: 0,
            data: [0; N],
            upload_last: [0; 7],
            block_upload_size: 0,
            block_upload_crc: false,
        }
    }
}

impl<const N: usize, RR, RW> ClientMachine<N, RR, RW> {
    
    pub fn is_ready(self: &Self) -> bool {
        match self.state {
            ClientState::Idle => true,
            _ => false,
        }
    }

    /// Reset state
    pub fn reset(self: &mut Self) {
        self.read_responder = None;
        self.write_responder = None;
        self.state = ClientState::Idle;
        self.data_index = 0;
        self.block_upload_size = 0;
        self.block_upload_crc = false;
    } 
    
    /// Initiates SDO read
    pub fn read(self: &mut Self, ix: CanIndex, r: RR) -> ClientOutput<N, RR, RW> {
        self.data_index = 0;
        self.current_index = ix;
        self.read_responder = Some(r);
        self.init_upload()
    }

    /// Initiates SDO write
    pub fn write<T>(self: &mut Self, ix: CanIndex, t: T, r: RW) -> ClientOutput<N, RR, RW>
    where
        T: IntoBuf,
    {
        self.data_index = 0;
        self.current_index = ix;
        let n = t.into_buf(&mut self.data);
        self.write_responder = Some(r);

        self.init_download(n)
    }

    fn init_upload(self: &mut Self) -> ClientOutput<N, RR, RW> {
        self.state = ClientState::InitUploading;
        ClientOutput::Output(ClientRequest::InitUpload(self.current_index))
    }

    fn init_download(self: &mut Self, n: usize) -> ClientOutput<N, RR, RW> {
        let req = if n <= 4 {
            self.state = ClientState::InitiateSingleDownload(n);
            let mut data = [0; 4];
            data[0..n].copy_from_slice(&self.data[0..n]);

            ClientRequest::InitSingleSegmentDownload(self.current_index, n as u8, data)
        } else {
            self.state = ClientState::InitiateMultipleDownload(n);
            ClientRequest::InitMultipleDownload(self.current_index, n as u32)
        };

        ClientOutput::Output(req)
    }

    fn output_data(self: &mut Self) -> ClientOutput<N, RR, RW> {
        use crate::sdo::machines::ClientOutput::*;
        use crate::sdo::machines::ClientResult::*;

        let resp = core::mem::replace(&mut self.read_responder, None);
        let res = UploadCompleted(
            self.current_index,
            self.data.clone(),
            self.data_index,
            resp,
        );
        Done(res)
    }

    /// Produces an error output and returns the machine to Idle, releasing
    /// any pending responder. An SDO transfer is single-shot: after any
    /// protocol error the transfer is dead, so the machine must be reusable
    /// for the next transfer without a manual reset.
    fn error(self: &mut Self, e: SdoError) -> ClientOutput<N, RR, RW> {
        self.state = ClientState::Idle;
        self.read_responder = None;
        self.write_responder = None;
        ClientOutput::Error(e)
    }

    fn complete_downloading(self: &mut Self) -> ClientOutput<N, RR, RW> {
        use crate::sdo::machines::ClientOutput::*;
        use crate::sdo::machines::ClientResult::*;

        let resp = core::mem::replace(&mut self.write_responder, None);
        Done(DownloadCompleted(resp))
    }

    fn download_segment(self: &mut Self, t: ToggleBit, len: usize) -> ClientOutput<N, RR, RW> {
        use crate::sdo::machines::ClientOutput::*;
        use crate::sdo::ClientRequest::*;

        self.state = ClientState::DownloadingSegments(t, len);

        // Prepare data segment to download
        let mut data = [0u8; 7];
        let ix0 = self.data_index;
        let ix1 = (ix0 + 7).min(len);
        let end = self.data_index + 7 >= len;

        data[..ix1 - ix0].copy_from_slice(&self.data[ix0..ix1]);
        let valid = (ix1 - ix0) as u8;
        Output(DownloadSegment(t, end, valid, data))
    }

    //-----------------------------------------------------------------------
    // Block transfer (CiA 301 7.2.4.7 - block download, 7.2.4.8 - block upload)
    //-----------------------------------------------------------------------

    /// Initiates a block upload (read): produces the block upload initiate
    /// request frame.
    pub fn read_block(self: &mut Self, ix: CanIndex, r: RR) -> ClientOutput<N, RR, RW> {
        self.data_index = 0;
        self.current_index = ix;
        self.read_responder = Some(r);
        self.block_upload_size = 0;
        self.block_upload_crc = false;
        self.state = ClientState::BlockUploadInitiated;
        ClientOutput::Output(ClientRequest::BlockUploadInitiate(
            ix,
            client_block_size::<N>(),
            0, // protocol switch threshold: never fall back to segmented transfer
        ))
    }

    /// Initiates a block download (write): produces the block download initiate
    /// request frame.
    pub fn write_block<T: IntoBuf>(
        self: &mut Self,
        ix: CanIndex,
        t: T,
        r: RW,
    ) -> ClientOutput<N, RR, RW> {
        self.data_index = 0;
        self.current_index = ix;
        let n = t.into_buf(&mut self.data);
        self.data_index = n;
        self.write_responder = Some(r);
        self.state = ClientState::BlockDownloadInitiated;
        ClientOutput::Output(ClientRequest::BlockDownloadInitiate(ix, n as u32, true))
    }

    /// Emits the next pending segment of the current block download sub-block.
    ///
    /// Returns `None` once the sub-block has been fully transmitted (the
    /// machine is then waiting for the server's block response) or when the
    /// machine is not currently transmitting a sub-block.
    pub fn pump_block(self: &mut Self) -> Option<ClientRequest> {
        self.next_download_segment()
    }

    /// Block size requested for the next sub-block of a block upload, derived
    /// from the amount of free space left in the data buffer.
    fn upload_ack_blksize(self: &Self, position: usize) -> u8 {
        ((N - position) / 7).min(127).max(1) as u8
    }

    /// Processes a raw CAN data field received from the server.
    ///
    /// Block transfer segments carry no command specifier and are therefore
    /// indistinguishable from segmented frames for a stateless decoder; the
    /// machine resolves them using its current state.
    pub fn transit_frame(self: &mut Self, data: [u8; 8]) -> ClientOutput<N, RR, RW> {
        match self.state {
            ClientState::BlockUploadReceiving {
                position, seqno, ..
            } => {
                if data[0] == 0x80 {
                    // abort from the server
                    match ServerResponse::try_from(data) {
                        Ok(response) => self.transit(response),
                        Err(err) => self.error(SdoError::DecodingFailure(err)),
                    }
                } else if position == 0 && seqno == 0 && (data[0] & 0xE3) == 0xC1 {
                    // the end frame of an empty transfer: no segments were
                    // sent, the server skipped straight to the end frame
                    let no_data = (data[0] >> 2) & 0x07;
                    let crc = u16::from_le_bytes([data[1], data[2]]);
                    self.transit(ServerResponse::BlockUploadEnd(no_data, crc))
                } else {
                    let seqno = data[0] & 0x7F;
                    let end = data[0] & 0x80 != 0;
                    let seg: [u8; 7] = data[1..8].try_into().unwrap();
                    self.transit(ServerResponse::BlockUploadSegment(seqno, end, seg))
                }
            }

            ClientState::BlockUploadAwaitingEnd { .. } => {
                if data[0] == 0x80 {
                    match ServerResponse::try_from(data) {
                        Ok(response) => self.transit(response),
                        Err(err) => self.error(SdoError::DecodingFailure(err)),
                    }
                } else if (data[0] & 0xE3) == 0xC1 {
                    let no_data = (data[0] >> 2) & 0x07;
                    let crc = u16::from_le_bytes([data[1], data[2]]);
                    self.transit(ServerResponse::BlockUploadEnd(no_data, crc))
                } else {
                    self.error(SdoError::DecodingFailure(
                        SdoDecodingError::UnknownServerCommandSpecifier(data[0] >> 5),
                    ))
                }
            }

            _ => match ServerResponse::try_from(data) {
                Ok(response) => self.transit(response),
                Err(err) => self.error(SdoError::DecodingFailure(err)),
            },
        }
    }

    /// Emits the next segment of the current block download sub-block and
    /// advances the machine, or returns `None` once the sub-block is complete.
    fn next_download_segment(self: &mut Self) -> Option<ClientRequest> {
        match self.state {
            ClientState::BlockDownloadSending {
                blocksize,
                block_start,
                position,
                seqno,
            } => {
                let total = self.data_index;
                if position >= total {
                    return None;
                }
                let count = (total - position).min(7);
                let end = position + count >= total;
                let no_data = if end { (7 - count) as u8 } else { 0 };

                let mut seg = [0u8; 7];
                seg[..count].copy_from_slice(&self.data[position..position + count]);

                let new_seqno = seqno + 1;
                let new_position = position + count;
                let req = ClientRequest::BlockDownloadSegment(new_seqno, end, seg);

                if new_seqno >= blocksize || end {
                    self.state = ClientState::BlockDownloadAwaitingResponse {
                        block_start,
                        sent: new_seqno,
                        finished: end,
                        no_data,
                    };
                } else {
                    self.state = ClientState::BlockDownloadSending {
                        blocksize,
                        block_start,
                        position: new_position,
                        seqno: new_seqno,
                    };
                }
                Some(req)
            }
            _ => None,
        }
    }
}

/// Finite State Machine implementation
impl<const N: usize, RR, RW> MealyMachine<ServerResponse, ClientOutput<N, RR, RW>>
    for ClientMachine<N, RR, RW>
{
    fn initiate(self: &mut Self) {
        self.state = ClientState::Idle;
        self.data_index = 0;
    }

    fn transit(self: &mut Self, response: ServerResponse) -> ClientOutput<N, RR, RW> {
        use crate::sdo::machines::ClientOutput::*;
        use crate::sdo::machines::ClientState::*;
        use crate::sdo::ClientRequest::*;
        use crate::sdo::ServerResponse::*;

        match (&self.state, response) {
            // ---- Upload Handling ----
            (InitUploading, UploadSingleSegment(res_index, len, data)) => {
                if res_index != self.current_index {
                    self.state = Idle;
                    Error(SdoError::CanIndexMismatch(res_index, self.current_index))
                } else {
                    self.data[self.data_index..self.data_index + 4].copy_from_slice(&data);
                    self.data_index += len as usize;

                    self.state = Idle;
                    self.output_data()
                }
            }

            (InitUploading, ServerResponse::UploadInitMultiples(res_index, _size)) => {
                if res_index != self.current_index {
                    self.state = Idle;
                    Error(SdoError::CanIndexMismatch(res_index, self.current_index))
                } else {
                    //self.data_index = 0;
                    let t = ToggleBit(false);
                    self.state = UploadingMultiples(t);
                    Output(UploadSegment(t))
                }
            }

            (UploadingMultiples(toggle), UploadMultiples(res_toggle, end, len, data)) => {
                if res_toggle != *toggle {
                    self.error(SdoError::ToggleMismatch)
                } else {
                    let idx = self.data_index;
                    let data_len = len as usize;
                    if idx + data_len > self.data.len() {
                        self.error(SdoError::BufferOverflow)
                    } else {
                        self.data[idx..idx + data_len].copy_from_slice(&data[0..data_len]);
                        self.data_index = idx + data_len;
                        if end {
                            self.state = Idle;
                            self.output_data()
                        } else {
                            let new_toggle = !*toggle;
                            self.state = UploadingMultiples(new_toggle);
                            Output(UploadSegment(new_toggle))
                        }
                    }
                }
            }

            // ---- Download Handling ----
            (InitiateSingleDownload(_len), DownloadInitAck(res_index)) => {
                self.state = Idle;
                if res_index != self.current_index {
                    Error(SdoError::CanIndexMismatch(res_index, self.current_index))
                } else {
                    self.complete_downloading()
                }
            }

            (InitiateMultipleDownload(len), DownloadInitAck(res_index)) => {
                if res_index != self.current_index {
                    self.state = Idle;
                    Error(SdoError::CanIndexMismatch(res_index, self.current_index))
                } else {
                    let t = ToggleBit(false);
                    self.download_segment(t, *len)
                }
            }

            (DownloadingSegments(toggle, n), DownloadSegmentAck(res_toggle)) => {
                if res_toggle != *toggle {
                    self.state = Idle;
                    Error(SdoError::ToggleMismatch)
                } else {
                    if self.data_index + 7 < *n {
                        let new_toggle = !*toggle;
                        self.data_index = self.data_index + 7;
                        self.download_segment(new_toggle, *n)
                    } else {
                        self.state = Idle;
                        self.complete_downloading()
                    }
                }
            }

            // ---- Block Download Handling ----
            (BlockDownloadInitiated, BlockDownloadInitiateAck(blksize)) => {
                let blksize = if (1..=127).contains(&blksize) { blksize } else { 127 };
                self.state = BlockDownloadSending {
                    blocksize: blksize,
                    block_start: 0,
                    position: 0,
                    seqno: 0,
                };
                match self.next_download_segment() {
                    Some(req) => Output(req),
                    None => {
                        // empty transfer: finish with an end request immediately
                        self.state = BlockDownloadEnding;
                        Output(BlockDownloadEnd(0, 7))
                    }
                }
            }

            (
                BlockDownloadAwaitingResponse {
                    block_start,
                    sent,
                    finished,
                    no_data,
                },
                BlockDownloadResponse(ackseq, blksize),
            ) => {
                let block_start = *block_start;
                let sent = *sent;
                let finished = *finished;
                let no_data = *no_data;
                let blksize = if (1..=127).contains(&blksize) { blksize } else { 127 };
                if ackseq > sent {
                    self.state = Idle;
                    Error(SdoError::TransferAborted(
                        self.current_index,
                        AbortCode::ClientServerCommandSpecifierNotValidOrUnknown,
                    ))
                } else {
                    // If not all segments were accepted, rewind to the first
                    // not acknowledged one and re-transmit the sub-block.
                    let rewind = block_start + ackseq as usize * 7;
                    self.state = BlockDownloadSending {
                        blocksize: blksize,
                        block_start: rewind,
                        position: rewind,
                        seqno: 0,
                    };

                    if ackseq == sent && finished {
                        // the last sub-block was fully accepted: end the transfer
                        let crc = crc16_ccitt(&self.data[..self.data_index], 0);
                        self.state = BlockDownloadEnding;
                        Output(BlockDownloadEnd(crc, no_data))
                    } else {
                        match self.next_download_segment() {
                            Some(req) => Output(req),
                            None => self.error(SdoError::Busy), // should not happen
                        }
                    }
                }
            }

            (BlockDownloadEnding, BlockDownloadEndAck) => {
                self.state = Idle;
                self.complete_downloading()
            }

            // ---- Block Upload Handling ----
            (BlockUploadInitiated, BlockUploadInitiateAck(ix, size, crc)) => {
                if ix != self.current_index {
                    self.state = Idle;
                    Error(SdoError::CanIndexMismatch(ix, self.current_index))
                } else if size > self.data.len() as u32 {
                    // the data to upload does not fit the client buffer
                    self.state = Idle;
                    Error(SdoError::BufferOverflow)
                } else {
                    self.block_upload_size = size;
                    self.block_upload_crc = crc;
                    self.state = BlockUploadReceiving {
                        blocksize: client_block_size::<N>(),
                        position: 0,
                        seqno: 0,
                    };
                    Output(BlockUploadStart)
                }
            }

            (
                BlockUploadReceiving {
                    blocksize,
                    position,
                    seqno,
                },
                BlockUploadSegment(seqno_in, end, data),
            ) => {
                let blocksize = *blocksize;
                let position = *position;
                let seqno = *seqno;
                let expected = seqno + 1;
                if seqno_in != expected {
                    // Duplicate segment or out-of-sequence: either ignore it or
                    // answer with the sequence number of the last good segment.
                    if seqno_in == seqno || seqno == 0 {
                        NoFrame
                    } else {
                        self.state = BlockUploadReceiving {
                            blocksize,
                            position,
                            seqno: 0,
                        };
                        Output(BlockUploadResponse(seqno, self.upload_ack_blksize(position)))
                    }
                } else if end {
                    // Last segment of the whole transfer: stash it, its valid
                    // length is only known from the end frame.
                    self.upload_last = data;
                    self.state = BlockUploadAwaitingEnd { position };
                    Output(BlockUploadResponse(
                        seqno_in,
                        self.upload_ack_blksize(position),
                    ))
                } else if position + 7 > self.data.len() {
                    // the peer sent more data than the buffer can hold
                    self.error(SdoError::BufferOverflow)
                } else {
                    self.data[position..position + 7].copy_from_slice(&data);
                    let new_position = position + 7;
                    if seqno_in >= blocksize {
                        self.state = BlockUploadReceiving {
                            blocksize,
                            position: new_position,
                            seqno: 0,
                        };
                        Output(BlockUploadResponse(
                            seqno_in,
                            self.upload_ack_blksize(new_position),
                        ))
                    } else {
                        self.state = BlockUploadReceiving {
                            blocksize,
                            position: new_position,
                            seqno: seqno_in,
                        };
                        NoFrame
                    }
                }
            }

            (BlockUploadReceiving { position, .. }, BlockUploadEnd(no_data, crc)) => {
                // The end frame of an empty transfer: the server skipped
                // straight to it because no segments had to be sent.
                let position = *position;
                if no_data != 7 || position != 0 {
                    self.error(SdoError::ClientStateResponseMismatch(
                        ClientState::BlockUploadReceiving {
                            blocksize: 0,
                            position,
                            seqno: 0,
                        },
                        ServerResponse::BlockUploadEnd(no_data, crc),
                    ))
                } else {
                    let total = position;
                    // verify the size indicated by the server, if any
                    let size = self.block_upload_size as usize;
                    if size != 0 && total != size {
                        let code = if total > size {
                            AbortCode::DataTypeDoesNotMatchLengthOfServiceParameterTooHigh
                        } else {
                            AbortCode::DataTypeDoesNotMatchLengthOfServiceParameterTooLow
                        };
                        self.error(SdoError::TransferAborted(self.current_index, code))
                    } else if self.block_upload_crc
                        && crc16_ccitt(&self.data[..total], 0) != crc
                    {
                        self.error(SdoError::TransferAborted(
                            self.current_index,
                            AbortCode::CrcError,
                        ))
                    } else {
                        use crate::sdo::machines::ClientResult::*;
                        self.data_index = total;
                        self.state = Idle;
                        let resp = core::mem::replace(&mut self.read_responder, None);
                        let res = UploadCompleted(self.current_index, self.data, total, resp);
                        FinalOutput(BlockUploadEndAck, res)
                    }
                }
            }

            (BlockUploadAwaitingEnd { position }, BlockUploadEnd(no_data, crc)) => {
                let position = *position;
                let count = 7 - no_data as usize;
                self.data[position..position + count].copy_from_slice(&self.upload_last[..count]);
                let total = position + count;

                // verify the size indicated by the server, if any
                let size = self.block_upload_size as usize;
                if size != 0 && total != size {
                    let code = if total > size {
                        AbortCode::DataTypeDoesNotMatchLengthOfServiceParameterTooHigh
                    } else {
                        AbortCode::DataTypeDoesNotMatchLengthOfServiceParameterTooLow
                    };
                    self.error(SdoError::TransferAborted(self.current_index, code))
                } else if self.block_upload_crc && crc16_ccitt(&self.data[..total], 0) != crc {
                    self.error(SdoError::TransferAborted(
                        self.current_index,
                        AbortCode::CrcError,
                    ))
                } else {
                    use crate::sdo::machines::ClientResult::*;
                    self.data_index = total;
                    self.state = Idle;
                    let resp = core::mem::replace(&mut self.read_responder, None);
                    let res = UploadCompleted(self.current_index, self.data, total, resp);
                    // The client must acknowledge the end frame (CiA 301
                    // 7.2.4.3.12); the acknowledgement and the result travel
                    // together so the caller can send the frame first.
                    FinalOutput(BlockUploadEndAck, res)
                }
            }

            (_, Abort(ix, code)) => {
                self.error(SdoError::TransferAborted(ix, code))
            }
            
            // Default: Unexpected response
            (state, response) => self.error(SdoError::ClientStateResponseMismatch(state.clone(), response)),
        }
    }
}

/// Server states
#[derive(Debug, Clone, Copy)]
pub enum ServerState {
    Idle,
    /// A segmented upload initiate was received; the application must provide
    /// the data to upload via `upload_data`.
    AwaitingData,
    //    UploadingSingleSegment,
    UploadingMultipleSegments {
        response_toggle: ToggleBit,
        position: usize,
    },
    //    DownloadingSingleSegment,
    DownloadingMultipleSegments(ToggleBit, usize),
    // Block transfer states (CiA 301 7.2.4.7 / 7.2.4.8)
    /// Block download initiate was acknowledged, awaiting the client's
    /// segments. `blksize` is the negotiated number of segments per block,
    /// `position` the received data offset, `seqno` the sequence number of the
    /// last received segment.
    BlockDownloadReceiving {
        blksize: u8,
        position: usize,
        seqno: u8,
    },
    /// The final sub-block was acknowledged, awaiting the block download end
    /// request.
    BlockDownloadAwaitingEnd { position: usize },
    /// Block upload initiate was received; the application must provide the
    /// data to upload via `upload_data`.
    BlockUploadAwaitingData { blksize: u8 },
    /// The upload data is buffered and the initiate response sent; awaiting
    /// the client's "start upload" frame.
    BlockUploadReady { blksize: u8 },
    /// The server is emitting the segments of the current sub-block.
    /// `block_start` is the data offset where the sub-block began (used for
    /// retransmission), `position` the offset of the next segment, `seqno` the
    /// number of segments already emitted in this sub-block.
    BlockUploadSending {
        blksize: u8,
        block_start: usize,
        position: usize,
        seqno: u8,
    },
    /// A sub-block was fully sent, awaiting the client's block response.
    BlockUploadAwaitingAck {
        block_start: usize,
        sent: u8,
        finished: bool,
        no_data: u8,
    },
    /// The block upload end frame was sent, awaiting the client's end ack.
    /// Carries the end frame contents for the final output.
    BlockUploadEnding { no_data: u8, crc: u16 },
}

/// Server context
pub struct ServerMachine<const N: usize> {
    index: CanIndex,
    state: ServerState,
    upload_data: [u8; N],
    upload_length: usize,
    download_data: [u8; N],
    download_length: usize,
    download_position: usize,
    /// Whether CRC checking was requested by the client for the block download.
    block_crc_enabled: bool,
    /// Last data segment received during a block download. Its valid length is
    /// only known when the block download end request arrives, so it cannot be
    /// committed to `download_data` immediately (the padding would overflow
    /// the buffer).
    download_last: [u8; 7],
}

/// Number of segments per block the server accepts, derived from the free
/// space in its download buffer (CiA 301: 1..127).
fn server_block_size<const N: usize>(position: usize) -> u8 {
    ((N - position) / 7).min(127).max(1) as u8
}

impl<const N: usize> ServerMachine<N> {
    pub fn is_ready(self: &Self) -> bool {
        match self.state {
            ServerState::Idle => true,
            _ => false,
        }
    }

    /// Produces an error output and returns the machine to Idle, so that the
    /// next transfer can start without a manual reset.
    fn error(self: &mut Self, e: SdoError) -> ServerOutput<N> {
        self.state = ServerState::Idle;
        ServerOutput::Error(e)
    }

    fn continue_uploading(
        self: &mut Self,
        response_toggle: ToggleBit,
        position: usize,
    ) -> ServerOutput<N> {
        let remaining = self.upload_length - position;
        let data_len = remaining.min(7);
        let end = remaining <= 7;
        let mut data = [0; 7];
        data[0..data_len].copy_from_slice(&self.upload_data[position..position + data_len]);
        let response = ServerResponse::UploadMultiples(response_toggle, end, data_len as u8, data);
        if end {
            self.state = ServerState::Idle;
            ServerOutput::FinalOutput(response, ServerResult::UploadCompleted)
        } else {
            // the state tracks the *next* segment to send: the opposite
            // toggle and the offset after the current segment
            self.state = ServerState::UploadingMultipleSegments {
                response_toggle: !response_toggle,
                position: position + data_len,
            };
            ServerOutput::Output(response)
        }
    }

    pub fn upload_data<T>(self: &mut Self, data: &T) -> ServerOutput<N>
    where
        T: IntoBuf
    {
        use crate::sdo::machines::ServerOutput::*;
        use crate::sdo::machines::ServerState::*;
        use crate::sdo::ServerResponse::*;
        //use crate::sdo::ServerResponse::*;
        //use crate::sdo::machines::ClientOutput::*;

        match self.state {
            ServerState::AwaitingData => {
                self.upload_length = data.into_buf(&mut self.upload_data);

                // The expedited/multi-segment decision must be based on the
                // freshly provided data length, not on a flag captured at
                // initiate time (the previous transfer's length is stale).
                if self.upload_length <= 4 {
                    self.state = Idle;

                    let mut data = [0; 4];
                    data[0..self.upload_length]
                        .copy_from_slice(&self.upload_data[0..self.upload_length]);
                    let response = UploadSingleSegment(self.index, self.upload_length as u8, data);
                    ServerOutput::FinalOutput(response, ServerResult::UploadCompleted)
                } else {
                    // multi-segment upload: answer the initiate with the
                    // initiate response first; the segments follow each
                    // upload-segment request
                    self.state = UploadingMultipleSegments {
                        response_toggle: ToggleBit(false),
                        position: 0,
                    };
                    let response = UploadInitMultiples(self.index, self.upload_length as u32);
                    Output(response)
                }
            }

            ServerState::BlockUploadAwaitingData { blksize } => {
                self.upload_length = data.into_buf(&mut self.upload_data);
                let response =
                    BlockUploadInitiateAck(self.index, self.upload_length as u32, true);
                self.state = BlockUploadReady { blksize };
                Output(response)
            }

            _ => self.error(SdoError::Busy),
        }
    }

    /// Emits the next pending segment of the current block upload sub-block.
    ///
    /// Returns `None` once the sub-block has been fully transmitted (the
    /// machine is then waiting for the client's block response) or when the
    /// machine is not currently transmitting a sub-block.
    pub fn pump_block(self: &mut Self) -> Option<ServerResponse> {
        self.next_upload_segment()
    }

    /// Processes a raw CAN data field received from the client.
    ///
    /// Block transfer segments carry no command specifier and are therefore
    /// indistinguishable from segmented frames for a stateless decoder; the
    /// machine resolves them using its current state.
    pub fn transit_frame(self: &mut Self, data: [u8; 8]) -> ServerOutput<N> {
        use crate::sdo::machines::ServerOutput::*;

        match self.state {
            ServerState::BlockDownloadReceiving { seqno, .. } => {
                if data[0] == 0x80 {
                    // abort from the client
                    match ClientRequest::try_from(data) {
                        Ok(req) => self.transit(req),
                        Err(err) => Error(SdoError::DecodingFailure(err)),
                    }
                } else if seqno == 0 && (data[0] & 0xE3) == 0xC1 {
                    // end request of an empty transfer (no segments sent)
                    let no_data = (data[0] >> 2) & 0x07;
                    let crc = u16::from_le_bytes([data[1], data[2]]);
                    self.transit(ClientRequest::BlockDownloadEnd(crc, no_data))
                } else {
                    let seqno_in = data[0] & 0x7F;
                    let end = data[0] & 0x80 != 0;
                    let seg: [u8; 7] = data[1..8].try_into().unwrap();
                    self.transit(ClientRequest::BlockDownloadSegment(seqno_in, end, seg))
                }
            }

            _ => match ClientRequest::try_from(data) {
                Ok(req) => self.transit(req),
                Err(err) => Error(SdoError::DecodingFailure(err)),
            },
        }
    }

    /// Emits the next segment of the current block upload sub-block and
    /// advances the machine, or returns `None` once the sub-block is complete.
    fn next_upload_segment(self: &mut Self) -> Option<ServerResponse> {
        match self.state {
            ServerState::BlockUploadSending {
                blksize,
                block_start,
                position,
                seqno,
            } => {
                let remaining = self.upload_length - position;
                if remaining == 0 {
                    return None;
                }
                let count = remaining.min(7);
                let end = position + count >= self.upload_length;
                let no_data = (7 - count) as u8;

                let mut seg = [0u8; 7];
                seg[..count].copy_from_slice(&self.upload_data[position..position + count]);

                let new_seqno = seqno + 1;
                let new_position = position + count;
                let resp = ServerResponse::BlockUploadSegment(new_seqno, end, seg);

                if new_seqno >= blksize || end {
                    self.state = ServerState::BlockUploadAwaitingAck {
                        block_start,
                        sent: new_seqno,
                        finished: end,
                        no_data,
                    };
                } else {
                    self.state = ServerState::BlockUploadSending {
                        blksize,
                        block_start,
                        position: new_position,
                        seqno: new_seqno,
                    };
                }
                Some(resp)
            }
            _ => None,
        }
    }

    /// Handles the block download end request: commits the stashed last
    /// segment, verifies the CRC and the transfer size and produces the final
    /// download result.
    fn handle_download_end(
        self: &mut Self,
        position: usize,
        no_data: u8,
        crc: u16,
    ) -> ServerOutput<N> {
        use crate::sdo::machines::ServerOutput::*;
        use crate::sdo::machines::ServerResult::*;
        use crate::sdo::ServerResponse::*;

        let count = 7 - no_data as usize;
        if count == 0 || position + count > self.download_data.len() {
            self.state = ServerState::Idle;
            Error(SdoError::BufferOverflow)
        } else {
            // commit the stashed last segment without its padding
            self.download_data[position..position + count]
                .copy_from_slice(&self.download_last[..count]);
            let valid = position + count;
            if self.block_crc_enabled && crc16_ccitt(&self.download_data[..valid], 0) != crc {
                self.state = ServerState::Idle;
                Error(SdoError::TransferAborted(self.index, AbortCode::CrcError))
            } else if self.download_length != 0 && valid != self.download_length {
                self.state = ServerState::Idle;
                let code = if valid > self.download_length {
                    AbortCode::DataTypeDoesNotMatchLengthOfServiceParameterTooHigh
                } else {
                    AbortCode::DataTypeDoesNotMatchLengthOfServiceParameterTooLow
                };
                Error(SdoError::TransferAborted(self.index, code))
            } else {
                self.download_position = valid;
                let response = BlockDownloadEndAck;
                let result = DownloadCompleted(self.index, self.download_data.clone(), valid);
                self.state = ServerState::Idle;
                FinalOutput(response, result)
            }
        }
    }
}

/// Possible final result that server produces
#[derive(Debug)]
pub enum ServerResult<const N: usize> {
    UploadCompleted,
    DownloadCompleted(CanIndex, [u8; N], usize),
}

/// All observations of server machine
#[derive(Debug)]
pub enum ServerOutput<const N: usize> {
    Output(ServerResponse),
    FinalOutput(ServerResponse, ServerResult<N>),
    Data(CanIndex),
    Error(SdoError),
    /// The machine processed an input but has nothing to send. Used by block
    /// transfer for frames that are acknowledged internally (e.g. duplicate
    /// segments) or that only advance the transfer state.
    NoFrame,
}

impl<const N: usize> Default for ServerMachine<N> {
    fn default() -> Self {
        ServerMachine {
            index: CanIndex::new(0, 0),
            state: ServerState::Idle,
            upload_data: [0; N],
            upload_length: 0,
            download_data: [0; N],
            download_length: 0,
            download_position: 0,
            block_crc_enabled: false,
            download_last: [0; 7],
        }
    }
}

impl<const N: usize> MealyMachine<ClientRequest, ServerOutput<N>> for ServerMachine<N> {
    fn initiate(self: &mut Self) {
        self.state = ServerState::Idle;
        self.download_position = 0;
    }

    fn transit(self: &mut Self, request: ClientRequest) -> ServerOutput<N> {
        use crate::sdo::machines::ServerOutput::*;
        use crate::sdo::machines::ServerResult::*;
        use crate::sdo::machines::ServerState::*;
        use crate::sdo::ClientRequest::*;
        use crate::sdo::ServerResponse::*;

        match (&self.state, request) {
            (Idle, InitUpload(index)) => {
                self.index = index;
                self.state = AwaitingData;
                Data(self.index)
            }

            (
                UploadingMultipleSegments {
                    response_toggle,
                    position,
                    ..
                },
                ClientRequest::UploadSegment(toggle),
            ) => {
                if toggle != *response_toggle {
                    self.error(SdoError::ToggleMismatch)
                } else {
                    // the request announces the next segment: emit it with
                    // the stored toggle at the stored offset
                    self.continue_uploading(*response_toggle, *position)
                }
            }

            (Idle, InitSingleSegmentDownload(index, len, data)) => {
                self.index = index;
                self.download_data[0..len as usize].copy_from_slice(&data[0..len as usize]);
                self.download_length = len as usize;
                let response = DownloadInitAck(index);
                let result =
                    DownloadCompleted(self.index, self.download_data.clone(), self.download_length);

                FinalOutput(response, result)
            }

            (Idle, InitMultipleDownload(index, length)) => {
                self.index = index;
                let length = length as usize;
                if length > self.download_data.len() {
                    self.error(SdoError::BufferOverflow)
                } else {
                    self.download_length = length;
                    self.download_position = 0;
                    let toggle = ToggleBit(false);
                    self.state = DownloadingMultipleSegments(toggle, 0);
                    // the initiate response, not a segment acknowledgement
                    let response = DownloadInitAck(index);
                    Output(response)
                }
            }

            (
                DownloadingMultipleSegments(expected_toggle, position),
                DownloadSegment(toggle, end, len, data),
            ) => {
                if toggle != *expected_toggle {
                    self.error(SdoError::ToggleMismatch)
                } else {
                    let data_len = len as usize;
                    let new_position = position + data_len;
                    if new_position > self.download_length {
                        self.error(SdoError::BufferOverflow)
                    } else if end && new_position != self.download_length {
                        // the client signalled the end before the declared
                        // size was transferred
                        self.state = ServerState::Idle;
                        Error(SdoError::TransferAborted(
                            self.index,
                            AbortCode::DataTypeDoesNotMatchLengthOfServiceParameterTooLow,
                        ))
                    } else {
                        self.download_data[*position..new_position]
                            .copy_from_slice(&data[0..data_len]);
                        self.download_position = new_position;
                        // the acknowledgement echoes the toggle bit of the
                        // received segment (CiA 301 7.2.4.3.4)
                        let response = ServerResponse::DownloadSegmentAck(toggle);
                        if end {
                            self.state = ServerState::Idle;
                            let result = DownloadCompleted(
                                self.index,
                                self.download_data.clone(),
                                new_position,
                            );
                            FinalOutput(response, result)
                        } else {
                            self.state = ServerState::DownloadingMultipleSegments(
                                !*expected_toggle,
                                new_position,
                            );
                            Output(response)
                        }
                    }
                }
            }

            // ---- Block Download Handling ----
            (Idle, BlockDownloadInitiate(ix, size, crc)) => {
                if size > self.download_data.len() as u32 {
                    Error(SdoError::BufferOverflow)
                } else {
                    self.index = ix;
                    self.block_crc_enabled = crc;
                    self.download_length = size as usize;
                    self.download_position = 0;
                    let blksize = server_block_size::<N>(0);
                    self.state = BlockDownloadReceiving {
                        blksize,
                        position: 0,
                        seqno: 0,
                    };
                    Output(BlockDownloadInitiateAck(blksize))
                }
            }

            (
                BlockDownloadReceiving {
                    blksize,
                    position,
                    seqno,
                },
                BlockDownloadSegment(seqno_in, end, data),
            ) => {
                let blksize = *blksize;
                let position = *position;
                let seqno = *seqno;
                let expected = seqno + 1;
                if seqno_in != expected {
                    // Duplicate segment or out-of-sequence: either ignore it or
                    // answer with the sequence number of the last good segment.
                    // The client re-transmits from there, renumbered from 1.
                    if seqno_in == seqno || seqno == 0 {
                        NoFrame
                    } else {
                        self.state = BlockDownloadReceiving {
                            blksize,
                            position,
                            seqno: 0,
                        };
                        Output(BlockDownloadResponse(seqno, blksize))
                    }
                } else if end {
                    // last segment of the whole transfer: stash it, its valid
                    // length is only known from the end request
                    self.download_last = data;
                    self.state = BlockDownloadAwaitingEnd { position };
                    Output(BlockDownloadResponse(seqno_in, blksize))
                } else if position + 7 > self.download_data.len() {
                    // the peer sent more data than the buffer can hold
                    self.error(SdoError::BufferOverflow)
                } else {
                    self.download_data[position..position + 7].copy_from_slice(&data);
                    let new_position = position + 7;
                    if seqno_in >= blksize {
                        let new_blksize = server_block_size::<N>(new_position);
                        self.state = BlockDownloadReceiving {
                            blksize: new_blksize,
                            position: new_position,
                            seqno: 0,
                        };
                        Output(BlockDownloadResponse(seqno_in, new_blksize))
                    } else {
                        self.state = BlockDownloadReceiving {
                            blksize,
                            position: new_position,
                            seqno: seqno_in,
                        };
                        NoFrame
                    }
                }
            }

            (BlockDownloadAwaitingEnd { position }, BlockDownloadEnd(crc, no_data)) => {
                self.handle_download_end(*position, no_data, crc)
            }

            // end request of an empty transfer (no segments were sent)
            (BlockDownloadReceiving { position, .. }, BlockDownloadEnd(crc, no_data)) => {
                self.handle_download_end(*position, no_data, crc)
            }

            // ---- Block Upload Handling ----
            (Idle, BlockUploadInitiate(ix, blksize, _pst)) => {
                if !(1..=127).contains(&blksize) {
                    Error(SdoError::TransferAborted(ix, AbortCode::InvalidBlockSize))
                } else {
                    self.index = ix;
                    self.state = BlockUploadAwaitingData { blksize };
                    Data(self.index)
                }
            }

            (BlockUploadReady { blksize }, BlockUploadStart) => {
                let blksize = *blksize;
                self.state = BlockUploadSending {
                    blksize,
                    block_start: 0,
                    position: 0,
                    seqno: 0,
                };
                match self.next_upload_segment() {
                    Some(resp) => Output(resp),
                    None => {
                        // empty upload: send the end frame immediately
                        self.state = BlockUploadEnding { no_data: 7, crc: 0 };
                        Output(BlockUploadEnd(7, 0))
                    }
                }
            }

            (
                BlockUploadAwaitingAck {
                    block_start,
                    sent,
                    finished,
                    no_data,
                },
                BlockUploadResponse(ackseq, blksize),
            ) => {
                let block_start = *block_start;
                let sent = *sent;
                let finished = *finished;
                let no_data = *no_data;
                if !(1..=127).contains(&blksize) {
                    self.state = Idle;
                    Error(SdoError::TransferAborted(self.index, AbortCode::InvalidBlockSize))
                } else if ackseq > sent {
                    self.state = Idle;
                    Error(SdoError::TransferAborted(
                        self.index,
                        AbortCode::ClientServerCommandSpecifierNotValidOrUnknown,
                    ))
                } else {
                    // If not all segments were accepted, rewind to the first
                    // not acknowledged one and re-transmit the sub-block.
                    let rewind = block_start + ackseq as usize * 7;
                    self.state = BlockUploadSending {
                        blksize,
                        block_start: rewind,
                        position: rewind,
                        seqno: 0,
                    };
                    if ackseq == sent && finished {
                        // the last sub-block was fully accepted: end the transfer
                        let crc = crc16_ccitt(&self.upload_data[..self.upload_length], 0);
                        self.state = BlockUploadEnding { no_data, crc };
                        Output(BlockUploadEnd(no_data, crc))
                    } else {
                        match self.next_upload_segment() {
                            Some(resp) => Output(resp),
                            None => self.error(SdoError::Busy), // should not happen
                        }
                    }
                }
            }

            (BlockUploadEnding { no_data, crc }, BlockUploadEndAck) => {
                let no_data = *no_data;
                let crc = *crc;
                self.state = Idle;
                FinalOutput(
                    ServerResponse::BlockUploadEnd(no_data, crc),
                    ServerResult::UploadCompleted,
                )
            }

            (_, AbortTransfer(ix, code)) => {
                self.state = Idle;
                Error(SdoError::TransferAborted(ix, code))
            }

            (state, response) => {
                self.error(SdoError::ServerStateResponseMismatch(state.clone(), response))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdo_upload_u32_value() {
        let mut client: ClientMachine<1024, (), ()> = ClientMachine::default();
        let mut server: ServerMachine<1024> = ServerMachine::default();

        let index = CanIndex {
            base: 0x6068,
            sub: 01,
        };

        let value: u32 = 5077;

        let fake_responder = ();

        let mut client_out = client.read(index, fake_responder);

        let mut gasoline = 10;

        while gasoline > 0 {
            let out = core::mem::replace(&mut client_out, ClientOutput::Error(SdoError::Busy));
            match out {
                ClientOutput::Output(req) => {
                    let server_out = server.transit(req);
                    match server_out {
                        ServerOutput::Output(resp) => {
                            client_out = client.transit(resp.clone());
                        }
                        ServerOutput::Data(sindex) => {
                            if sindex == index {
                                if let ServerOutput::FinalOutput(resp, result) =
                                    server.upload_data(&value)
                                {
                                    client_out = client.transit(resp);
                                    if let ServerResult::UploadCompleted = result {
                                        continue;
                                    } else {
                                        panic!("Wrong final upload result: {:?}", result);
                                    }
                                } else {
                                    panic!("Server state mismatch");
                                }
                            } else {
                                panic!("CanIndex mismatch");
                            }
                        }

                        ServerOutput::FinalOutput(resp, result) => {
                            client_out = client.transit(resp.clone());
                            if let ServerResult::UploadCompleted = result {
                                continue;
                            } else {
                                panic!("Wrong final upload result: {:?}", result);
                            }
                        }
 
                        ServerOutput::Error(err) => {
                            panic!("Server error: {:?}", err);
                        }

                        ServerOutput::NoFrame => {
                            panic!("Unexpected NoFrame!");
                        }
                    }
                }

                ClientOutput::Done(res) => match res {
                    ClientResult::UploadCompleted(_i, data, n, _) => {
                        assert_eq!(n, 4);
                        let uploaded_value =
                            u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                        assert_eq!(uploaded_value, value);
                        break;
                    }

                    _ => panic!("Wrong result!"),
                },

                ClientOutput::Error(err) => {
                    panic!("Client error: {:?}", err);
                }

                
                ClientOutput::TransferCompleted => {
                    break;
                }

                ClientOutput::FinalOutput(..) => {
                    panic!("Unexpected FinalOutput!");
                }

                ClientOutput::NoFrame => {
                    panic!("Unexpected NoFrame!");
                }
                
            }

            gasoline = gasoline - 1;
        }

        if gasoline == 0 {
            panic!("SDO exchange is stuck!");
        }
    }

    #[test]
    fn sdo_download_u32_value() {
        let mut client: ClientMachine<1024, (), ()> = ClientMachine::default();
        let mut server: ServerMachine<1024> = ServerMachine::default();

        let index = CanIndex {
            base: 0x6068,
            sub: 0x04,
        };
        let value: u32 = 0x55aa;

        let fake_responder = ();

        let mut client_out = client.write(index, value, fake_responder);

        let mut gasoline = 10;

        while gasoline > 0 {
            let out = core::mem::replace(&mut client_out, ClientOutput::Error(SdoError::Busy));

            match out {
                ClientOutput::Output(req) => {
                    let server_out = server.transit(req);

                    match server_out {
                        ServerOutput::Output(resp) => {
                            client_out = client.transit(resp);
                        }
                        ServerOutput::Data(_) => {
                            panic!("State mismatch");
                        }

                        ServerOutput::FinalOutput(resp, result) => {
                            client_out = client.transit(resp.clone());

                            if let ServerResult::DownloadCompleted(dindex, data, n) = result {
                                assert_eq!(index, dindex);
                                assert_eq!(n, 4);
                                let downloaded_value =
                                    u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                                assert_eq!(downloaded_value, value);
                            } else {
                                panic!("Wrong result download result: {:?}", result);
                            }
                        }

                        ServerOutput::Error(err) => {
                            panic!("Server error: {:?}", err);
                        }

                        ServerOutput::NoFrame => {
                            panic!("Unexpected NoFrame!");
                        }
                    }
                }

                ClientOutput::Done(res) => match res {
                    ClientResult::DownloadCompleted(_) => {
                        break;
                    }

                    _ => panic!("Wrong result!"),
                },

                ClientOutput::Error(err) => {
                    panic!("Client error: {:?}", err);
                }
                
                ClientOutput::TransferCompleted => {
                    break;
                }

                ClientOutput::FinalOutput(..) => {
                    panic!("Unexpected FinalOutput!");
                }

                ClientOutput::NoFrame => {
                    panic!("Unexpected NoFrame!");
                }
            }

            gasoline = gasoline - 1;
        }

        if gasoline == 0 {
            panic!("SDO exchange is stuck!");
        }
    }

    #[test]
    fn sdo_download_u16_value() {
        let mut client: ClientMachine<1024, (), ()> = ClientMachine::default();
        let mut server: ServerMachine<1024> = ServerMachine::default();

        let index = CanIndex {
            base: 0x60c0,
            sub: 0x02,
        };
        let value: u16 = 0xa5;

        let fake_responder = ();

        let mut client_out = client.write(index, value, fake_responder);

        let mut gasoline = 10;

        while gasoline > 0 {
            let out = core::mem::replace(&mut client_out, ClientOutput::Error(SdoError::Busy));

            match out {
                ClientOutput::Output(req) => {
                    let server_out = server.transit(req);

                    match server_out {
                        ServerOutput::Output(resp) => {
                            client_out = client.transit(resp);
                        }
                        ServerOutput::Data(_) => {
                            panic!("State mismatch");
                        }

                        ServerOutput::FinalOutput(resp, result) => {
                            client_out = client.transit(resp.clone());

                            if let ServerResult::DownloadCompleted(dindex, data, n) = result {
                                assert_eq!(index, dindex);
                                assert_eq!(n, 2);
                                let downloaded_value =
                                    u16::from_le_bytes([data[0], data[1]]);
                                assert_eq!(downloaded_value, value);
                            } else {
                                panic!("Wrong result download result: {:?}", result);
                            }
                        }

                        ServerOutput::Error(err) => {
                            panic!("Server error: {:?}", err);
                        }

                        ServerOutput::NoFrame => {
                            panic!("Unexpected NoFrame!");
                        }
                    }
                }

                ClientOutput::Done(res) => match res {
                    ClientResult::DownloadCompleted(_) => {
                        break;
                    }

                    _ => panic!("Wrong result!"),
                },

                ClientOutput::Error(err) => {
                    panic!("Client error: {:?}", err);
                }
                
                ClientOutput::TransferCompleted => {
                    break;
                }

                ClientOutput::FinalOutput(..) => {
                    panic!("Unexpected FinalOutput!");
                }

                ClientOutput::NoFrame => {
                    panic!("Unexpected NoFrame!");
                }
            }

            gasoline = gasoline - 1;
        }

        if gasoline == 0 {
            panic!("SDO exchange is stuck!");
        }
    }

    //----------------------------Block transfer tests----------------------------//

    /// Drives a complete block download exchange between a client and a server
    /// machine and returns the data received by the server.
    fn drive_block_download<const N: usize>(
        client: &mut ClientMachine<N, (), ()>,
        server: &mut ServerMachine<N>,
        index: CanIndex,
        mut client_out: ClientOutput<N, (), ()>,
    ) -> std::vec::Vec<u8> {
        use crate::sdo::machines::ServerOutput::*;

        let mut fuel = 200;
        loop {
            fuel -= 1;
            if fuel == 0 {
                panic!("Block download exchange is stuck!");
            }
            match client_out {
                ClientOutput::Output(req) => match server.transit(req) {
                    Output(resp) => {
                        client_out = client.transit(resp);
                    }
                    FinalOutput(resp, result) => {
                        let received = match result {
                            ServerResult::DownloadCompleted(dindex, data, n) => {
                                assert_eq!(dindex, index);
                                data[..n].to_vec()
                            }
                            ServerResult::UploadCompleted => panic!("Wrong server result!"),
                        };
                        client_out = client.transit(resp);
                        if let ClientOutput::Done(_) = client_out {
                            return received;
                        }
                        panic!("Client did not finish after the end acknowledgement!");
                    }
                    NoFrame => {
                        // mid-block segment accepted: send the next one
                        match client.pump_block() {
                            Some(req) => client_out = ClientOutput::Output(req),
                            None => panic!("Pump returned None in the middle of a sub-block!"),
                        }
                    }
                    Data(_) => panic!("Unexpected Data output!"),
                    Error(err) => panic!("Server error: {:?}", err),
                },
                ClientOutput::Done(res) => match res {
                    ClientResult::DownloadCompleted(_) => panic!("Unexpected client finish!"),
                    _ => panic!("Wrong client result!"),
                },
                ClientOutput::FinalOutput(..) => panic!("Unexpected FinalOutput from the client!"),
                ClientOutput::Error(err) => panic!("Client error: {:?}", err),
                ClientOutput::TransferCompleted => panic!("Unexpected TransferCompleted!"),
                ClientOutput::NoFrame => panic!("Unexpected NoFrame from the client!"),
            }
        }
    }

    /// Drives a complete block upload exchange between a client and a server
    /// machine and returns the data received by the client.
    fn drive_block_upload<const N: usize, T: IntoBuf>(
        client: &mut ClientMachine<N, (), ()>,
        server: &mut ServerMachine<N>,
        index: CanIndex,
        payload: &T,
        mut client_out: ClientOutput<N, (), ()>,
    ) -> std::vec::Vec<u8> {
        use crate::sdo::machines::ServerOutput::*;

        let mut fuel = 200;
        loop {
            fuel -= 1;
            if fuel == 0 {
                panic!("Block upload exchange is stuck!");
            }
            match client_out {
                ClientOutput::Output(req) => match server.transit(req) {
                    Data(sindex) => {
                        assert_eq!(sindex, index);
                        match server.upload_data(payload) {
                            Output(resp) => client_out = client.transit(resp),
                            out => panic!("Unexpected server output: {:?}", out),
                        }
                    }
                    Output(resp) => {
                        client_out = client.transit(resp);
                        // the server may now be emitting a sub-block: pump the
                        // remaining segments into the client
                        while let Some(seg) = server.pump_block() {
                            client_out = client.transit(seg);
                            if !matches!(client_out, ClientOutput::NoFrame) {
                                break;
                            }
                        }
                    }
                    FinalOutput(resp, result) => {
                        // The server's upload-completion output is its
                        // reaction to the client's end acknowledgement, which
                        // is sent from the client-FinalOutput arm below; the
                        // repeated end frame must not be re-fed into the
                        // client. Unreachable here.
                        let _ = (resp, result);
                        panic!("Unexpected server final output in the main loop!");
                    }
                    NoFrame => panic!("Unexpected NoFrame from the server!"),
                    Error(err) => panic!("Server error: {:?}", err),
                },
                ClientOutput::FinalOutput(req, res) => {
                    // app: send the end acknowledgement, then the server
                    // completes and the client result carries the data
                    match server.transit(req) {
                        FinalOutput(_, ServerResult::UploadCompleted) => {}
                        o => panic!("Unexpected server reaction to the end ack: {:?}", o),
                    }
                    match res {
                        ClientResult::UploadCompleted(_ix, data, n, _) => {
                            return data[..n].to_vec();
                        }
                        _ => panic!("Wrong client result!"),
                    }
                }
                ClientOutput::Done(res) => match res {
                    ClientResult::UploadCompleted(_ix, data, n, _) => {
                        return data[..n].to_vec();
                    }
                    _ => panic!("Wrong client result!"),
                },
                ClientOutput::Error(err) => panic!("Client error: {:?}", err),
                ClientOutput::TransferCompleted => panic!("Unexpected TransferCompleted!"),
                ClientOutput::NoFrame => panic!("Unexpected NoFrame from the client!"),
            }
        }
    }


    #[test]
    fn block_download_u32_value() {
        let mut client: ClientMachine<1024, (), ()> = ClientMachine::default();
        let mut server: ServerMachine<1024> = ServerMachine::default();

        let index = CanIndex { base: 0x6068, sub: 0x01 };
        let value: u32 = 0xDEAD_BEEF;

        let init = client.write_block(index, value, ());
        let received = drive_block_download(&mut client, &mut server, index, init);
        assert_eq!(received, value.to_le_bytes().to_vec());
    }

    #[test]
    fn block_download_multi_block() {
        let mut client: ClientMachine<20, (), ()> = ClientMachine::default();
        let mut server: ServerMachine<20> = ServerMachine::default();

        let index = CanIndex { base: 0x6068, sub: 0x01 };
        // 20 bytes -> 3 segments; the server negotiates 2 segments per block
        let payload: [u8; 20] = [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19,
        ];

        let init = client.write_block(index, payload, ());
        let received = drive_block_download(&mut client, &mut server, index, init);
        assert_eq!(received, payload.to_vec());
    }

    #[test]
    fn block_download_server_retransmission() {
        let mut server: ServerMachine<64> = ServerMachine::default();

        let index = CanIndex { base: 0x6068, sub: 0x01 };
        let out = server.transit(ClientRequest::BlockDownloadInitiate(index, 21, true));
        assert!(matches!(
            out,
            ServerOutput::Output(ServerResponse::BlockDownloadInitiateAck(_))
        ));

        // segment 1 received
        assert!(matches!(
            server.transit(ClientRequest::BlockDownloadSegment(1, false, [1u8; 7])),
            ServerOutput::NoFrame
        ));
        // segment 3 arrives out of sequence (segment 2 was lost): the server
        // acknowledges only up to segment 1
        match server.transit(ClientRequest::BlockDownloadSegment(3, false, [3u8; 7])) {
            ServerOutput::Output(ServerResponse::BlockDownloadResponse(ackseq, _)) => {
                assert_eq!(ackseq, 1)
            }
            out => panic!("Expected block response, got {:?}", out),
        }
        // the client re-transmits from segment 1, renumbered; the segments
        // carry the data of the original segments 2 and 3, the last one with
        // the end flag (21 bytes = 3 full segments)
        assert!(matches!(
            server.transit(ClientRequest::BlockDownloadSegment(1, false, [2u8; 7])),
            ServerOutput::NoFrame
        ));
        assert!(matches!(
            server.transit(ClientRequest::BlockDownloadSegment(2, true, [3u8; 7])),
            ServerOutput::Output(ServerResponse::BlockDownloadResponse(2, _))
        ));

        // the received data is [1;7] ++ [2;7] ++ [3;7] (21 bytes)
        let mut expected = [0u8; 21];
        expected[0..7].copy_from_slice(&[1u8; 7]);
        expected[7..14].copy_from_slice(&[2u8; 7]);
        expected[14..21].copy_from_slice(&[3u8; 7]);
        let crc = crc16_ccitt(&expected, 0);

        let out = server.transit(ClientRequest::BlockDownloadEnd(crc, 0));
        match out {
            ServerOutput::FinalOutput(
                ServerResponse::BlockDownloadEndAck,
                ServerResult::DownloadCompleted(dindex, data, n),
            ) => {
                assert_eq!(dindex, index);
                assert_eq!(n, 21);
                assert_eq!(&data[..n], &expected);
            }
            out => panic!("Expected final download output, got {:?}", out),
        }
    }


    #[test]
    fn block_download_client_retransmission() {
        let mut client: ClientMachine<64, (), ()> = ClientMachine::default();

        let index = CanIndex { base: 0x6068, sub: 0x01 };
        // 14 bytes -> 2 segments of 7 bytes; the last one carries the end flag
        let payload: [u8; 14] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
        client.write_block(index, payload, ());

        // the server negotiated 2 segments per block
        let out = client.transit(ServerResponse::BlockDownloadInitiateAck(2));
        let seg1 = match out {
            ClientOutput::Output(req) => req,
            _ => panic!("Expected first segment!"),
        };
        assert!(matches!(seg1, ClientRequest::BlockDownloadSegment(1, false, _)));
        let seg2 = client.pump_block().expect("second segment");
        if let ClientRequest::BlockDownloadSegment(2, end, data) = seg2 {
            assert!(end);
            assert_eq!(&data[..7], &payload[7..14]);
        } else {
            panic!("Expected second segment!");
        }
        assert!(client.pump_block().is_none());

        // the server acknowledges only segment 1: the client must re-transmit
        // from segment 1, carrying the data of the original segment 2
        let out = client.transit(ServerResponse::BlockDownloadResponse(1, 2));
        let seg1r = match out {
            ClientOutput::Output(req) => req,
            _ => panic!("Expected re-transmitted segment!"),
        };
        if let ClientRequest::BlockDownloadSegment(1, end, data) = seg1r {
            assert!(end);
            assert_eq!(&data[..7], &payload[7..14]);
        } else {
            panic!("Expected re-transmitted segment!");
        }
        assert!(client.pump_block().is_none());

        // now the server acknowledges the whole sub-block: the transfer ends
        let out = client.transit(ServerResponse::BlockDownloadResponse(1, 2));
        let req = match out {
            ClientOutput::Output(req) => req,
            _ => panic!("Expected end request!"),
        };
        if let ClientRequest::BlockDownloadEnd(crc, no_data) = req {
            assert_eq!(no_data, 0);
            assert_eq!(crc, crc16_ccitt(&payload, 0));
        } else {
            panic!("Expected block download end request!");
        }

        let out = client.transit(ServerResponse::BlockDownloadEndAck);
        assert!(matches!(out, ClientOutput::Done(ClientResult::DownloadCompleted(_))));
        let _ = index;
    }

    #[test]
    fn block_download_crc_mismatch() {
        let mut server: ServerMachine<64> = ServerMachine::default();

        let index = CanIndex { base: 0x6068, sub: 0x01 };
        server.transit(ClientRequest::BlockDownloadInitiate(index, 4, true));

        assert!(matches!(
            server.transit(ClientRequest::BlockDownloadSegment(1, true, [1, 2, 3, 4, 0, 0, 0])),
            ServerOutput::Output(ServerResponse::BlockDownloadResponse(1, _))
        ));

        // wrong CRC in the end request
        let out = server.transit(ClientRequest::BlockDownloadEnd(0xFFFF, 3));
        assert!(matches!(
            out,
            ServerOutput::Error(SdoError::TransferAborted(_, AbortCode::CrcError))
        ));
    }

    #[test]
    fn block_upload_u32_value() {
        let mut client: ClientMachine<1024, (), ()> = ClientMachine::default();
        let mut server: ServerMachine<1024> = ServerMachine::default();

        let index = CanIndex { base: 0x6068, sub: 0x01 };
        let value: u32 = 0xCAFE_F00D;

        let init = client.read_block(index, ());
        let received = drive_block_upload(&mut client, &mut server, index, &value.to_le_bytes(), init);
        assert_eq!(received, value.to_le_bytes().to_vec());
    }

    #[test]
    fn block_upload_multi_block() {
        let mut client: ClientMachine<20, (), ()> = ClientMachine::default();
        let mut server: ServerMachine<20> = ServerMachine::default();

        let index = CanIndex { base: 0x6068, sub: 0x01 };
        // 20 bytes -> 3 segments; the client requests 2 segments per block
        let payload: [u8; 20] = [
            20, 19, 18, 17, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1,
        ];

        let init = client.read_block(index, ());
        let received = drive_block_upload(&mut client, &mut server, index, &payload, init);
        assert_eq!(received, payload.to_vec());
    }

    #[test]
    fn block_upload_crc_mismatch() {
        let mut client: ClientMachine<64, (), ()> = ClientMachine::default();

        let index = CanIndex { base: 0x6068, sub: 0x01 };
        client.read_block(index, ());

        let out = client.transit(ServerResponse::BlockUploadInitiateAck(index, 4, true));
        assert!(matches!(out, ClientOutput::Output(ClientRequest::BlockUploadStart)));

        let out = client.transit(ServerResponse::BlockUploadSegment(1, true, [1, 2, 3, 4, 0, 0, 0]));
        assert!(matches!(
            out,
            ClientOutput::Output(ClientRequest::BlockUploadResponse(1, _))
        ));

        // wrong CRC in the end frame
        let out = client.transit(ServerResponse::BlockUploadEnd(3, 0xFFFF));
        assert!(matches!(
            out,
            ClientOutput::Error(SdoError::TransferAborted(_, AbortCode::CrcError))
        ));
    }

    #[test]
    fn block_upload_segment_raw_decode() {
        let mut client: ClientMachine<64, (), ()> = ClientMachine::default();

        let index = CanIndex { base: 0x6068, sub: 0x01 };
        client.read_block(index, ());
        client.transit(ServerResponse::BlockUploadInitiateAck(index, 14, true));

        // raw frame with seqno 1: accepted, no output
        let mut data = [0u8; 8];
        data[0] = 1;
        data[1..8].copy_from_slice(&[9u8; 7]);
        assert!(matches!(client.transit_frame(data), ClientOutput::NoFrame));

        // raw frame with seqno 2 and the end flag: produces the block response
        data[0] = 0x80 | 2;
        assert!(matches!(
            client.transit_frame(data),
            ClientOutput::Output(ClientRequest::BlockUploadResponse(2, _))
        ));

        // raw end frame: finishes the transfer; the client acknowledges the
        // end frame and reports the result together
        let mut end = [0u8; 8];
        end[0] = 0xC1 | (0 << 2); // no_data = 0
        end[1..3].copy_from_slice(&crc16_ccitt(&[9u8; 14], 0).to_le_bytes());
        let out = client.transit_frame(end);
        match out {
            ClientOutput::FinalOutput(ClientRequest::BlockUploadEndAck, result) => {
                assert!(matches!(
                    result,
                    ClientResult::UploadCompleted(_, _, 14, _)
                ));
            }
            o => panic!("Expected the end acknowledgement output, got {:?}", o),
        }
    }

    #[test]
    fn block_download_segment_raw_decode() {
        let mut server: ServerMachine<64> = ServerMachine::default();

        let index = CanIndex { base: 0x6068, sub: 0x01 };
        server.transit(ClientRequest::BlockDownloadInitiate(index, 14, true));

        // raw frame with seqno 1: accepted, no output
        let mut data = [0u8; 8];
        data[0] = 1;
        assert!(matches!(server.transit_frame(data), ServerOutput::NoFrame));

        // raw frame with seqno 2 and the end flag: produces the block response
        data[0] = 0x80 | 2;
        assert!(matches!(
            server.transit_frame(data),
            ServerOutput::Output(ServerResponse::BlockDownloadResponse(2, _))
        ));

        // raw end request: finishes the transfer with the received data
        let mut end = [0u8; 8];
        end[0] = 0xC1 | (0 << 2);
        end[1..3].copy_from_slice(&crc16_ccitt(&[0u8; 14], 0).to_le_bytes());
        let out = server.transit_frame(end);
        match out {
            ServerOutput::FinalOutput(
                ServerResponse::BlockDownloadEndAck,
                ServerResult::DownloadCompleted(dindex, data, n),
            ) => {
                assert_eq!(dindex, index);
                assert_eq!(n, 14);
                assert_eq!(&data[..n], &[0u8; 14]);
            }
            out => panic!("Expected final download output, got {:?}", out),
        }
    }
}
