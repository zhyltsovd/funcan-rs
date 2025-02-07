// use crate::interfaces::*;
use crate::machine::*;
use crate::sdo::*;

#[derive(Debug, PartialEq, Clone)]
pub enum ClientRequest {
    InitUpload(Index),
    UploadSegment(ToggleBit),
    InitSingleSegmentDownload(Index, u8, [u8; 4]), // index, length, data
    InitMultipleDownload(Index, u32),              // index and length,
    DownloadSegment(ToggleBit, bool, u8, [u8; 7]), // toogle bit, end bit, length, data
    AbortTransfer(Index, AbortCode),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServerResponse {
    UploadSingleSegment(Index, u8, [u8; 4]),
    UploadInitMultiples(Index, u32),
    UploadMultiples(ToggleBit, bool, u8, [u8; 7]),
    DownloadInitAck(Index),
    DownloadSegmentAck(ToggleBit),
}

/// Possible errors during SDO communications
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    StateResponseMismatch,
    IndexMismatch(Index, Index),
    TransferAborted(AbortCode),
    ToggleMismatch,
    BufferOverflow,
}

/// Client states
enum ClientState {
    Idle,
    InitUpload,
    SingleSegmentUploaded,
    UploadingMultiples(ToggleBit),
    MultiplesUploaded,

    InitSingleDownload(usize),
    InitMultipleDownload(usize),
    DownloadingSegments(ToggleBit, usize),
    DownloadCompleted,

    ErrorState(Error),
}

/// Client context
pub struct ClientMachine<RR, RW> {
    index: Index,
    state: ClientState,
    data_index: usize,
    read_responder: Option<RR>,
    write_responder: Option<RW>,
    data: [u8; 1024],
}

/// Possible final result that machine produces
pub enum ClientResult<RR, RW> {
    UploadCompleted(Index, [u8; 1024], usize, Option<RR>),
    DownloadCompleted(Option<RW>),
    TransferAborted(AbortCode),
}

/// All possible observations of client machine
pub enum ClientOutput<RR, RW> {
    Output(ClientRequest),
    Done(ClientResult<RR, RW>),
    Error(Error),
    Ready,
}

impl<RR, RW> ClientOutput<RR, RW> {
    pub fn is_ready(self: &Self) -> bool {
        match self {
            ClientOutput::Output(_) => false,
            _ => true,
        }
    }
}

impl<RR, RW> Default for ClientMachine<RR, RW> {
    fn default() -> Self {
        ClientMachine {
            read_responder: None,
            write_responder: None,
            index: Index::new(0, 0),
            state: ClientState::Idle,
            data_index: 0,
            data: [0; 1024],
        }
    }
}

impl<RR, RW> ClientMachine<RR, RW> {
    /// Initiates SDO read
    pub fn read(self: &mut Self, index: Index, r: RR) {
        self.index = index;
        self.read_responder = Some(r);
        self.state = ClientState::InitUpload;
    }

    /// Initiates SDO write
    pub fn write<T>(self: &mut Self, index: Index, t: T, r: RW)
    where
        T: IntoBuf,
    {
        self.index = index;
        let n = t.into_buf(&mut self.data);
        self.write_responder = Some(r);
        if n <= 4 {
            self.state = ClientState::InitSingleDownload(n);
        } else {
            self.state = ClientState::InitMultipleDownload(n);
        };
    }
}

/// Finite State Machine implementation
impl<RR, RW> MachineTrans<ServerResponse> for ClientMachine<RR, RW> {
    type Observation = Option<ClientOutput<RR, RW>>;

    fn initial(self: &mut Self) {
        self.state = ClientState::Idle;
        self.data_index = 0;
    }

    fn transit(self: &mut Self, response: ServerResponse) {
        match (&self.state, response) {
            // ---- Upload Handling ----
            // InitUpload -> UploadSingleSegment
            (
                ClientState::InitUpload,
                ServerResponse::UploadSingleSegment(res_index, len, data),
            ) => {
                if res_index != self.index {
                    self.state =
                        ClientState::ErrorState(Error::IndexMismatch(res_index, self.index));
                } else {
                    self.data[0..4].copy_from_slice(&data);
                    self.data_index = len as usize;
                    self.state = ClientState::SingleSegmentUploaded;
                }
            }

            // InitUpload -> InitMupltipleSegments
            (ClientState::InitUpload, ServerResponse::UploadInitMultiples(res_index, _size)) => {
                if res_index != self.index {
                    self.state =
                        ClientState::ErrorState(Error::IndexMismatch(res_index, self.index));
                } else {
                    self.data_index = 0;
                    self.state = ClientState::UploadingMultiples(ToggleBit(false));
                }
            }

            // UploadingMultiples -> UploadMupltipleSegments
            (
                ClientState::UploadingMultiples(toggle),
                ServerResponse::UploadMultiples(res_toggle, end, len, data),
            ) => {
                if res_toggle != *toggle {
                    self.state = ClientState::ErrorState(Error::ToggleMismatch);
                } else {
                    let idx = self.data_index;
                    let data_len = len as usize;
                    if idx + data_len > self.data.len() {
                        self.state = ClientState::ErrorState(Error::BufferOverflow);
                    } else {
                        self.data[idx..idx + data_len].copy_from_slice(&data[0..data_len]);
                        self.data_index = idx + data_len;
                        if end {
                            self.state = ClientState::MultiplesUploaded;
                        } else {
                            let new_toggle = !*toggle;
                            self.state = ClientState::UploadingMultiples(new_toggle);
                        }
                    }
                }
            }

            // ---- Download Handling ----
            // InitDownload -> DownloadInitAck (single segment)
            (ClientState::InitSingleDownload(_len), ServerResponse::DownloadInitAck(res_index)) => {
                if res_index != self.index {
                    self.state =
                        ClientState::ErrorState(Error::IndexMismatch(res_index, self.index));
                } else {
                    self.state = ClientState::DownloadCompleted
                }
            }

            // InitDownload -> DownloadInitAck (multi-segment)
            (
                ClientState::InitMultipleDownload(len),
                ServerResponse::DownloadInitAck(res_index),
            ) => {
                if res_index != self.index {
                    self.state =
                        ClientState::ErrorState(Error::IndexMismatch(res_index, self.index));
                } else {
                    self.state = ClientState::DownloadingSegments(ToggleBit(false), *len);
                }
            }

            (
                ClientState::DownloadingSegments(toggle, n),
                ServerResponse::DownloadSegmentAck(res_toggle),
            ) => {
                if res_toggle != *toggle {
                    self.state = ClientState::ErrorState(Error::ToggleMismatch);
                } else {
                    if self.data_index + 7 < *n {
                        self.state = ClientState::DownloadingSegments(!*toggle, *n);
                        self.data_index = self.data_index + 7;
                    } else {
                        self.state = ClientState::DownloadCompleted;
                    }
                }
            }

            // Default: Unexpected response
            (_state, _response) => {
                self.state = ClientState::ErrorState(Error::StateResponseMismatch);
            }
        };
    }

    fn observe(&mut self) -> Self::Observation {
        match &self.state {
            ClientState::Idle => Some(ClientOutput::Ready),

            ClientState::InitUpload => {
                Some(ClientOutput::Output(ClientRequest::InitUpload(self.index)))
            }

            ClientState::SingleSegmentUploaded => {
                let resp = core::mem::replace(&mut self.read_responder, None);
                Some(ClientOutput::Done(ClientResult::UploadCompleted(
                    self.index,
                    self.data.clone(),
                    self.data_index,
                    resp,
                )))
            }

            ClientState::UploadingMultiples(toggle) => {
                Some(ClientOutput::Output(ClientRequest::UploadSegment(*toggle)))
            }

            ClientState::MultiplesUploaded => {
                let resp = core::mem::replace(&mut self.read_responder, None);

                Some(ClientOutput::Done(ClientResult::UploadCompleted(
                    self.index,
                    self.data.clone(),
                    self.data_index,
                    resp,
                )))
            }

            ClientState::InitSingleDownload(len) => {
                let mut data = [0; 4];
                data.copy_from_slice(&self.data[0..*len]);

                Some(ClientOutput::Output(
                    ClientRequest::InitSingleSegmentDownload(self.index, *len as u8, data),
                ))
            }

            ClientState::InitMultipleDownload(len) => Some(ClientOutput::Output(
                ClientRequest::InitMultipleDownload(self.index, *len as u32),
            )),

            ClientState::DownloadingSegments(toggle, n) => {
                // Prepare data segment to download
                let mut data = [0u8; 7];
                let ix0 = self.data_index;
                let ix1 = (ix0 + 7).min(*n);
                let end = self.data_index + 7 >= *n;

                data.copy_from_slice(&self.data[ix0..ix1]);
                Some(ClientOutput::Output(ClientRequest::DownloadSegment(
                    *toggle, end, 7, data,
                )))
            }

            ClientState::DownloadCompleted => {
                let resp = core::mem::replace(&mut self.write_responder, None);
                Some(ClientOutput::Done(ClientResult::DownloadCompleted(resp)))
            }

            ClientState::ErrorState(err) => Some(ClientOutput::Error(err.clone())),
        }
    }
}

/*

/// Server states
enum ServerState {
    Idle,
    UploadingSingleSegment,
    UploadingMultipleSegments {
        response_toggle: ToggleBit,
        expected_next_toggle: ToggleBit,
        position: usize,
    },
    DownloadingSingleSegment,
    DownloadingMultipleSegments(ToggleBit, usize),
    ErrorState(Error),
}

/// Server context
pub struct ServerMachine<RR, RW> {
    index: Index,
    state: ServerState,
    upload_data: [u8; 1024],
    upload_length: usize,
    download_data: [u8; 1024],
    download_length: usize,
    download_position: usize,
    read_responder: Option<RR>,
    write_responder: Option<RW>,
}

/// Possible final result that server produces
pub enum ServerResult<RR, RW> {
    UploadCompleted(Option<RR>),
    DownloadCompleted(Index, [u8; 1024], usize, Option<RW>),
    TransferAborted(AbortCode),
}

/// All observations of server machine
pub enum ServerOutput<RR, RW> {
    Output(ServerResponse),
    Done(ServerResult<RR, RW>),
    Error(Error),
    Ready,
}

impl<RR, RW> Default for ServerMachine<RR, RW> {
    fn default() -> Self {
        ServerMachine {
            index: Index::new(0, 0),
            state: ServerState::Idle,
            upload_data: [0; 1024],
            upload_length: 0,
            download_data: [0; 1024],
            download_length: 0,
            download_position: 0,
            read_responder: None,
            write_responder: None,
        }
    }
}

impl<RR, RW> MachineTrans<ClientRequest> for ServerMachine<RR, RW> {
    type Observation = Option<ServerOutput<RR, RW>>;

    fn initial(&mut self) {
        self.state = ServerState::Idle;
        self.download_position = 0;
    }

    fn transit(&mut self, request: ClientRequest) {
        match (&self.state, request) {
            (ServerState::Idle, ClientRequest::InitUpload(index)) => {
                self.index = index;
                if self.upload_length <= 4 {
                    self.state = ServerState::UploadingSingleSegment;
                } else {
                    self.state = ServerState::UploadingMultipleSegments {
                        response_toggle: ToggleBit(false),
                        expected_next_toggle: ToggleBit(true),
                        position: 0,
                    };
                }
            }

            (
                ServerState::UploadingMultipleSegments {
                    expected_next_toggle,
                    position,
                    ..
                },
                ClientRequest::UploadSegment(toggle),
            ) => {
                if toggle != *expected_next_toggle {
                    self.state = ServerState::ErrorState(Error::ToggleMismatch);
                } else {
                    let new_position = position + 7;
                    self.state = ServerState::UploadingMultipleSegments {
                        response_toggle: toggle,
                        expected_next_toggle: !toggle,
                        position: new_position,
                    };
                }
            }

            (ServerState::Idle, ClientRequest::InitSingleSegmentDownload(index, len, data)) => {
                self.index = index;
                if len as usize > self.download_data.len() {
                    self.state = ServerState::ErrorState(Error::BufferOverflow);
                } else {
                    self.download_data[0..len as usize].copy_from_slice(&data[0..len as usize]);
                    self.download_length = len as usize;
                    self.state = ServerState::DownloadingSingleSegment;
                }
            }

            (ServerState::Idle, ClientRequest::InitMultipleDownload(index, length)) => {
                self.index = index;
                let length = length as usize;
                if length > self.download_data.len() {
                    self.state = ServerState::ErrorState(Error::BufferOverflow);
                } else {
                    self.download_length = length;
                    self.download_position = 0;
                    self.state = ServerState::DownloadingMultipleSegments(ToggleBit(false), 0);
                }
            }

            (
                ServerState::DownloadingMultipleSegments(expected_toggle, position),
                ClientRequest::DownloadSegment(toggle, end, len, data),
            ) => {
                if toggle != *expected_toggle {
                    self.state = ServerState::ErrorState(Error::ToggleMismatch);
                } else {
                    let data_len = len as usize;
                    let new_position = position + data_len;
                    if new_position > self.download_length {
                        self.state = ServerState::ErrorState(Error::BufferOverflow);
                    } else {
                        self.download_data[position..new_position].copy_from_slice(&data[0..data_len]);
                        self.download_position = new_position;
                        self.state = if end {
                            ServerState::Idle
                        } else {
                            ServerState::DownloadingMultipleSegments(!expected_toggle, new_position)
                        };
                    }
                }
            }

            _ => {
                self.state = ServerState::ErrorState(Error::StateResponseMismatch);
            }
        }
    }

    fn observe(&mut self) -> Self::Observation {
        match &self.state {
            ServerState::Idle => Some(ServerOutput::Ready),

            ServerState::UploadingSingleSegment => {
                let mut data = [0; 4];
                data[0..self.upload_length].copy_from_slice(&self.upload_data[0..self.upload_length]);
                let response = ServerResponse::UploadSingleSegment(self.index, self.upload_length as u8, data);
                self.state = ServerState::Idle;
                Some(ServerOutput::Output(response))
            }

            ServerState::UploadingMultipleSegments {
                response_toggle,
                position,
                ..
            } => {
                let remaining = self.upload_length - position;
                let data_len = remaining.min(7);
                let end = remaining <= 7;
                let mut data = [0; 7];
                data[0..data_len].copy_from_slice(&self.upload_data[*position..position + data_len]);
                let response = ServerResponse::UploadMultiples(*response_toggle, end, data_len as u8, data);
                if end {
                    self.state = ServerState::Idle;
                }
                Some(ServerOutput::Output(response))
            }

            ServerState::DownloadingSingleSegment => {
                let response = ServerResponse::DownloadInitAck(self.index);
                self.state = ServerState::Idle;
                Some(ServerOutput::Output(response))
            }

            ServerState::DownloadingMultipleSegments(toggle, _) => {
                let response = ServerResponse::DownloadSegmentAck(*toggle);
                Some(ServerOutput::Output(response))
            }

            ServerState::ErrorState(err) => {
                let err = err.clone();
                self.state = ServerState::Idle;
                Some(ServerOutput::Error(err))
            }
        }
    }
}
*/
