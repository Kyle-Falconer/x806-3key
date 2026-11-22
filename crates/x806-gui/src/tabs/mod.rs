pub mod advanced;
pub mod base;
pub mod led;
pub mod macro_tab;

use x806_core::keycode::hid_display_name;
use x806_core::model::{
    BuiltinShortcut, DeviceConfig, KeyAction, LedMode, MediaKey, PollingRate, ReactiveStyle,
};

/// Top-level configurator tabs matching `X806.exe`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Base,
    Macro,
    Advanced,
    Led,
}

impl ActiveTab {
    pub const ALL: [Self; 4] = [Self::Base, Self::Macro, Self::Advanced, Self::Led];

    pub fn label(self) -> &'static str {
        match self {
            Self::Base => "Base Setting",
            Self::Macro => "Macro",
            Self::Advanced => "Advanced Setting",
            Self::Led => "LED Setting",
        }
    }
}

/// Returns the human-readable label for a [`MediaKey`] matching `X806.exe`.
pub fn media_key_label(key: MediaKey) -> &'static str {
    match key {
        MediaKey::Browser => "Browser",
        MediaKey::Player => "Player",
        MediaKey::Mail => "Mail",
        MediaKey::VolUp => "Vol+",
        MediaKey::VolDown => "Vol-",
        MediaKey::Previous => "Previous",
        MediaKey::Next => "Next",
        MediaKey::Mute => "Mute",
        MediaKey::PlayPause => "Play/Pause",
        MediaKey::Calculator => "Calculator",
    }
}

/// Returns the human-readable label for a [`BuiltinShortcut`] matching `X806.exe`.
pub fn shortcut_label(shortcut: BuiltinShortcut) -> &'static str {
    match shortcut {
        BuiltinShortcut::Copy => "Copy",
        BuiltinShortcut::Paste => "Paste",
        BuiltinShortcut::MyComputer => "My Computer",
        BuiltinShortcut::Undo => "Undo",
        BuiltinShortcut::LockPc => "Lock PC",
        BuiltinShortcut::CloseWindow => "Close Window",
        BuiltinShortcut::SelectAll => "Select All",
        BuiltinShortcut::Find => "Find",
    }
}

/// Returns the human-readable label for a [`KeyAction`] in keycap cards and headers.
pub fn key_action_label(action: KeyAction, cfg: &DeviceConfig) -> String {
    match action {
        KeyAction::Disabled => "Disabled".to_string(),
        KeyAction::LeftButton => "Left Button".to_string(),
        KeyAction::MidButton => "Mid Button".to_string(),
        KeyAction::RightButton => "Right Button".to_string(),
        KeyAction::Backward => "Backward".to_string(),
        KeyAction::Forward => "Forward".to_string(),
        KeyAction::DpiLoop => "DPI Loop".to_string(),
        KeyAction::ShowDesktop => "Show Desktop".to_string(),
        KeyAction::DoubleClick => "Double Click".to_string(),
        KeyAction::Fire => "Fire".to_string(),
        KeyAction::LedLoop => "LED Loop".to_string(),
        KeyAction::DpiUp => "DPI+".to_string(),
        KeyAction::DpiDown => "DPI-".to_string(),
        KeyAction::FnKey => "Fn function".to_string(),
        KeyAction::ProfileGame1 => "Mode: Game I".to_string(),
        KeyAction::ProfileGame2 => "Mode: Game II".to_string(),
        KeyAction::ProfileGame3 => "Mode: Game III".to_string(),
        KeyAction::Media(m) => format!("Media: {}", media_key_label(m)),
        KeyAction::Shortcut(s) => format!("Shortcut: {}", shortcut_label(s)),
        KeyAction::SingleKey { hid } => format!("Key: {}", hid_display_name(hid)),
        KeyAction::PubgMacro { index } => {
            if let Some(m) = cfg.macros.pubg_macros.get(usize::from(index)) {
                format!("PUBG: {}", m.name)
            } else {
                format!("PUBG #{index}")
            }
        }
        KeyAction::UserMacro { group, index } => {
            if let Some(g) = cfg.macros.groups.get(usize::from(group))
                && let Some(m) = g.macros.get(usize::from(index))
            {
                format!("Macro: {}", m.name)
            } else {
                format!("Macro ({group}:{index})")
            }
        }
        KeyAction::RawLegacy(code) => format!("Legacy ({code})"),
    }
}

/// Returns the human-readable label for a [`PollingRate`].
pub fn polling_rate_label(rate: PollingRate) -> &'static str {
    match rate {
        PollingRate::Hz125 => "125 Hz",
        PollingRate::Hz250 => "250 Hz",
        PollingRate::Hz500 => "500 Hz",
        PollingRate::Hz1000 => "1000 Hz",
    }
}

/// Returns the human-readable label for a [`ReactiveStyle`].
pub fn reactive_style_label(style: ReactiveStyle) -> &'static str {
    match style {
        ReactiveStyle::CycleBreath => "Cycle Breath",
        ReactiveStyle::FlowWater => "Flow Water",
        ReactiveStyle::CometTail => "Comet Tail",
        ReactiveStyle::SingleWater => "Single Water",
    }
}

/// Returns the human-readable label for a [`LedMode`].
pub fn led_mode_label(mode: LedMode) -> &'static str {
    match mode {
        LedMode::DpiBreath => "DPI Breath (0)",
        LedMode::CycleBreath => "Cycle breath",
        LedMode::Constant => "Constant (2)",
        LedMode::ButtonResponse => "Button response",
        LedMode::Mute => "Mute",
        LedMode::SingleBreath => "Single Breath (5)",
        LedMode::ConstantAlt => "Constant Alt (6)",
        LedMode::CometTail => "Comet Tail (7)",
    }
}
