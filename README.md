# X806 Cross-Platform Rust Suite

Open-source, cross-platform configurator, macro manager, and EEPROM flasher for the **X806 / K806** 8-key (2×4) mechanical macro keypad (`VID 0x30FA / PID 0x1340`, Wuxi Instant Microelectronics USB HID MCU).

Replaces the Windows-only `X806.exe` configurator with native **Linux**, **macOS**, and **Windows** support — zero background daemon required after flashing to the keypad's on-board EEPROM.

## Workspace Layout

- `crates/x806-core` — Protocol library: USB HID transport (`hidapi`), deterministic EEPROM compiler, safety checks, `config.ini` / `.dat` / TOML codecs, and keycode tables.
- `crates/x806-cli` (`x806`) — Scriptable CLI for status probing, config flashing, bundle import/export, `.dat` macro inspection, and Linux `udev` setup.
- `udev/99-x806.rules` — Linux `hidraw` permissions rule for non-root access.
- `docs/protocol-spec.md` — Complete reverse-engineered HID, EEPROM, and file-format specification.

> **Note on GUI (`x806-gui`):** The native `eframe`/`egui` desktop configurator is planned in the workspace roadmap alongside `x806-core` and `x806-cli`.

## Building from Source

Requires Rust (`1.85+` / Edition 2024):

### Linux

`x806-core` uses `hidapi` with the pure-Rust `linux-native` (`hidraw`) backend, so no `libudev-dev` or `libusb-1.0-0-dev` C headers are required to build:

```bash
cargo build --release -p x806-cli
```

Install the bundled `udev` rule so your user can access `/dev/hidraw*` without `sudo`:

```bash
sudo cp udev/99-x806.rules /etc/udev/rules.d/99-x806.rules
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Unplug and replug the keypad if it was already connected (or print the rule anytime with `x806 udev`).

### macOS & Windows

```bash
cargo build --release -p x806-cli
```

No extra drivers or administrator privileges are required on macOS (`IOHIDManager`) or Windows (built-in HID class driver).

## CLI Quickstart (`x806`)

```bash
# 1. Probe attached hardware (read-only)
cargo run -p x806-cli -- status

# 2. Import an existing Windows X806.exe config.ini + macro bundle to TOML
x806 import path/to/config.ini --macros-dir path/to/macro --out ~/.config/x806/config.toml

# 3. Preview an EEPROM write plan safely without touching hardware
x806 apply ~/.config/x806/config.toml --dry-run

# 4. Flash a TOML config (or Windows config.ini directly) to the keypad
x806 apply ~/.config/x806/config.toml

# 5. Reset to factory defaults (keys 1..8 -> HID '1'..'8', Cycle breath, 500 Hz)
x806 reset --dry-run
x806 reset

# 6. Export TOML back to X806.exe-compatible config.ini + macroGrp.ini + <group>.dat
x806 export ~/.config/x806/config.toml --out exported/config.ini --macros-dir exported/macro

# 7. Inspect any .dat macro library (plain decimal, legacy, or ISD=1 Base64)
x806 macro inspect path/to/Streaming.dat

# 8. Print Linux udev rules
x806 udev
```

If `[path]` is omitted from `x806 apply`, it reads from the OS standard configuration directory (`~/.config/x806/config.toml` on Linux, `~/Library/Application Support/x806/config.toml` on macOS, `%APPDATA%\x806\config\config.toml` on Windows).

## Notes & Safety

- **EEPROM safety:** `x806-core` validates every write plan before sending any HID report, refusing writes to protected factory bytes (`0x0000`, `0x0002..=0x0007`), preserving `0x0076[7:2]` via read-modify-write, and aborting if compiled macros exceed the device's reported capacity (`0x07F8` or `0x0FF8`).
- **Windows-only OS settings:** `DPI_SPEED` (pointer precision) and `WHEEL_SPEED` in `config.ini` are Windows `SystemParametersInfoW` settings never stored in keypad EEPROM. They are preserved on TOML/INI round-trip (`legacy_os_settings`) and intentionally omitted from OS pointer calls.

## Testing

```bash
# Offline unit, codec, EEPROM compiler, mock HID, and CLI integration tests
cargo test --workspace

# Strict lint check
cargo clippy --workspace --all-targets -- -D warnings
```

## License

MIT
