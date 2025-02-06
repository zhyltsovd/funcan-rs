// use crate::interfaces::*;
use crate::machine::*;
use crate::sdo::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    StateResponseMismatch,
    IndexMismatch(Index, Index),
    TransferAborted(AbortCode),
    ToggleMismatch,
    BufferOverflow,
    ProtocolError,
}

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

pub struct ClientMachine<RR, RW> {
    index: Index,
    state: ClientState,
    data_index: usize,
    read_responder: Option<RR>,
    write_responder: Option<RW>,
    data: [u8; 1024],
}

pub enum ClientResult<RR, RW> {
    UploadCompleted(Index, [u8; 1024], usize, Option<RR>),
    DownloadCompleted(Option<RW>),
    TransferAborted(AbortCode),
}

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
    pub fn read(self: &mut Self, index: Index, r: RR) {
        self.index = index;
        self.read_responder = Some(r);
        self.state = ClientState::InitUpload;
    }

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
