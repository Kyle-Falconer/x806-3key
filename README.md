# X806 Cross-Platform Rust Suite

Open-source, cross-platform configurator, macro manager, and EEPROM flasher for the **X806 / K806** 8-key (2×4) mechanical macro keypad (`VID 0x30FA / PID 0x1340`, Wuxi Instant Microelectronics USB HID MCU).

Replaces the Windows-only `X806.exe` configurator with native **Linux**, **macOS**, and **Windows** support — zero background daemon required after flashing to the keypad's on-board EEPROM.

## Workspace Layout

- `crates/x806-core` — Protocol library: USB HID transport (`hidapi`), deterministic EEPROM compiler, safety checks, `config.ini` / `.dat` / TOML codecs, and keycode tables.
- `crates/x806-cli` (`x806`) — Scriptable CLI for status probing, config flashing, bundle import/export, `.dat` macro inspection, and Linux `udev` setup.
- `crates/x806-gui` (`x806-gui`) — Native cross-platform `eframe`/`egui` desktop configurator covering key remapping, live macro recording/editing, polling/fire settings, and RGB LED control.
- `udev/99-x806.rules` — Linux `hidraw` permissions rule for non-root access.
- `docs/protocol-spec.md` — Complete reverse-engineered HID, EEPROM, and file-format specification.

## Building from Source

Requires Rust (`1.88+` / Edition 2024):

### Linux

`x806-core` uses `hidapi` with the pure-Rust `linux-native-basic-udev` (`hidraw` + `basic-udev`) backend, so no `libudev-dev`, `libusb-1.0-0-dev`, or `pkg-config` C headers are required to build the protocol library or CLI:

```bash
# Build the entire workspace (x806 CLI + x806-gui desktop app)
cargo build --release --workspace

# Or CLI-only (headless)
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
cargo build --release --workspace
```

No extra drivers or administrator privileges are required on macOS (`IOHIDManager`) or Windows (built-in HID class driver).

## GUI Quickstart (`x806-gui`)

Launch the native desktop configurator:

```bash
cargo run --release -p x806-gui
```

- **Base Setting:** Interactive 2×4 physical keypad layout, categorized action picker (`Direct Mouse & DPI`, `Extend`, `Media`, `Macro`, `Shortcut`, `Mode`), and a `Record Key...` modal for single-key bindings.
- **Macro:** Macro group and item CRUD, live keyboard/modifier recorder with millisecond timing and stuck-key protection, quick-insert buttons (modifiers, mouse buttons, delays, XY movement, DPI), editable step table, and repeat modes (`Run Until Release`, `Run Until Click`, `Repeat On`, `Run Once`, plus `Finish cycle on stop`).
- **Advanced Setting:** USB report rate (`125` / `250` / `500` / `1000 Hz`) and Fire key interval (`50`–`300 ms`).
- **LED Setting:** Standard modes (`Cycle breath`, `Button response`, `Mute`), collapsible advanced firmware modes & brightness, 4-slot active-low 4-bit RGB cycle palette, and per-key Button Response styles.
- **Online & Offline Workflow:** Automatically probes attached hardware on startup (`Refresh` to re-probe) and switches between active profiles (`Game I`, `Game II`, `Game III`). Clicking **Apply** flashes directly to EEPROM and persists local TOML when connected, or validates the EEPROM plan offline and saves locally when disconnected. Supports **Import** / **Export** (TOML or `X806.exe` `config.ini` + `.dat` bundles), **Reset**, and **Cancel**.

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
#    (--macros-dir is optional and defaults to <out_dir>/macro)
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
