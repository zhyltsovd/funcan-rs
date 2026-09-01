# funcan-rs 0.3.1 — Variable CAN Frame Formats

**Status:** implemented and released in **0.3.0** (branch `dsh-frames`, commit
`e6cbcac` "version 0.3.0: frame format polymorphism"); unchanged in **0.3.1**
(branch `dsh-3.1`, the SDO audit release). Design decisions were confirmed by the
maintainer on 2026-08; the plan below reflects the decisions and the final
implementation, including one deviation forced by Rust's orphan rule (see §2.4).

**Branch:** `dsh-frames` (created from `dsh` at `5ea7d23`). Setup commit `e6cbcac`
bumped the version to 0.3.0; the frame-polymorphism implementation landed there.
Version 0.3.1 (branch `dsh-3.1`) carries the SDO audit fixes and does not touch
the frame formats.

**Goal.** Support both known CAN frame wire formats in the library through
**parametric polymorphism**: a `CanFrame` trait plus two concrete implementations
(16-byte frames and 13-byte frames), with the required device type selected at
compile time per use case.

> **Terminology note.** The task brief calls the two formats "16-bit" and "13-bit"
> frames. The wire buffers are actually **16 bytes** and **13 bytes** long; this
> document uses the byte-accurate terms.

---

## 1. Historical analysis (verified from Git history)

### 1.1 Timeline of the frame formats

| Commit | Date | Version | Frame state |
|---|---|---|---|
| `a07bb98` / `14551a4` ("initial commit") | — | 0.1.0 | earliest sources |
| `f93f3c3` ("version inc", tip of `main`, merge-base of `dsh`) | — | **0.2.0** | `src/raw.rs` defines `CANFrame { can_cobid: u32, can_len: usize, can_data: [u8;8] }` with a **16-byte serialization** and a byte-level `CANFrameMachine` parser |
| `a81c977^` | — | 0.2.x | struct renamed to `CanFrame { cobid: CobId, len: usize, data: [u8;8] }` (typed COB-ID), **but still the 16-byte wire format** |
| `a81c977` ("**new format**") | 2026-05-22 | 0.2.x | switch to the **13-byte layout** `[len][cobid][data]` — with offset bugs (data written at `6..14`, cobid read as `1..4`) |
| `345876d` ("version update") | — | **0.2.1** | version 0.2.0 → 0.2.1 |
| `ae7b09d` ("fix") | — | 0.2.1 | corrects 13-byte offsets (`data 5..13`, `cobid 1..5`); cobid still **little-endian** |
| `7424623` ("interim") | — | 0.2.1 | cobid switched to **big-endian** (`to_be_bytes`/`from_be_bytes`) — the current wire format |
| `39325e2`, `5ea7d23` | — | 0.2.1 | SDO block transfer + documentation (current `dsh` HEAD) |
| `e6cbcac` | — | **0.3.0** | version bump: frame format polymorphism (branch `dsh-frames`) |
| `a14ea83` | — | 0.3.0 | PDO `Consumer::deserialize` byte-offset fix for multi-object mappings |
| (this release) | now | **0.3.1** | version bump: SDO audit fixes (branch `dsh-3.1`) — frame formats unchanged |

`obsolete/raw.rs` (gitignored, not compiled) preserves the old **16-byte**
`CANFrame` (with `can_cobid`/`can_len`/`can_data` fields, little-endian) as a
reference remnant.

### 1.2 Format 16 — SocketCAN-style ("16-bit")

Source: `git show a81c977^:src/raw.rs`, `git show f93f3c3:src/raw.rs`,
`obsolete/raw.rs`.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | cobid | little-endian u32 |
| 4 | 1 | len | DLC |
| 5 | 3 | padding | zero-filled |
| 8 | 8 | data | payload |

**16 bytes total.** Layout matches the Linux SocketCAN `struct can_frame`
(`can_id` u32 | `can_dlc` u8 | `__pad`[3] | `data`[8]) — i.e. frames as delivered by
CAN sockets, e.g. USB-CANABLE devices. Structs that used it: `CANFrame` (raw u32
COB-ID, 0.2.0 era) and later `CanFrame` (typed `CobId`, same 16-byte wire).

### 1.3 Format 13 — compact Ethernet-to-CAN ("13-bit")

Source: current `src/raw.rs` before this change.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | len | DLC |
| 1 | 4 | cobid | **big-endian** (LE in the interim commits `a81c977`–`51a8ff7`) |
| 5 | 8 | data | payload |

**13 bytes total.** Compact framing as produced by Ethernet-to-CAN adapter
gateways. No padding; length byte first.

### 1.4 Structural differences (summary)

| Aspect | 16-byte | 13-byte |
|---|---|---|
| Wire size | 16 | 13 |
| Field order | cobid → len → pad → data | len → cobid → data |
| Padding | 3 zero bytes (`5..8`) | none |
| COB-ID endianness | little-endian | big-endian (final form) |
| Semantic type | `u32` (early) / `CobId` (later) | `CobId` |
| Field names | `can_cobid`/`can_len`/`can_data` | `cobid`/`len`/`data` |
| Parser | `CANFrameMachine` byte state machine (obsolete) | direct slice access |
| Typical source | CAN socket / USB-CANABLE (SocketCAN) | Ethernet-to-CAN adapter |

---

## 2. Decisions (confirmed by the maintainer)

1. **Trait name:** `CanFrame` — the trait owns the natural name; the former struct
   is gone.
2. **Struct names:** `CanFrame16` (SocketCAN-style) and `CanFrame13` (compact).
3. **COB-ID type:** typed `CobId` enum on both formats; endianness is an
   implementation detail of each impl.
4. **Endianness:** preserved per format — 16-byte little-endian, 13-byte
   big-endian (byte-for-byte wire compatibility).
5. **Cross-format conversions:** **none** — the two types are independent;
   bridging is the caller's responsibility.
6. **Serialization API:** `write_to_slice` / `read_from_slice` on the trait.

## 3. Implementation

### 3.1 `CanFrame` trait (`src/raw.rs`)

```rust
pub trait CanFrame: Clone + Copy + PartialEq + Eq + fmt::Debug + Default {
    const WIRE_SIZE: usize;                                    // 16 or 13
    fn from_parts(cobid: CobId, len: usize, data: [u8; 8]) -> Self;
    fn cobid(&self) -> CobId;
    fn len(&self) -> usize;
    fn data(&self) -> [u8; 8];
    fn write_to_slice(&self, buffer: &mut [u8]);               // asserts WIRE_SIZE
    fn read_from_slice(buffer: &[u8]) -> Self;                 // asserts WIRE_SIZE
}
```

- `CanFrame16` implements the historical 16-byte layout (LE COB-ID, 3-byte padding).
- `CanFrame13` implements the current 13-byte layout (len first, BE COB-ID).
- The trait is intentionally **not object-safe** (`read_from_slice -> Self`); the
  library is parametric, not dynamic — no `dyn`, no allocation, stays `no_std`.

### 3.2 Where the generic parameter threads

| File | Change |
|---|---|
| `src/raw.rs` | `CanFrame` trait + `CanFrame16` / `CanFrame13` impls; golden-vector and generic round-trip tests |
| `src/nmt.rs` | `NmtRequest` conversions implemented **per concrete format** (see §3.3) |
| `src/pdo.rs` | `Producer<D, F: CanFrame>` (via `PhantomData<F>`); `serialize(&self, node_id) -> F`; `PdoMap<'a, D, F>` |
| `src/sdo/client.rs` | `SdoClient<N, F: CanFrame, R, W, D>`; `SdoInput<F, R, W, D>` with `Frame(F)`; frame payload passed via `frame.data()` |
| `src/sdo/server.rs` | `SdoServer<N, F: CanFrame, D>`; `handle_frame(frame: F)` |

Unaffected (frame-agnostic, operate on `CobId` + `[u8;8]` only): `sdo/machines.rs`,
`heartbeat.rs`, `dictionary.rs`, `machine.rs`, `interfaces.rs`, `sdo.rs` codecs.

The generic parameter is carried by the facade **types** — the device type is fixed
at compile time: `SdoClient<1024, CanFrame13, ...>` for an Ethernet-to-CAN adapter,
`SdoClient<1024, CanFrame16, ...>` for a USB-CANABLE device.

### 3.3 Deviation forced by the orphan rule

The original plan specified `impl<F: CanFrame> From<F> for NmtRequest` (and the
reverse). Both directions are **illegal** under RFC 2451: a type parameter must
appear (covered) in the trait arguments, and it cannot be an uncovered type
parameter before the first local type. Consequently:

- `impl From<NmtRequest> for CanFrame16` and `impl From<NmtRequest> for CanFrame13`
  (shared `NmtRequest::into_parts` helper);
- `impl From<CanFrame16> for NmtRequest` and `impl From<CanFrame13> for NmtRequest`
  (shared `NmtRequest::from_frame_payload` helper).

The same per-format pattern applies anywhere a local type converts from/to an
arbitrary `F: CanFrame`.

---

## 4. Breaking API changes (0.2.1 → 0.3.0)

1. `CanFrame` struct → trait; concrete types `CanFrame16` / `CanFrame13`.
2. `SdoClient<N, R, W, D>` → `SdoClient<N, F, R, W, D>` (F: CanFrame).
3. `SdoServer<N, D>` → `SdoServer<N, F, D>`.
4. `SdoInput::Frame(CanFrame)` → `SdoInput::Frame(F)`.
5. `NmtRequest ↔ CanFrame` conversions → per-format impls for `CanFrame16`/`CanFrame13`.
6. `Producer<D>::serialize() -> CanFrame` → `Producer<D, F>::serialize() -> F`.
7. `write_to_slice` / `read_from_slice` moved onto the trait (same behaviour per format).

**Unchanged:** machine APIs, codecs, `CobId`, `CanIndex`, `[u8;8]` payloads, output
enums, heartbeat/PDO mapping semantics, `crc16_ccitt`.

---

## 5. Migration guide

- Users of the current 13-byte format: replace `CanFrame` with `CanFrame13`.
  Mechanical; wire behaviour identical.
- Users of the 16-byte SocketCAN format: use `CanFrame16`; LE COB-ID and 3-byte
  padding restored as in 0.2.0-era and `obsolete/`.
- Mixed setups select the frame type per instance at compile time — no runtime cost.

---

## 6. Test strategy (implemented)

- **Golden vectors:** exact byte sequences for both formats (e.g. an SDO request
  `0x600+2` frame → 13-byte `[08 00 00 06 02 …]` and 16-byte `[02 06 00 00 08 00 00
  00 …]`).
- **Round-trips:** `read_from_slice(write_to_slice(f)) == f` for each format.
- **Generic round-trip:** a `fn round_trip<F: CanFrame>()` exercised for both
  formats through the trait.
- **NMT conversions:** `NmtRequest ↔ CanFrame16` / `NmtRequest ↔ CanFrame13`.
- **Regression:** all pre-existing tests pass with `CanFrame13`-driven facades;
  the machines themselves are frame-agnostic and untouched.

Result: **48 tests**, all passing (`cargo test`).

---

## 7. Implementation checklist (done)

1. ✅ `src/raw.rs`: `CanFrame` trait; `CanFrame16` (16-byte LE layout from
   `a81c977^`/`obsolete`) and `CanFrame13` (13-byte BE layout from HEAD); golden-
   vector, round-trip and generic round-trip tests.
2. ✅ `src/nmt.rs`: per-format `NmtRequest` conversions (orphan-rule compliant) +
   tests for both formats.
3. ✅ `src/pdo.rs`: `Producer<D, F>` / `PdoMap<'a, D, F>`; `serialize` returns `F`.
4. ✅ `src/sdo/client.rs`, `src/sdo/server.rs`: `F: CanFrame` threaded through
   `SdoClient` / `SdoInput` / `SdoServer` / `handle_frame`.
5. ✅ Docs updated: this file, `docs/LIBRARY.md`, `README.md`.
6. ✅ `cargo check` clean (only pre-existing warnings), `cargo test` 48/48.
