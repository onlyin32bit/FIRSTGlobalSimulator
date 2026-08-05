# FGS1 match state protocol

Match state is sent as binary WebSocket messages. Client input and ping/pong
remain JSON text messages because they are small and infrequent.

All numeric values are little-endian. The 16-byte envelope is:

| Offset | Type | Meaning |
| --- | --- | --- |
| 0 | 4 bytes | `FGS1` identifier |
| 4 | `u16` | major version (`1`) |
| 6 | `u16` | compatible minor version (`2`) |
| 8 | `u16` | message type (`1` = state snapshot) |
| 10 | `u16` | envelope flags |
| 12 | `u32` | payload byte length |

The payload contains sections. Each section starts with `tag:u16`,
`flags:u16`, and `byte_length:u32`, followed by exactly `byte_length` bytes.
Readers must skip unknown tags. Existing field meanings and widths are never
changed; additions use a new section tag or append to a section in a new minor
version.

Current section tags:

1. Pack and object metadata.
2. Physics tick, authoritative wall clock, simulation clock, and lag.
3. Timing, contact, broad-phase, sleeping, process CPU, and RSS metrics.
4. Length-prefixed player records.
5. A count followed by tightly packed `f32 x/y/z` ball positions.
6. Pack-provided physical material and robot properties.

The Rust encoder and TypeScript decoder are the executable specification.
Protocol tests validate the identifier, declared payload length, and maximum
size of a 1,000-ball state frame.
