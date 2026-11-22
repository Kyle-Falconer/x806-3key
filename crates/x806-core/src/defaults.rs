use crate::DEFAULT_KEYMAP;
use crate::model::*;

fn default_profile(name: &str) -> ProfileConfig {
    let keys = std::array::from_fn(|k| KeyAction::SingleKey {
        hid: 0x1E + k as u8, // HID '1'..='8'
    });
    ProfileConfig {
        name: name.to_string(),
        keys,
        left_mac: [false; 8],
        led_react_en: [false; 8],
        led_react_type: [ReactiveStyle::CycleBreath; 8],
        wheel_status: 0,
    }
}

/// Returns the factory-default X806 device configuration matching `configReset.ini`.
pub fn factory_default_config() -> DeviceConfig {
    let dpi_colors = [
        Rgb4 { r: 0, g: 15, b: 15 },  // Red
        Rgb4 { r: 15, g: 0, b: 15 },  // Green
        Rgb4 { r: 15, g: 15, b: 0 },  // Blue
        Rgb4 { r: 0, g: 0, b: 15 },   // Yellow
        Rgb4 { r: 15, g: 0, b: 0 },   // Cyan
        Rgb4 { r: 0, g: 15, b: 0 },   // Magenta
    ];
    let user_colors = [
        Rgb4 { r: 0, g: 15, b: 15 },
        Rgb4 { r: 0, g: 8, b: 15 },
        Rgb4 { r: 0, g: 0, b: 15 },
        Rgb4 { r: 15, g: 0, b: 15 },
        Rgb4 { r: 15, g: 0, b: 0 },
        Rgb4 { r: 15, g: 15, b: 0 },
        Rgb4 { r: 0, g: 15, b: 0 },
    ];

    DeviceConfig {
        active_profile: 0,
        keymap: DEFAULT_KEYMAP,
        profiles: [
            default_profile("Game I"),
            default_profile("Game"),
            default_profile("Game II"),
            default_profile("Game III"),
        ],
        led: LedConfig {
            mode: LedMode::CycleBreath,
            speed: 3,
            brightness: 4,
            cycle_mask: 0x7F,
            user_color_en: false,
            symmetry: false,
            direction: 0,
            dpi_colors,
            user_colors,
            single_color_flag: false,
            type_sw_en: [true; 10],
        },
        polling_rate: PollingRate::Hz500,
        fire_time_ms: 50,
        dpi_stages: [2, 3, 5, 7, 10, 11],
        dpi_enable: [true, true, true, true, false, false],
        dpi_level: 1,
        macros: MacroLibrary {
            groups: Vec::new(),
            pubg_macros: Vec::new(),
            builtin_shortcuts: builtin_shortcuts().to_vec(),
        },
        legacy_os_settings: Some(LegacyOsSettings::default()),
    }
}

fn chord_macro(name: &str, modifier_hid: u8, key_hid: u8) -> Macro {
    Macro {
        name: name.to_string(),
        repeat_mode: RepeatMode::Once,
        finish_cycle: false,
        opt_flag: 0,
        opt_val: 10,
        events: vec![
            MacroEvent {
                kind: MacroEventKind::KeyDown { hid: modifier_hid },
                delay_ms: 1,
            },
            MacroEvent {
                kind: MacroEventKind::KeyDown { hid: key_hid },
                delay_ms: 1,
            },
            MacroEvent {
                kind: MacroEventKind::KeyUp { hid: key_hid },
                delay_ms: 1,
            },
            MacroEvent {
                kind: MacroEventKind::KeyUp { hid: modifier_hid },
                delay_ms: 1,
            },
        ],
        led_blocks: Vec::new(),
    }
}

/// Returns the 8 built-in shortcuts matching `constMacro.dat` (`protocol-spec.md` §6.3).
pub fn builtin_shortcuts() -> [Macro; 8] {
    const LCTRL: u8 = 0xE0;
    const LALT: u8 = 0xE2;
    const LWIN: u8 = 0xE3;

    [
        chord_macro("Copy", LCTRL, 0x06),         // LCtrl + C
        chord_macro("Paste", LCTRL, 0x19),        // LCtrl + V
        chord_macro("My Computer", LWIN, 0x08),   // LWin + E
        chord_macro("Undo", LCTRL, 0x1D),         // LCtrl + Z
        chord_macro("Lock PC", LWIN, 0x0F),       // LWin + L
        chord_macro("Close Window", LALT, 0x3D),  // LAlt + F4
        chord_macro("Select All", LCTRL, 0x04),   // LCtrl + A
        chord_macro("Find", LCTRL, 0x09),         // LCtrl + F
    ]
}
