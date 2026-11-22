# X806 Cross-Platform Rust Suite — Design Document

**Date:** 2026-11-21
**Repository:** `C:\Users\renan\source\repos\X806`
**Hardware Target:** X806 / K806 8-key (2×4) mechanical macro keypad (`VID 0x30FA / PID 0x1340`, Wuxi Instant Microelectronics USB HID MCU)
**Protocol Reference:** [docs/protocol-spec.md](../../protocol-spec.md)

---

## 1. Intent & Why

The stock `X806.exe` configurator only ships for Windows (32-bit MFC), leaving Linux and macOS users unable to program their X806 8-key macro keypad. Because the keypad stores all key bindings, macros, fire speed, USB polling rate, and LED settings in its on-board 4 KB EEPROM, a cross-platform Rust configurator that speaks the native HID protocol unlocks full hardware functionality on Linux, macOS, and Windows with **zero background daemon required** after flashing.

**"Done"** means a Linux/macOS/Windows user can use every hardware-backed feature present in `X806.exe` (all 4 tabs, 3 profiles, live macro recording, Import/Export/Reset, and Apply to hardware), plus automate configuration from the command line.

---

## 2. Goals, Non-Goals & Anti-Goals

### Goals
1. **Full hardware feature parity with `X806.exe`:**
   - **3 active profiles:** `Game I` (EEPROM profile slot `0`), `Game II` (slot `2`), `Game III` (slot `3`), matching `X806.exe` (`slot 1` kept zeroed for firmware compatibility).
   - **Base Setting (2×4 keypad visual layout):** Bind all 8 keys (`1`–`8`, indexed `0..7` in `config.ini` `K0..K7` and mapped via `KEYMAP = [7, 3, 2, 1, 9, 6, 5, 4]` to 1-based hardware slots `hw_slot`) to any supported action: Mouse buttons (`1..6`), Extend functions (`12..18`: Show Desktop, Double Click, Fire, LED Loop, DPI±, Fn), Media Consumer keys (`23..32`: 10 usages — Browser, Player, Mail, Vol+, Vol-, Previous, Next, Mute, Play/Pause, Calculator), User Macros (`>= 256`), Built-in Shortcuts (`33..40`: Copy, Paste, My Computer, Undo, Lock PC, Close Window, Select All, Find), Profile Mode switches (`19..21`: Game I/II/III -> `0x17/0x19/0x1A`), or Single Keyboard Key (`42,<hid>`). *(Non-functional legacy UI entries `22=Mode/Media` and `41=Mode Loop` are preserved on `config.ini` round-trip.)*
   - **Macro Editor & Live Recorder:** Manage macro groups and macros, live press/release keyboard recording (capturing down/up timing in ms, deduplicating auto-repeat, tracking held keys), manual insertion/editing/reordering/deletion of key events, mouse click events (`LMB`/`RMB`/`MMB`), relative XY moves (`0xF6`), DPI changes (`0xF8`), delays, and repeat modes (`Run Until Release`, `Run Until Click`, `Repeat N times`). `LEFT_MAC` is preserved on `config.ini` round-trip (and supported in `x806-core` EEPROM compilation) without cluttering the keypad GUI.
   - **Advanced Setting (hardware-backed):** USB polling rate (`125 / 250 / 500 / 1000 Hz`) and Fire key interval (`50–300 ms`).
   - **LED Setting:** All exposed LED modes (`Cycle breath` = `1`, `Button response` = `3` with per-key reactive enable & style, `Mute`/off = `4`, plus optional access to distinct firmware modes `0, 2, 5, 6, 7` and brightness `0..7`), 4-slot color cycle enable checkboxes (`CYC_7COL_EN` bits `0..3` -> `0x001D` bits `0, 2, 4, 6`, with `LED_USER_COLER_EN` in `0x001D[7]`) + 4-bit active-low RGB color pickers (`DPI0..5` -> `0x0020..0x0031`, where `DPI0..3` correspond to the 4 cycle slots), and active-mode cycle speed slider (`0..=6`; `7` is firmware-reserved for `Mute / Off`, so `0x001C` needs no RMW).
   - **Config Lifecycle:** `Import`, `Export`, `Reset` (restores factory defaults: keys `1..8` mapped to HID `'1'..'8'`), `Apply` (flashes EEPROM + sends reload), and `Cancel` (reverts unsaved UI edits).
2. **Cross-platform first-class Linux, macOS, and Windows support** via `hidapi` (`linux-native` / `hidraw` on Linux, native HID on macOS and Windows), plus bundled Linux `udev` rules (`99-x806.rules`) for non-root access.
3. **100% file compatibility with `X806.exe`** for importing/exporting existing `config.ini` and `.dat` macro files (both `DAT_VER=3` and legacy, plain decimal and `ISD=1` Base64), alongside clean native TOML persistence as the primary app config format.
4. **EEPROM safety:** Never write `0x0000` or `0x0002..0x0007` (protects factory `0x5A`/PID/VID), always perform read-modify-write on `0x0076`, always validate macro bytecode size against the device's reported EEPROM limit (`0x07F8` or `0x0FF8` from `GetFeature(0x07)`), and support `--dry-run` / mock transport for full offline testing.

### Non-Goals
- **Windows-only OS settings:** `Pointer precision` (`DPI_SPEED`) and `Wheel speed` (`WHEEL_SPEED`) in `X806.exe`'s Advanced tab only call Windows `SystemParametersInfoW` and are never written to EEPROM. They will not be wired to OS pointer/scroll settings (we will clearly note this in the UI/docs rather than faking hardware persistence).
- **PUBG live recoil background daemon:** The `0xFF00` live recoil-tuning daemon (`FUN_0042dab0`) is not required for core keypad usage; `x806-core` will still support reading `0xFF00` events cleanly (and decoding `PUBG_*.dat` macros) so a listener subcommand can be added trivially if ever wanted.
- **Pixel-identical bitmap skinning:** The GUI should recreate the clean 4-tab + 2×4 keypad ergonomics of `X806.exe` using native `egui` widgets (clean dark theme), not bundle Wuxi Instant's copyrighted `.jpg`/`.bmp` skin assets.

### Anti-Goals
- **Bricking or corrupting device EEPROM:** Any bug that writes to `0x0000` or `0x0002..0x0007`, overflows the reported macro flash limit, or skips the `0x0076` read-modify-write is an unacceptable failure.
- **Requiring root/admin privileges** on Windows or properly udev-configured Linux systems.
- **Silent failure on hardware errors:** Every HID/EEPROM transfer error or disconnected state must surface clearly in both CLI exit codes and GUI status banners.

---

## 3. Constraints

- **Toolchain:** Rust (`rustc 1.99.0`, `cargo 1.99.0`), Cargo workspace in `C:\Users\renan\source\repos\X806`.
- **Hardware Protocol (live-verified on attached X806):**
  - `VID = 0x30FA`, `PID = 0x1340`.
  - Feature control collection: `UsagePage = 0xFF01`, `Usage = 0x0001`, `FeatureReportByteLength = 8`, Report ID `0x07`.
  - Event read collection: `UsagePage = 0xFF00`, `Usage = 0x0001`, `InputReportByteLength = 3`, Report ID `0x06`.
  - Exact timing constraints from `X806.exe`: `Sleep(4 ms)` after staging block before commit (`0x09`), `Sleep(2 ms)` after commit before idle (`0x00`), `Sleep(20 ms)` after apply (`0x10`) before idle (`0x00`).
  - Active-low 4-bit PWM RGB channels (`0` = 100% ON, `15` = OFF).
  - Exact `config.ini` quirks: inline `//` comments, mixed line endings, duplicate keys across `[Config0]`/`[ShareData]` (`[ShareData]` is authoritative for shared fields).
  - Exact `.dat` quirks: 3-digit zero-padded decimal bytes, UTF-16LE 60-byte names, Windows VK codes on disk translated to/from USB HID Usage IDs (fixing `X806.exe`'s `` `~ `` `0x32` single-key bug to standard `0x35`).
- **Testing:** Because live hardware is physically attached on this Windows workstation, all unit/codec/compiler tests run offline against a `HidTransport` trait mock AND against exact golden fixtures from `C:\Program Files (x86)\K806`, while live hardware integration tests default to read-only (`status` / `GetFeature(0x07)` / read `0x0076`) and require an explicit opt-in flag (`--write` / `apply`) before mutating EEPROM.

---

## 4. Approaches Considered

| Approach | Summary | Trade-offs | Decision |
| :--- | :--- | :--- | :--- |
| **A. `x806-core` + `x806-cli` + `x806-gui` (`eframe`/`egui`)** | Shared protocol/codec library + scriptable CLI + native cross-platform desktop GUI recreating all 4 tabs and live press/release macro recording. | Larger compile footprint (`eframe`/`egui`), but gives full 1:1 visual & interactive parity with `X806.exe` on Linux/macOS/Windows plus automation. | **Selected** (user choice) |
| **B. `x806-core` + `x806-cli` + `x806-tui` (`ratatui`)** | Shared library + CLI + terminal UI. | Smaller build, works over SSH, but terminals cannot reliably deliver raw key-up timing or bare left/right modifiers for live macro recording. | Rejected |
| **C. `x806-core` + `x806-cli` only** | Declarative config + CLI flashing only. | Fastest to ship, but lacks interactive macro recording and visual keypad/LED editing. | Rejected |

---

## 5. Architecture & Decided Components

### 5.1 Workspace Layout
```text
X806/
├── Cargo.toml                  # Workspace root (edition 2024 or 2021)
├── README.md                   # Quickstart, Linux udev setup, CLI & GUI usage
├── docs/
│   ├── protocol-spec.md        # Complete reverse-engineered HID/EEPROM/file spec
│   └── superpowers/specs/      # This design doc
├── udev/
│   └── 99-x806.rules           # Linux hidraw non-root udev rule
└── crates/
    ├── x806-core/              # Pure library: protocol, EEPROM compiler, codecs, HID transport
    │   └── src/
    │       ├── lib.rs
    │       ├── error.rs        # Typed errors (thiserror)
    │       ├── keycode.rs      # USB HID Usage <-> Windows VK <-> display names
    │       ├── model.rs        # DeviceConfig, Profile, KeyAction, Macro, LedConfig, etc.
    │       ├── codec/
    │       │   ├── mod.rs
    │       │   ├── ini.rs      # X806.exe config.ini reader & writer (comment-tolerant)
    │       │   ├── dat.rs      # .dat macro reader & writer (v3, legacy, ISD Base64)
    │       │   └── toml.rs     # Clean native TOML config + macro bundle persistence
    │       ├── eeprom.rs       # Compiles DeviceConfig + Macros -> EEPROM image & write plan
    │       ├── hid.rs          # HidTransport trait + HidApiTransport (hidapi) + MockTransport
    │       └── defaults.rs     # Factory reset defaults (keys 1..8 = HID '1'..'8', 8 built-in shortcuts)
    ├── x806-cli/               # Binary `x806`: scriptable CLI
    │   └── src/main.rs
    └── x806-gui/               # Binary `x806-gui`: native eframe/egui desktop configurator
        └── src/
            ├── main.rs
            ├── app.rs          # App state, tabs, profile bar, Apply/Cancel/Import/Export/Reset
            ├── tabs/
            │   ├── base.rs     # Interactive 2×4 keypad diagram + action picker + Record Key modal
            │   ├── macro_tab.rs# Macro groups/list, live recorder, step editor, repeat controls
            │   ├── advanced.rs # USB polling rate & Fire speed
            │   └── led.rs      # LED mode, 4-color cycle swatches/pickers, reactive per-key, speed
            └── keymap.rs       # egui::Key + modifiers -> USB HID Usage ID mapping
```

### 5.2 Config Storage & `X806.exe` Interop Strategy (Our Call — Confirmable)
- **Primary local storage:** Clean human-readable **TOML** stored in the standard OS config directory (`directories::ProjectDirs` -> `~/.config/x806/config.toml` on Linux, `~/Library/Application Support/x806/config.toml` on macOS, `%APPDATA%\x806\config.toml` on Windows), with user macro groups stored alongside it (or inline in TOML).
- **Full `X806.exe` interop:** Both `Import` and `Export` (GUI and CLI) support native `TOML` **and** legacy Windows `X806.exe` `config.ini` + `macroGrp.ini` + `.dat` macro files (auto-detected by file extension):
  - **`config.ini`:** ANSI/UTF-8 without BOM, CRLF line endings; reader strips inline `//` comments and tolerates mixed line endings and duplicate keys (`[ShareData]` authoritative for shared fields). `K0..K7`, `LEFT_MAC0..7`, `LED_REACT_{EN,TYPE}0..7` are indexed by **UI key index `0..7`**.
  - **`macroGrp.ini`:** UTF-16LE with BOM (`0xFF 0xFE`, auto-detected on read, emitted with UTF-16LE BOM on export) containing `[MACRO_GROUP_NUM] macGrpNum=<N>` and `[MACRO_GROUP_NAME] 0=<group_name> .. 19=`, where group `g` resolves to `<macros_dir>/<group_name>.dat`.
  - **`constMacro.dat` & `PUBG_*.dat`:** On `import`, `constMacro.dat` and `PUBG_EN.dat` (fallback `PUBG_CN.dat`) are resolved automatically from the directory containing `config.ini` (or its parent for `constMacro.dat`), with optional CLI overrides `--const-macro` / `--pubg-dat` and built-in fallback defaults for the 8 standard shortcuts.

### 5.3 EEPROM Compilation & Flashing Contract (`x806-core::eeprom` & `hid`)
All exact byte/bit formulas are defined in [docs/protocol-spec.md](../../protocol-spec.md) §3–§4:
1. Open `UsagePage 0xFF01 / Usage 0x0001` (`VID 0x30FA / PID 0x1340`).
2. Read `GetFeature(0x07)` -> parse `CurConfig`, `macro_limit` (`0x07F8` or `0x0FF8`), and `hw_flags` (`buf[7] & 0x0F`).
3. Read existing EEPROM byte `0x0076` via the verified single-byte read sequence (`03 -> Sleep(4) -> 00 -> 05 -> Sleep(2) -> GetFeature -> Sleep(2) -> 00`) to preserve bits `7:2`.
4. Compile `DeviceConfig` + resolved macros into deterministic EEPROM write operations:
   - Single-byte write at `0x0001` ONLY (never touching `0x0000` or `0x0002..0x0007`).
   - 8-byte block writes covering `0x0008..0x007F`:
     - Profile hardware button slots (`hw_slot = KEYMAP[ui_btn]`, 1-based `1..11`): Profile 0 (`0x08..0x0F`, `0x10`, `0x3B..0x3C`), Profile 1 (`0x44..0x4E`, zeroed), Profile 2 (`0x50..0x5A`), Profile 3 (`0x60..0x6A`); unused slot-12 bytes (`0x3D`, `0x4F`, `0x5B`, `0x6B`) = `0x00`; `0x0043` = `0xFC`.
     - LED byte `0x001C`: active modes clamp `LED_CYCTIME` to `0..=6`; `Mute` writes `speed = 7, react = 0, mode = 0` (no RMW needed).
     - `0x003A[3:0] = hw_flags`; `0x0076 = (existing_0x76 & 0xFC) | (cur_profile & 3)`.
   - Action descriptor table at `0x00A0..0x02CF` (`70 × 8` bytes = `4 profiles × 14 slots × 10 bytes`), indexed by **0-based `hw_slot - 1`**:
     - `hw_key_code` for macros: `LEFT_MAC == 0` -> `0x0E` if `flags == 0x02` else `0x0D`; `LEFT_MAC != 0` or PUBG -> `0x1F` if `flags == 0x02` else `0x1E`; single-key (`42,<hid>`) -> `0x0E` with descriptor `+2 = <hid>`, `step_count = 1`, non-deduplicated; media (`23..32`) -> `0x0F` with descriptor `+3..+4 = consumer_usage_id`, 0 macro bytes.
     - `step_count` in descriptor `+1[6:0]` / `+7[5:0]` equals `source_events + split_delay_packets`.
     - Deduplicates identical non-`42` action IDs across slots to share the same `macro_addr`.
   - Compiled macro bytecodes starting at `0x0400` (8-byte aligned):
     - Runs recoil expansion (`floor(40 / total_delay_ms) >= 2` when `flags == 0x02`), splits delays `> 127` ticks into trailing `[0x7F, 0x00]` packets (`count = (ticks - 1) / 127`, remainder `1..=127`), and emits absolute-addressed 8-byte-aligned `0xF9` LED blocks if present (`out[0] = block[0] - 1`).
     - Fails cleanly with `Error::MacroOverflow { used, limit }` before sending any write if total exceeds `macro_limit`.
5. Execute writes with protocol timings (`4 ms` / `2 ms`), then send `Apply` (`07 18 10` -> `Sleep(20)` -> `07 18 00`), and send `Exit` (`07 20`) on clean close.
6. Support `--dry-run` / `compile_plan()` so callers can inspect the exact EEPROM diff/image without touching hardware.

### 5.4 CLI Commands (`x806`)
- `x806 status` — Probe connected keypad, print VID/PID/firmware strings, active profile, EEPROM macro capacity (`0x07F8`/`0x0FF8`), HW flags, and read `0x0076`.
- `x806 apply [path] [--dry-run]` — Compile config (TOML or `config.ini` + `--macros-dir`) and flash to keypad.
- `x806 reset [--dry-run]` — Flash factory default configuration (`1..8` -> HID digits `1..8`, `Cycle breath`, `500 Hz`).
- `x806 import <config.ini> [--macros-dir <dir>] [--out <config.toml>]` — Convert Windows `X806.exe` config + `.dat` files into native TOML.
- `x806 export <config.toml> --out <config.ini> [--macros-dir <dir>]` — Export native TOML back to `X806.exe`-compatible `config.ini` + `.dat`.
- `x806 macro inspect <file.dat>` — Decode and print all macros/events inside any `.dat` file (including `constMacro.dat`, `PUBG_*.dat`, `ISD=1` Base64).
- `x806 udev` — Print the `99-x806.rules` udev rule and installation command for Linux.

### 5.5 GUI Behavior (`x806-gui`)
- Window layout mirroring `X806.exe`'s workflow:
  - **Top bar:** 4 tabs (`Base Setting`, `Macro`, `Advanced Setting`, `LED Setting`) + live hardware connection badge (`Connected (X806, 4 KB EEPROM)` / `Disconnected` + Refresh button).
  - **Bottom bar:** 3 profile selector buttons (`Game I`, `Game II`, `Game III`), `Import`, `Export`, `Reset`, `Apply` (flashes hardware + saves local config, with progress/result toast), `Cancel` (reverts dirty state).
  - **Base Setting tab:** Visual 2×4 keypad layout matching the physical key positions (`1..4` top row, `5..8` bottom row) with clickable key buttons showing current binding and dropdown/modal picker for all action categories + `Record Key` dialog.
  - **Macro tab:** Left pane (macro groups & macros CRUD + `Record` / `Stop`), middle pane (`Key list` with editable key/action, down/up, delay ms, insert key/mouse/XY/delay/delete/move up/down), right pane (`Run Until Release`, `Run Until Click`, `Repeat On` + count).
  - **Advanced Setting tab:** `USB Report Rate` (`125/250/500/1000 Hz`) and `Fire Speed` (`50–300 ms`), plus an explicit note explaining why OS pointer/wheel speed are omitted on Linux/macOS.
  - **LED Setting tab:** Mode selector (`Cycle breath`, `Button response`, `Mute`, plus expandable "Advanced Firmware Modes" for distinct hardware modes `0, 2, 5, 6, 7` and brightness `0..7`), 4 color-cycle enable checkboxes (`CYC_7COL_EN` bits `0..3`) + interactive RGB color pickers (`DPI0..3`, plus optional `DPI4..5`) quantized cleanly to 4-bit active-low hardware levels (`0..15`), per-key reactive settings when `Button response` is selected, and cycle speed slider (`0..=6`).

---

## 6. Left to the Builder

- Exact Rust crate versions (`hidapi`, `eframe`/`egui`, `clap`, `serde`, `toml`, `thiserror`, `rfd`, `directories`, `base64`) compatible with `rustc 1.99.0` and clean cross-compilation on Linux/macOS/Windows.
- Internal module layout within `x806-gui` and exact `egui` visual styling/spacing.
- Whether `x806-core` uses `hidapi` default features on Windows/macOS and `linux-native` (`hidraw`) via target-specific Cargo dependency config (`[target.'cfg(target_os = "linux")'.dependencies]`) so Linux builds require **zero** C `libudev`/`libusb` system headers (`hidapi` `linux-native` uses pure Rust/hidraw!). *(Recommended!)*
- Exact handling of `egui` key recording for bare modifiers (`Shift`/`Ctrl`/`Alt`/`Super`) via raw `winit` or `egui` modifier-delta tracking (noting that `egui`'s `Event::Key` doesn't distinguish left vs right modifiers natively, so the builder should either inspect `Modifiers` transitions or offer explicit L/R modifier toggle/buttons in the recorder).
