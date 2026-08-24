# funcan-rs

CANopen application-layer library for embedded systems, written in `no_std` Rust.

## Project Status

Early-stage development: features and documentation are subject to change. The most
complete service is SDO, with **expedited, segmented, and block transfers**
(CiA 301 §7.2.4) on both client and server side, including CRC-16/CCITT checking and
sub-block retransmission. NMT is currently a frame codec only, and heartbeat/PDO
support is partial.

## Documentation

- [**`docs/LIBRARY.md`**](docs/LIBRARY.md) — full library reference: architecture,
  module map, protocol details (including exact SDO block-transfer wire formats),
  machine state tables, usage patterns, and an extension guide. Written to be useful
  for both humans and AI agents.

## Quick look

- `src/raw.rs` — the `CanFrame` trait (parametric over the wire format) with two
  implementations: `CanFrame16` (16-byte SocketCAN-style, for CAN sockets /
  USB-CANABLE) and `CanFrame13` (13-byte compact, for Ethernet-to-CAN adapters);
  plus `CobId` (CiA 301 function-code mapping).
- `src/dictionary.rs` — `CanIndex`, `Dictionary`, `DictionaryValue` (trait-based
  object dictionary; no concrete OD is provided).
- `src/sdo/` — SDO codecs (`sdo.rs`), state machines (`sdo/machines.rs`), and the
  `SdoClient` / `SdoServer` facades (`sdo/client.rs`, `sdo/server.rs`).
- `src/nmt.rs`, `src/heartbeat.rs`, `src/pdo.rs`, `src/machine.rs`,
  `src/interfaces.rs` — NMT codec, heartbeat consumer, PDO producer/consumer, the
  `MealyMachine` trait, and driver-facing interface traits.

No CAN hardware driver is included: applications pick the frame format per device
(`CanFrame16` or `CanFrame13`) and feed raw frames in, sending the frames the
machines produce. See `docs/LIBRARY.md` §8 for usage patterns and
`docs/FORMAT.md` for the frame-format design.

## Contributing

Contributions are welcome! Please open issues or submit pull requests for any
improvements or feature suggestions.

## License

This project is licensed under the MIT License. See the LICENSE file for more details.

## Contact

For further information, questions, or suggestions, feel free to reach out to me at
zhyltsovd@gmail.com.

## TO-DO List

- [x] Base
  - [x] State machine trait (`MealyMachine`)
  - [x] Raw CAN frames (`CanFrame` trait + `CanFrame16` / `CanFrame13`)
- Core CANopen functionalities
  - [x] SDO client and server
    - [x] Expedited transfers
    - [x] Segmented transfers
    - [x] **Block transfers** (download + upload, CRC-16/CCITT, retransmission)
    - [x] SDO abort codes (CiA 301)
  - [x] NMT frame codec (commands, states, node targeting)
  - [x] Heartbeat consumer (liveness tracking)
  - [x] Static PDO mapping (producer/consumer)
  - [ ] NMT master / slave runtime (lifecycle state machine, boot-up)
  - [ ] SYNC producer / consumer
  - [ ] EMCY (emergency) messages
  - [ ] Heartbeat producer / node-guard
  - [ ] Dynamic PDO mapping, synchronous / asynchronous PDO triggering
  - [ ] Timeouts for SDO transfers
  - [ ] COB-ID dispatcher for incoming frames
