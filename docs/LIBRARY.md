# funcan-rs — Library Reference

**A CANopen application-layer library for embedded systems, written in `no_std` Rust.**

This document is the canonical description of the library. It is written to serve two
audiences:

- **Humans** — architecture overview, protocol details, usage patterns, and the
  reasoning behind design decisions.
- **AI agents** — a self-contained context block: exact file paths, public API
  signatures, wire-format tables, state tables, and an extension guide, so that a
  fresh agent can modify the codebase without re-scanning it.

> **Freshness.** Last updated together with the SDO block-transfer implementation
> (block download CiA 301 §7.2.4.7, block upload §7.2.4.8). Verify against `src/`
> when in doubt — the code is the source of truth.

---

## 1. Project overview

| Attribute | Value |
|---|---|
| Crate name | `funcan-rs` |
| Version | 0.3.0 (branch `dsh-frames`; 0.2.1 on `dsh`) — single crate, **not** a workspace |
| License | MIT |
| Repository | https://github.com/zhyltsovd/funcan-rs |
| Edition / toolchain | 2021, `rust-toolchain.toml` pins channel `1.85` |
| Execution model | `#![no_std]`, no `alloc`, no runtime, no async, no interrupts |
| Build scripts | none (`build.rs` absent) |
| Target configs | none (`memory.x`, `.env`, `config/` absent) — platform independent |
| Dependencies | `heapless =0.8` (exact), `paste = "*"` |
| Tests | 48 library tests (`cargo test`) |
| Status | Early stage; SDO (expedited / segmented / block) is the most complete service |

The library provides **codecs, state machines, and facades** for CANopen services. It
does **not** include a CAN hardware driver: applications feed raw frame payloads in
(`CanFrame16` / `CanFrame13` implement the `CanFrame` trait) and receive frames to
transmit out. All protocol state machines are synchronous and caller-driven (Mealy
machines). The required frame format is selected at compile time per device (see
`docs/FORMAT.md`).

### Working-tree state

As of this writing the working tree contains **uncommitted changes** implementing
SDO block transfer (modified: `src/sdo.rs`, `src/sdo/machines.rs`,
`src/sdo/client.rs`, `src/sdo/server.rs`, `src/interfaces.rs`, `src/lib.rs`).
`obsolete/` holds legacy code (gitignored, not compiled).

---

## 2. CANopen feature matrix

| Feature | Status | Location |
|---|---|---|
| Object Dictionary | **Trait-based only** — `Dictionary` / `DictionaryValue` + `CanIndex`. No concrete OD, no static tables, no codegen | `src/dictionary.rs` |
| NMT | **Codec only** — commands, states, node targeting, frame (de)serialization. No lifecycle state machine, no boot-up emission, no master/slave runtime | `src/nmt.rs` |
| SDO expedited | ✅ Client + server, ≤4-byte single-frame transfers | `src/sdo.rs`, `src/sdo/machines.rs` |
| SDO segmented | ✅ Client + server, toggle-bit multi-frame transfers | same |
| SDO **block download** | ✅ Client + server (CiA 301 §7.2.4.7), multi-segment sub-blocks, retransmission, CRC-16/CCITT | same |
| SDO **block upload** | ✅ Client + server (CiA 301 §7.2.4.8), same features | same |
| SDO abort codes | ✅ Full CiA 301 `AbortCode` table | `src/sdo/abort.rs` |
| PDO | **Static mapping only** — `Producer`/`Consumer` with fixed `heapless` capacity 8; mapping-parameter helper. No dynamic mapping, no sync/async/event triggering | `src/pdo.rs` |
| Heartbeat | **Consumer only** — per-node liveness tracking with `ClockInstant` timeouts. No producer, no node-guard | `src/heartbeat.rs` |
| EMCY | ❌ Only legacy `EmergencyClass` enum in `obsolete/emcy.rs` (not compiled) | `obsolete/` |
| SYNC / TIME | ❌ Only `CobId::Sync` / `CobId::TimeStamp` variants; no protocol logic | `src/raw.rs` |
| Node ID | Plain `u8` threaded into `CobId` conversions and machines; no central node configuration | — |

**Network roles.** Both SDO client and server are implemented for every transfer
type. NMT/PDO/heartbeat do not yet distinguish master/slave roles.

---

## 3. Crate dependency graph

```
funcan-rs (no_std lib)
├── heapless 0.8 (exact pin)      # FnvIndexMap, Vec — fixed-capacity collections
│   ├── hash32 0.3 → byteorder 1.5
│   └── stable_deref_trait 1.2
└── paste 1.0 (unpinned)          # declared, but UNUSED in src/ (legacy macro in obsolete/)
(no embedded-hal, no embedded-can, no embassy, no defmt, no futures [commented out in Cargo.toml])
```

---

## 4. Module map

| Module | Path | Responsibility |
|---|---|---|
| `raw` | `src/raw.rs` | `CanFrame` trait (parametric over the wire format) with two impls: `CanFrame16` (16-byte SocketCAN-style, LE COB-ID) and `CanFrame13` (13-byte compact, BE COB-ID); `CobId` enum with bidirectional CiA 301 function-code mapping |
| `machine` | `src/machine.rs` | `MealyMachine<X,Y>` trait + `MorphMachine` decode/encode adapter — the architectural backbone |
| `interfaces` | `src/interfaces.rs` | Driver-facing traits: `OneshotResponder`, `ClockInstant`, `CanSize`, `IntoBuf` (impls: u8, u16, u32, `[u8; K]`) |
| `dictionary` | `src/dictionary.rs` | `CanIndex{base:u16, sub:u8}` (+ LE 3-byte codec), `Dictionary`, `DictionaryValue` |
| `nmt` | `src/nmt.rs` | `NmtCommand`, `NmtState`, `NodeTarget`, `NmtRequest` codec (COB-ID 0x000) |
| `heartbeat` | `src/heartbeat.rs` | `HeartbeatMachine<N, I>` — consumer/liveness tracker |
| `pdo` | `src/pdo.rs` | `PdoId`, `Producer`, `Consumer`, `PdoMap`, `pdo_map_param()` |
| `sdo` | `src/sdo.rs` | SDO codecs: `ClientRequest`/`ServerResponse` (+ block variants), `TransferType`, `ToggleBit`, command specifiers, `Error`, `crc16_ccitt()` |
| `sdo::abort` | `src/sdo/abort.rs` | `AbortCode` — full CiA 301 abort-code table |
| `sdo::machines` | `src/sdo/machines.rs` | **The protocol core**: `ClientMachine<N,RR,RW>` and `ServerMachine<N>` state machines (segmented + block), `SdoError`, `ClientOutput`/`ServerOutput`/`ClientResult`/`ServerResult` |
| `sdo::client` | `src/sdo/client.rs` | `SdoClient<N,R,W,D>` facade — typed `SdoInput` API over the client machine |
| `sdo::server` | `src/sdo/server.rs` | `SdoServer<N,D>` facade — frame API over the server machine + `Dictionary` wiring |

**Notable absences:** no dispatcher for incoming frames by COB-ID (the legacy macro
`build_sdo_dispatcher!` lives in `obsolete/`), no message queues, no timers, no ISR
hooks, no `embedded-can`/`embedded-hal` integration.

---

## 5. Architecture & layering

```
┌──────────────────────────────────────────────────────────────┐
│ Application layer (user code)                                 │
│   - defines Dictionary implementation                         │
│   - owns the CAN driver and the receive loop                  │
├──────────────────────────────────────────────────────────────┤
│ Facades: SdoClient / SdoServer                                │
│   - typed input API, Dictionary wiring, responder dispatch    │
├──────────────────────────────────────────────────────────────┤
│ Service machines: ClientMachine / ServerMachine /             │
│ HeartbeatMachine / PDO Producer-Consumer / NMT codec          │
│   - Mealy state machines, one frame per transition            │
│   - block mode: pump_block() emits segments between frames    │
├──────────────────────────────────────────────────────────────┤
│ Codecs: ClientRequest / ServerResponse / CobId / CanIndex     │
│   - Into<[u8;8]> / TryFrom<[u8;8]>                            │
│   - state-aware decode: transit_frame()                       │
├──────────────────────────────────────────────────────────────┤
│ Foundation: MealyMachine, interfaces traits, heapless         │
└──────────────────────────────────────────────────────────────┘
```

**Scheduling model:** fully **cooperative / caller-driven**. There is no event loop,
no preemption, no async. Every machine step is a function call:

- segmented SDO: `transit()` consumes one typed frame and produces one output frame;
- block SDO: `transit()` consumes one typed frame, and `pump_block()` emits the
  remaining segments of the current sub-block between frames;
- `transit_frame([u8;8])` is the **state-aware decoder**: it routes raw payloads to
  the correct typed variant based on the machine's current state (required because
  block segments carry no command specifier — see §7.4).

**Time** exists only as an injected `ClockInstant` (`interfaces.rs`); there are no
timeouts anywhere in the protocol machines (documented limitation).

---

## 6. Core data structures & traits

### 6.1 Frames & identifiers — `raw`

```rust
pub trait CanFrame: Clone + Copy + PartialEq + Eq + Debug + Default {
    const WIRE_SIZE: usize;                                    // 16 or 13
    fn from_parts(cobid: CobId, len: usize, data: [u8; 8]) -> Self;
    fn cobid(&self) -> CobId;
    fn len(&self) -> usize;
    fn data(&self) -> [u8; 8];
    fn write_to_slice(&self, buffer: &mut [u8]);               // asserts WIRE_SIZE
    fn read_from_slice(buffer: &[u8]) -> Self;                 // asserts WIRE_SIZE
}

// 16-byte SocketCAN-style:  [cobid LE 4][len 1][pad 3][data 8]  (CAN sockets / USB-CANABLE)
pub struct CanFrame16 { pub cobid: CobId, pub len: usize, pub data: [u8; 8] }
// 13-byte compact:          [len 1][cobid BE 4][data 8]         (Ethernet-to-CAN adapters)
pub struct CanFrame13 { pub cobid: CobId, pub len: usize, pub data: [u8; 8] }

pub enum CobId {
    NmtService(u8, u8),  // 0x000, carries cmd+node in the payload
    Sync,                // 0x080
    TimeStamp,           // 0x100
    Emergency(u8),       // 0x080 + node
    PdoTx { pdo_id: u8 /*1..4*/, node_id: u8 },  // 0x180 + 0x100*(pdo-1) + node
    PdoRx { pdo_id: u8, node_id: u8 },           // 0x200 + 0x100*(pdo-1) + node
    SdoResponse(u8),     // 0x580 + node
    SdoRequest(u8),      // 0x600 + node
    Heartbeat(u8),       // 0x700 + node
    ManufacturerSpecific(u16),
}
```

The frame type is a compile-time parameter of every facade that touches the wire
(`SdoClient<N, F, …>`, `SdoServer<N, F, …>`, `SdoInput::Frame(F)`,
`Producer<D, F>::serialize`, `NmtRequest ↔ F` conversions). The protocol machines
are frame-agnostic and only see `CobId` + `[u8; 8]`.

`write_to_slice`/`read_from_slice` are **per-format** — do not assume one wire
format when porting; see `docs/FORMAT.md` for the exact byte layouts and the
historical rationale.

### 6.2 Dictionary — `dictionary`

```rust
pub struct CanIndex { pub base: u16, pub sub: u8 }   // LE 3-byte codec
pub trait Dictionary {
    type Index: Sized;
    type Object: Sized;
    fn set(&mut self, x: Self::Object);
    fn get(&self, ix: &Self::Index) -> Self::Object;
}
pub trait DictionaryValue<D: Dictionary>: TryFrom<D::Object> {
    fn index() -> D::Index;
}
```

The Dictionary abstraction keeps the machines independent of any concrete OD layout.
`SdoClient`/`SdoServer` convert between `CanIndex` (wire) and `D::Index` (app) via
`TryFrom<CanIndex>` / `Into<CanIndex>`, and between wire bytes and `D::Object` via
`TryFrom<(D::Index, &[u8])>` + `IntoBuf`.

### 6.3 Machine trait — `machine`

```rust
pub trait MealyMachine<X, Y> {
    fn initiate(&mut self);
    fn transit(&mut self, x: X) -> Y;
}
pub struct MorphMachine<'a, M, U, V, X, Y> { pub machine: M, pub decode: &'a dyn Fn(X)->U, pub encode: &'a dyn Fn(V)->Y }
```

### 6.4 Interface traits — `interfaces`

| Trait | Purpose | Impls |
|---|---|---|
| `OneshotResponder<X>` | `respond(self, x) -> Result<(), X>` — completion callback carried through transfers | user-supplied |
| `ClockInstant` | `now()` + `duration_since` — abstract time for heartbeat | user-supplied |
| `CanSize` | `can_size() -> usize` — wire size of a mapped object | user-supplied |
| `IntoBuf` | `into_buf(&self, buf) -> usize` — serialize into a byte buffer | `u8`, `u16`, `u32`, `[u8; K]` |

### 6.5 Machine outputs & errors — `sdo::machines`

```rust
pub enum ClientOutput<const N: usize, RR, RW> {
    Output(ClientRequest),                    // send this frame
    Done(ClientResult<N, RR, RW>),            // transfer finished
    TransferCompleted,                        // responder already dispatched
    Error(SdoError),
    NoFrame,                                  // block mode: nothing to send (duplicate/ignored frame)
}
pub enum ServerOutput<const N: usize> {
    Output(ServerResponse),                   // send this frame
    FinalOutput(ServerResponse, ServerResult<N>),
    Data(CanIndex),                           // upload: the facade must provide the object
    Error(SdoError),
    NoFrame,
}
pub enum ClientResult<const N, RR, RW> { UploadCompleted(CanIndex, [u8;N], usize, Option<RR>), DownloadCompleted(Option<RW>) }
pub enum ServerResult<const N> { UploadCompleted, DownloadCompleted(CanIndex, [u8;N], usize) }
```

`SdoError` variants: `ClientStateResponseMismatch`, `ServerStateResponseMismatch`,
`CanIndexMismatch`, `TransferAborted(CanIndex, AbortCode)`, `ToggleMismatch`,
`BufferOverflow`, `Busy`, `DictionaryUnsupportedIndex(CanIndex)`,
`DictionaryDecodingFailure(CanIndex)`, `DecodingFailure(sdo::Error)`, `NoResponder`.

---

## 7. SDO protocol details

### 7.1 Command specifiers (`sdo.rs`)

| Direction | Specifier | Value |
|---|---|---|
| client→server | `InitDownload` | `1<<5` (0x20) |
| client→server | `DownloadSegment` | `0<<5` |
| client→server | `InitUpload` | `2<<5` (0x40) |
| client→server | `UploadSegment` | `3<<5` (0x60) |
| client→server | `AbortTransfer` | `4<<5` (0x80) |
| server→client | `InitDownloadAck` | `3<<5` |
| server→client | `DownloadSegmentAck` | `1<<5` |
| server→client | `InitUpload` | `2<<5` |
| server→client | `UploadSegment` | `0<<5` |
| server→client | `Abort` | `4<<5` |

`TransferType`: `Normal` (0x01), `NormalUnspecifiedSize` (0x00),
`ExpeditedWithSize(n)` (0x03 | (4-n)<<2). `ToggleBit` = bit 4 of byte 0.
All **index fields are little-endian** (`CanIndex::write_to_slice`), as are u32
size fields (`to_le_bytes`) — consistent with the crate's segmented codec and with
CANopenNode's wire format on little-endian targets.

### 7.2 Block transfer wire formats

Segment frames (`seqno` 1..block-size, `end` = bit 7 = last segment of the whole
transfer) carry **no command specifier** — they are decoded state-aware (§7.4).

**Block download** (client → server):
| Frame | byte0 | bytes 1–3 | bytes 4–7 |
|---|---|---|---|
| Initiate | `0xC0 \| crc(0x04) \| s(0x02)` | index LE | size LE (only if `s`) |
| Segment | `seqno \| (end << 7)` | 7 data bytes | — |
| End request | `0xC1 \| (n << 2)` | CRC LE (bytes 1–2) | unused |

**Block download** (server → client):
| Frame | byte0 | bytes 1–3 | bytes 4–7 |
|---|---|---|---|
| Initiate ack | `0xA4` | unused | **block size** (byte 4) |
| Block response | `0xA2` | `ackseq` (b1), `blksize` (b2) | unused |
| End ack | `0xA1` | — | — |

**Block upload** (client → server):
| Frame | byte0 | bytes 1–3 | bytes 4–7 |
|---|---|---|---|
| Initiate | `0xA4` | index LE | `blksize` (b4), `pst` (b5) |
| Start | `0xA3` | — | — |
| Block response | `0xA2` | `ackseq` (b1), `blksize` (b2) | — |
| End ack | `0xA1` | — | — |

**Block upload** (server → client):
| Frame | byte0 | bytes 1–3 | bytes 4–7 |
|---|---|---|---|
| Initiate ack | `0xC0 \| crc(0x04) \| s(0x02)` | index LE | size LE (only if `s`) |
| Segment | `seqno \| (end << 7)` | 7 data bytes | — |
| End frame | `0xC1 \| (n << 2)` | CRC LE (bytes 1–2) | unused |

`n` = number of invalid bytes in the **last** segment (0–7); valid length of the last
segment = `7 − n`. The **CRC covers the actual data** of the whole transfer (final
segment's padding excluded).

### 7.3 Block transfer flows

**Download** (data flows client → server; block size chosen by the **server**):
1. client: `BlockDownloadInitiate(index, size, crc)` → server ack: `BlockDownloadInitiateAck(blksize)`
2. client sends segments in sub-blocks of `blksize` (seqno restarts at 1 per block;
   the final segment of the transfer has `end = 1`), server replies per sub-block:
   `BlockDownloadResponse(ackseq, new_blksize)`
3. after the final sub-block: client sends `BlockDownloadEnd(crc, n)` → server
   verifies CRC/size → `BlockDownloadEndAck` → `FinalOutput(_, DownloadCompleted)`

**Upload** (data flows server → client; block size negotiated from the **client's**
request, adjustable per sub-block via the ack):
1. client: `BlockUploadInitiate(index, blksize, pst)` → server `Data(index)` → facade
   calls `upload_data(&obj)` → `BlockUploadInitiateAck(index, size, crc)`
2. client: `BlockUploadStart` → server sends segments in sub-blocks (last segment of
   the transfer has `end = 1`), client replies per sub-block:
   `BlockUploadResponse(ackseq, new_blksize)`
3. server: `BlockUploadEnd(n, crc)` → client verifies CRC/size →
   `BlockUploadEndAck` → `Done(UploadCompleted)`

**Retransmission:** if `ackseq < sent`, the sender rewinds its data pointer to
`block_start + ackseq*7` and re-transmits the remainder of the sub-block **renumbered
from 1** (both machines reset their seqno counter after every response). Out-of-
sequence frames on the receiver produce a response with the last good `ackseq`;
duplicates and pre-block frames (`seqno==0` state) are ignored (`NoFrame`).

**CRC:** `crc16_ccitt(data, seed)` — CRC-16/CCITT (XMODEM): poly `0x1021`, init `0`,
no reflection, no final XOR, table-driven. Check value `0x31C3` for `"123456789"`.

### 7.4 State-aware decoding — `transit_frame`

Block segments have no command specifier, so a stateless `TryFrom<[u8;8]>` cannot
classify them (e.g. `seqno ≤ 31` collides with `DownloadSegment`; `0x80|seqno`
collides with `AbortTransfer` — both cases are unit-tested). Therefore:

- `ClientMachine::transit_frame` / `ServerMachine::transit_frame` decode **raw**
  payloads using the machine's current state (e.g. a `BlockUploadReceiving` client
  treats every non-`0x80` frame as a segment).
- The typed `TryFrom` codecs handle everything else (initiates, acks, responses, end
  frames, aborts).
- Facades `SdoClient::input(Frame)` and `SdoServer::handle_frame` route through
  `transit_frame`, so callers never decode manually.

### 7.5 Machine state tables

`ClientState`: `Idle`, `InitUploading`, `UploadingMultiples(ToggleBit)`,
`InitiateSingleDownload(usize)`, `InitiateMultipleDownload(usize)`,
`DownloadingSegments(ToggleBit, usize)`,
`BlockDownloadInitiated`, `BlockDownloadSending{blocksize, block_start, position, seqno}`,
`BlockDownloadAwaitingResponse{block_start, sent, finished, no_data}`,
`BlockDownloadEnding`, `BlockUploadInitiated`,
`BlockUploadReceiving{blocksize, position, seqno}`, `BlockUploadAwaitingEnd{position}`.

`ServerState`: `Idle`, `AwaitingData(bool)`,
`UploadingMultipleSegments{response_toggle, position}`,
`DownloadingMultipleSegments(ToggleBit, usize)`,
`BlockDownloadReceiving{blksize, position, seqno}`, `BlockDownloadAwaitingEnd{position}`,
`BlockUploadAwaitingData{blksize}`, `BlockUploadReady{blksize}`,
`BlockUploadSending{blksize, block_start, position, seqno}`,
`BlockUploadAwaitingAck{block_start, sent, finished, no_data}`,
`BlockUploadEnding{no_data, crc}`.

The **last segment of a transfer is stashed** (client: `upload_last`, server:
`download_last`) and committed only when the end frame reveals `n`, so the final
segment's padding never overflows the `[u8; N]` data buffers.

---

## 8. Usage patterns

### 8.1 SDO client (segmented and block)

```rust
use funcan_rs::sdo::client::{SdoClient, SdoInput};
use funcan_rs::sdo::machines::{ClientOutput, SdoError};
use funcan_rs::sdo::ClientRequest;
use funcan_rs::raw::{CanFrame13, CobId};   // or CanFrame16 for SocketCAN devices

// D: your Dictionary; R/W: your responder types (e.g. a channel or closure)
// F: the frame format of your device — CanFrame13 (Ethernet-to-CAN) or CanFrame16 (USB-CANABLE)
let mut client: SdoClient<1024, CanFrame13, R, W, MyDict> = SdoClient::new(node_id);

// --- segmented read / write ---
let out = client.input(SdoInput::Read(index, responder));
let out = client.input(SdoInput::Write(index, value, responder));

// --- block download (write): drive segments between frames ---
let mut out = client.input(SdoInput::BlockWrite(index, value, responder));
loop {
    match out {
        ClientOutput::Output(req) => send_frame(CanFrame13::from_parts(CobId::SdoRequest(node_id), 8, req.into())),
        ClientOutput::NoFrame => {}                       // nothing to send
        ClientOutput::Error(e) => { /* handle */ break }
        _ => break,                                       // Done / TransferCompleted
    }
    // when the machine is mid-sub-block, pump remaining segments:
    if let Some(req) = client.pump() {
        send_frame(CanFrame13::from_parts(CobId::SdoRequest(node_id), 8, req.into()));
        continue;
    }
    // else wait for the next incoming frame and feed it back:
    //   out = client.input(SdoInput::Frame(frame));
}
```

### 8.2 SDO server

```rust
let mut server: SdoServer<1024, CanFrame13, MyDict> = SdoServer::new();   // D: Default

// per incoming frame (COB-ID 0x600 + node):
let out = server.handle_frame(frame);
match out {
    ServerOutput::Output(resp) => send_frame(CanFrame13::from_parts(CobId::SdoResponse(node_id), 8, resp.into())),
    ServerOutput::FinalOutput(_, ServerResult::DownloadCompleted(..)) => { /* dictionary already updated */ }
    // ServerOutput::Data is handled internally by handle_frame via the Dictionary
    _ => {}
}
// mid-block upload: emit remaining segments
while let Some(resp) = server.pump() {
    send_frame(CanFrame13::from_parts(CobId::SdoResponse(node_id), 8, resp.into()));
}
```

### 8.3 Frame construction

`ClientRequest` / `ServerResponse` convert to payloads via `Into<[u8;8]>` (and back
via `TryFrom`, except segments — see §7.4). Combine with `CobId` and the frame type:

```rust
let data: [u8; 8] = req.into();
let frame = CanFrame13::from_parts(CobId::SdoRequest(node_id), 8, data);
// or CanFrame16::from_parts(...) — same logical frame, different wire bytes
```

### 8.4 CAN driver port

The frame trait `CanFrame` (impls `CanFrame16`, `CanFrame13`) is the library's wire
type. Write an adapter that converts `F::cobid()/len()/data()` to/from your
controller's frame type, or reuse `write_to_slice`/`read_from_slice` for buffers
that match the 13-byte or 16-byte layout. The `CobId` → u32 mapping is
`impl From<CobId> for u32` in `raw.rs`. There is no `embedded-can` or
`embedded-hal` dependency; the slice formats are per-format (see `docs/FORMAT.md`)
and are not controller formats.

---

## 9. Build, testing, safety

```sh
cargo check          # clean; only pre-existing warnings (raw.rs unused d0/d1, pdo.rs unused Results)
cargo test           # 48 tests, all pass
cargo test --lib     # same suite (lib tests)
```

**Test map:**

| Area | File | Tests |
|---|---|---|
| Frame formats (golden vectors, round-trips, generic) | `src/raw.rs` | 13-byte and 16-byte serialization vectors, `round_trip<F: CanFrame>` for both |
| NMT ↔ frame conversions | `src/nmt.rs` | `NmtRequest` ↔ `CanFrame16` / `CanFrame13` |
| `CanIndex` codec | `src/dictionary.rs` | write/read/inverse |
| SDO segmented codecs (CiA 301 vectors) | `src/sdo.rs` | client upload/download init/segments, server responses, abort |
| CRC-16/CCITT | `src/sdo.rs` | check value `0x31C3`, incremental == one-shot |
| SDO block codecs | `src/sdo.rs` | initiate/ack/response/end/segment encodings, ambiguity proof |
| SDO segmented machines | `src/sdo/machines.rs` | client↔server upload/download of u32/u16 ("gasoline" loops) |
| SDO block machines | `src/sdo/machines.rs` | single/multi-block download+upload exchanges, retransmission both directions, CRC mismatch aborts, raw `transit_frame` decoding |

**Safety:** **zero `unsafe` blocks** in the crate — no FFI, no MMIO, no critical
sections, platform-independent. Panics are used for API misuse (buffer-length
asserts, `unreachable!()` on unknown NMT/state/command bytes, `todo!()` in
`SdoServer::handle_frame` for a non-`TryFrom`-able index).

---

## 10. Risks & technical debt

- **No timers/timeouts** anywhere in the protocol machines — a lost frame hangs the
  transfer (acceptable for a library; the application must supply timeouts).
- **No dispatcher** for routing incoming frames by COB-ID to the right service
  (legacy macro in `obsolete/`).
- **README TO-DO list** previously lagged reality (SDO server was marked
  unimplemented while implemented); the current README reflects the real state.
- **Stale/legacy files**: `obsolete/` (gitignored) holds an old SDO client, EMCY
  enum, dispatcher macro; emacs backup files (`*~`, `#*#`) litter `src/` (gitignored).
- **Unused dependency**: `paste` is declared but unused in `src/`.
- **Commented-out code**: `futures` in `Cargo.toml`, `AsyncResponder` in
  `interfaces.rs`, a large commented block in `sdo/client.rs`.
- **Hardcoded values**: PDO capacity 8; segment size 7 and block-size bounds 1..127
  are protocol constants; `pst` (protocol switch threshold) is always 0 — no
  fallback to segmented transfer, and a received `pst` is ignored.
- **Panics on malformed traffic**: `unreachable!()` for unknown NMT/state bytes
  (bus noise → panic), `panic!` on missing PDO map entries.
- **Empty transfers** (0 bytes): client emits an end request with `n=7`; server
  rejects with `BufferOverflow` (mirrors CANopenNode).
- **Russian debug strings** in `sdo/client.rs` `Debug` impl.
- **No CI**, no benchmarks, no fuzzing.

---

## 11. Extension guide

### 11.1 Add a new OD entry

Implement `Dictionary` for your struct (choose `Index` and `Object` types) and
`DictionaryValue<D>` for each typed value (binds `index()` to the entry). Objects
must satisfy `TryFrom<(Index, &[u8])>` (deserialize) and `IntoBuf` (serialize); add
`CanSize` for PDO mapping. The `IntoBuf for [u8; K]` impl covers raw byte arrays.

### 11.2 Add a new service handler (e.g. SYNC, EMCY)

1. Add `CobId` variants if new COB-IDs are needed (`raw.rs`; extend both `From` impls).
2. Add typed frames as variants of `ClientRequest`/`ServerResponse` (or a new enum)
   with `Into<[u8;8]>` / `TryFrom<[u8;8]>` arms (`sdo.rs`).
3. Add states + `transit` arms in `machines.rs`; keep the match arms **before** the
   catch-all `(state, response) => Error(...)` arm.
4. If the new frames lack a command specifier (like block segments), extend
   `transit_frame` with the corresponding state branch so decoding stays state-aware.
5. Expose through the facades if the application drives it; add codec + machine
   tests mirroring the existing ones.

### 11.3 Port to a different CAN driver

Write an adapter: `F: CanFrame` → controller frame (use `cobid() → u32` + `data()` +
`len()`), and controller frame → `F` (decode the 11-bit identifier with
`CobId::from(u32)`, build with `F::from_parts`). The library is fully
driver-agnostic; only the application layer touches hardware. Pick `CanFrame16` for
SocketCAN-style buffers or `CanFrame13` for compact Ethernet-adapter buffers; the
slice formats are per-format (see `docs/FORMAT.md`).

### 11.4 Known deviations from CiA 301 (deliberate)

- `pst` is always 0; the segmented fallback on the server is not implemented.
- No timeouts, no SDO block-mode "protocol switch".
- The upload initiate request does not send the client's buffer-derived block size
  differently from CANopenNode (it does send a requested block size in byte 4).
- Size fields are little-endian (matching CANopenNode on typical LE targets and the
  crate's segmented codec).

---

## 12. Reference checklist for agents

- Wire payloads are 8 bytes; frame `len` is informational (per `CanFrame` impl).
- **Segment frames are decoded by machine state only** — never add them to a
  stateless `TryFrom` dispatch.
- Block CRC covers the actual data; the final segment's padding (`n`) is excluded
  and committed only at the end frame.
- Retransmission renumbers segments from 1 after every response; both machines reset
  their seqno counter on response.
- `NoFrame` outputs mean "nothing to send" and are legal in block mode — the
  existing test drivers treat them as such.
- All machine buffers are `[u8; N]` const-generic; capacity = `N` bytes, block size
  ≤ 127, segment payload 7 bytes.
