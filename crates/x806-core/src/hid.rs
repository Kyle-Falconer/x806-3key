use std::collections::VecDeque;
use std::time::Duration;

use crate::eeprom::{CompileContext, EepromPlan, compile_plan};
use crate::model::DeviceConfig;
use crate::{PID, USAGE_PAGE_CONTROL, USAGE_VENDOR, VID, X806Error};

const REPORT_ID_CONTROL: u8 = 0x07;
const FEATURE_REPORT_LEN: usize = 8;
const MAX_EEPROM_ADDR: u16 = 0x3FFF;
const RMW_PROFILE_MIRROR_ADDR: u16 = 0x0076;

const STAGE_TO_COMMIT_DELAY: Duration = Duration::from_millis(4);
const COMMIT_TO_IDLE_DELAY: Duration = Duration::from_millis(2);
const READ_LATCH_DELAY: Duration = Duration::from_millis(2);
const APPLY_RELOAD_DELAY: Duration = Duration::from_millis(20);

/// Low-level HID feature-report transport abstraction for X806 protocol I/O and timing.
pub trait HidTransport {
    /// Reads an 8-byte HID Feature Report (`buf[0]` must contain the Report ID).
    fn get_feature_report(&mut self, buf: &mut [u8]) -> Result<usize, X806Error>;
    /// Sends an 8-byte HID Feature Report (`data[0]` must contain the Report ID).
    fn send_feature_report(&mut self, data: &[u8]) -> Result<(), X806Error>;
    /// Enforces protocol-mandated inter-command delays.
    fn sleep(&mut self, dur: Duration);
}

impl<T: HidTransport + ?Sized> HidTransport for &mut T {
    fn get_feature_report(&mut self, buf: &mut [u8]) -> Result<usize, X806Error> {
        (**self).get_feature_report(buf)
    }

    fn send_feature_report(&mut self, data: &[u8]) -> Result<(), X806Error> {
        (**self).send_feature_report(data)
    }

    fn sleep(&mut self, dur: Duration) {
        (**self).sleep(dur);
    }
}

/// Decoded device status returned by `GetFeature(0x07)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceStatus {
    pub active_profile: u8,
    pub macro_limit: usize,
    pub hw_flags: u8,
    pub raw_report: [u8; 8],
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

/// Cross-platform `hidapi` transport bound to the X806 Vendor Control collection (`0xFF01:0x0001`).
pub struct HidApiTransport {
    device: hidapi::HidDevice,
}

impl HidApiTransport {
    /// Enumerates HID interfaces and opens the first device strictly matching
    /// `VID == 0x30FA && PID == 0x1340 && usage_page == 0xFF01 && usage == 0x0001`.
    pub fn open_first() -> Result<(Self, Option<String>, Option<String>), X806Error> {
        let api = hidapi::HidApi::new().map_err(|e| X806Error::Hid(e.to_string()))?;
        let info = api
            .device_list()
            .find(|dev| {
                dev.vendor_id() == VID
                    && dev.product_id() == PID
                    && dev.usage_page() == USAGE_PAGE_CONTROL
                    && dev.usage() == USAGE_VENDOR
            })
            .ok_or(X806Error::DeviceNotFound)?;

        let manufacturer = info.manufacturer_string().map(ToString::to_string);
        let product = info.product_string().map(ToString::to_string);
        let device = info
            .open_device(&api)
            .map_err(|e| X806Error::Hid(e.to_string()))?;

        let manufacturer =
            manufacturer.or_else(|| device.get_manufacturer_string().ok().flatten());
        let product = product.or_else(|| device.get_product_string().ok().flatten());

        Ok((Self { device }, manufacturer, product))
    }
}

impl HidTransport for HidApiTransport {
    fn get_feature_report(&mut self, buf: &mut [u8]) -> Result<usize, X806Error> {
        self.device
            .get_feature_report(buf)
            .map_err(|e| X806Error::Hid(e.to_string()))
    }

    fn send_feature_report(&mut self, data: &[u8]) -> Result<(), X806Error> {
        self.device
            .send_feature_report(data)
            .map_err(|e| X806Error::Hid(e.to_string()))
    }

    fn sleep(&mut self, dur: Duration) {
        std::thread::sleep(dur);
    }
}

/// Deterministic in-memory transport for offline protocol and error-handling tests.
#[derive(Debug, Default)]
pub struct MockTransport {
    pub sent_reports: Vec<Vec<u8>>,
    pub sleeps: Vec<Duration>,
    pub feature_responses: VecDeque<Result<[u8; 8], String>>,
    pub fail_on_send_index: Option<usize>,
}

impl HidTransport for MockTransport {
    fn get_feature_report(&mut self, buf: &mut [u8]) -> Result<usize, X806Error> {
        let report = self
            .feature_responses
            .pop_front()
            .ok_or_else(|| X806Error::Hid("MockTransport: no feature_responses queued".into()))?
            .map_err(X806Error::Hid)?;
        let n = buf.len().min(report.len());
        buf[..n].copy_from_slice(&report[..n]);
        Ok(n)
    }

    fn send_feature_report(&mut self, data: &[u8]) -> Result<(), X806Error> {
        let idx = self.sent_reports.len();
        if self.fail_on_send_index == Some(idx) {
            return Err(X806Error::Hid(format!("mock send failure at index {idx}")));
        }
        self.sent_reports.push(data.to_vec());
        Ok(())
    }

    fn sleep(&mut self, dur: Duration) {
        self.sleeps.push(dur);
    }
}

/// High-level X806 protocol driver and EEPROM flasher over an abstract [`HidTransport`].
pub struct X806Device<T: HidTransport> {
    transport: T,
    manufacturer: Option<String>,
    product: Option<String>,
}

impl<T: HidTransport> X806Device<T> {
    /// Wraps a HID transport without USB descriptor strings.
    pub fn new(transport: T) -> Self {
        Self::with_strings(transport, None, None)
    }

    /// Wraps a HID transport along with enumerated manufacturer and product strings.
    pub fn with_strings(
        transport: T,
        manufacturer: Option<String>,
        product: Option<String>,
    ) -> Self {
        Self {
            transport,
            manufacturer,
            product,
        }
    }

    /// Reads live firmware status via `GetFeature(0x07)`.
    pub fn read_status(&mut self) -> Result<DeviceStatus, X806Error> {
        let mut buf = [0u8; FEATURE_REPORT_LEN];
        buf[0] = REPORT_ID_CONTROL;
        let n = self.transport.get_feature_report(&mut buf)?;
        if n < FEATURE_REPORT_LEN {
            return Err(X806Error::Hid(format!(
                "short status feature report: got {n} bytes, expected {FEATURE_REPORT_LEN}"
            )));
        }

        let active_profile = (buf[6] >> 2) & 0x03;
        let macro_limit = ((usize::from(buf[6]) & 0x02) << 10) | 0x07F8;
        let hw_flags = buf[7] & 0x0F;

        Ok(DeviceStatus {
            active_profile,
            macro_limit,
            hw_flags,
            raw_report: buf,
            manufacturer: self.manufacturer.clone(),
            product: self.product.clone(),
        })
    }

    /// Executes the 8-step EEPROM single-byte read sequence (`protocol-spec.md` §2.2).
    pub fn read_eeprom_byte(&mut self, addr: u16) -> Result<u8, X806Error> {
        if addr > MAX_EEPROM_ADDR {
            return Err(X806Error::SafetyViolation(format!(
                "EEPROM read address {addr:#06x} exceeds 14-bit address space"
            )));
        }
        let [addr_lo, addr_hi] = addr.to_le_bytes();

        self.transport.send_feature_report(&[
            REPORT_ID_CONTROL,
            0x18,
            0x03,
            0x00,
            addr_lo,
            addr_hi,
            0x00,
            0x00,
        ])?;
        self.transport.sleep(STAGE_TO_COMMIT_DELAY);
        self.transport.send_feature_report(&[
            REPORT_ID_CONTROL,
            0x18,
            0x00,
            0x00,
            addr_lo,
            addr_hi,
            0x00,
            0x00,
        ])?;
        self.transport.send_feature_report(&[
            REPORT_ID_CONTROL,
            0x18,
            0x05,
            0x00,
            addr_lo,
            addr_hi,
            0x00,
            0x00,
        ])?;
        self.transport.sleep(READ_LATCH_DELAY);

        let mut buf = [0u8; FEATURE_REPORT_LEN];
        buf[0] = REPORT_ID_CONTROL;
        let n = self.transport.get_feature_report(&mut buf)?;
        if n < 2 {
            return Err(X806Error::Hid(format!(
                "short EEPROM read feature report: got {n} bytes, expected >= 2"
            )));
        }

        self.transport.sleep(COMMIT_TO_IDLE_DELAY);
        self.transport.send_feature_report(&[
            REPORT_ID_CONTROL,
            0x18,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
        ])?;

        Ok(buf[1])
    }

    /// Stages, commits, and returns to idle for a `1..=8` byte EEPROM write block (`protocol-spec.md` §2.1).
    ///
    /// Refuses writes targeting protected factory bytes (`0x0000` and `0x0002..=0x0007`).
    pub fn write_eeprom_block(&mut self, addr: u16, data: &[u8]) -> Result<(), X806Error> {
        validate_write_range(addr, data.len())?;

        let [addr_lo, addr_hi] = addr.to_le_bytes();
        let len_minus_1 = (data.len() - 1) as u8;

        for (i, &byte) in data.iter().enumerate() {
            self.transport.send_feature_report(&[
                REPORT_ID_CONTROL,
                0x18,
                0x03,
                i as u8,
                addr_lo,
                addr_hi,
                byte,
                len_minus_1,
            ])?;
        }
        self.transport.sleep(STAGE_TO_COMMIT_DELAY);
        self.transport.send_feature_report(&[
            REPORT_ID_CONTROL,
            0x18,
            0x09,
            0x00,
            addr_lo,
            addr_hi,
            0x00,
            len_minus_1,
        ])?;
        self.transport.sleep(COMMIT_TO_IDLE_DELAY);
        self.transport.send_feature_report(&[
            REPORT_ID_CONTROL,
            0x18,
            0x00,
            0x00,
            addr_lo,
            addr_hi,
            0x00,
            0x00,
        ])?;

        Ok(())
    }

    /// Sends the firmware apply/reload command (`07 18 10 ...`, `20 ms`, `07 18 00 ...`).
    pub fn apply_reload(&mut self) -> Result<(), X806Error> {
        self.transport
            .send_feature_report(&[REPORT_ID_CONTROL, 0x18, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00])?;
        self.transport.sleep(APPLY_RELOAD_DELAY);
        self.transport
            .send_feature_report(&[REPORT_ID_CONTROL, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])?;
        Ok(())
    }

    /// Sends the host exit/idle notification (`07 20 00 ...`).
    pub fn send_exit(&mut self) -> Result<(), X806Error> {
        self.transport
            .send_feature_report(&[REPORT_ID_CONTROL, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
    }

    /// Reads live hardware context, compiles and safety-validates the EEPROM plan,
    /// flashes every write block in order, and commits via `apply_reload`.
    pub fn flash_config(&mut self, cfg: &DeviceConfig) -> Result<EepromPlan, X806Error> {
        let status = self.read_status()?;
        let existing_0x76 = self.read_eeprom_byte(RMW_PROFILE_MIRROR_ADDR)?;

        let ctx = CompileContext {
            hw_flags: status.hw_flags,
            existing_0x76,
            macro_limit: status.macro_limit,
        };
        let plan = compile_plan(cfg, ctx)?;

        for write in &plan.writes {
            self.write_eeprom_block(write.addr, &write.data)?;
        }
        self.apply_reload()?;

        Ok(plan)
    }

    /// Consumes the device wrapper and returns the underlying transport.
    pub fn into_transport(self) -> T {
        self.transport
    }
}

fn validate_write_range(addr: u16, len: usize) -> Result<(), X806Error> {
    if len == 0 || len > 8 {
        return Err(X806Error::SafetyViolation(format!(
            "EEPROM write at {addr:#06x} has invalid block length {len} (must be 1..=8)"
        )));
    }
    let end = addr
        .checked_add(len as u16)
        .ok_or_else(|| X806Error::SafetyViolation("EEPROM write address overflow".into()))?;
    for byte_addr in addr..end {
        if byte_addr == 0x0000 || (0x0002..=0x0007).contains(&byte_addr) {
            return Err(X806Error::SafetyViolation(format!(
                "refusing write to protected factory EEPROM byte {byte_addr:#06x}"
            )));
        }
        if byte_addr > MAX_EEPROM_ADDR {
            return Err(X806Error::SafetyViolation(format!(
                "EEPROM write byte {byte_addr:#06x} exceeds 14-bit address space"
            )));
        }
    }
    Ok(())
}
