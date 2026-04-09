# Protocol Definition TOML Reference

Glass scans the `protocols/` directory next to the executable at startup and lists every `*.toml` file as a selectable protocol. Each file describes one protocol.

A definition file has three main parts:

1. `[protocol]` — metadata, frame extraction defaults, sequence-diagram settings
2. `[[protocol.frame_rules]]` — rules that carve a raw byte stream into frames (one or more)
3. `[[messages]]` — patterns that identify and decode the extracted frames (one or more)

---

## 1. `[protocol]` section

```toml
[protocol]
title = "My Protocol"          # required; shown in the protocol selector
frame_idle_threshold_ms = 1.7  # default 5.0
```

| Key | Type | Default | Description |
|---|---|---|---|
| `title` | string | (required) | Display name in the settings UI |
| `frame_idle_threshold_ms` | float | `5.0` | An IDLE shorter than this is treated as an intra-frame gap, not a frame boundary |

### 1.1 `[protocol.sequence]` — sequence diagram (optional)

Controls how the sequence diagram view derives source / destination / master participants.

```toml
[protocol.sequence]
master = "Main"        # always pinned to the leftmost column of the diagram
broadcast = "99"       # if a resolved destination matches this, it is rendered as a broadcast note
source = "{Src}"       # default expression when a message has no sequence_source
destination = "{Dst}"  # default expression for sequence_destination
```

| Key | Type | Description |
|---|---|---|
| `master` | string | Participant pinned to the leftmost column. Useful for master/slave layouts |
| `broadcast` | string | If the resolved destination string equals this, it is shown as a broadcast |
| `source` | string | Default source expression for messages that omit `sequence_source` |
| `destination` | string | Default destination expression |

`source` / `destination` use the **expression syntax** described in §4, the same as `sequence_source` / `sequence_destination` on individual messages.

---

## 2. `[[protocol.frame_rules]]` — frame extraction

When a trigger byte appears in the incoming stream, the matching `frame_rule` is used to slice off one frame's worth of bytes. If multiple rules are defined, the first one whose `trigger` matches wins.

### 2.1 Fixed-length frame

```toml
[[protocol.frame_rules]]
trigger = "05"  # ENQ
length = 4      # 4 bytes total, including the trigger
```

### 2.2 Variable-length frame with terminator + extra bytes

```toml
[[protocol.frame_rules]]
trigger = "02"     # STX
end = "03"         # finish at ETX
end_extra = 2      # also pull in 2 more bytes after ETX (e.g. CRC)
max_length = 256   # safety cap
```

### 2.3 All fields

| Key | Type | Default | Description |
|---|---|---|---|
| `trigger` | hex string | (required) | First byte of the frame, as 2 hex digits, e.g. `"02"` |
| `length` | int | — | Fixed length, including the trigger byte |
| `end` | hex string | — | Terminator byte for variable-length frames |
| `end_extra` | int | `0` | Additional bytes consumed after `end` (checksum / CRC etc.) |
| `max_length` | int | `512` | Safety cap; frames larger than this are dropped |
| `checksum` | table | — | Checksum / CRC verification spec (see below) |

Use either `length` or `end`. If both are set, `length` wins.

### 2.4 `checksum = { ... }` — frame check value

Checks a trailing checksum / CRC and highlights mismatches in the UI.

```toml
checksum = { algorithm = "crc16_arc", range = "after_trigger_to_end", size = 2, endian = "big" }
```

| Key | Type | Default | Description |
|---|---|---|---|
| `algorithm` | enum | (required) | Algorithm identifier (see below) |
| `range` | enum | (required) | Which bytes are fed into the algorithm |
| `size` | int | algorithm default | Width of the trailing check value, in bytes |
| `endian` | enum | `"big"` | Only meaningful for 16-bit algorithms; `"big"` or `"little"` |

#### `algorithm` (snake_case)

| Value | Description |
|---|---|
| `crc16_arc` | poly=0x8005, init=0x0000, refin/refout=true (2 bytes) |
| `crc16_modbus` | poly=0x8005, init=0xFFFF, refin/refout=true (2 bytes) |
| `crc16_ccitt_false` | poly=0x1021, init=0xFFFF (2 bytes) |
| `crc16_xmodem` | poly=0x1021, init=0x0000 (2 bytes) |
| `crc8` | CRC-8/SMBus, poly=0x07, init=0x00 (1 byte) |
| `sum8` | low byte of an 8-bit additive sum (1 byte) |
| `xor8` | byte-wise XOR (1 byte) |
| `bcc` | two's complement of an 8-bit sum (1 byte) |

#### `range` (snake_case)

| Value | Bytes covered |
|---|---|
| `whole_frame_excluding_checksum` | The entire frame minus the trailing check value |
| `trigger_to_end` | From the trigger byte up to (but not including) the trailing check value |
| `after_trigger_to_end` | The byte after the trigger up to the trailing check value |
| `after_trigger_before_end` | The byte after the trigger up to the byte before `end` (`end` itself is excluded) |

---

## 3. `[[messages]]` — message definitions

Once a frame is extracted, Glass converts it to an upper-case hex string and tests each message's `pattern` (regex) in order. The first match wins, and its title, color and field layout are applied.

### 3.1 Minimal example

```toml
[[messages]]
id = "msg_status"
title = "Status<01>"
color = "5B9BD5"
pattern = "^02[0-9A-F]{6}303103[0-9A-F]{4}$"
```

### 3.2 All fields

| Key | Type | Default | Description |
|---|---|---|---|
| `id` | string | (required) | Stable identifier, unique within the protocol. Used by search and UI state |
| `title` | string | (required) | Display name |
| `pattern` | regex string | (required) | Regex matched against the **upper-case hex string** of the frame, e.g. `^02[0-9A-F]{6}03[0-9A-F]{4}$` |
| `color` | hex RGB string | — | Title color, 6 hex digits like `"FF8800"` |
| `first_byte` | hex string | — | Optional first-byte hint that short-circuits regex evaluation when it does not match |
| `sequence_source` | expr string | — | Sequence-diagram source expression (overrides `[protocol.sequence].source`) |
| `sequence_destination` | expr string | — | Sequence-diagram destination expression |
| `fields` | table array | `[]` | Field decoders (see below) |

### 3.3 `[[messages.fields]]` — field decoding

```toml
[[messages.fields]]
name = "Addr"
offset = 2          # byte offset from the start of the frame (0-based, includes the trigger)
size = 2            # number of bytes
inline = true       # if true, displayed inline next to the title in list view
description = "Destination address"  # shown in the field detail view (optional)
```

| Key | Type | Default | Description |
|---|---|---|---|
| `name` | string | (required) | Field name. Referenced from sequence-diagram expressions as `{name}` |
| `offset` | int | (required) | Byte offset from the start of the frame, including the trigger |
| `size` | int | (required) | Width in bytes |
| `inline` | bool | `false` | If true, the field is displayed on the title row in list view |
| `description` | string | — | Description shown in the detail view |

Field values are extracted as **plain ASCII** from the byte slice. If the slice is empty the value is shown as `—`.

---

## 4. Sequence-diagram expression syntax

`sequence_source`, `sequence_destination`, and the corresponding defaults under `[protocol.sequence]` are evaluated as one of:

| Form | Example | Meaning |
|---|---|---|
| `=literal` | `"=Main"` | Leading `=` makes the rest a literal participant name |
| Contains `{field}` | `"{Type}:{Addr}"` | Each `{name}` is replaced with the value of that field |
| Plain field name | `"Addr"` | Use the value of that field directly |

Participant names containing `:` are rewritten to a full-width colon (`：`) and spaces become `_`, since both conflict with Mermaid syntax.

### Example

```toml
[protocol.sequence]
master = "Main"

[[messages]]
id = "req"
title = "Request"
pattern = "^..$"
sequence_source = "=Main"               # always from "Main"
sequence_destination = "{Type}:{Addr}"  # built from field values
```

---

## 5. Complete minimal example

```toml
[protocol]
title = "Demo"
frame_idle_threshold_ms = 2.0

[protocol.sequence]
master = "M"
broadcast = "99"

[[protocol.frame_rules]]
trigger = "05"
length = 4

[[protocol.frame_rules]]
trigger = "02"
end = "03"
end_extra = 2
max_length = 256
checksum = { algorithm = "crc16_arc", range = "after_trigger_to_end", size = 2, endian = "little" }

[[messages]]
id = "enq"
title = "ENQ"
color = "78A0DC"
pattern = "^05[0-9A-F]{6}$"
sequence_source = "=M"
sequence_destination = "{Type}:{Addr}"

[[messages.fields]]
name = "Type"
offset = 1
size = 1
inline = true

[[messages.fields]]
name = "Addr"
offset = 2
size = 2
inline = true

[[messages]]
id = "msg_01"
title = "Status<01>"
color = "5B9BD5"
pattern = "^02[0-9A-F]{6}303103[0-9A-F]{4}$"
sequence_source = "=M"
sequence_destination = "{Type}:{Addr}"

[[messages.fields]]
name = "Type"
offset = 1
size = 1
inline = true

[[messages.fields]]
name = "Addr"
offset = 2
size = 2
inline = true
```

See `protocols/sample.toml` for a longer working example.
