use thiserror::Error;

#[derive(Debug, Error)]
pub enum X806Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("INI parse error: {0}")]
    IniParse(String),

    #[error("DAT parse error: {0}")]
    DatParse(String),

    #[error("TOML error: {0}")]
    Toml(String),

    #[error("macro bytecode overflow: used {used} bytes, limit {limit} bytes")]
    MacroOverflow { used: usize, limit: usize },

    #[error("invalid macro reference: group {group}, index {index}")]
    InvalidMacroRef { group: u8, index: u8 },

    #[error("X806 device not found (VID 0x30FA, PID 0x1340)")]
    DeviceNotFound,

    #[error("HID error: {0}")]
    Hid(String),

    #[error("EEPROM safety violation: {0}")]
    SafetyViolation(String),
}
