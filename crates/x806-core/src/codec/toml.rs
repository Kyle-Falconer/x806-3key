use crate::X806Error;
use crate::model::DeviceConfig;

/// Serializes a `DeviceConfig` to human-readable TOML.
pub fn to_toml_string(cfg: &DeviceConfig) -> Result<String, X806Error> {
    toml::to_string_pretty(cfg).map_err(|e| X806Error::Toml(e.to_string()))
}

/// Deserializes a `DeviceConfig` from TOML.
pub fn from_toml_str(s: &str) -> Result<DeviceConfig, X806Error> {
    toml::from_str(s).map_err(|e| X806Error::Toml(e.to_string()))
}
