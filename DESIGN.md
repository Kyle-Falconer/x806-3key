# Design System: X806 Industrial Keypad Configurator

Specification extracted directly from the `x806-gui` Rust desktop application (`egui` dark theme, `x806-core`, and HID hardware protocol).

## 1. Visual Theme & Atmosphere

An exacting, low-latency, dark engineering HUD designed for physical mechanical keypad configuration, EEPROM safety verification, and hardware diagnostics. The interface combines dark industrial tooling with high-contrast electric cyan and cobalt accents, tactile keycap geometries, and real-time hardware telemetry status bars.

## 2. Color Palette & Roles

### Primary Foundation
- **Chassis Background (`#181a1f`)**: Deep slate/charcoal root surface for main application panels.
- **Surface Elevation 1 (`#202228`)**: Container group backgrounds, modal backdrops, and inspector cards.
- **Surface Elevation 2 (`#282c34`)**: Interactive cards, keycap switch sockets, and table headers.
- **Keycap Base (`#2c3038`)**: Standard unpressed physical keycap body.
- **Keycap Disabled (`#202226`)**: Inactive or unmapped keycap surface.

### Accent & Interactive
- **Accent Cyan (`#78c3ff`)**: Primary brand accent, window header title, and active tab highlights.
- **Keycap Selected Fill (`#264876`)**: Active keycap selection fill color.
- **Keycap Selected Border (`#5fb4ff`)**: 2px perimeter border for the selected matrix key.
- **Keycap Hover Fill (`#363c48`)**: Subtle tactile hover transition.
- **Accent Neon Blue (`#82d2ff`)**: Sub-legend scancodes, active action bindings, and hardware slot indicators.
- **Record LED Red (`#ef4444`)**: Pulsing recording indicator and error toasts.
- **Commit Button Green (`#10b981`)**: Armed EEPROM flash confirmation button.

### Typography & Text Hierarchy
- **Primary Text (`#ffffff` / `#f0f3f8`)**: High-contrast labels, active key legends, and modal headers.
- **Secondary Text (`#a5b2c6`)**: Physical key numbers ("Key 1"–"Key 8"), scancode notes, and breadcrumbs.
- **Muted Metadata (`#6b7280`)**: Hardware slot strings (`K1_MAP = 0x04`), memory offsets (`0x00A0`), and tick dividers.
- **Hairline Border (`#383e4a` to `#4c525c`)**: 1px structural separators between navigation, viewport, and inspector.

### Functional States
- **Status Connected (`#10b981`)**: Live HID connection pill (`X806 Connected - VID 30FA PID 1340`).
- **Status Offline (`#eab308`)**: Yellow warning pill when device is not connected.
- **Toast Success (`#059669`)**: Flash confirmation and file save toasts.
- **Toast Error (`#dc2626`)**: Hardware write reject and parsing error toasts.

## 3. Typography Rules

- **Display & Window Headers**: Proportional sans-serif (`17px` / `18px`, bold, `#78c3ff`).
- **Section Headings**: Proportional sans-serif (`14.5px` to `16px`, bold/semibold, `#ffffff`).
- **Keycap Legends**: Centered proportional font (`12px` to `13px`, semibold, `#ffffff`), with top-left key number (`11.5px`, `#a5b2c6`).
- **Monospace Telemetry & Code**: Monospace (JetBrains Mono / Courier, `11px` to `12px`) for hardware slots (`K{}_MAP`), hex offsets (`0x001E`, `0x00A0`, `0x0400`), bytecode disassembly, and byte limits.

## 4. Component Stylings

### Physical Keycap (2×4 Matrix)
- Dimensions: `102px × 86px` physical proportions.
- Corner Radius: `8px`.
- Layout: 2 rows of 4 keys (Keys 1–4 top row, Keys 5–8 bottom row), with `10px` inter-key spacing.
- Visuals: Dual text layout (top-left key title `Key N`, center action label `Play/Pause`, `LCtrl+C`, etc.).
- Active State: `2px` stroke with `#5fb4ff`, `#264876` fill, and light blue binding caption.

### Window Navigation & Tabs
- Height: `46px` fixed top bar.
- Tab Buttons: `Base Setting`, `Macro`, `Advanced Setting`, `LED Setting`.
- Right Controls: `Refresh` hardware button and live connection badge (`VID 0x30FA / PID 0x1340`).

### Bottom Action Bar
- Left: Active profile switcher (`Game I` [Slot 0], `Game II` [Slot 2], `Game III` [Slot 3]).
- Center: Active toast alerts (Success, Error, Info, Warning).
- Right: `Reset` (factory defaults), `Cancel` (revert edits), and primary `Apply` (flashes hardware EEPROM or saves offline TOML).

### Action Category Drawers (Base Setting)
- Categorized accordions: Single Key (`Record Key...` modal), Macro, PUBG Macro, Direct Button, Extend Function, Media, Shortcut, Mode Switch, Disable Key.

## 5. Layout Principles

- **Top Bar**: Fixed `46px` app header with title, 4 tabs, and right-aligned hardware probe status.
- **Main Viewport**:
  - `Base Setting`: 2-column split (Left: 2×4 keycap matrix + selected keycard; Right: scrollable categorized action picker).
  - `Macro`: 3-column split (Left: Macro groups/list; Center: Event sequence table + recording toolbar; Right: RepeatMode options + compiled bytecode gauge).
  - `Advanced Setting`: Stacked setting cards for USB Report Rate (125/250/500/1000 Hz), Fire Key speed, and Hardware diagnostics.
  - `LED Setting`: Stacked cards for Lighting Modes (Cycle Breath, Button Response, Mute), Cycle Speed (0–6), Reactive Style, and color palette.
- **Bottom Bar**: Fixed footer with profile selector, dirty state toast, and Reset / Cancel / Apply action buttons.

## 6. Design System Notes for Stitch Generation

- **Atmosphere Keywords**: Industrial macro keypad configurator, dark slate background, electric cyan accents, physical 2x4 keycap matrix, EEPROM hardware safety HUD.
- **Key Prompts**:
  - `Base Setting`: Exact 2x4 physical keypad matrix with Keys 1–8, selected key highlight, and right-side categorized action picker.
  - `Macro`: Chronological event sequence list, recording box with modifier pills, repeat modes, and bytecode memory budget gauge.
  - `Advanced Setting`: Polling rate selectors (125Hz to 1000Hz), fire key sliders, and EEPROM protected byte diagnostics.
  - `LED Setting`: Standard modes (Cycle Breath, Button Response, Mute), speed slider 0–6, reactive styles, and profile color palette.
