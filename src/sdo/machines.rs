use crate::interfaces::*;
use crate::machine::*;
use crate::sdo::{Error as SdoDecodingError};
use crate::sdo::*;
use heapless::vec::*;

/// Possible errors during SDO communications
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdoError {
    StateResponseMismatch,
    CanIndexMismatch(CanIndex, CanIndex),
    TransferAborted(AbortCode),
    ToggleMismatch,
    BufferOverflow,
    Busy,
    DecodingFailure(SdoDecodingError),
    NoResponder
}

/// Client states
enum ClientState {
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
    current_mode: Vec<u8, 254>,
    state: ClientState,
    data_index: usize,
    read_responder: Option<RR>,
    write_responder: Option<RW>,
    data: [u8; N],
}

/// Possible final result that machine produces
#[derive(Debug)]
pub enum ClientResult<const N: usize, RR, RW> {
    UploadCompleted(CanBaseIndex, [u8; N], usize, Option<RR>),
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
            current_mode: Vec::new(),
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
    pub fn read(self: &mut Self, indices: CanIndices, r: RR) -> ClientOutput<N, RR, RW> {
        self.data_index = 0;
        self.current_index = indices.base_index.into();
        self.current_mode = indices.can_type.is_compound();
        self.read_responder = Some(r);
        self.init_upload()
    }

    /// Initiates SDO write
    pub fn write<T>(self: &mut Self, indices: CanIndices, t: T, r: RW) -> ClientOutput<N, RR, RW>
    where
        T: IntoBuf,
    {
        self.data_index = 0;
        self.current_index = indices.base_index.into();
        self.current_mode = indices.can_type.is_compound();
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
            data.copy_from_slice(&self.data[0..n]);

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
            self.current_index.into(),
            self.data.clone(),
            self.data_index,
            resp,
        );
        Done(res)
    }

    fn continue_uploading(self: &mut Self) -> ClientOutput<N, RR, RW> {
        use crate::sdo::machines::ClientState::*;

        let _ = self.current_mode.pop();
        if self.current_mode.len() == 0 {
            self.state = Idle;
            self.output_data()
        } else {
            self.current_index.inc_sub();
            self.init_upload()
        }
    }

    fn continue_downloading(self: &mut Self) -> ClientOutput<N, RR, RW> {
        use crate::sdo::machines::ClientState::*;

        let _ = self.current_mode.pop();
        if self.current_mode.len() == 0 {
            self.state = Idle;
            self.complete_downloading()
        } else {
            self.current_index.inc_sub();
            self.init_download(self.current_mode[0] as usize)
        }
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

                    self.continue_uploading()
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
                            self.continue_uploading()
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
                        self.continue_downloading()
                    }
                }
            }

            // Default: Unexpected response
            (_state, _response) => Error(SdoError::StateResponseMismatch),
        }
    }
}

/// Server states
enum ServerState {
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

            (_, _) => {
                todo!()
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

        let base_index = CanBaseIndex(0x6068);
        let index = CanIndices {
            base_index: base_index,
            can_type: CanType::Base(4),
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
                            if sindex.base == base_index.0 {
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
    fn sdo_upload_struct_value() {
        let mut client: ClientMachine<1024, (), ()> = ClientMachine::default();
        let mut server: ServerMachine<1024> = ServerMachine::default();

        let mut types = Vec::<_, 254>::new();
        types.push(1).unwrap();
        types.push(2).unwrap();
        types.push(1).unwrap();

        let base_index = CanBaseIndex(0x6068);
        let index = CanIndices {
            base_index: base_index,
            can_type: CanType::Struct(types),
        };

        let value = TestStruct0 {
            p0: 0x11,
            p1: 0x55aa,
            p2: 0x22,
        };

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
                            //println!("{:?}", client_out);
                        }
                        ServerOutput::Data(sindex) => {
                            if sindex.base == base_index.0 {
                                let r = match sindex.sub {
                                    0 => server.upload_data(&[value.p0]),

                                    1 => server.upload_data(&value.p1.to_le_bytes()),

                                    2 => server.upload_data(&[value.p2]),

                                    n => {
                                        panic!("Unknown sub: {}", n);
                                    }
                                };

                                match r {
                                    ServerOutput::FinalOutput(resp, result) => {
                                        client_out = client.transit(resp);
                                        if let ServerResult::UploadCompleted = result {
                                            continue;
                                        } else {
                                            panic!("Wrong final upload result: {:?}", result);
                                        }
                                    }

                                    ServerOutput::Output(resp) => {
                                        client_out = client.transit(resp);
                                    }

                                    out => {
                                        panic!("Wrong output while uploading: {:?}", out);
                                    }
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

                        let p0 = data[0];
                        let p1 = u16::from_le_bytes([data[1], data[2]]);
                        let p2 = data[3];
                        let uploaded_value = TestStruct0 { p0, p1, p2 };

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

        let base_index = CanBaseIndex(0x6068);
        let index = CanIndices {
            base_index: base_index,
            can_type: CanType::Base(4),
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
                                assert_eq!(base_index.0, dindex.base);
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

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct TestStruct0 {
        p0: u8,
        p1: u16,
        p2: u8,
    }

    impl IntoBuf for TestStruct0 {
        fn into_buf<'a>(self: &'a Self, buf: &'a mut [u8]) -> usize {
            buf[0] = self.p0;
            buf[1..3].copy_from_slice(&self.p1.to_le_bytes());
            buf[3] = self.p2;
            4
        }
    }

    #[test]
    fn sdo_download_struct_value() {
        let mut client: ClientMachine<1024, (), ()> = ClientMachine::default();
        let mut server: ServerMachine<1024> = ServerMachine::default();

        let mut types = Vec::<_, 254>::new();
        types.push(1).unwrap();
        types.push(2).unwrap();
        types.push(1).unwrap();

        let base_index = CanBaseIndex(0x6068);
        let index = CanIndices {
            base_index: base_index,
            can_type: CanType::Struct(types),
        };

        let value = TestStruct0 {
            p0: 0x11,
            p1: 0x55aa,
            p2: 0x22,
        };

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
                                assert_eq!(base_index.0, dindex.base);
                                assert_eq!(n, 4);

                                let p0 = data[0];
                                let p1 = u16::from_le_bytes([data[1], data[2]]);
                                let p2 = data[3];
                                let downloaded_value = TestStruct0 { p0, p1, p2 };
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
