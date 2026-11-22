pub mod codec;
pub mod defaults;
pub mod eeprom;
pub mod error;
pub mod hid;
pub mod keycode;
pub mod model;

pub use error::X806Error;

/// USB Vendor ID for the X806 / K806 keypad (Wuxi Instant Microelectronics).
pub const VID: u16 = 0x30FA;
/// USB Product ID for the X806 / K806 keypad.
pub const PID: u16 = 0x1340;
/// HID Usage Page for Feature control reports (Report ID `0x07`, 8 bytes).
pub const USAGE_PAGE_CONTROL: u16 = 0xFF01;
/// HID Usage Page for Vendor event input reports (Report ID `0x06`, 3 bytes).
pub const USAGE_PAGE_EVENT: u16 = 0xFF00;
/// HID Usage ID shared by the Vendor Control and Event collections.
pub const USAGE_VENDOR: u16 = 0x0001;
/// Mapping from UI button index `0..8` to 1-based EEPROM hardware slot (`K1_MAP..K8_MAP`).
pub const DEFAULT_KEYMAP: [u8; 8] = [7, 3, 2, 1, 9, 6, 5, 4];
