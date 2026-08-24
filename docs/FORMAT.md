# funcan-rs 0.3.0 — Variable CAN Frame Formats (Implementation Plan)

**Status:** plan only — **no implementation code yet**. Awaiting approval.

**Branch:** `dsh-frames` (created from `dsh` at `5ea7d23`; setup commit `e6cbcac` bumps
the version to 0.3.0).

**Goal.** Support both known CAN frame wire formats in the library through
**parametric polymorphism**: a `CanFrame` trait plus two concrete implementations
(16-byte frames and 13-byte frames), with the required device type selected at
compile time per use case.

> **Terminology note.** The task brief calls the two formats "16-bit" and "13-bit"
> frames. Git analysis shows the wire buffers are actually **16 bytes** and
> **13 bytes** long; this document uses the byte-accurate terms "16-byte" and
> "13-byte" (identical to the brief's concepts).

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
| `e6cbcac` | now | **0.3.0** | version bump (this branch) |

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

Source: current `src/raw.rs` (HEAD).

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

## 2. Design — parametric polymorphism

### 2.1 Naming (decision needed)

The task brief says "define a `CANFrame` trait". Rust naming convention is
`CanFrame`, but `CanFrame` is currently a **struct** name. Proposal:

- **Trait:** `CanFrame` (the struct is renamed — see below). Alternative:
  keep the struct `CanFrame` and name the trait `Frame` / `CanFrameTrait`.
  Recommendation: **rename the structs** so the trait owns the natural name.
- **Structs:** `CanFrame16` (SocketCAN-style) and `CanFrame13` (compact), or the
  more descriptive `SocketCanFrame` / `EthernetCanFrame`. Recommendation:
  `CanFrame16` / `CanFrame13` (format-focused, matches the brief's framing);
  provide type aliases for the descriptive names.

### 2.2 Trait surface (proposal)

```text
trait CanFrame: Clone + Copy + PartialEq + Eq + Debug + Default {
    const WIRE_SIZE: usize;                    // 16 or 13
    fn from_parts(cobid: CobId, len: usize, data: [u8; 8]) -> Self;
    fn cobid(&self) -> CobId;
    fn len(&self) -> usize;
    fn data(&self) -> [u8; 8];
    fn write_to_slice(&self, buf: &mut [u8]);  // asserts buf.len() >= WIRE_SIZE
    fn read_from_slice(buf: &[u8]) -> Self;    // asserts buf.len() >= WIRE_SIZE
}
```

- `CanFrame16` implements the exact historical layout (LE COB-ID, 3-byte padding)
  so SocketCAN buffers round-trip byte-for-byte.
- `CanFrame13` implements the current layout (len first, BE COB-ID) so the existing
  wire behaviour is preserved exactly.
- `CobId` stays the semantic identifier type on both; the endianness difference is
  an implementation detail of each `write_to_slice`/`read_from_slice`.

### 2.3 Where the generic parameter threads

`CanFrame` is referenced by exactly **four** active locations (grep-verified); the
protocol machines are frame-agnostic and need **no** changes:

| File | Current usage | 0.3.0 change |
|---|---|---|
| `src/raw.rs` | `CanFrame` struct + impls | trait + `CanFrame16`/`CanFrame13` impls; tests per format |
| `src/nmt.rs` | `impl Into<CanFrame> for NmtRequest`, `impl From<CanFrame> for NmtRequest` | `NmtRequest` becomes generic: `impl<F: CanFrame> Into<F>`, `impl<F: CanFrame> From<F>` |
| `src/pdo.rs` | `Producer<D>::serialize(&self, node_id) -> CanFrame` | `Producer<D, F: CanFrame>::serialize(...) -> F` |
| `src/sdo/client.rs` | `SdoInput::Frame(CanFrame)`; `SdoClient<N, R, W, D>` | `SdoInput<F: CanFrame>::Frame(F)`; `SdoClient<N, F, R, W, D>` |
| `src/sdo/server.rs` | `handle_frame(&mut self, frame: CanFrame)`; `SdoServer<N, D>` | `handle_frame(&mut self, frame: F)`; `SdoServer<N, F, D>` |
| `src/lib.rs` | module list | re-export trait/structs; `pub type SocketCanFrame = CanFrame16; pub type EthCanFrame = CanFrame13;` |

Unaffected (frame-agnostic, operate on `CobId` + `[u8;8]` only): `sdo/machines.rs`,
`heartbeat.rs`, `dictionary.rs`, `machine.rs`, `interfaces.rs`, `sdo.rs` codecs.

The generic parameter is carried by the facade/machine **types** (not dyn, not
heap), so the device type is fixed at compile time exactly as required:
`type DeviceFrame = CanFrame16;` → `SdoClient<1024, DeviceFrame, ...>`.

---

## 3. Breaking API changes (0.2.1 → 0.3.0)

1. `CanFrame` struct → trait; concrete types `CanFrame16` / `CanFrame13`.
2. `SdoClient<N, R, W, D>` gains a frame type parameter → `SdoClient<N, F, R, W, D>`.
3. `SdoServer<N, D>` gains a frame type parameter → `SdoServer<N, F, D>`.
4. `SdoInput::Frame(CanFrame)` → `SdoInput::Frame(F)`.
5. `NmtRequest → CanFrame` / `CanFrame → NmtRequest` conversions become generic.
6. `Producer<D>::serialize()` return type becomes `F`.
7. `write_to_slice` / `read_from_slice` move onto the trait (same behaviour per
   format).

**Unchanged:** all machine APIs, codecs, `CobId`, `CanIndex`, `[u8;8]` payloads,
`NoFrame`/output enums, heartbeat/PDO mapping semantics, `crc16_ccitt`.

---

## 4. Migration guide

- Users of the current 13-byte format: replace `CanFrame` with `CanFrame13`
  (or `EthCanFrame` alias). Mechanical; wire behaviour identical.
- Users of the 16-byte SocketCAN format: switch to `CanFrame16`
  (or `SocketCanFrame`); LE COB-ID and 3-byte padding restored as in 0.2.0-era
  and `obsolete/`.
- Mixed setups (both device kinds on one node) select the frame type per instance
  at compile time — no runtime cost.

---

## 5. Test strategy

- **Golden vectors:** exact byte sequences for both formats (derived from the
  historical implementations) — e.g. SDO request `0x600+node` frame → the 16-byte
  LE layout and the 13-byte BE layout.
- **Round-trips:** `read_from_slice(write_to_slice(f)) == f` for each format.
- **Cross-format:** same logical frame converted 16 ↔ 13 yields the correct,
  format-specific bytes.
- **Regression:** all 44 existing tests continue to pass with `CanFrame13`
  selected (the current default behaviour).
- **Parser parity (optional):** the obsolete `CANFrameMachine` can serve as a
  reference oracle for 16-byte parsing edge cases.

---

## 6. Open questions (confirm before implementation)

1. Trait name (`CanFrame` with renamed structs — recommended) vs keeping the
   struct and naming the trait `Frame`/`CanFrameTrait`.
2. Struct names: `CanFrame16`/`CanFrame13` (recommended) vs
   `SocketCanFrame`/`EthernetCanFrame`.
3. Keep `CobId` as the semantic type on both formats (recommended) vs exposing
   raw `u32`.
4. Preserve per-format endianness exactly (16-byte LE, 13-byte BE) — recommended
   for wire compatibility.
5. Should the two types be mutually convertible (`TryFrom<CanFrame16> for
   CanFrame13` and vice versa)?
6. Keep `write_to_slice`/`read_from_slice` on the trait (recommended) vs separate
   `From`/`TryFrom` conversions.

---

## 7. Implementation checklist (blocked on approval)

1. `src/raw.rs`: introduce `CanFrame` trait; add `CanFrame16` (16-byte LE layout
   from `a81c977^`/`obsolete`) and `CanFrame13` (13-byte BE layout from HEAD);
   golden-vector + round-trip tests per format.
2. `src/nmt.rs`: make `NmtRequest` conversions generic over `F: CanFrame`.
3. `src/pdo.rs`: `Producer` gains `F: CanFrame`; `serialize` returns `F`.
4. `src/sdo/client.rs`, `src/sdo/server.rs`: thread `F: CanFrame` through
   `SdoClient`/`SdoInput`/`SdoServer`/`handle_frame`.
5. `src/lib.rs`: re-exports and `SocketCanFrame`/`EthCanFrame` aliases.
6. Cross-format conversion tests + full regression run.
7. Update `README.md` and `docs/LIBRARY.md` (API surface, migration notes).
