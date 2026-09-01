# funcan-rs — Migration Guide (target version 0.3.0)

**Audience.** Projects that consume `funcan-rs` as a dependency and need to upgrade
their code from an older release to **0.3.0**. Written to be actionable for both
humans and AI agents: read §2 to pick the frame format, §4 for the step-by-step
0.2.1 → 0.3.0 procedure, and §7 to fix compile errors.

**What changed in 0.3.0.** CAN frames are now **parametric over the wire format**:
the old `CanFrame` struct became the `CanFrame` **trait**, implemented by
`CanFrame16` (16-byte SocketCAN-style) and `CanFrame13` (13-byte compact). Every
public API that touches the wire carries the frame type as a compile-time generic
parameter. The protocol machines, codecs, `CobId`, `CanIndex` and the
`Dictionary`/responder abstractions are **unchanged**.

---

## 1. Version roadmap

| Version | Frame type | Wire format | Notes |
|---|---|---|---|
| 0.1.x | `CANFrame { can_cobid: u32, … }` | 16-byte, LE | raw u32 COB-IDs |
| 0.2.0 | `CanFrame { cobid: CobId, … }` | 16-byte, LE (`[cobid LE][len][pad 3][data]`) | typed `CobId`; SocketCAN-style |
| 0.2.1 | `CanFrame { cobid, len, data }` | 13-byte, BE (`[len][cobid BE][data]`) | "new format" transition; SDO block transfer added |
| **0.3.0** | `CanFrame` trait + `CanFrame16` / `CanFrame13` | both, preserved per format | this guide |
| 0.3.1 | unchanged | unchanged | SDO audit release: segmented interop fixes, error recovery, block upload end ack (`ClientOutput::FinalOutput`), abort encoding fix — see §6.1 |

> Historical note (for context): the 16→13 byte switch happened inside 0.2.1
> (`a81c977` "new format", followed by offset/endianness fixes). 0.3.0 makes both
> layouts first-class and selectable at compile time.

---

## 2. Step 0 — choose the frame format

Pick one type per device; it is fixed at compile time (no runtime dispatch):

| Your device | Frame type | Wire layout |
|---|---|---|
| Ethernet-to-CAN adapter | `CanFrame13` | 13 bytes: `[len 1][cobid BE 4][data 8]` |
| CAN socket / USB-CANABLE (SocketCAN) | `CanFrame16` | 16 bytes: `[cobid LE 4][len 1][pad 3][data 8]` |

If you were on 0.2.1, `CanFrame13` is byte-for-byte the format you already had. If
you were on 0.2.0/0.1.x, `CanFrame16` restores your exact wire bytes.

---

## 3. Cargo.toml

```toml
[dependencies]
funcan-rs = "0.3"
```

(Optionally pin `=0.3.1` for reproducible builds.)

---

## 4. Migration 0.2.1 → 0.3.0 (main path)

Assume `use funcan_rs::raw::{CanFrame13, CobId};` (or `CanFrame16` — the steps are
identical). Only frame-touching code changes; SDO machines, codecs, dictionaries
and responders are untouched.

### 4.1 Type parameters — add `F`

```rust
// before (0.2.1)
let mut client: SdoClient<1024, R, W, MyDict> = SdoClient::new(node_id);
let mut server: SdoServer<1024, MyDict> = SdoServer::new();

// after (0.3.0)
let mut client: SdoClient<1024, CanFrame13, R, W, MyDict> = SdoClient::new(node_id);
let mut server: SdoServer<1024, CanFrame13, MyDict> = SdoServer::new();
```

### 4.2 Incoming frames

```rust
// before: SdoInput::Frame(CanFrame) — field access
let out = client.input(SdoInput::Frame(frame));
let payload = frame.data;

// after: SdoInput::Frame(F) — method access
let out = client.input(SdoInput::Frame(frame));
let payload = frame.data();
```

Server side:

```rust
// before
let out = server.handle_frame(frame);   // frame: CanFrame
// after
let out = server.handle_frame(frame);   // frame: CanFrame13 (same signature shape)
```

### 4.3 Constructing frames to send

```rust
// before
let frame = CanFrame { cobid: CobId::SdoRequest(node_id), len: 8, data };

// after — from_parts (or the struct literal still works, fields are public)
let frame = CanFrame13::from_parts(CobId::SdoRequest(node_id), 8, data);
```

Reading fields: `frame.cobid` / `frame.len` / `frame.data` become **method calls**
`frame.cobid()` / `frame.len()` / `frame.data()` (needed whenever the frame is
generic `F: CanFrame`; works on concrete types too).

### 4.4 NMT conversions (call sites unchanged)

```rust
let req = NmtRequest { cmd: NmtCommand::EnterPreOperational, target: NodeTarget::Node(7) };

let frame: CanFrame13 = req.into();        // From<NmtRequest> for CanFrame13/16
let back: NmtRequest = NmtRequest::from(frame);
```

The conversions are implemented **per concrete format** (Rust's orphan rule forbids
a generic `From<F> for NmtRequest` — see §7).

### 4.5 PDO producer

```rust
// before
let producer: Producer<MyDict> = Producer::new(PdoId::Tx(1));
let frame: CanFrame = producer.serialize(node_id);

// after
let producer: Producer<MyDict, CanFrame13> = Producer::new(PdoId::Tx(1));
let frame: CanFrame13 = producer.serialize(node_id);
```

### 4.6 Slice (de)serialization

`write_to_slice` / `read_from_slice` are now **trait methods**; buffer sizes differ
per format:

```rust
let mut buf = [0u8; CanFrame13::WIRE_SIZE];      // 13, or CanFrame16::WIRE_SIZE = 16
frame.write_to_slice(&mut buf);
let back = CanFrame13::read_from_slice(&buf);
```

### 4.7 Verify

```sh
cargo check    # fix errors with §7
cargo test
```

---

## 5. Migration from 0.2.0 / 0.1.x (16-byte era)

1. Follow §2–§4, choosing `CanFrame16`.
2. Renames: `CANFrame` → `CanFrame16`; fields `can_cobid`/`can_len`/`can_data` →
   methods `cobid()`/`len()`/`data()`.
3. COB-IDs: raw `u32` → `CobId` (`CobId::from(u32)`, `u32::from(cobid)`); the
   function-code mapping lives in `src/raw.rs`.
4. Wire bytes are preserved (LE COB-ID, 3 padding bytes) — SocketCAN buffers
   round-trip unchanged.
5. The transient **little-endian 13-byte** layout that existed briefly inside
   0.2.1 (`a81c977`–`51a8ff7`) is not reproduced by either impl; if you must talk
   to a peer frozen on that transient layout, convert bytes manually.

---

## 6. New in 0.2.1/0.3.0 (additive — optional)

SDO **block transfer** (CiA 301 §7.2.4.7/.8) is available since 0.2.1 and unchanged
in 0.3.0:

```rust
let out = client.input(SdoInput::BlockRead(index, responder));   // block upload
let out = client.input(SdoInput::BlockWrite(index, value, responder)); // block download
// mid-sub-block segments:
while let Some(req) = client.pump() { send(req); }
// server side: server.pump()
```

### 6.1 New in 0.3.1 (additive — one new output variant)

0.3.1 fixes SDO protocol bugs (segmented download/upload interop, error recovery —
machines return to Idle after any error —, the block upload end acknowledgement,
the server abort-frame encoding, empty block uploads, buffer-overflow guards). The
frame formats and the whole 0.3.0 API are unchanged **except** one new `ClientOutput`
variant:

- The client now **acknowledges the block upload end frame** (CiA 301 §7.2.4.3.12).
  On the end frame it produces `ClientOutput::FinalOutput(BlockUploadEndAck, result)`
  instead of `Done`: transmit the acknowledgement first, then handle the result
  exactly like `Done` (the read responder travels inside the result).

```rust
// before (0.3.0): the client never acked the end frame
//   -> the server stayed in its end state; a new transfer failed with
//      ServerStateResponseMismatch
// after (0.3.1):
match client.input(SdoInput::Frame(frame)) {
    ClientOutput::FinalOutput(req, result) => {
        send_frame(CanFrame13::from_parts(CobId::SdoRequest(node_id), 8, req.into()));
        // transfer complete; dispatch the responder from `result` as for `Done`
    }
    other => { /* as before */ }
}
```

Code that matches `ClientOutput` **exhaustively** must add the new arm; matches
with a catch-all (`_ => …`) compile unchanged. `docs/LIBRARY.md` §6.5/§7.3/§8.1
documents the new flow.

---

## 7. Compile-error troubleshooting

| Error | Cause | Fix |
|---|---|---|
| E0107 "wrong number of generic arguments" | `SdoClient`/`SdoServer`/`Producer` missing the frame parameter | add `CanFrame13` (or `CanFrame16`) as the first type argument |
| E0004 "non-exhaustive patterns: `ClientOutput::FinalOutput(_, _)` not covered" | exhaustive match on `ClientOutput` written for ≤0.3.0 | add the `FinalOutput(req, result)` arm — send the frame, then handle the result like `Done` (see §6.1); or use a catch-all `_ => …` |
| E0412/E0433 "`CanFrame` is not a struct" | code still refers to the old struct | use `CanFrame13` / `CanFrame16` |
| E0599 "no method `data`" / E0609 "no field `data`" | field access on a frame | use methods: `frame.data()`, `frame.cobid()`, `frame.len()` |
| E0282 "type annotations needed" | `SdoInput::Frame(...)` with an unannotated frame | annotate the variable or the generic argument |
| E0277 "`F` does not implement `CanFrame`" | a custom type passed as `F` | derive/implement the trait, or use the provided impls |
| E0308 mismatched frame types | two different `F`s on one instance | use a single frame type per `SdoClient`/`SdoServer`/`Producer` |
| E0210 orphan rule | a generic `impl<F: CanFrame> From<F> for MyLocalType` | implement per concrete format (`CanFrame16`, `CanFrame13`) |

---

## 8. Behavioural checklist (what actually changes on the wire)

- 0.2.1 users on Ethernet-to-CAN adapters: **no byte-level change** with
  `CanFrame13`.
- 0.2.0/0.1.x users: `CanFrame16` restores the 16-byte LE layout.
- COB-ID endianness differs per format (16-byte LE, 13-byte BE) — do not assume one
  order when debugging against a peer.
- Protocol payloads (`[u8; 8]`), SDO/PDO/NMT semantics, block-transfer CRC and
  retransmission behaviour are unchanged.

## 9. Rollback

Pin `funcan-rs = "0.2.1"` in `Cargo.toml` to restore the pre-0.3.0 API. Note the
two APIs diverge only at the frame boundary; keeping the upgrade confined to a
small adapter module (construction + field access + the three type parameters)
makes the migration reversible.
