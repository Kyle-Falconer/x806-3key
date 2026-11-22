use x806_core::model::{MacroEvent, MacroEventKind};

/// Maximum number of recorded events in a single live capture session (`55` key-down/up pairs).
pub const MAX_RECORDED_EVENTS: usize = 110;
/// Maximum number of key-down events in a single live capture session.
pub const MAX_RECORDED_KEY_DOWNS: usize = 55;
/// Trailing delay assigned to the most recently recorded event (`20 ms`).
pub const DEFAULT_TRAILING_DELAY_MS: u16 = 20;
/// Maximum delay recorded between two consecutive events (`10,000 ms`, matching EEPROM cap).
pub const MAX_EVENT_DELAY_MS: u16 = 10_000;

/// Snapshot of currently active keyboard modifiers for delta-based live recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModState {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub command: bool,
}

impl ModState {
    /// Extracts physical modifier flags from `egui::Modifiers`.
    ///
    /// Uses `mac_cmd` rather than `command` so holding Ctrl on Windows/Linux only sets `ctrl`
    /// instead of double-emitting both `0xE0` (LCtrl) and `0xE3` (LWin).
    pub fn from_egui(mods: egui::Modifiers) -> Self {
        Self {
            ctrl: mods.ctrl,
            shift: mods.shift,
            alt: mods.alt,
            command: mods.mac_cmd,
        }
    }
}

/// Outcome returned when feeding a key or modifier transition to [`MacroRecorder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordOutcome {
    Recorded,
    IgnoredRepeat,
    IgnoredOrphanUp,
    LimitReached,
}

/// Stateful live keyboard macro recorder with repeat deduplication, orphan key-up filtering,
/// modifier-delta tracking (`0xE0..=0xE3`), and millisecond inter-event timing.
///
/// Reserves cap headroom for every currently held key so `KeyUp` events always fit within
/// [`MAX_RECORDED_EVENTS`] and [`MacroRecorder::stop`] auto-releases any still-held keys in
/// deterministic LIFO order.
#[derive(Debug, Default)]
pub struct MacroRecorder {
    recording: bool,
    events: Vec<MacroEvent>,
    held: Vec<u8>,
    mods: ModState,
    last_event_ms: Option<u64>,
    key_downs: usize,
}

impl MacroRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a fresh recording session, clearing any previously captured events and held state.
    pub fn start(&mut self, _now_ms: u64) {
        self.recording = true;
        self.events.clear();
        self.held.clear();
        self.mods = ModState::default();
        self.last_event_ms = None;
        self.key_downs = 0;
    }

    /// Stops recording, auto-releases any still-held keys in LIFO order, and returns the events.
    pub fn stop(&mut self) -> Vec<MacroEvent> {
        self.recording = false;
        for hid in self.held.drain(..).rev() {
            if self.events.len() < MAX_RECORDED_EVENTS {
                self.events.push(MacroEvent {
                    kind: MacroEventKind::KeyUp { hid },
                    delay_ms: DEFAULT_TRAILING_DELAY_MS,
                });
            }
        }
        self.mods = ModState::default();
        self.last_event_ms = None;
        self.key_downs = 0;
        std::mem::take(&mut self.events)
    }

    /// Returns `true` while a live capture session is active.
    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// Returns a slice of events recorded so far in the active session.
    pub fn events(&self) -> &[MacroEvent] {
        &self.events
    }

    /// Records a single USB HID key press or release at timestamp `now_ms`.
    pub fn on_key_event(&mut self, hid: u8, pressed: bool, now_ms: u64) -> RecordOutcome {
        if !self.recording || self.events.len() >= MAX_RECORDED_EVENTS {
            return RecordOutcome::LimitReached;
        }

        if pressed {
            if self.held.contains(&hid) {
                return RecordOutcome::IgnoredRepeat;
            }
            // Reserve 1 slot for this KeyDown, 1 for its matching KeyUp, and 1 per currently
            // held key so every pressed key can always be released within MAX_RECORDED_EVENTS.
            if self.key_downs >= MAX_RECORDED_KEY_DOWNS
                || self.events.len() + self.held.len() + 2 > MAX_RECORDED_EVENTS
            {
                return RecordOutcome::LimitReached;
            }
            self.backfill_previous_delay(now_ms);
            self.held.push(hid);
            self.key_downs += 1;
            self.events.push(MacroEvent {
                kind: MacroEventKind::KeyDown { hid },
                delay_ms: DEFAULT_TRAILING_DELAY_MS,
            });
            self.last_event_ms = Some(now_ms);
            RecordOutcome::Recorded
        } else {
            let Some(pos) = self.held.iter().position(|&h| h == hid) else {
                return RecordOutcome::IgnoredOrphanUp;
            };
            self.held.remove(pos);
            self.backfill_previous_delay(now_ms);
            self.events.push(MacroEvent {
                kind: MacroEventKind::KeyUp { hid },
                delay_ms: DEFAULT_TRAILING_DELAY_MS,
            });
            self.last_event_ms = Some(now_ms);
            RecordOutcome::Recorded
        }
    }

    /// Compares `new_mods` against the previous modifier snapshot and emits `0xE0..=0xE3`
    /// down/up events for every changed modifier.
    pub fn on_modifiers_changed(&mut self, new_mods: ModState, now_ms: u64) -> Vec<RecordOutcome> {
        if !self.recording {
            self.mods = new_mods;
            return Vec::new();
        }

        let transitions = [
            (self.mods.ctrl, new_mods.ctrl, 0xE0u8),
            (self.mods.shift, new_mods.shift, 0xE1u8),
            (self.mods.alt, new_mods.alt, 0xE2u8),
            (self.mods.command, new_mods.command, 0xE3u8),
        ];

        let mut outcomes = Vec::new();
        for (was_down, is_down, hid) in transitions {
            if was_down != is_down {
                outcomes.push(self.on_key_event(hid, is_down, now_ms));
            }
        }
        self.mods = new_mods;
        outcomes
    }

    fn backfill_previous_delay(&mut self, now_ms: u64) {
        if let (Some(prev), Some(prev_ms)) = (self.events.last_mut(), self.last_event_ms) {
            let delta = now_ms
                .saturating_sub(prev_ms)
                .clamp(1, u64::from(MAX_EVENT_DELAY_MS));
            prev.delay_ms = delta as u16;
        }
    }
}
