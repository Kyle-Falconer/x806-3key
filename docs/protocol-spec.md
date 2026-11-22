# X806 Macro Keypad — Complete HID Protocol, EEPROM Map & File Format Specification

Reverse engineered from `C:\Program Files (x86)\K806\X806.exe` (`SHA-256: 927b0e4bbd6fc33856e12708516f820cc8028910f7a108304bc08c0e8fbdd983`, PE32 x86, PDB `Gaming Mouse 3.0.pdb`, signed by **Wuxi Instant Microelectronics Co., Ltd.**) using REA + Ghidra 12.2, and live-verified on attached hardware.

---

## 1. USB HID Device Identity & Endpoints (`FUN_0040d6f0`)

| Property | Value | Binary Source |
| :--- | :--- | :--- |
| **USB VID** | `0x30FA` | `DAT_00600038` (`.data`) |
| **USB PID** | `0x1340` | `DAT_0060003c` (`.data`) |
| **Event Collection (Read)** | `UsagePage = 0xFF00`, `Usage = 0x0001` | `DAT_00606278` (`GENERIC_READ`, `ReadFile`) |
| **Control Collection (Feature)** | `UsagePage = 0xFF01`, `Usage = 0x0001` | `DAT_00606274` (`GENERIC_WRITE`, `HidD_GetFeature` / `HidD_SetFeature`) |
| **Feature Report Length** | `Caps.FeatureReportByteLength` (8 bytes) | `DAT_00605ef0` |
| **Build Feature Flags** | `DAT_00600032 = 1` (magic `0x5A`), `DAT_00600033 = 1` (extended flash layout), `DAT_00600034 = 10000` (max macro delay ms), `DAT_0060007b = 2` | `0x600030`–`0x60008c` |

### 1.1 Live Hardware Verification (Attached X806, `VID 0x30FA / PID 0x1340 / Ver 0x0100`)
Read-only Windows `SetupAPI` + `hid.dll` probe confirmed Manufacturer `'INSTANT'`, Product `'USB GAMING MOUSE '`, and 6 HID collections:
- `MI_00`: Mouse (`0x0001:0x0002`, `In=7`)
- `MI_01&Col01`: Keyboard (`0x0001:0x0006`, `In=8`)
- `MI_01&Col02`: Consumer Control (`0x000C:0x0001`, `In=3`, R/W openable)
- `MI_01&Col03`: **Vendor Event Read** (`0xFF00:0x0001`, `In=3`, R/W openable without elevation)
- `MI_01&Col04`: **Vendor Feature Control** (`0xFF01:0x0001`, `Feat=8`, R/W openable without elevation)
- `MI_01&Col05`: System Control (`0x0001:0x0080`, `In=2`, R/W openable)

Live `HidD_GetFeature(Report ID 0x07)` on `MI_01&Col04` returned:
```text
07 00 40 09 40 00 23 1F
```
- `buf[6] = 0x23`: active profile `CurConfig = (0x23 >> 2) & 3 = 0`; extended EEPROM macro limit `((0x23 & 2) << 10) | 0x7F8 = 0x0FF8` (4 KB).
- `buf[7] = 0x1F`: hardware flags `0x1F & 0x0F = 0x0F` (preserved in EEPROM `0x003A[3:0]`).

---

## 2. HID Transport Commands

### 2.1 EEPROM / Flash Write Primitive (`FUN_0040d470(uint16 addr, uint8 *buf, uint8 len)`)
Writes `len` bytes (typically `1` or `8`) to on-board EEPROM address `addr` over Feature Report ID `0x07`:

1. **Stage bytes** — repeat for `i = 0 .. len - 1`:
   ```text
   [0] = 0x07               // Report ID
   [1] = 0x18               // Command: EEPROM access
   [2] = 0x03               // Subcommand: Load staging byte
   [3] = i                  // Byte offset within block (0..len-1)
   [4] = addr & 0xFF        // EEPROM address low byte
   [5] = (addr >> 8) & 0xFF // EEPROM address high byte
   [6] = buf[i]             // Payload byte
   [7] = len - 1            // Block length minus 1
   ```
2. **Wait `4 ms`** (`Sleep(4)`).
3. **Commit block to EEPROM:**
   ```text
   07 18 09 00 <addr_lo> <addr_hi> 00 <len-1>
   ```
4. **Wait `2 ms`** (`Sleep(2)`).
5. **Finish / return to idle:**
   ```text
   07 18 00 00 <addr_lo> <addr_hi> 00 00
   ```

### 2.2 EEPROM Single-Byte Read (`FUN_00413a10` @ `0x414ec6`–`0x414f7e`)
Used to read address `0x0076` before read-modify-write:
1. `SetFeature`: `07 18 03 00 <addr_lo> <addr_hi> 00 00`
2. `Sleep(4)` (two `Sleep(2)` calls)
3. `SetFeature`: `07 18 00 00 <addr_lo> <addr_hi> 00 00`
4. `SetFeature`: `07 18 05 00 <addr_lo> <addr_hi> 00 00`
5. `Sleep(2)`
6. `GetFeature` (Report ID `0x07`): **returned byte is in `buf[1]`**.
7. `Sleep(2)`
8. `SetFeature`: `07 18 00 00 00 00 00 00`

### 2.3 Device Status, Apply & Control Feature Reports (`Report ID 0x07`)

| Action | Sequence / Packet Bytes | Notes |
| :--- | :--- | :--- |
| **Read Device Status** | `GetFeature(07 00 ...)` | `(buf[6] >> 2) & 3` = active profile (`CurConfig`); `((buf[6] & 2) << 10) \| 0x7F8` = EEPROM macro limit (`0x7F8` or `0xFF8`); `buf[7] & 0x0F` = hardware flags preserved in `0x003A[3:0]` |
| **Apply / Reload Config** | `SetFeature(07 18 10 00 00 00 00 00)` -> `Sleep(20)` -> `SetFeature(07 18 00 00 00 00 00 00)` | Commits EEPROM changes to active firmware (`FUN_00413a10`) |
| **App Exit / Idle** | `SetFeature(07 20 00 00 00 00 00 00)` | Sent on shutdown (`FUN_004167b0`, `FUN_0041ea00`) |
| **Software Trigger Ack** | `SetFeature(07 11 02 00 00 00 00 00)` / `SetFeature(07 11 00 00 00 00 00 00)` | Macro/key press & release sync (`FUN_00413220`) |
| **Inject Consumer Key** | `SetFeature(07 17 <usage_hi> <usage_lo> 00 00 00 00)` | Emits HID Consumer Usage ID from `FUN_0040fee0` (`FUN_00413220`) |

### 2.4 Incoming Hardware Events (`UsagePage 0xFF00`, Report ID `0x06`, `FUN_00411fc0`)
- `buf[0] == 0x06`:
  - `buf[1] == 0x01` / `0x02`: state toggle (`DAT_0061e820 = 1 / 0`).
  - `buf[1] == 0x03`: hardware profile switch -> reads `GetFeature(0x07)` and updates `Init.CurConfig`.
  - `buf[1] >= 0xF1`: hardware button event (`hw_slot = buf[1] - 0xF0`):
    - `buf[2] & 0x80` set: button/DPI feedback event; for DPI buttons (`hw_code` in `{0x06, 0x0B, 0x0C}`), `buf[2] & 7` is the new active `DPI_LEVEL`.
    - `buf[2] == 0x80`: key released; otherwise key pressed.

---

## 3. Complete On-Board EEPROM Memory Map (`FUN_00413a10`)

> [!IMPORTANT]
> The driver **never overwrites `0x0000` or `0x0002`–`0x0007`** (which hold factory magic `0x5A`, PID `0x1340`, and VID `0x30FA`). Its very first write targets **only 1 byte at `0x0001`**.

### 3.1 Configuration Header & Core Settings (`0x0000`–`0x007F`)

| EEPROM Addr | Size | Field / Bit-Level Definition |
| :--- | :---: | :--- |
| `0x0001` | 1 | **Active Profile & Layout Mode:** `((CurConfig & 3) << 5) \| 0x03` *(only byte written in `0x0000..0x0007`)* |
| `0x0008`–`0x000F` | 8 | **Profile 0 (`Game I`) Hardware Buttons 1..8** (1 byte each): `((LED_REACT_TYPE & 3) << 6) \| ((LED_REACT_EN & 1) << 5) \| (hw_key_code & 0x1F)` |
| `0x0010` | 1 | **Profile 0 Hardware Button 9** (same byte format as `0x0008..0x000F`) |
| `0x0011`–`0x0012` | 2 | Reserved (`0x00, 0x00`) |
| `0x0013` | 1 | **DPI Stages 0..5 MSB (bit 4 of sensor code):** `(dpi5_b4 << 5) \| (dpi4_b4 << 4) \| (dpi3_b4 << 3) \| (dpi2_b4 << 2) \| (dpi1_b4 << 1) \| dpi0_b4` |
| `0x0014` | 1 | **Constant Sensor/Chip Flag:** `(DAT_0060007b & 3) << 3` = **`0x10`** |
| `0x0015` | 1 | **DPI Stages 0 & 1 Low Nibbles:** `((dpi_reg[0] & 0xF) << 4) \| (dpi_reg[1] & 0xF)` |
| `0x0016` | 1 | **DPI Stages 2 & 3 Low Nibbles:** `((dpi_reg[2] & 0xF) << 4) \| (dpi_reg[3] & 0xF)` |
| `0x0017` | 1 | **DPI Stages 4 & 5 Low Nibbles:** `((dpi_reg[4] & 0xF) << 4) \| (dpi_reg[5] & 0xF)` |
| `0x0018` | 1 | **Active DPI Stage & Sensor Flags:** `0x18 \| (DPI_LEVEL & 7)` |
| `0x0019` | 1 | **DPI Stage Enable Mask (Stages 0..3):** `(DPI_SET_EN3 << 3) \| (DPI_SET_EN2 << 2) \| (DPI_SET_EN1 << 1) \| DPI_SET_EN0` |
| `0x001A`–`0x001B` | 2 | **Fire Key Interval (`uint16_le`):** `FIRE_TIME / 8` (default `50 / 8 = 6`) |
| `0x001C` | 1 | **LED Mode & Speed:** `0x80 \| ((led_hw_mode & 7) << 4) \| ((led_react_bit & 1) << 3) \| (led_speed & 7)` *(see Section 4.4)* |
| `0x001D` | 1 | **LED Cycle Palette Mask & User Color Enable:** `((LED_USER_COLER_EN & 1) << 7) \| ((c3 << 6) \| (c2 << 4) \| (c1 << 2) \| c0)` where `c0..c3` are bits `0..3` of `CYC_7COL_EN` |
| `0x001E` | 1 | **USB Polling Rate Divider:** `0` = 1000 Hz (1 ms), `1` = 500 Hz (2 ms), `3` = 250 Hz (4 ms), `7` = 125 Hz (8 ms). Also defines macro tick duration `tick_ms = val + 1`. |
| `0x001F` | 1 | Reserved (`0x00`) |
| `0x0020`–`0x0031` | 18 | **6-Stage DPI LED RGB Colors (Active-Low 4-bit PWM!):** `[DPI0_R, DPI0_G, DPI0_B, ..., DPI5_R, DPI5_G, DPI5_B]` (`0x00` = full bright, `0x0F` = off) |
| `0x0032`–`0x0033` | 2 | Extended layout marker (`0x07, 0x30` when `DAT_00600033 != 0`) |
| `0x0034`–`0x0039` | 6 | Reserved (`0x00`) |
| `0x003A` | 1 | **LED Direction/Symmetry + HW Flags:** `((LED_FEATURE_SYMMETRY & 1) << 6) \| ((LED_FEATURE_DIREC & 3) << 4) \| (hw_flags_0x07_b7 & 0x0F)` |
| `0x003B`–`0x003C` | 2 | **Profile 0 Hardware Buttons 10..11** (`hw_slot 10..11`) |
| `0x003D` | 1 | **Profile 0 Slot 12** (unused on X806; `0x00`) |
| `0x003E` | 1 | Reserved (`0x00`) |
| `0x003F` | 1 | **LED Brightness:** `(LED_BRIGHT & 7) << 4` |
| `0x0040`–`0x0043` | 4 | `[0x00, 0x00, 0x00, 0xFC]` (`DAT_0061e244` is never written in `X806.exe`, so `0x0043` is always `0xFC`) |
| `0x0044`–`0x004E` | 11 | **Profile 1 Hardware Buttons 1..11** (unused/zeroed in X806; `0x004F` = Profile 1 slot 12, `0x00`) |
| `0x0050`–`0x005A` | 11 | **Profile 2 (`Game II`) Hardware Buttons 1..11** (`0x005B` = Profile 2 slot 12, `0x00`; `0x005C`–`0x005F` = `0x00`) |
| `0x0060`–`0x006A` | 11 | **Profile 3 (`Game III`) Hardware Buttons 1..11** (`0x006B` = Profile 3 slot 12, `0x00`; `0x006C`–`0x006F` = `0x00`) |
| `0x0070`–`0x0075` | 6 | Auxiliary `uint16_le × 3` (`0x00` in this build) |
| `0x0076` | 1 | **Active Profile Mirror (RMW):** `(existing_0x76 & 0xFC) \| (CurConfig & 3)` |
| `0x0077`–`0x007F` | 9 | Reserved (`0x00`) |

### 3.2 Action / Macro Descriptor Table (`0x00A0`–`0x02CF`)
Written as `70` consecutive 8-byte blocks (`560` bytes total = `4 profiles × 14 slots × 10 bytes`).
- **Indexing:** Indexed by **0-based hardware slot `slot_idx = hw_slot - 1`** (`0..13`), where `hw_slot = KEYMAP[ui_btn]` (`KEYMAP = [7, 3, 2, 1, 9, 6, 5, 4]` for UI keys `1..8`, so key `1` -> `slot_idx = 6`, key `5` -> `slot_idx = 8`). Byte address = `0x00A0 + (profile * 14 + slot_idx) * 10`.
- Only populated for slots whose `hw_key_code` is in `{0x0D, 0x0E, 0x0F, 0x1E, 0x1F}` (macros, single-key remaps, media keys); all other slots are `0x00 × 10`.

| Offset | Bit Field | Description |
| :---: | :--- | :--- |
| `+0` | `7:0` | `polling_ms - 1` (`0, 1, 3, or 7` — same as `0x001E`) |
| `+1` | `7` | `repeat_until_click` (macro `flags & 3 == 2`: toggle loop on/off; `0` for `K=42` single-key and `0x0F` media) |
| `+1` | `6:0` | `step_count[6:0]` (low 7 bits of **total emitted steps**, i.e. `source_events + split_delay_packets`; `1` for `K=42` single-key; `0` for `0x0F` media) |
| `+2` | `7:0` | `single_key_hid_usage` (non-zero for `K = 42, <hid_code>`; `0x00` for macros & media keys — distinguishes `0x0E` single-key from `0x0E` toggle macro) |
| `+3..+4` | `15:0` | `consumer_usage_id` (`uint16_le`, from `FUN_0040fee0(config_val - 23)` for `0x0F` media actions; `0x0000` otherwise) |
| `+5` | `7:0` | `macro_addr[7:0]` (EEPROM start address of macro bytecode, starting at `0x0400`, 8-byte aligned; identical non-`42` action IDs across slots share the same `macro_addr`; `0x0F` media keys advance `macro_addr` by `0`) |
| `+6` | `7` | `repeat_until_release` (macro `flags & 3 == 1`: loop while held) |
| `+6` | `6` | `repeat_fixed_count` (macro `flags & 3 == 0` with bit `2` set) |
| `+6` | `5:0` | `macro_addr[13:8]` (high 6 bits of EEPROM macro address) |
| `+7` | `6` | `finish_cycle_flag` (macro `flags` bit `3`, when mode `1` or `2`) |
| `+7` | `5:0` | `step_count[12:7]` (high 6 bits of total emitted steps) |
| `+8` | `7` | `macro_opt_flag` (`macro.field_0x4c`) |
| `+8` | `6:0` | `macro_opt_val` (`macro.field_0x4d`) |
| `+9` | `7:0` | `0x00` |

### 3.3 Compiled Macro Bytecode (`0x0400`–`0x07F8` or `0x0FF8`)
- **Single-key remap (`K = 42, <hid_code>`):** Never deduplicated; always allocates 8 bytes at `macro_addr`:
  ```text
  01 00 80 <hid_code> 00 00 00 00
  ```
- **Recoil loop expansion (`FUN_004130f0`):** If `macro.flags == 0x02` and `floor(40 / total_delay_ms) >= 2` (for short recoil loops `<= 20 ms`), `FUN_00413a10` duplicates the event sequence `floor(40 / total_delay_ms)` times before bytecode compilation.
- **Macro Stream (`uint8[]`, padded to 8-byte boundary):**
  - **Bytes `0..1` (`uint16_le`):** `repeat_count` (`1` if `flags & 3` is `1` or `2`; else `macro.repeat_count`).
  - **Delay quantization & splitting (`FUN_00413a10` lines 1415–1444):**
    - `ticks = min(delay_ms, 10000) / polling_ms`, minimum `1`.
    - While `ticks > 127`: emit a trailing `[0x7F, 0x00]` 2-byte pure-delay packet after the event and subtract `127` (`ticks -= 127`), incrementing `step_count` by `1` per trailing packet; the remaining `rem` (`1..=127`) goes into the event's delay byte. *(Note: `X806.exe` computes `count = ticks / 127` and emits a harmless `[0x00, 0x00]` on exact multiples of `127`; emitting `count = (ticks - 1) / 127` with `rem = 127` avoids the nop and is identical for all other values.)*
  - **Event packets:**
    - **Key/Mouse Press (`type = 1`):** `[(rem & 0x7F) | 0x80, hid_code]` (2 bytes)
    - **Key/Mouse Release (`type = 2`) / Pure Delay (`hid_code = 0`):** `[rem & 0x7F, hid_code]` (2 bytes)
    - **Relative XY Move (`hid_code = 0xF6`):** `[rem & 0x7F, 0xF6, dx_lo, dx_hi, dy_lo, dy_hi]` (6 bytes; signed 16-bit `dx, dy`, sign negated per quadrant `type = 3:(-X,-Y), 4:(-X,+Y), 5:(+X,-Y), 6:(+X,+Y)`)
    - **DPI Change (`hid_code = 0xF8`):** `[rem | (0x80 if type==9 else 0), 0xF8, dpi_code, 0x00]` (4 bytes; `type = 9` sets DPI, `type = 10` restores)
    - **LED Animation (`hid_code = 0xF9`):** `[rem, 0xF9, data_addr_lo, (data_addr_hi & 0x3F) | (channel << 6)]` (4 bytes; `type = 13/14/15` -> `channel = 0/1/2`; `data_addr` is the **absolute** 8-byte-aligned EEPROM address appended after the event stream; each block `b` has length `b[0] * 4 + 4` padded to 8 bytes, written with `out[0] = b[0] - 1` and `out[1..] = b[1..]`)

---

## 4. Lookup Tables

### 4.1 Button Action Mapping (`config.ini` `K0..K8` -> EEPROM `hw_key_code`)
From `FUN_00413a10` (`0x413e55`–`0x413f95`) and `skin_main_en.ini`:

| `config.ini` Value (`uVar13`) | UI Meaning | EEPROM `hw_key_code` (`0..0x1F`) | Notes |
| :--- | :--- | :---: | :--- |
| `0` | Disabled / Unassigned | `0x00` | |
| `1` | Left Button | `0x01` | |
| `2` | Middle Button | `0x02` | |
| `3` | Right Button | `0x03` | |
| `4` | Backward (Mouse 4) | `0x04` | |
| `5` | Forward (Mouse 5) | `0x05` | |
| `6` | DPI Loop | `0x06` | Triggers live DPI UI feedback |
| `12` (`0x0C`) | Show Desktop | `0x07` | `val - 5` for `7..18` (except `15, 18`) |
| `13` (`0x0D`) | Double Click | `0x08` | |
| `14` (`0x0E`) | Fire Key | `0x09` | Uses `FIRE_TIME / 8` at `0x001A` |
| `15` (`0x0F`) | LED Loop | `0x15` | |
| `16` (`0x10`) | DPI+ | `0x0B` | Triggers live DPI UI feedback |
| `17` (`0x11`) | DPI- | `0x0C` | Triggers live DPI UI feedback |
| `18` (`0x12`) | Fn Function | `0x14` | |
| `19` (`0x13`) | Profile: Game I | `0x17` | Sets in-RAM host flag `DAT_0061e018` only (not written to EEPROM) |
| `20` (`0x14`) | Profile: Game II | `0x19` | Sets in-RAM host flag `DAT_0061e018` only |
| `21` (`0x15`) | Profile: Game III | `0x1A` | Sets in-RAM host flag `DAT_0061e018` only |
| `22` (`0x16`) | Mode / Media | `0x00` | Sets in-RAM host flag `DAT_0061e018` only (no EEPROM key action) |
| `23`–`32` (`0x17`–`0x20`) | Media Keys (`Browser`..`Calculator`, 10 items) | `0x0F` | Consumer Usage ID `FUN_0040fee0(val - 23)` (`0..9`) stored in `0x00A0+` descriptor *(case `10` = `0x00B7` Stop in `FUN_0040fee0` is unreachable from `FUN_00413a10`)* |
| `33`–`40` (`0x21`–`0x28`) | Built-in Shortcuts (`constMacro.dat` `[0..7]`) | `0x0D` *(or `0x1E`/`0x1F` if `LEFT_MAC != 0`)* | `33`=Copy, `34`=Paste, `35`=My Computer, `36`=Undo, `37`=Lock PC, `38`=Close Window, `39`=Select All, `40`=Find *(note: `41`=Mode Loop in `skin_main_en.ini` indexes empty `constMacro[8]`)* |
| `42,<hid>` (`0x2A`) | Single Keyboard Key | `0x0E` | `<hid>` is USB HID Keyboard Usage ID (Page `0x07`); descriptor `+2 = <hid>` |
| `50`–`255` (`0x32`–`0xFF`) | Gun / PUBG Macro (`PUBG_*.dat` `[val - 50]`) | `0x1F` if `macro.flags == 2`, else `0x1E` | Compiled from `PUBG_*.dat` |
| `>= 256` (`0x100+`) | User Macro (`group = (val >> 8) - 1`, `idx = val & 0xFF`) | `LEFT_MAC == 0`: `0x0E` if `macro.flags == 2` else `0x0D`<br>`LEFT_MAC != 0`: `0x1F` if `macro.flags == 2` else `0x1E` | Loaded from `skins/INI_*/macro/*.dat` via `macroGrp.ini` |

### 4.2 HID Consumer Usage Table (`FUN_0040fee0`)
Indexed by `config_val - 23`:
`0: 0x0223` (AC Home / Browser), `1: 0x0183` (Media Player), `2: 0x018A` (Mail), `3: 0x00E9` (Vol+), `4: 0x00EA` (Vol-), `5: 0x00B6` (Prev Track), `6: 0x00B5` (Next Track), `7: 0x00E2` (Mute), `8: 0x00CD` (Play/Pause), `9: 0x0192` (Calculator). *(Index `10: 0x00B7` Stop exists in `FUN_0040fee0` but `config_val = 33` is routed to `constMacro.dat[0]` in `FUN_00413a10`.)*

### 4.3 DPI Index -> Sensor Register -> DPI (`0x600068` & `FUN_0040b3e0`)

| `DPI_SET` Index | `0x600068` Reg Byte | DPI | `DPI_SET` Index | `0x600068` Reg Byte | DPI |
| :---: | :---: | :---: | :---: | :---: | :---: |
| `0` | `0x01` | 200 | `9` | `0x0A` | 2000 |
| `1` | `0x02` | 400 | `10` | `0x0C` | 2400 |
| `2` | `0x03` | 600 | `11` | `0x0D` | 3200 |
| `3` | `0x04` | 800 | `12` | `0x0E` | 4000 |
| `4` | `0x05` | 1000 | `13` | `0x0F` | 4800 |
| `5` | `0x06` | 1200 | `14` | `0x10` | 5600 |
| `6` | `0x07` | 1400 | `15` | `0x11` | 6400 |
| `7` | `0x08` | 1600 | `16` | `0x12` | 7200 |
| `8` | `0x09` | 1800 | *(unused)* | `0x13` / `0x14` | 8000 / 8800 |

### 4.4 LED Modes (`LED_TYPE`, `FUN_0042d920`) & EEPROM `0x001C` Encoding

> [!NOTE]
> Active LED cycle speed (`LED_CYCTIME`) uses values **`0..=6`** (the UI has 6 steps `LEDCYC_TIME1..6`). Value **`7` (`0x07`)** in `0x001C[2:0]` is exclusively reserved by firmware to mean **LED Mute / Off**, so active modes never collide with `Mute` and `0x001C` does not require read-modify-write. Modes `8..10` mask with `& 7` to hardware-identical values `0..2` (no separate bit exists in EEPROM).

| `LED_TYPE` | UI Name (EN / CN) | `led_hw_mode` (`0x1C[6:4]`) | `led_react_bit` (`0x1C[3]`) | `led_speed` (`0x1C[2:0]`) |
| :---: | :--- | :---: | :---: | :--- |
| `0` | DPI breath (`DPI色呼吸`) | `0` | `0` | `min(LED_CYCTIME, 6)` |
| `1` | Cycle breath (`循环呼吸`) | `1` | `0` | `min(LED_CYCTIME, 6)` |
| `2` | Constantly (`常亮`) | `2` | `0` | `min(LED_CYCTIME, 6)` |
| `3` | Button response (`按键响应`) | `1` | `1` | `min(LED_CYCTIME, 6)` |
| `4` | Mute / Off (`静音`) | `0` *(or last mode)* | `0` | **`7` (`0x07`)** |
| `5` | Single Breath (`单色呼吸`) | `5` | `0` | `min(LED_CYCTIME, 6)` |
| `6` | Constantly (`常亮`) | `6` | `0` | `min(LED_CYCTIME, 6)` |
| `7` | Comet tail (`彗星拖尾`) | `7` | `0` | `min(LED_CYCTIME, 6)` |
| `8`–`10` | Neon / Ambilight / Cycle breath | `LED_TYPE & 7` (`0..2`, hardware alias of `0..2`) | `0` | `min(LED_CYCTIME, 6)` |

**Button-Response Sub-Modes (`LED_REACT_TYPE0..8`, `FUN_00433630`):**
`0` = Cycle breath, `1` = Flow water, `2` = Comet tail, `3` = Single water.

**RGB Color Encoding (`FUN_00421280` lines 637–640 & 1015–1017):**
Every RGB channel (`DPI0..5_R/G/B` and `LED_USER0..6_R/G/B`) is **active-low 4-bit PWM (`0`..`15`)**:
- Hardware/INI value `0` -> full ON (`~((0) << 4) = 0xFF`)
- Hardware/INI value `15` -> OFF (`~((15) << 4) = 0x0F`)
- Example from `config.ini`: `DPI0_R=0, DPI0_G=15, DPI0_B=15` displays as **Red**, not Cyan.

---

## 5. `config.ini` & `macroGrp.ini` Schemas

### 5.1 `config.ini` -> Global Variable Map (`FUN_0040df70`, `FUN_00421280`, `FUN_0042b6b0`)
ANSI/UTF-8 (no BOM), CRLF, may contain inline `//` comments and duplicate shared keys across `[Config0]` and `[ShareData]` (`[ShareData]` is authoritative). `K0..K7`, `LEFT_MAC0..7`, `LED_REACT_{EN,TYPE}0..7` are indexed by **UI button index `0..7`**.

| INI Section | INI Key | Global Address | Type / Size | Notes |
| :--- | :--- | :--- | :--- | :--- |
| `[Init]` | `VER_SEL` | `DAT_00606878` | `uint32` | `0` = CN, `1` = EN (`FUN_00418d30`) |
| `[Init]` | `CurConfig` | `DAT_0061e16c` | `uint32` | Active profile `0..3` |
| `[Init]` | `ConfigNum` | `DAT_0062a1ac` | `CStringW` | Number of profiles (`4`) |
| `[Init]` | `ConfigName1..4[_EN]` | `DAT_0062a1e4..f0` | `CStringW[4]` | Profile display names |
| `[PRODUCTNAME]` | `PRODUCTNAME_CN/EN` | `DAT_0062a624/28` | `CStringW` | Tray tooltip / product name (`X806`) |
| `[KEYNUM]` | `KeyNum` | `DAT_00606648` | `int32` | Number of buttons (`8`) |
| `[KEYMAP]` | `K1_MAP..K9_MAP` | `DAT_00606858[0..8]` | `uint8[9]` | UI button index -> 1-based hardware slot (`7,3,2,1,9,6,5,4,9`) |
| `[KEY_UIFC]` | `K1_INIT_VAL..K9_INIT_VAL` | `DAT_00606a24[0..8]` | `uint8[9]` | Default UI function per button |
| `[ConfigN]` (`0..3`) | `K0..K8` (first value) | `DAT_00606658[N*9 + i]` | `int32[4][9]` | Button action ID (`FUN_0040ffb0`) |
| `[ConfigN]` (`0..3`) | `K0..K8` (after `,` if `42`) | `DAT_00605df8[N*11 + i]` | `uint8[4][11]` | Single-key USB HID Usage ID (`FUN_0040ffb0`) |
| `[ConfigN]` (`0..3`) | `LEFT_MAC0..8` | `DAT_006066e8[N*9 + i]` | `int32[4][9]` | Left-click macro trigger mode flag |
| `[ConfigN]` (`0..3`) | `LED_REACT_EN0..8` | `DAT_006068e8[N*9 + i]` | `int32[4][9]` | Per-button reactive LED enable (`0/1`) |
| `[ConfigN]` (`0..3`) | `LED_REACT_TYPE0..8` | `DAT_00606978[N*9 + i]` | `int32[4][9]` | Per-button reactive LED style (`0..3`) |
| `[ConfigN]` (`0..3`) | `WHEEL_STATUS` | `DAT_00606ac4` | `int32` | Wheel mode flag |
| `[ShareData]` | `DPI_LEVEL` | `DAT_00606ac0` | `int32` | Active DPI stage `0..5` |
| `[ShareData]` | `DPI_SET0..5` | `DAT_00606ba0[0..5]` | `uint8[6]` | DPI step index `0..16` (mapped via `0x600068`) |
| `[ShareData]` | `DPI_SET_EN0..5` | `DAT_00606ac8[0..5]` | `uint8[6]` | Stage enable flags (`0/1`; only `0..3` written to `0x0019`) |
| `[ShareData]` | `DPI0..5_R/G/B` | `DAT_00606ae8[0..17]` | `int32[6][3]` | Active-low 4-bit RGB (`0..15`) |
| `[ShareData]` | `LED_USER0..6_R/G/B` | `DAT_00606b30[0..20]` | `int32[7][3]` | User RGB colors (INI only; **not** written to EEPROM) |
| `[ShareData]` | `LED_TYPE` | `DAT_006068bc` | `uint8` | LED effect ID `0..10` |
| `[ShareData]` | `LED_TYPE_SW_EN0..9` | `DAT_006068c0[0..9]` | `int32[10]` | INI only; **not** written to EEPROM |
| `[ShareData]` | `LED_CYCTIME` / `LED_CYC_TIME` | `DAT_006068bd` | `uint8` | LED speed `0..6` |
| `[ShareData]` | `LED_USER_COLER_EN` | `DAT_00606a08` | `int32` | User color enable (`0/1`, bit 7 of `0x001D`) |
| `[ShareData]` | `LED_FEATURE_SCOLOR` | `DAT_00606a10` | `int32` | Single-color flag (INI only) |
| `[ShareData]` | `LED_FEATURE_SYMMETRY` | `DAT_00606a14` | `int32` | LED symmetry (`0/1`, bit 6 of `0x003A`) |
| `[ShareData]` | `LED_FEATURE_DIREC` | `DAT_00606a18` | `int32` | LED direction (`0..3`, bits `5:4` of `0x003A`) |
| `[ShareData]` | `LED_BRIGHT` | `DAT_00606a1c` | `int32` | Brightness (`0..7`, bits `6:4` of `0x003F`) |
| `[ShareData]` | `CYC_7COL_EN` | `DAT_0060686f` & `DAT_006068a0..b8` | `uint8` + `int32[7]` | 7-bit color-cycle enable mask (bits `0..3` written to `0x001D`) |
| `[ShareData]` | `DPI_SPEED` | `DAT_00606862` | `uint8` | Windows-only OS pointer speed (`SPI_SETMOUSESPEED`, not EEPROM) |
| `[ShareData]` | `WHEEL_SPEED` | `DAT_00606863` | `uint8` | Windows-only OS scroll lines (`SPI_SETWHEELSCROLLLINES`, not EEPROM) |
| `[ShareData]` | `USB_SPEED` | `DAT_0060686d` -> `DAT_0060663e` | `uint8` | `0=125Hz (7), 1=250Hz (3), 2=500Hz (1), 3=1000Hz (0)` |
| `[ShareData]` | `FIRE_TIME` | `DAT_0060005c` | `int32` | Fire key interval in ms (written as `FIRE_TIME / 8` to `0x001A`) |

### 5.2 `macroGrp.ini` Schema
UTF-16LE with BOM (`0xFF 0xFE`). Maps group index `g = (action_id >> 8) - 1` to `<macros_dir>/<name>.dat`:
```ini
[MACRO_GROUP_NUM]
macGrpNum=1
[MACRO_GROUP_NAME]
0=Streaming
1=
...
19=
```

---

## 6. `.dat` Macro File Format (`FUN_00424850`, `FUN_00423690`, `FUN_0040f090`, `FUN_0040e4d0`, `FUN_00422e50`)

All macro libraries (`constMacro.dat`, `skins/config/PUBG_{EN,CN}.dat`, and `skins/INI_{EN,CN}/macro/*.dat` indexed by `macroGrp.ini`) share the same Windows INI container format:

```ini
[Init]
DAT_VER=3
; Optional: ISD=1 enables RFC 4648 Base64 encoding of [Macros] values

[Macros]
macroNum=<N>
0=<3-digit-decimal byte stream (or Base64 if ISD != 0)>
1=<...>
```

### 6.1 Wire Encoding & `ISD` Base64 Mode
- **Decimal Encoding (`FUN_0040a840`):** Every logical byte (`0..255`) is serialized as a **3-digit zero-padded ASCII decimal** (`"000"`..`"255"`) concatenated with no separator.
- **Record Size:** `FUN_00424850` writes **`1,861` logical bytes** (`5,583` ASCII chars) for `DAT_VER=3`, or **`1,855` bytes** (`5,565` chars) for legacy `DAT_VER != 3`. Readers iterate up to `0x19CB` (`6,603`) / `0x19B9` (`6,585`) slots and discard bytes past `0x744` (`1,860`).
- **Optional Base64 Mode (`[Init] ISD != 0`):** Encodes/decodes the ASCII decimal string with standard RFC 4648 Base64 (`FUN_0040ae60` / `FUN_0040a960`, alphabet `A-Za-z0-9+/`, `'='` padded).
- **Placeholder Check (`[Init] MacMd5`):** `FUN_00423690` compares `[Init] MacMd5` literally against `"MacMd5"`.
- **`constMacro.dat` Buffer-Size Quirk (`FUN_00422e50`):** Reads only `0x5B8` (`1,464`) WCHARs (`488` logical bytes) per entry.

### 6.2 Byte-for-Byte Layout (`DAT_VER=3` Stream <-> `0x89A`-Byte In-Memory Struct)

| Stream Byte Offset (`DAT_VER=3`) | In-Memory Offset (`0x89A` struct) | Type / Size | Field & Semantics |
| :--- | :--- | :--- | :--- |
| `0x00`–`0x3B` | `+0x00`–`+0x3B` | `wchar_t[30]` (60 B) | Macro display name (UTF-16LE, zero-padded) |
| `0x3C` | `+0x3C` | `uint8` (+1 B pad at `+0x3D`) | `flags`: bits `1:0` = repeat mode (`0` = repeat `N`, `1` = loop while held, `2` = toggle until next click); bit `2` = repeat enabled; bit `3` = finish cycle before stopping |
| `0x3D`–`0x3E` | `+0x3E` | `uint16_le` | `repeat_count` |
| `0x3F`–`0x40` | `+0x40` | `uint16_le` | `step_count` (active events in the 180-slot array) |
| `0x41`–`0x42` | `+0x42` | `uint16_le` | `xy_move_count` (number of `0xF6` XY-move events) |
| `0x43`–`0x44` | `+0x44` | `uint16_le` | `dpi_step_count` |
| `0x45`–`0x46` | `+0x46` | `uint16_le` | `led_step_count_0` / auxiliary counter |
| `0x47`–`0x48` | `+0x48` | `uint16_le` | `led_step_count_1` / auxiliary counter |
| `0x49` | `+0x4A` | `uint8` | `led_block_count_0` |
| `0x4A` | `+0x4B` | `uint8` | `led_block_count_1` |
| `0x4B` *(v3 only)* | `+0x4C` | `uint8` | `macro_opt_flag` (`1` on PUBG gun-recoil macros, `0` otherwise -> EEPROM descriptor `+8` bit 7) |
| `0x4C` *(v3 only)* | `+0x4D` | `uint8` | `macro_opt_val` (default `10`, or gun recoil Y parameter -> EEPROM descriptor `+8` bits `6:0`) |
| `0x4D`–`0x50` *(v3 only)* | `+0x4E`–`+0x51` | `uint8[4]` | Reserved (`0x00`) |
| `0x51`–`0x5F0` *(v!=3: `0x4B`–`0x5EA`)* | SoA arrays below | `180 × 8` B (`1,440` B) | **Event table** (packed 8-byte AoS on disk, unpacked to SoA in RAM):<br>• `ev[i]+0` -> `+0x52[i]` (`uint8`): `event_type` (`1`=down, `2`=up, `3..6`=XY quadrant, `9/10`=DPI set/restore, `13/14/15`=LED 0/1/2)<br>• `ev[i]+1` -> `+0x106[i]` (`uint8`): **Windows VK code on disk**, translated on load to **USB HID Usage ID** via `FUN_0042e1c0` (and back to VK on save via `FUN_00410b10`)<br>• `ev[i]+2..+3` -> `+0x1BA[i*4]` (`int16_le`): `dx` / DPI code / LED index<br>• `ev[i]+4..+5` -> `+0x1BC[i*4]` (`int16_le`): `dy`<br>• `ev[i]+6..+7` -> `+0x48A[i*2]` (`uint16_le`): `delay_ms` |
| `0x5F1`–`0x744` *(v!=3: `0x5EB`–`0x73E`)* | `+0x5F2`–`+0x745` | `5 × 68` B (`340` B) | LED animation frame blocks (`0x44` bytes each) |
| *(not serialized)* | `+0x746`–`+0x899` | `5 × 68` B (`340` B) | Secondary zero-initialized LED buffer in memory |

### 6.3 Verified Decoded Shipped `.dat` Files
- **`constMacro.dat`** (`DAT_VER=3`, `8` built-in shortcuts, `flags=0x00`, `rep=1`, `extra=[0, 10, 0, 0, 0, 0]`):
  `0` `复制` (LCtrl+C), `1` `粘贴` (LCtrl+V), `2` `我的电脑` (LWin+E), `3` `撤销` (LCtrl+Z), `4` `锁定电脑` (LWin+L), `5` `关闭窗口` (LAlt+F4), `6` `全选` (LCtrl+A), `7` `查找` (LCtrl+F).
- **`skins/INI_EN/macro/Streaming.dat`** (`macroGrp.ini` group `0 = Streaming`, `macroNum=1`):
  `0` `ZoomIt` — `LCtrl` (`0xA2`) down `1 ms`, `'4'` (`0x34`) down `1 ms`.
- **`skins/config/PUBG_EN.dat`** (`[Init] DAT_VER=3` at EOF, `macroNum=11`, `flags=0x02`, `opt_flag=1`): `AKM`, `M16`, `M4`, `UMP9`, `SCAR-L`, `TOMPSON`, `UZI`, `556 bullets`, `556 4 times scope`, `762 bullets`, `762 4 times scope`.

---

## 7. Windows VK <-> USB HID Usage ID & Display-Name Tables

### 7.1 Translation Routines (`FUN_0042e1c0`, `FUN_00410b10`, `FUN_00405db0`)

| Key Group | Windows VK (`hex`) | USB HID Usage (`hex`) | Macro Display (`FUN_00411180`) | Single-Key Display (`FUN_00412290`) |
| :--- | :--- | :--- | :--- | :--- |
| Letters `A`–`Z` | `0x41`–`0x5A` | `0x04`–`0x1D` (`vk - 0x3D`) | `A`–`Z` | `A`–`Z` |
| Digits `1`–`9`, `0` | `0x31`–`0x39`, `0x30` | `0x1E`–`0x26` (`vk - 0x13`), `0x27` | `1`–`9`, `0` | `1`–`9`, `0` |
| Function `F1`–`F12` | `0x70`–`0x7B` | `0x3A`–`0x45` (`vk - 0x36`) | `F1`–`F12` | `F1`–`F12` |
| Function `F13`–`F24` | `0x7C`–`0x87` | `0x68`–`0x73` (`vk - 0x14`) | `F13`–`F24` | `F13`–`F24` |
| Control / Edit | `0x0D` Enter, `0x1B` Esc, `0x08` Back, `0x09` Tab, `0x20` Space, `0x14` Caps | `0x28`, `0x29`, `0x2A`, `0x2B`, `0x2C`, `0x39` | `RETURN`, `ESCAPE`, `BACKSPACE`, `TAB`, `SPACE`, `CAPITAL` | Same except `0x2A` = `BACK` |
| OEM Punctuation | `0xBD` `-`, `0xBB` `=`, `0xDB` `[`, `0xDD` `]`, `0xDC` `\`, `0xBA` `;`, `0xDE` `'`, `0xC0` `` ` ``, `0xBC` `,`, `0xBE` `.`, `0xBF` `/` | `0x2D`, `0x2E`, `0x2F`, `0x30`, `0x31`, `0x33`, `0x34`, **`0x35`**, `0x36`, `0x37`, `0x38` | `-`, `+`, `[{`, `]}`, `\|`, `;:`, ` ' `, `` `~ ``, `,`, `.`, `/?` | Same |
| Navigation & Lock | `0x2C` PrtSc, `0x91` ScrLk, `0x13` Pause, `0x2D` Ins, `0x24` Home, `0x21` PgUp, `0x2E` Del, `0x23` End, `0x22` PgDn, `0x27` R, `0x25` L, `0x28` D, `0x26` U, `0x90` NumLk | `0x46`–`0x53` | `SNAPSHOT`, `SCROLL`, `PAUSE`, `INSERT`, `HOME`, `PRIOR`, `DELETE`, `END`, `NEXT`, `RIGHT`, `LEFT`, `DOWN`, `UP`, `NUMLOCK` | Same |
| Numpad | `0x6F` `/`, `0x6A` `*`, `0x6D` `-`, `0x6B` `+`, `0x61`–`0x69` (`1`–`9`), `0x60` (`0`), `0x6E` `.`, `0x5D` Apps | `0x54`, `0x55`, `0x56`, `0x57`, `0x59`–`0x61`, `0x62`, `0x63`, `0x65` | `DIVIDE`, `MULTIPLY`, `SUBTRACT`, `ADD`, `NUMPAD1`–`9`, `NUMPAD0`, `DECIMAL`, `APPS` | Same |
| Special Keys | `0x2B` Exec, `0x2F` Help, `0x29` Select, `0x0C` Clear, `0x6C` Sep | `0x74`, `0x75`, `0x77`, `0x9C`, `0x9F` | `EXECUTE`, `HELP`, `SELECT`, `CLEAR`, `SEPARATOR` (`0x76`=`MENU`) | Same |
| Modifiers (L/R) | `0xA2` LCtrl, `0xA0` LShift, `0xA4` LAlt, `0x5B` LWin, `0xA3` RCtrl, `0xA1` RShift, `0xA5` RAlt, `0x5C` RWin | `0xE0`–`0xE7` | `LCONTROL`, `LSHIFT`, `LMENU`, `LWIN`, `RCONTROL`, `RSHIFT`, `RMENU`, `RWIN` | Same |
| Mouse Pseudo-VKs | `0xF0` (LMB), `0xF1` (RMB), `0xF2` (MMB) | `FUN_0042e1c0`: `0xF0 -> 0xF1`, `0xF1 -> 0xF3`, `0xF2 -> 0xF2`<br>`FUN_00405db0`: passthrough (`0xF0..0xF2`) | `0xF1`=`L-KEY`/`左键`, `0xF2`=`M-KEY`/`中键`, `0xF3`=`R-KEY`/`右键` | `0xF0`=`L-KEY`, `0xF1`=`R-KEY`, `0xF2`=`M-KEY` |

---

## 8. Runtime Input Capture & Hardware Event Handlers

- **No Global Hooks:** Both `CDialogGetKey::PreTranslateMessage` (`FUN_004058c0`) and the macro recorder (`FUN_0042cee0`) capture keyboard input purely via MFC `PreTranslateMessage` (`WM_KEYDOWN`/`WM_KEYUP`/`WM_SYSKEYDOWN`/`WM_SYSKEYUP`).
- **Live Recoil Tuning (`FUN_0042dab0`):** Triggered by Report ID `0x06` (`buf[1] == 1/2`), adjusts `dy` (`0..30`) on every `0xF6` step of Button 0's user macro, recompiles, flashes EEPROM, and saves back to `.dat`.
- **Hardware DPI Sync (`FUN_0042cc10`):** Writes updated `DPI_LEVEL` back to `config.ini` when Report ID `0x06` reports a hardware DPI button event.
