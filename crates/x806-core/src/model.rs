use serde::{Deserialize, Serialize};

/// Active-low 4-bit PWM RGB color (`0` = full ON, `15` = OFF).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb4 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb4 {
    /// Quantizes standard 8-bit RGB (`0..=255`) into active-low 4-bit PWM (`0..=15`).
    pub fn from_rgb8(r: u8, g: u8, b: u8) -> Self {
        let conv = |c: u8| 15 - ((u16::from(c) * 15 + 127) / 255) as u8;
        Self {
            r: conv(r),
            g: conv(g),
            b: conv(b),
        }
    }

    /// Expands active-low 4-bit PWM (`0..=15`) to standard 8-bit RGB (`0..=255`).
    pub fn to_rgb8(self) -> [u8; 3] {
        let conv = |c: u8| (15 - c.min(15)) * 17;
        [conv(self.r), conv(self.g), conv(self.b)]
    }
}

/// USB polling rate and macro tick period (`125..=1000 Hz`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PollingRate {
    Hz125,
    Hz250,
    Hz500,
    Hz1000,
}

impl PollingRate {
    /// Hardware divider stored in EEPROM `0x001E` and macro descriptor `+0`.
    pub fn eeprom_divider(self) -> u8 {
        match self {
            Self::Hz125 => 7,
            Self::Hz250 => 3,
            Self::Hz500 => 1,
            Self::Hz1000 => 0,
        }
    }

    /// Tick period in milliseconds (`eeprom_divider() + 1`).
    pub fn tick_ms(self) -> u16 {
        u16::from(self.eeprom_divider()) + 1
    }

    /// Parses `[ShareData] USB_SPEED` INI code (`0..=3`), defaulting unknown values to `Hz500`.
    pub fn from_ini_code(code: u8) -> Self {
        match code {
            0 => Self::Hz125,
            1 => Self::Hz250,
            2 => Self::Hz500,
            3 => Self::Hz1000,
            _ => Self::Hz500,
        }
    }

    /// Returns the `[ShareData] USB_SPEED` INI code (`0..=3`).
    pub fn to_ini_code(self) -> u8 {
        match self {
            Self::Hz125 => 0,
            Self::Hz250 => 1,
            Self::Hz500 => 2,
            Self::Hz1000 => 3,
        }
    }
}

/// HID Consumer Control media keys (`config.ini` action IDs `23..=32`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum MediaKey {
    Browser = 0,
    Player = 1,
    Mail = 2,
    VolUp = 3,
    VolDown = 4,
    Previous = 5,
    Next = 6,
    Mute = 7,
    PlayPause = 8,
    Calculator = 9,
}

impl MediaKey {
    pub const ALL: [Self; 10] = [
        Self::Browser,
        Self::Player,
        Self::Mail,
        Self::VolUp,
        Self::VolDown,
        Self::Previous,
        Self::Next,
        Self::Mute,
        Self::PlayPause,
        Self::Calculator,
    ];

    /// Returns the USB HID Consumer Page (`0x000C`) usage ID from `FUN_0040fee0`.
    pub fn consumer_usage(self) -> u16 {
        match self {
            Self::Browser => 0x0223,
            Self::Player => 0x0183,
            Self::Mail => 0x018A,
            Self::VolUp => 0x00E9,
            Self::VolDown => 0x00EA,
            Self::Previous => 0x00B6,
            Self::Next => 0x00B5,
            Self::Mute => 0x00E2,
            Self::PlayPause => 0x00CD,
            Self::Calculator => 0x0192,
        }
    }

    /// Returns the `config.ini` action ID (`23..=32`).
    pub fn ini_action_id(self) -> u32 {
        23 + u32::from(self as u8)
    }

    pub fn from_ini_action_id(id: u32) -> Option<Self> {
        Self::ALL.get(id.checked_sub(23)? as usize).copied()
    }
}

/// Built-in shortcuts stored in `constMacro.dat[0..8]` (`config.ini` action IDs `33..=40`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum BuiltinShortcut {
    Copy = 0,
    Paste = 1,
    MyComputer = 2,
    Undo = 3,
    LockPc = 4,
    CloseWindow = 5,
    SelectAll = 6,
    Find = 7,
}

impl BuiltinShortcut {
    pub const ALL: [Self; 8] = [
        Self::Copy,
        Self::Paste,
        Self::MyComputer,
        Self::Undo,
        Self::LockPc,
        Self::CloseWindow,
        Self::SelectAll,
        Self::Find,
    ];

    /// Returns the `config.ini` action ID (`33..=40`).
    pub fn ini_action_id(self) -> u32 {
        33 + u32::from(self as u8)
    }

    pub fn from_ini_action_id(id: u32) -> Option<Self> {
        Self::ALL.get(id.checked_sub(33)? as usize).copied()
    }
}

/// Binding assigned to a physical keypad button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyAction {
    Disabled,
    LeftButton,
    MidButton,
    RightButton,
    Backward,
    Forward,
    DpiLoop,
    ShowDesktop,
    DoubleClick,
    Fire,
    LedLoop,
    DpiUp,
    DpiDown,
    FnKey,
    ProfileGame1,
    ProfileGame2,
    ProfileGame3,
    Media(MediaKey),
    Shortcut(BuiltinShortcut),
    SingleKey { hid: u8 },
    PubgMacro { index: u8 },
    UserMacro { group: u8, index: u8 },
    RawLegacy(u32),
}

/// Repeat behavior for a recorded macro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RepeatMode {
    Once,
    Count(u16),
    UntilRelease,
    UntilClick,
}

impl RepeatMode {
    /// Encodes `RepeatMode` and `finish_cycle` into `.dat` `(flags, repeat_count)`.
    pub fn to_dat_flags_and_count(self, finish_cycle: bool) -> (u8, u16) {
        let finish_bit = if finish_cycle { 0x08 } else { 0 };
        match self {
            Self::Once => (finish_bit, 1),
            Self::Count(n) => (0x04 | finish_bit, n),
            Self::UntilRelease => (0x01 | finish_bit, 1),
            Self::UntilClick => (0x02 | finish_bit, 1),
        }
    }

    /// Decodes `.dat` `(flags, repeat_count)` into `(RepeatMode, finish_cycle)`.
    pub fn from_dat_flags(flags: u8, repeat_count: u16) -> (Self, bool) {
        let finish_cycle = (flags & 0x08) != 0;
        let mode = match flags & 0x03 {
            1 => Self::UntilRelease,
            2 => Self::UntilClick,
            _ if (flags & 0x04) != 0 => Self::Count(repeat_count),
            _ => Self::Once,
        };
        (mode, finish_cycle)
    }
}

/// Payload kind for an individual step in a macro sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MacroEventKind {
    KeyDown {
        hid: u8,
    },
    KeyUp {
        hid: u8,
    },
    Delay,
    MoveRelative {
        dx: i16,
        dy: i16,
    },
    DpiSet {
        dpi_code: u8,
    },
    DpiRestore {
        dpi_code: u8,
    },
    LedAnim {
        channel: u8,
        index: u16,
    },
    Raw {
        event_type: u8,
        hid: u8,
        arg1: i16,
        arg2: i16,
    },
}

/// Single timed event inside a [`Macro`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroEvent {
    pub kind: MacroEventKind,
    pub delay_ms: u16,
}

/// Macro definition compatible with `.dat` libraries and EEPROM bytecode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Macro {
    pub name: String,
    pub repeat_mode: RepeatMode,
    pub finish_cycle: bool,
    pub opt_flag: u8,
    pub opt_val: u8,
    pub events: Vec<MacroEvent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub led_blocks: Vec<Vec<u8>>,
}

/// Named group of user macros (`macroGrp.ini` + `<name>.dat`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MacroGroup {
    pub name: String,
    pub macros: Vec<Macro>,
}

/// Complete collection of user, PUBG, and built-in macros.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MacroLibrary {
    pub groups: Vec<MacroGroup>,
    pub pubg_macros: Vec<Macro>,
    pub builtin_shortcuts: Vec<Macro>,
}

/// Per-key reactive LED sub-mode (`LED_REACT_TYPE0..8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum ReactiveStyle {
    CycleBreath = 0,
    FlowWater = 1,
    CometTail = 2,
    SingleWater = 3,
}

impl ReactiveStyle {
    pub fn from_u8(val: u8) -> Self {
        match val & 0x03 {
            0 => Self::CycleBreath,
            1 => Self::FlowWater,
            2 => Self::CometTail,
            _ => Self::SingleWater,
        }
    }
}

/// Firmware LED animation mode (`LED_TYPE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum LedMode {
    DpiBreath = 0,
    CycleBreath = 1,
    Constant = 2,
    ButtonResponse = 3,
    Mute = 4,
    SingleBreath = 5,
    ConstantAlt = 6,
    CometTail = 7,
}

impl LedMode {
    /// Parses `[ShareData] LED_TYPE` (`0..=10`), mapping aliases `8..=10` via `& 7`.
    pub fn from_ini_code(code: u8) -> Self {
        match code {
            0 | 8 => Self::DpiBreath,
            1 | 9 => Self::CycleBreath,
            2 | 10 => Self::Constant,
            3 => Self::ButtonResponse,
            4 => Self::Mute,
            5 => Self::SingleBreath,
            6 => Self::ConstantAlt,
            7 => Self::CometTail,
            _ => Self::CycleBreath,
        }
    }

    pub fn to_ini_code(self) -> u8 {
        self as u8
    }

    /// Returns `(led_hw_mode, led_react_bit, clamped_speed)` for EEPROM `0x001C`.
    pub fn eeprom_fields(self, speed: u8) -> (u8, bool, u8) {
        if self == Self::Mute {
            return (0, false, 7);
        }
        let clamped_speed = speed.min(6);
        match self {
            Self::ButtonResponse => (1, true, clamped_speed),
            _ => (self as u8 & 0x07, false, clamped_speed),
        }
    }
}

/// Device-wide LED configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedConfig {
    pub mode: LedMode,
    pub speed: u8,
    pub brightness: u8,
    pub cycle_mask: u8,
    pub user_color_en: bool,
    pub symmetry: bool,
    pub direction: u8,
    pub dpi_colors: [Rgb4; 6],
    pub user_colors: [Rgb4; 7],
    pub single_color_flag: bool,
    pub type_sw_en: [bool; 10],
}

/// Single keypad profile (`Game I` = 0, `Game II` = 2, `Game III` = 3; slot 1 unused).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub name: String,
    pub keys: [KeyAction; 8],
    pub left_mac: [bool; 8],
    pub led_react_en: [bool; 8],
    pub led_react_type: [ReactiveStyle; 8],
    pub wheel_status: i32,
}

/// Windows-only OS pointer/scroll settings preserved for `config.ini` round-trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyOsSettings {
    pub dpi_speed: u8,
    pub wheel_speed: u8,
    pub ver_sel: u32,
}

impl Default for LegacyOsSettings {
    fn default() -> Self {
        Self {
            dpi_speed: 10,
            wheel_speed: 3,
            ver_sel: 1,
        }
    }
}

/// Full X806 keypad configuration and macro library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub active_profile: u8,
    pub keymap: [u8; 8],
    pub profiles: [ProfileConfig; 4],
    pub led: LedConfig,
    pub polling_rate: PollingRate,
    pub fire_time_ms: u16,
    pub dpi_stages: [u8; 6],
    pub dpi_enable: [bool; 6],
    pub dpi_level: u8,
    pub macros: MacroLibrary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_os_settings: Option<LegacyOsSettings>,
}
