//! Audit test suite for the SDO client/server machines and facades.
//!
//! Focus areas (per the audit request):
//! - multiple consecutive transfers on the same machines (state leakage);
//! - multi-segment (non-expedited) transfers in both directions;
//! - block transfer edge cases: empty transfers, exact buffer fits, sweeps
//!   over many sizes, multi-block flows;
//! - error paths: aborts, toggles, index mismatches, oversized peers — and
//!   recovery (the machine must return to Idle so the next transfer works);
//! - codec round-trips, including the server abort frame.

#![cfg(test)]

use std::cell::RefCell;
use std::rc::Rc;
use std::vec::Vec;

use crate::dictionary::*;
use crate::interfaces::*;
use crate::machine::*;
use crate::raw::*;
use crate::sdo::abort::*;
use crate::sdo::client::*;
use crate::sdo::machines::*;
use crate::sdo::server::*;
use crate::sdo::*;

//---------------------------------------------------------------------------------------------------
// Test infrastructure
//---------------------------------------------------------------------------------------------------

/// Captures the value passed to a `OneshotResponder`, so tests can assert on
/// the completion callback.
#[derive(Clone, Debug)]
struct Cap<T>(Rc<RefCell<Option<T>>>);

impl<T> Cap<T> {
    fn new() -> Self {
        Cap(Rc::new(RefCell::new(None)))
    }
    fn get(&self) -> Option<T>
    where
        T: Clone,
    {
        self.0.borrow().clone()
    }
}

impl<T> OneshotResponder<T> for Cap<T>
where
    T: Clone,
{
    fn respond(self, x: T) -> Result<(), T> {
        *self.0.borrow_mut() = Some(x);
        Ok(())
    }
}

/// `IntoBuf` adapter over an arbitrary byte slice (the crate only implements
/// `IntoBuf` for u8/u16/u32/[u8; K], which cannot cover a sweep of sizes).
struct SliceBuf<'a>(&'a [u8]);

impl<'a> IntoBuf for SliceBuf<'a> {
    fn into_buf(&self, buf: &mut [u8]) -> usize {
        assert!(buf.len() >= self.0.len());
        buf[..self.0.len()].copy_from_slice(self.0);
        self.0.len()
    }
}

/// Drives a segmented (or block) upload: client reads `index`, server uploads
/// `payload`. Returns the bytes the client received.
fn drive_upload<const N: usize, T: IntoBuf>(
    client: &mut ClientMachine<N, (), ()>,
    server: &mut ServerMachine<N>,
    index: CanIndex,
    payload: &T,
) -> Vec<u8> {
    let mut out = client.read(index, ());
    let mut fuel = 200;
    loop {
        fuel -= 1;
        assert!(fuel > 0, "segmented upload exchange stuck");
        match out {
            ClientOutput::Output(req) => match server.transit(req) {
                ServerOutput::Output(resp) => out = client.transit(resp),
                ServerOutput::Data(si) => {
                    assert_eq!(si, index);
                    match server.upload_data(payload) {
                        ServerOutput::Output(resp) => out = client.transit(resp),
                        ServerOutput::FinalOutput(resp, ServerResult::UploadCompleted) => {
                            out = client.transit(resp)
                        }
                        o => panic!("Unexpected server output: {:?}", o),
                    }
                }
                ServerOutput::FinalOutput(resp, ServerResult::UploadCompleted) => {
                    out = client.transit(resp)
                }
                o => panic!("Unexpected server output: {:?}", o),
            },
            ClientOutput::Done(ClientResult::UploadCompleted(_, data, n, _)) => {
                return data[..n].to_vec();
            }
            ClientOutput::FinalOutput(..) => {
                panic!("Unexpected final output in a segmented upload!")
            }
            o => panic!("Unexpected client output: {:?}", o),
        }
    }
}

/// Drives a segmented (or block) download: client writes `payload`, server
/// receives it. Returns the bytes the server received.
fn drive_download<const N: usize, T: IntoBuf>(
    client: &mut ClientMachine<N, (), ()>,
    server: &mut ServerMachine<N>,
    index: CanIndex,
    payload: T,
) -> Vec<u8> {
    let mut out = client.write(index, payload, ());
    let mut fuel = 200;
    loop {
        fuel -= 1;
        assert!(fuel > 0, "segmented download exchange stuck");
        match out {
            ClientOutput::Output(req) => match server.transit(req) {
                ServerOutput::Output(resp) => out = client.transit(resp),
                ServerOutput::FinalOutput(resp, ServerResult::DownloadCompleted(_, data, n)) => {
                    out = client.transit(resp);
                    match out {
                        ClientOutput::Done(ClientResult::DownloadCompleted(_)) => {
                            return data[..n].to_vec();
                        }
                        o => panic!("Client did not finish the download: {:?}", o),
                    }
                }
                o => panic!("Unexpected server output: {:?}", o),
            },
            ClientOutput::Done(ClientResult::DownloadCompleted(_)) => {
                panic!("Client finished before the server reported the result!");
            }
            ClientOutput::FinalOutput(..) => {
                panic!("Unexpected final output in a segmented download!")
            }
            o => panic!("Unexpected client output: {:?}", o),
        }
    }
}

/// Drives a complete block download exchange and returns the server-received data.
fn drive_block_download<const N: usize>(
    client: &mut ClientMachine<N, (), ()>,
    server: &mut ServerMachine<N>,
    index: CanIndex,
    mut client_out: ClientOutput<N, (), ()>,
) -> Vec<u8> {
    use crate::sdo::machines::ServerOutput::*;
    let mut fuel = 300;
    loop {
        fuel -= 1;
        assert!(fuel > 0, "block download exchange stuck");
        match client_out {
            ClientOutput::Output(req) => match server.transit(req) {
                Output(resp) => client_out = client.transit(resp),
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
                NoFrame => match client.pump_block() {
                    Some(req) => client_out = ClientOutput::Output(req),
                    None => panic!("Pump returned None in the middle of a sub-block!"),
                },
                o => panic!("Unexpected server output: {:?}", o),
            },
            ClientOutput::Done(_) => panic!("Unexpected client finish!"),
            o => panic!("Unexpected client output: {:?}", o),
        }
    }
}

/// Drives a complete block upload exchange and returns the client-received data.
fn drive_block_upload<const N: usize, T: IntoBuf>(
    client: &mut ClientMachine<N, (), ()>,
    server: &mut ServerMachine<N>,
    index: CanIndex,
    payload: &T,
    mut client_out: ClientOutput<N, (), ()>,
) -> Vec<u8> {
    use crate::sdo::machines::ServerOutput::*;
    let mut fuel = 300;
    loop {
        fuel -= 1;
        assert!(fuel > 0, "block upload exchange stuck");
        match client_out {
            ClientOutput::Output(req) => match server.transit(req) {
                Data(sindex) => {
                    assert_eq!(sindex, index);
                    match server.upload_data(payload) {
                        Output(resp) => {
                            client_out = client.transit(resp);
                            while let Some(seg) = server.pump_block() {
                                client_out = client.transit(seg);
                                if !matches!(client_out, ClientOutput::NoFrame) {
                                    break;
                                }
                            }
                        }
                        o => panic!("Unexpected server output: {:?}", o),
                    }
                }
                Output(resp) => {
                    client_out = client.transit(resp);
                    while let Some(seg) = server.pump_block() {
                        client_out = client.transit(seg);
                        if !matches!(client_out, ClientOutput::NoFrame) {
                            break;
                        }
                    }
                }
                FinalOutput(resp, result) => {
                    // the server's final output is its response to the client's
                    // end acknowledgement; nothing further needs to be sent
                    if let ServerResult::UploadCompleted = result {
                        client_out = client.transit(resp);
                        continue;
                    }
                    panic!("Wrong server result!");
                }
                o => panic!("Unexpected server output: {:?}", o),
            },
            ClientOutput::FinalOutput(req, res) => {
                // app: send the end acknowledgement, then the transfer is done
                match server.transit(req) {
                    FinalOutput(_, ServerResult::UploadCompleted) => {}
                    o => panic!("Unexpected server reaction to the end ack: {:?}", o),
                }
                match res {
                    ClientResult::UploadCompleted(_ix, data, n, _) => return data[..n].to_vec(),
                    _ => panic!("Wrong client result!"),
                }
            }
            ClientOutput::Done(_) => panic!("Unexpected client finish!"),
            o => panic!("Unexpected client output: {:?}", o),
        }
    }
}

/// App-side dispatch of a client result (used where the facade passes
/// `FinalOutput` through: the app sends the frame, then handles the result).
fn dispatch_result<const N: usize>(
    res: ClientResult<N, Cap<TObject>, Cap<()>>,
) -> Option<TObject> {
    match res {
        ClientResult::UploadCompleted(ix, data, len, maybe_r) => {
            let index = TIndex::try_from(ix).ok()?;
            let x = TObject::try_from((index, &data[..len])).ok()?;
            if let Some(r) = maybe_r {
                let _ = r.respond(x);
            }
            Some(x)
        }
        ClientResult::DownloadCompleted(_) => None,
    }
}

//---------------------------------------------------------------------------------------------------
// Test object dictionary
//---------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TIndex {
    Speed, // 0x6068:01, u32
    Blob,  // 0x2400:01, [u8; 10]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TObject {
    Speed(u32),
    Blob([u8; 10]),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TDict {
    speed: u32,
    blob: [u8; 10],
}

impl Dictionary for TDict {
    type Index = TIndex;
    type Object = TObject;

    fn set(&mut self, x: TObject) {
        match x {
            TObject::Speed(v) => self.speed = v,
            TObject::Blob(b) => self.blob = b,
        }
    }

    fn get(&self, ix: &TIndex) -> TObject {
        match ix {
            TIndex::Speed => TObject::Speed(self.speed),
            TIndex::Blob => TObject::Blob(self.blob),
        }
    }
}

impl TryFrom<CanIndex> for TIndex {
    type Error = ();
    fn try_from(ix: CanIndex) -> Result<Self, ()> {
        match (ix.base, ix.sub) {
            (0x6068, 1) => Ok(TIndex::Speed),
            (0x2400, 1) => Ok(TIndex::Blob),
            _ => Err(()),
        }
    }
}

impl Into<CanIndex> for TIndex {
    fn into(self) -> CanIndex {
        match self {
            TIndex::Speed => CanIndex::new(0x6068, 1),
            TIndex::Blob => CanIndex::new(0x2400, 1),
        }
    }
}

impl TryFrom<(TIndex, &[u8])> for TObject {
    type Error = ();
    fn try_from(x: (TIndex, &[u8])) -> Result<Self, ()> {
        match x {
            (TIndex::Speed, b) if b.len() == 4 => {
                Ok(TObject::Speed(u32::from_le_bytes(b.try_into().unwrap())))
            }
            (TIndex::Blob, b) if b.len() == 10 => Ok(TObject::Blob(b.try_into().unwrap())),
            _ => Err(()),
        }
    }
}

impl IntoBuf for TObject {
    fn into_buf(&self, buf: &mut [u8]) -> usize {
        match self {
            TObject::Speed(v) => v.into_buf(buf),
            TObject::Blob(b) => b.into_buf(buf),
        }
    }
}

/// End-to-end facade harness: drives `SdoClient` against `SdoServer` over
/// `CanFrame13` frames, exactly like an application would.
struct FacadeHarness<const N: usize> {
    client: SdoClient<N, CanFrame13, Cap<TObject>, Cap<()>, TDict>,
    server: SdoServer<N, CanFrame13, TDict>,
    node: u8,
}

fn facade_frame(node: u8, req: impl Into<[u8; 8]>) -> CanFrame13 {
    CanFrame13::from_parts(CobId::SdoRequest(node), 8, req.into())
}

impl<const N: usize> FacadeHarness<N> {
    fn new() -> Self {
        FacadeHarness {
            client: SdoClient::new(7),
            server: SdoServer::new(),
            node: 7,
        }
    }

    /// Sends one client frame to the server and returns the resulting client
    /// output, pumping both sides as needed.
    fn server_turn(
        &mut self,
        frame: CanFrame13,
    ) -> ClientOutput<N, Cap<TObject>, Cap<()>> {
        let mut out = match self.server.handle_frame(frame) {
            ServerOutput::Output(resp) => {
                let f = facade_frame(self.node, resp);
                self.client.input(SdoInput::Frame(f))
            }
            ServerOutput::FinalOutput(resp, result) => {
                // for downloads and (segmented/expedited) uploads the final
                // frame must reach the client; only the block-upload
                // completion repeats the already-sent end frame, and that
                // case is consumed in `drive` as the reply to the client's
                // end acknowledgement
                let f = facade_frame(self.node, resp);
                self.client.input(SdoInput::Frame(f))
            }
            ServerOutput::NoFrame => {
                // the server accepted a block download segment: pump the client
                match self.client.pump() {
                    Some(req) => ClientOutput::Output(req),
                    None => panic!("Server NoFrame but the client has nothing to pump!"),
                }
            }
            o => panic!("Unexpected server output: {:?}", o),
        };
        // Drain silent client consumption (block upload mid-sub-block): pump
        // the server; if the server has nothing left, the client may have a
        // pending segment to emit (block download mid-sub-block).
        loop {
            match out {
                ClientOutput::NoFrame => match self.server.pump() {
                    Some(resp) => {
                        let f = facade_frame(self.node, resp);
                        out = self.client.input(SdoInput::Frame(f));
                    }
                    None => match self.client.pump() {
                        Some(req) => {
                            out = ClientOutput::Output(req);
                            break;
                        }
                        None => break,
                    },
                },
                _ => break,
            }
        }
        out
    }

    /// Drives the exchange until the client transfer terminates.
    fn drive(
        &mut self,
        mut out: ClientOutput<N, Cap<TObject>, Cap<()>>,
    ) -> ClientOutput<N, Cap<TObject>, Cap<()>> {
        let mut fuel = 1000;
        loop {
            fuel -= 1;
            assert!(fuel > 0, "facade transfer stuck");
            out = match out {
                ClientOutput::Output(req) => {
                    let frame = facade_frame(self.node, req);
                    self.server_turn(frame)
                }
                ClientOutput::NoFrame => {
                    // nothing pending on the client: the server may be emitting
                    // a block upload sub-block
                    match self.server.pump() {
                        Some(resp) => {
                            let f = facade_frame(self.node, resp);
                            self.client.input(SdoInput::Frame(f))
                        }
                        None => return out,
                    }
                }
                ClientOutput::FinalOutput(req, res) => {
                    // app: send the end acknowledgement, then handle the result
                    let frame = facade_frame(self.node, req.clone());
                    let _ = self.server.handle_frame(frame);
                    return ClientOutput::FinalOutput(req, res);
                }
                other => return other,
            };
        }
    }
}

//---------------------------------------------------------------------------------------------------
// Codec tests
//---------------------------------------------------------------------------------------------------

#[test]
fn server_abort_response_codec_round_trip() {
    // The abort response must use the Abort command specifier (cs = 4, 0x80).
    let resp = ServerResponse::Abort(CanIndex::new(0x6068, 1), AbortCode::ObjectDoesNotExistInTheObjectDictionary);
    let buf: [u8; 8] = resp.clone().into();
    assert_eq!(buf[0], 0x80, "abort response must use cs=4 (0x80)");
    let decoded = ServerResponse::try_from(buf).unwrap();
    assert_eq!(decoded, resp);
}

#[test]
fn client_abort_request_codec_round_trip() {
    let req = ClientRequest::AbortTransfer(CanIndex::new(0x6068, 1), AbortCode::CrcError);
    let buf: [u8; 8] = req.clone().into();
    assert_eq!(buf[0], 0x80);
    let decoded = ClientRequest::try_from(buf).unwrap();
    assert_eq!(decoded, req);
}

//---------------------------------------------------------------------------------------------------
// Segmented (non-expedited) transfer tests
//---------------------------------------------------------------------------------------------------

#[test]
fn segmented_upload_10_bytes() {
    let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
    let mut server: ServerMachine<64> = ServerMachine::default();
    let index = CanIndex::new(0x6068, 1);

    let payload: Vec<u8> = (0..10).map(|i| i as u8).collect();
    let received = drive_upload(&mut client, &mut server, index, &SliceBuf(&payload));
    assert_eq!(received, payload);
}

#[test]
fn segmented_download_sweep() {
    // Multi-segment downloads of every size from 1..=24 bytes must transfer
    // the exact payload (this exercises the last-partial-segment length and
    // the toggle-bit echo).
    for size in 1..=24usize {
        let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
        let mut server: ServerMachine<64> = ServerMachine::default();
        let index = CanIndex::new(0x6068, 1);
        let payload: Vec<u8> = (0..size).map(|i| (i as u8).wrapping_mul(17)).collect();
        let received = drive_download(&mut client, &mut server, index, SliceBuf(&payload));
        assert_eq!(received, payload, "download size {}", size);
    }
}

#[test]
fn segmented_download_7_bytes_exact_segment() {
    let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
    let mut server: ServerMachine<64> = ServerMachine::default();
    let index = CanIndex::new(0x6068, 1);
    let payload: Vec<u8> = (0..7).map(|i| 0xA0 + i as u8).collect();
    let received = drive_download(&mut client, &mut server, index, SliceBuf(&payload));
    assert_eq!(received, payload);
}

#[test]
fn segmented_upload_consecutive() {
    let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
    let mut server: ServerMachine<64> = ServerMachine::default();
    let index = CanIndex::new(0x6068, 1);

    // 10-byte multi-segment upload, then a 4-byte expedited one, then 20 bytes.
    let p1: Vec<u8> = (0..10).map(|i| i as u8).collect();
    let r1 = drive_upload(&mut client, &mut server, index, &SliceBuf(&p1));
    assert_eq!(r1, p1);

    let v2: u32 = 0xCAFE_BEEF;
    let r2 = drive_upload(&mut client, &mut server, index, &v2);
    assert_eq!(r2, v2.to_le_bytes().to_vec());

    let p3: Vec<u8> = (0..20).map(|i| (i * 3) as u8).collect();
    let r3 = drive_upload(&mut client, &mut server, index, &SliceBuf(&p3));
    assert_eq!(r3, p3);
}

#[test]
fn segmented_download_consecutive() {
    let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
    let mut server: ServerMachine<64> = ServerMachine::default();
    let index = CanIndex::new(0x6068, 1);

    let p1: Vec<u8> = (0..10).map(|i| i as u8).collect();
    let r1 = drive_download(&mut client, &mut server, index, SliceBuf(&p1));
    assert_eq!(r1, p1);

    let v2: u16 = 0xBEEF;
    let r2 = drive_download(&mut client, &mut server, index, v2);
    assert_eq!(r2, v2.to_le_bytes().to_vec());

    let p3: Vec<u8> = (0..15).map(|i| (i * 5) as u8).collect();
    let r3 = drive_download(&mut client, &mut server, index, SliceBuf(&p3));
    assert_eq!(r3, p3);
}

//---------------------------------------------------------------------------------------------------
// Block transfer tests
//---------------------------------------------------------------------------------------------------

#[test]
fn block_download_sizes_sweep() {
    for size in 1..=64usize {
        let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
        let mut server: ServerMachine<64> = ServerMachine::default();
        let index = CanIndex::new(0x6068, 1);
        let payload: Vec<u8> = (0..size).map(|i| (i as u8).wrapping_mul(13).wrapping_add(3)).collect();
        let init = client.write_block(index, SliceBuf(&payload), ());
        let received = drive_block_download(&mut client, &mut server, index, init);
        assert_eq!(received, payload, "block download size {}", size);
    }
}

#[test]
fn block_upload_sizes_sweep() {
    for size in 1..=64usize {
        let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
        let mut server: ServerMachine<64> = ServerMachine::default();
        let index = CanIndex::new(0x6068, 1);
        let payload: Vec<u8> = (0..size).map(|i| (i as u8).wrapping_mul(29).wrapping_add(11)).collect();
        let init = client.read_block(index, ());
        let received = drive_block_upload(&mut client, &mut server, index, &SliceBuf(&payload), init);
        assert_eq!(received, payload, "block upload size {}", size);
    }
}

#[test]
fn block_upload_empty_transfer() {
    // An empty block upload: the server skips straight to the end frame.
    let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
    let mut server: ServerMachine<64> = ServerMachine::default();
    let index = CanIndex::new(0x6068, 1);

    let init = client.read_block(index, ());
    let received = drive_block_upload(&mut client, &mut server, index, &SliceBuf(&[]), init);
    assert!(received.is_empty());
    // both sides must be Idle and reusable
    assert!(client.is_ready());
    assert!(server.is_ready());
}

#[test]
fn block_download_empty_transfer_rejected() {
    // Documented deliberate behaviour (mirrors CANopenNode): the server
    // rejects an empty block download with BufferOverflow.
    let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
    let mut server: ServerMachine<64> = ServerMachine::default();
    let index = CanIndex::new(0x6068, 1);

    client.write_block(index, SliceBuf(&[]), ());
    let out = client.transit(ServerResponse::BlockDownloadInitiateAck(7));
    let req = match out {
        ClientOutput::Output(req) => req,
        o => panic!("Expected the end request, got {:?}", o),
    };
    // the server must first accept the (empty) initiate
    let ack = server.transit(ClientRequest::BlockDownloadInitiate(index, 0, true));
    assert!(matches!(ack, ServerOutput::Output(ServerResponse::BlockDownloadInitiateAck(_))));
    let out = server.transit(req);
    assert!(matches!(out, ServerOutput::Error(SdoError::BufferOverflow)));
    // the server must be reusable
    assert!(server.is_ready());
}

#[test]
fn block_transfers_consecutive() {
    let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
    let mut server: ServerMachine<64> = ServerMachine::default();
    let index = CanIndex::new(0x6068, 1);

    // download 20 bytes
    let p1: Vec<u8> = (0..20).map(|i| i as u8).collect();
    let init = client.write_block(index, SliceBuf(&p1), ());
    let r1 = drive_block_download(&mut client, &mut server, index, init);
    assert_eq!(r1, p1);

    // upload 30 bytes
    let p2: Vec<u8> = (0..30).map(|i| 200 - i as u8).collect();
    let init = client.read_block(index, ());
    let r2 = drive_block_upload(&mut client, &mut server, index, &SliceBuf(&p2), init);
    assert_eq!(r2, p2);

    // upload 3 bytes (single tiny segment)
    let p3: Vec<u8> = [9u8, 8, 7].to_vec();
    let init = client.read_block(index, ());
    let r3 = drive_block_upload(&mut client, &mut server, index, &SliceBuf(&p3), init);
    assert_eq!(r3, p3);

    // download 49 bytes (multi-block: 7 segments per block)
    let p4: Vec<u8> = (0..49).map(|i| (i as u8) ^ 0x55).collect();
    let init = client.write_block(index, SliceBuf(&p4), ());
    let r4 = drive_block_download(&mut client, &mut server, index, init);
    assert_eq!(r4, p4);

    // download 64 bytes (exact buffer fit)
    let p5: Vec<u8> = (0..64).map(|i| (i as u8).wrapping_mul(7)).collect();
    let init = client.write_block(index, SliceBuf(&p5), ());
    let r5 = drive_block_download(&mut client, &mut server, index, init);
    assert_eq!(r5, p5);

    assert!(client.is_ready());
    assert!(server.is_ready());
}

#[test]
fn mixed_script_consecutive_transfers() {
    // A long script mixing segmented and block transfers, reads and writes,
    // small and large payloads: catches cross-transfer state leakage.
    let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
    let mut server: ServerMachine<64> = ServerMachine::default();
    let index = CanIndex::new(0x6068, 1);

    let v1: u32 = 0x1122_3344;
    assert_eq!(drive_upload(&mut client, &mut server, index, &v1), v1.to_le_bytes().to_vec());

    let p2: Vec<u8> = (0..10).map(|i| i as u8).collect();
    assert_eq!(drive_download(&mut client, &mut server, index, SliceBuf(&p2)), p2);

    let p3: Vec<u8> = (0..25).map(|i| 100 - i as u8).collect();
    let init = client.write_block(index, SliceBuf(&p3), ());
    assert_eq!(drive_block_download(&mut client, &mut server, index, init), p3);

    let p4: Vec<u8> = (0..25).map(|i| (i as u8) + 50).collect();
    let init = client.read_block(index, ());
    assert_eq!(drive_block_upload(&mut client, &mut server, index, &SliceBuf(&p4), init), p4);

    let v5: u16 = 0xA5A5;
    assert_eq!(drive_download(&mut client, &mut server, index, v5), v5.to_le_bytes().to_vec());

    let p6: Vec<u8> = [1u8, 2, 3].to_vec();
    let init = client.read_block(index, ());
    assert_eq!(drive_block_upload(&mut client, &mut server, index, &SliceBuf(&p6), init), p6);

    let p7: Vec<u8> = (0..49).map(|i| (i as u8).wrapping_mul(3)).collect();
    let init = client.write_block(index, SliceBuf(&p7), ());
    assert_eq!(drive_block_download(&mut client, &mut server, index, init), p7);

    let p8: Vec<u8> = (0..20).map(|i| (i as u8).wrapping_mul(9)).collect();
    assert_eq!(drive_upload(&mut client, &mut server, index, &SliceBuf(&p8)), p8);

    let p9: Vec<u8> = (0..7).map(|i| (i as u8).wrapping_mul(11)).collect();
    assert_eq!(drive_download(&mut client, &mut server, index, SliceBuf(&p9)), p9);

    assert!(client.is_ready());
    assert!(server.is_ready());
}

//---------------------------------------------------------------------------------------------------
// Error paths and recovery (the machine must return to Idle after any error)
//---------------------------------------------------------------------------------------------------

#[test]
fn recovery_after_server_abort() {
    let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
    let mut server: ServerMachine<64> = ServerMachine::default();
    let index = CanIndex::new(0x6068, 1);

    // start a block upload, then the server aborts it
    client.read_block(index, ());
    let out = client.transit(ServerResponse::BlockUploadInitiateAck(index, 10, true));
    assert!(matches!(out, ClientOutput::Output(ClientRequest::BlockUploadStart)));

    let out = client.transit(ServerResponse::Abort(index, AbortCode::ObjectDoesNotExistInTheObjectDictionary));
    assert!(matches!(out, ClientOutput::Error(SdoError::TransferAborted(..))));
    assert!(client.is_ready(), "client must return to Idle after an abort");

    // the next transfer must work
    let v: u32 = 42;
    let r = drive_upload(&mut client, &mut server, index, &v);
    assert_eq!(r, v.to_le_bytes().to_vec());
}

#[test]
fn recovery_after_upload_toggle_mismatch() {
    let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
    let mut server: ServerMachine<64> = ServerMachine::default();
    let index = CanIndex::new(0x6068, 1);

    client.read(index, ());
    let out = client.transit(ServerResponse::UploadInitMultiples(index, 10));
    assert!(matches!(out, ClientOutput::Output(ClientRequest::UploadSegment(_))));

    // the "server" answers with the wrong toggle bit
    let out = client.transit(ServerResponse::UploadMultiples(ToggleBit(true), false, 7, [0u8; 7]));
    assert!(matches!(out, ClientOutput::Error(SdoError::ToggleMismatch)));
    assert!(client.is_ready(), "client must return to Idle after a toggle mismatch");

    let v: u32 = 7;
    let r = drive_upload(&mut client, &mut server, index, &v);
    assert_eq!(r, v.to_le_bytes().to_vec());
}

#[test]
fn recovery_after_index_mismatch() {
    let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
    let mut server: ServerMachine<64> = ServerMachine::default();
    let index = CanIndex::new(0x6068, 1);
    let other = CanIndex::new(0x2000, 1);

    client.read(index, ());
    let out = client.transit(ServerResponse::UploadSingleSegment(other, 4, [1, 2, 3, 4]));
    assert!(matches!(out, ClientOutput::Error(SdoError::CanIndexMismatch(..))));
    assert!(client.is_ready(), "client must return to Idle after an index mismatch");

    let v: u32 = 9;
    let r = drive_upload(&mut client, &mut server, index, &v);
    assert_eq!(r, v.to_le_bytes().to_vec());
}

#[test]
fn recovery_after_block_upload_oversized_ack() {
    let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
    let mut server: ServerMachine<64> = ServerMachine::default();
    let index = CanIndex::new(0x6068, 1);

    client.read_block(index, ());
    let out = client.transit(ServerResponse::BlockUploadInitiateAck(index, 1000, true));
    assert!(matches!(out, ClientOutput::Error(SdoError::BufferOverflow)));
    assert!(client.is_ready());

    let v: u32 = 5;
    let r = drive_upload(&mut client, &mut server, index, &v);
    assert_eq!(r, v.to_le_bytes().to_vec());
}

#[test]
fn recovery_after_block_download_bad_ackseq() {
    let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
    let mut server: ServerMachine<64> = ServerMachine::default();
    let index = CanIndex::new(0x6068, 1);

    let payload: Vec<u8> = (0..14).map(|i| i as u8).collect();
    client.write_block(index, SliceBuf(&payload), ());
    let out = client.transit(ServerResponse::BlockDownloadInitiateAck(2));
    assert!(matches!(out, ClientOutput::Output(ClientRequest::BlockDownloadSegment(1, _, _))));
    let _ = client.pump_block().unwrap();

    // ackseq greater than the number of sent segments
    let out = client.transit(ServerResponse::BlockDownloadResponse(3, 2));
    assert!(matches!(
        out,
        ClientOutput::Error(SdoError::TransferAborted(_, AbortCode::ClientServerCommandSpecifierNotValidOrUnknown))
    ));
    assert!(client.is_ready());

    let v: u32 = 3;
    let r = drive_download(&mut client, &mut server, index, v);
    assert_eq!(r, v.to_le_bytes().to_vec());
}

#[test]
fn recovery_after_unexpected_response() {
    let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
    let mut server: ServerMachine<64> = ServerMachine::default();
    let index = CanIndex::new(0x6068, 1);

    client.read(index, ());
    // a download ack while the client is uploading
    let out = client.transit(ServerResponse::DownloadInitAck(index));
    assert!(matches!(out, ClientOutput::Error(SdoError::ClientStateResponseMismatch(..))));
    assert!(client.is_ready());

    let v: u32 = 1;
    let r = drive_upload(&mut client, &mut server, index, &v);
    assert_eq!(r, v.to_le_bytes().to_vec());
}

#[test]
fn recovery_after_server_toggle_mismatch() {
    // the server must also return to Idle after its own protocol errors
    let mut client: ClientMachine<64, (), ()> = ClientMachine::default();
    let mut server: ServerMachine<64> = ServerMachine::default();
    let index = CanIndex::new(0x6068, 1);

    // put the server mid multi-segment upload (first segment toggle = false)
    server.transit(ClientRequest::InitUpload(index));
    let out = server.upload_data(&SliceBuf(&[0u8; 10]));
    assert!(matches!(out, ServerOutput::Output(ServerResponse::UploadInitMultiples(..))));

    // the client sends a request with the wrong toggle bit
    let out = server.transit(ClientRequest::UploadSegment(ToggleBit(true)));
    assert!(matches!(out, ServerOutput::Error(SdoError::ToggleMismatch)));
    assert!(server.is_ready());

    let v: u32 = 8;
    let r = drive_upload(&mut client, &mut server, index, &v);
    assert_eq!(r, v.to_le_bytes().to_vec());
}

//---------------------------------------------------------------------------------------------------
// Malformed peers (must produce errors, not panics)
//---------------------------------------------------------------------------------------------------

#[test]
fn block_upload_peer_sends_too_many_segments() {
    // The client requested blksize 2 (N=20 -> 14 bytes); a misbehaving server
    // sends more segments than the client buffer can hold.
    let mut client: ClientMachine<20, (), ()> = ClientMachine::default();
    let index = CanIndex::new(0x6068, 1);

    client.read_block(index, ());
    let out = client.transit(ServerResponse::BlockUploadInitiateAck(index, 20, true));
    assert!(matches!(out, ClientOutput::Output(ClientRequest::BlockUploadStart)));

    // sub-block 1: segments 1 and 2 fit
    assert!(matches!(
        client.transit(ServerResponse::BlockUploadSegment(1, false, [1u8; 7])),
        ClientOutput::NoFrame
    ));
    let out = client.transit(ServerResponse::BlockUploadSegment(2, false, [2u8; 7]));
    assert!(matches!(out, ClientOutput::Output(ClientRequest::BlockUploadResponse(2, _))));

    // the server ignores the negotiated blksize 1 and sends another segment
    // that would overflow the buffer: must be an error, not a panic
    let out = client.transit(ServerResponse::BlockUploadSegment(1, false, [3u8; 7]));
    assert!(matches!(out, ClientOutput::Error(SdoError::BufferOverflow)));
    assert!(client.is_ready());
}

#[test]
fn block_download_peer_sends_too_many_segments() {
    // The server negotiated blksize 2 for N=20; a misbehaving client sends
    // more segments than fit the download buffer.
    let mut server: ServerMachine<20> = ServerMachine::default();
    let index = CanIndex::new(0x6068, 1);

    let out = server.transit(ClientRequest::BlockDownloadInitiate(index, 20, true));
    assert!(matches!(out, ServerOutput::Output(ServerResponse::BlockDownloadInitiateAck(_))));

    assert!(matches!(
        server.transit(ClientRequest::BlockDownloadSegment(1, false, [1u8; 7])),
        ServerOutput::NoFrame
    ));
    let out = server.transit(ClientRequest::BlockDownloadSegment(2, false, [2u8; 7]));
    assert!(matches!(out, ServerOutput::Output(ServerResponse::BlockDownloadResponse(2, _))));

    let out = server.transit(ClientRequest::BlockDownloadSegment(1, false, [3u8; 7]));
    assert!(matches!(out, ServerOutput::Error(SdoError::BufferOverflow)));
    assert!(server.is_ready());
}

//---------------------------------------------------------------------------------------------------
// Facade end-to-end tests (frames, dictionary, responders)
//---------------------------------------------------------------------------------------------------

#[test]
fn facade_segmented_read_write_consecutive() {
    let mut h: FacadeHarness<64> = FacadeHarness::new();

    // write Speed (u32) -> server dictionary
    let wcap = Cap::new();
    let out = h.client.input(SdoInput::Write(TIndex::Speed, TObject::Speed(1234), wcap.clone()));
    let out = h.drive(out);
    assert!(matches!(out, ClientOutput::TransferCompleted), "out: {:?}", out);
    assert_eq!(wcap.get(), Some(()));
    assert_eq!(h.server.dictionary.speed, 1234);

    // read Speed back
    let rcap = Cap::new();
    let out = h.client.input(SdoInput::Read(TIndex::Speed, rcap.clone()));
    let out = h.drive(out);
    assert!(matches!(out, ClientOutput::TransferCompleted), "out: {:?}", out);
    assert_eq!(rcap.get(), Some(TObject::Speed(1234)));

    // write Blob (10 bytes, multi-segment)
    let blob = [9u8; 10];
    let wcap = Cap::new();
    let out = h.client.input(SdoInput::Write(TIndex::Blob, TObject::Blob(blob), wcap.clone()));
    let out = h.drive(out);
    assert!(matches!(out, ClientOutput::TransferCompleted), "out: {:?}", out);
    assert_eq!(wcap.get(), Some(()));
    assert_eq!(h.server.dictionary.blob, blob);

    // read Blob back
    let rcap = Cap::new();
    let out = h.client.input(SdoInput::Read(TIndex::Blob, rcap.clone()));
    let out = h.drive(out);
    assert!(matches!(out, ClientOutput::TransferCompleted), "out: {:?}", out);
    assert_eq!(rcap.get(), Some(TObject::Blob(blob)));
}

#[test]
fn facade_block_read_write_consecutive() {
    let mut h: FacadeHarness<64> = FacadeHarness::new();

    // block write Speed
    let wcap = Cap::new();
    let out = h.client.input(SdoInput::BlockWrite(TIndex::Speed, TObject::Speed(0xDEADBEEF), wcap.clone()));
    let out = h.drive(out);
    assert!(matches!(out, ClientOutput::TransferCompleted), "out: {:?}", out);
    assert_eq!(wcap.get(), Some(()));
    assert_eq!(h.server.dictionary.speed, 0xDEADBEEF);

    // block read Speed
    let rcap = Cap::new();
    let out = h.client.input(SdoInput::BlockRead(TIndex::Speed, rcap.clone()));
    let out = h.drive(out);
    match out {
        ClientOutput::FinalOutput(_, res) => {
            assert_eq!(dispatch_result(res), Some(TObject::Speed(0xDEADBEEF)));
        }
        o => panic!("Expected the block upload end output, got {:?}", o),
    }
    assert_eq!(rcap.get(), Some(TObject::Speed(0xDEADBEEF)));

    // block write Blob (10 bytes -> 2 segments)
    let blob = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let wcap = Cap::new();
    let out = h.client.input(SdoInput::BlockWrite(TIndex::Blob, TObject::Blob(blob), wcap.clone()));
    let out = h.drive(out);
    assert!(matches!(out, ClientOutput::TransferCompleted), "out: {:?}", out);
    assert_eq!(wcap.get(), Some(()));
    assert_eq!(h.server.dictionary.blob, blob);

    // block read Blob
    let rcap = Cap::new();
    let out = h.client.input(SdoInput::BlockRead(TIndex::Blob, rcap.clone()));
    let out = h.drive(out);
    match out {
        ClientOutput::FinalOutput(_, res) => {
            assert_eq!(dispatch_result(res), Some(TObject::Blob(blob)));
        }
        o => panic!("Expected the block upload end output, got {:?}", o),
    }
    assert_eq!(rcap.get(), Some(TObject::Blob(blob)));
}

#[test]
fn facade_mixed_script_consecutive() {
    let mut h: FacadeHarness<64> = FacadeHarness::new();

    let blob = [7, 7, 7, 7, 7, 7, 7, 7, 7, 7];

    let wcap = Cap::new();
    let out = h.client.input(SdoInput::BlockWrite(TIndex::Blob, TObject::Blob(blob), wcap.clone()));
    let out = h.drive(out);
    assert!(matches!(out, ClientOutput::TransferCompleted));
    assert_eq!(h.server.dictionary.blob, blob);

    let wcap = Cap::new();
    let out = h.client.input(SdoInput::Write(TIndex::Speed, TObject::Speed(99), wcap.clone()));
    let out = h.drive(out);
    assert!(matches!(out, ClientOutput::TransferCompleted));
    assert_eq!(h.server.dictionary.speed, 99);

    let rcap = Cap::new();
    let out = h.client.input(SdoInput::BlockRead(TIndex::Blob, rcap.clone()));
    let out = h.drive(out);
    match out {
        ClientOutput::FinalOutput(_, res) => {
            assert_eq!(dispatch_result(res), Some(TObject::Blob(blob)));
        }
        o => panic!("Expected the block upload end output, got {:?}", o),
    }

    let rcap = Cap::new();
    let out = h.client.input(SdoInput::Read(TIndex::Speed, rcap.clone()));
    let out = h.drive(out);
    assert!(matches!(out, ClientOutput::TransferCompleted));
    assert_eq!(rcap.get(), Some(TObject::Speed(99)));

    let rcap = Cap::new();
    let out = h.client.input(SdoInput::Read(TIndex::Blob, rcap.clone()));
    let out = h.drive(out);
    assert!(matches!(out, ClientOutput::TransferCompleted));
    assert_eq!(rcap.get(), Some(TObject::Blob(blob)));

    let wcap = Cap::new();
    let out = h.client.input(SdoInput::Write(TIndex::Blob, TObject::Blob([5u8; 10]), wcap.clone()));
    let out = h.drive(out);
    assert!(matches!(out, ClientOutput::TransferCompleted));
    assert_eq!(h.server.dictionary.blob, [5u8; 10]);
}

#[test]
fn facade_server_unknown_index_aborts() {
    let mut h: FacadeHarness<64> = FacadeHarness::new();

    // a raw upload initiate for an index that is not in the dictionary
    let frame = facade_frame(h.node, ClientRequest::InitUpload(CanIndex::new(0x1234, 1)));
    let out = h.server.handle_frame(frame);
    assert!(matches!(
        out,
        ServerOutput::Error(SdoError::DictionaryUnsupportedIndex(_))
    ));
    // the server machine must be reusable
    assert!(h.server.sdo.is_ready());

    // and a normal transfer must still work
    let wcap = Cap::new();
    let out = h.client.input(SdoInput::Write(TIndex::Speed, TObject::Speed(7), wcap.clone()));
    let out = h.drive(out);
    assert!(matches!(out, ClientOutput::TransferCompleted), "out: {:?}", out);
    assert_eq!(wcap.get(), Some(()));
    assert_eq!(h.server.dictionary.speed, 7);
}

#[test]
fn facade_busy_while_transfer_in_progress() {
    let mut h: FacadeHarness<64> = FacadeHarness::new();

    let rcap = Cap::new();
    let out = h.client.input(SdoInput::BlockRead(TIndex::Speed, rcap.clone()));
    assert!(matches!(out, ClientOutput::Output(ClientRequest::BlockUploadInitiate(..))));

    // a second transfer while one is in flight must be rejected with Busy
    let wcap = Cap::new();
    let out2 = h.client.input(SdoInput::Write(TIndex::Speed, TObject::Speed(1), wcap.clone()));
    assert!(matches!(out2, ClientOutput::Error(SdoError::Busy)));

    // the in-flight transfer still completes (the server dictionary default
    // speed is 0)
    let out = h.drive(out);
    match out {
        ClientOutput::FinalOutput(_, res) => {
            assert_eq!(dispatch_result(res), Some(TObject::Speed(0)));
        }
        o => panic!("Expected the block upload end output, got {:?}", o),
    }
    assert_eq!(rcap.get(), Some(TObject::Speed(0)));
}
