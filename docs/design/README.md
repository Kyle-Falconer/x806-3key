# X806 UI/UX Design System & Screen Gallery

High-fidelity UI/UX design specifications, HTML prototypes, and rendered mockups generated in Google Stitch for the X806 Keypad Configurator.

- **Stitch Project:** [Open in Google Stitch Canvas](https://stitch.google.com/projects/747317654181584122)
- **Design Tokens & Style Spec:** [`DESIGN.md`](../../DESIGN.md)

---

## Screen Catalog

### 1. Base Setting — Keypad 2×4 Matrix Configurator
Interactive 2×4 physical keypad matrix with selected key telemetry and 8-category action picker.

- **HTML Prototype:** [`screens/base_setting.html`](screens/base_setting.html)
- **Screenshot Mockup:** [`screens/base_setting.png`](screens/base_setting.png)
- **Stitch Canvas:** [Open Screen](https://stitch.google.com/projects/747317654181584122?node-id=f37e289ff699486eb1f84a24f118d5de)
- **Components:**
  - 2×4 physical switch matrix (Keys 1–8) with coordinate indices `[R00:C00]` to `[R01:C03]`.
  - Active keycap highlight (`#264876` fill, `#5fb4ff` 2px stroke).
  - Selected key status card with hardware mapping slot label (`K{}_MAP`).
  - Right-column action categories: Single Key (with `Record Key...` modal), Macro, PUBG Macro, Direct Button, Extend Function, Media, Shortcut, Mode Switch, Disable Key.

---

### 2. Macro — Macro Sequencer & Event Recorder
Chronological keystroke and mouse event editor with repeat modes and bytecode budget gauge.

- **HTML Prototype:** [`screens/macro_tab.html`](screens/macro_tab.html)
- **Screenshot Mockup:** [`screens/macro_tab.png`](screens/macro_tab.png)
- **Stitch Canvas:** [Open Screen](https://stitch.google.com/projects/747317654181584122?node-id=b535afcd361547778ba709a4a0efc1d6)
- **Components:**
  - Macro groups hierarchy and macro item catalog with step count and bytecode size badges.
  - Event sequence table (`Step`, `Event`, `Delay ms`) with delay micro-steppers.
  - Recording box: `Record Key Event`, `Insert Delay`, `Insert Mouse`, `Insert Modifier` (`LCtrl`, `LShift`, `LAlt`, `LWin`), `Clear`.
  - Playback RepeatMode options: *Play once*, *Repeat N times* (`1..=255`), *Repeat while held*, *Toggle loop*.
  - Compiled bytecode memory gauge (`X / 560 bytes`, limit `0x0FF8`).

---

### 3. LED Setting — Lighting & RGB Underglow Configurator
Lighting modes, cycle speed dynamics, reactive patterns, and 4-bit RGB palette settings.

- **HTML Prototype:** [`screens/led_setting.html`](screens/led_setting.html)
- **Screenshot Mockup:** [`screens/led_setting.png`](screens/led_setting.png)
- **Stitch Canvas:** [Open Screen](https://stitch.google.com/projects/747317654181584122?node-id=9b798334279d4f9ca14023262c36a8f4)
- **Components:**
  - Interactive 2×4 keypad preview with simulated underglow halo diffusion and rotary encoder rings.
  - Standard modes: `Cycle breath`, `Button response` (reactive), `Mute (Off)`.
  - Advanced modes: `Dpi breath`, `Constant (Static)`, `Single breath`, `Constant alt`, `Comet tail`.
  - Cycle Speed slider (`0` fastest to `6` slowest).
  - Reactive style selector: `Cycle breath`, `Flow water`, `Comet tail`, `Single water`.
  - 4-bit RGB (RGB4) active-low color palette swatches per profile.

---

### 4. Advanced Setting — Hardware Diagnostics & EEPROM Safety
USB polling rates, fire key repeat dynamics, hardware diagnostics, and EEPROM write plan safety verifier.

- **HTML Prototype:** [`screens/advanced_setting.html`](screens/advanced_setting.html)
- **Screenshot Mockup:** [`screens/advanced_setting.png`](screens/advanced_setting.png)
- **Stitch Canvas:** [Open Screen](https://stitch.google.com/projects/747317654181584122?node-id=9818042f88cb499dbb6ecf86d1860d71)
- **Components:**
  - USB Report Rate selector: `125 Hz`, `250 Hz`, `500 Hz`, `1000 Hz` with EEPROM `0x001E` reference and live tick period readout.
  - Fire key settings: repeat count (`1..=255`) and fire interval delay (ms).
  - Hardware diagnostics: `VID 0x30FA / PID 0x1340`, product `USB GAMING MOUSE`, manufacturer `INSTANT`, version `0x0100`.
  - EEPROM safety verification: protected byte invariants (magic byte `0x5A` at `0x0032`), 560-byte descriptor table at `0x00A0`, macro memory ceiling `0x0FF8`, and staged dry-run write plan diff table.
