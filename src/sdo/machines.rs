use crate::interfaces::*;
use crate::machine::*;
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
#[derive(Debug, Clone)]
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
}

/// Client context
pub struct ClientMachine<const N: usize, RR, RW> {
    current_index: CanIndex,
    state: ClientState,
    data_index: usize,
    read_responder: Option<RR>,
    write_responder: Option<RW>,
    data: [u8; N],
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
    TransferCompleted,
    Error(SdoError),
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

        data.copy_from_slice(&self.data[ix0..ix1]);
        Output(DownloadSegment(t, end, 7, data))
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
                    Error(SdoError::ToggleMismatch)
                } else {
                    let idx = self.data_index;
                    let data_len = len as usize;
                    if idx + data_len > self.data.len() {
                        Error(SdoError::BufferOverflow)
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

            (_, Abort(ix, code)) => {
                Error(SdoError::TransferAborted(ix, code))
            }
            
            // Default: Unexpected response
            (state, response) => Error(SdoError::ClientStateResponseMismatch(state.clone(), response)),
        }
    }
}

/// Server states
#[derive(Debug, Clone)]
pub enum ServerState {
    Idle,
    AwaitingData(bool),
    //    UploadingSingleSegment,
    UploadingMultipleSegments {
        response_toggle: ToggleBit,
        position: usize,
    },
    //    DownloadingSingleSegment,
    DownloadingMultipleSegments(ToggleBit, usize),
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
}

impl<const N: usize> ServerMachine<N> {
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
            self.state = ServerState::UploadingMultipleSegments {
                response_toggle: response_toggle,
                position: position,
            };
            ServerOutput::Output(response)
        }
    }

    pub fn upload_data(self: &mut Self, data: &[u8]) -> ServerOutput<N> {
        use crate::sdo::machines::ServerOutput::*;
        use crate::sdo::machines::ServerState::*;
        use crate::sdo::ServerResponse::*;
        //use crate::sdo::ServerResponse::*;
        //use crate::sdo::machines::ClientOutput::*;

        if let ServerState::AwaitingData(b) = self.state {
            let n = data.len();
            self.upload_length = n;
            self.upload_data[0..n].copy_from_slice(data);

            if b {
                self.state = Idle;

                let mut data = [0; 4];
                data[0..self.upload_length]
                    .copy_from_slice(&self.upload_data[0..self.upload_length]);
                let response = UploadSingleSegment(self.index, self.upload_length as u8, data);
                //Output(response)
                ServerOutput::FinalOutput(response, ServerResult::UploadCompleted)
            } else {
                let response_toggle = ToggleBit(false);
                self.continue_uploading(response_toggle, 0)
            }
        } else {
            Error(SdoError::Busy)
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
                let b = self.upload_length <= 4;
                self.state = AwaitingData(b);
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
                    Error(SdoError::ToggleMismatch)
                } else {
                    let new_position = position + 7;
                    self.continue_uploading(!toggle, new_position)
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
                    Error(SdoError::BufferOverflow)
                } else {
                    self.download_length = length;
                    self.download_position = 0;
                    let toggle = ToggleBit(false);
                    self.state = DownloadingMultipleSegments(toggle, 0);
                    let response = DownloadSegmentAck(toggle);
                    Output(response)
                }
            }

            (
                DownloadingMultipleSegments(expected_toggle, position),
                DownloadSegment(toggle, end, len, data),
            ) => {
                if toggle != *expected_toggle {
                    Error(SdoError::ToggleMismatch)
                } else {
                    let data_len = len as usize;
                    let new_position = position + data_len;
                    if new_position > self.download_length {
                        Error(SdoError::BufferOverflow)
                    } else {
                        self.download_data[*position..new_position]
                            .copy_from_slice(&data[0..data_len]);
                        self.download_position = new_position;
                        self.state = if end {
                            ServerState::Idle
                        } else {
                            ServerState::DownloadingMultipleSegments(
                                !*expected_toggle,
                                new_position,
                            )
                        };

                        let response = ServerResponse::DownloadSegmentAck(!toggle);
                        Output(response)
                    }
                }
            }

            (state, response) => {
                Error(SdoError::ServerStateResponseMismatch(state.clone(), response))
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
                                let data: [u8; 4] = value.to_le_bytes();
                                if let ServerOutput::FinalOutput(resp, result) =
                                    server.upload_data(&data)
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
            }

            gasoline = gasoline - 1;
        }

        if gasoline == 0 {
            panic!("SDO exchange is stuck!");
        }
    }
}
