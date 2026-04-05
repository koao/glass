# Glass

[日本語](README.ja.md) | **English**

A half-duplex serial monitor for Windows, built with Rust and egui.

![Glass Screenshot](docs/screenshots/main.png)

## Features

- **HEX / ASCII display modes** — switch on the fly
- **IDLE detection** — configurable threshold (ms) with visual markers
- **Mixed pattern search** — combine hex bytes (`$XX`) and ASCII text in one query (e.g. `OK$0D$0A`)
- **Save / Load** — export and import captured data in `.glm` format with timing preserved
- **Screenshot** — capture the current window as PNG
- **Bilingual UI** — Japanese / English, switchable in settings
- **Serial configuration** — baud rate, data bits, parity, stop bits
- **Dark theme** — eye-friendly for long monitoring sessions
- **Error tracking** — framing, overrun, and parity error counts

## Requirements

- Windows 10 / 11
- Rust toolchain (for building from source)

## Build & Run

```bash
cargo build --release
cargo run --release
```

## Usage

1. Select a COM port and configure serial parameters in **Settings**
2. Click **Start** to begin receiving data
3. Toggle between **HEX** and **ASCII** display modes
4. Use **Ctrl+F** to open the search bar
   - Hex bytes: `$0D$0A`
   - ASCII text: `OK`
   - Mixed: `OK$0D$0A`
5. **Pause** to freeze the display while continuing to buffer data
6. **Save** to export captured data as `.glm`, or **Load** to import a previous session

## File Format (.glm)

Glass Monitor files (`.glm`) are JSON-based and store:

- Serial configuration used during capture
- Byte data with microsecond-precision relative timestamps
- IDLE markers

## License

MIT
