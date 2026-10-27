//! The stick and trigger tuning: dead zones, thresholds and dwell times.
//!
//! **ESTABLISHED**:
//!
//!  * The key is `controller_tuning` and its fields are, in file order,
//!    `movement_stick`, `opposing_stick`, `back_slice`, `fire_mode`,
//!    `edge_threshold`, `retract_delta`, `trigger_close_pct`,
//!    `trigger_open_pct`, `fire_debounce_ms`, `release_grace_ms`,
//!    `aim_dwell_ms`, `nav_dwell_ms`, `wheel_min_open_ms`, `deadzone`
//!    (`0x102417f22`).
//!  * The defaults the constructor writes are `150`, `150`, `300`, `40`
//!    (`0x10145a598`) — which land on the four percentages in that order.
//!  * [`Tuning::trigger`] clamps (`0x10145b48c`, `f32::clamp`'s own panic
//!    message `0x102417f90`).

use crate::text::{self, Value};

/// The key the tuning lives under.
pub const FILE: &str = "controller_tuning";

/// The trigger is held down from this percentage of its travel.
///
/// **ESTABLISHED** (`0x10145a598`): the fourth default, `40`.
pub const DEFAULT_TRIGGER_OPEN_PCT: f32 = 40.0;

/// The trigger is let go below this percentage.
///
/// **ESTABLISHED** (`0x10145a598`): the third default, `300` — the two together
/// are the hysteresis, and `open < close` is what [`Tuning::trigger`] clamps
/// between.
pub const DEFAULT_TRIGGER_CLOSE_PCT: f32 = 300.0;

/// The edge threshold and retract delta defaults.
///
/// **ESTABLISHED** (`0x10145a598`): the first two defaults, both `150`.
pub const DEFAULT_EDGE_THRESHOLD: f32 = 150.0;
pub const DEFAULT_RETRACT_DELTA: f32 = 150.0;

/// Which stick walks and which one aims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stick {
    #[default]
    Left,
    Right,
}

impl Stick {
    /// **ESTABLISHED** (`0x102417f22`): the file spells them `left` and `right`.
    pub fn parse(text: &str) -> Option<Stick> {
        match text.trim() {
            "left" => Some(Stick::Left),
            "right" => Some(Stick::Right),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Stick::Left => "left",
            Stick::Right => "right",
        }
    }

    /// The other stick.
    pub fn opposing(&self) -> Stick {
        match self {
            Stick::Left => Stick::Right,
            Stick::Right => Stick::Left,
        }
    }
}

/// How the wheel fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FireMode {
    /// Fire when the aim rests on a slice.
    #[default]
    Dwell,
    /// Fire when the trigger is let go.
    Release,
}

impl FireMode {
    pub fn parse(text: &str) -> Option<FireMode> {
        match text.trim() {
            "dwell" => Some(FireMode::Dwell),
            "release" => Some(FireMode::Release),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            FireMode::Dwell => "dwell",
            FireMode::Release => "release",
        }
    }
}

/// The tuning of the pad.
#[derive(Debug, Clone, PartialEq)]
pub struct Tuning {
    pub movement_stick: Stick,
    pub opposing_stick: Stick,
    /// The slice the wheel opens on when it comes back up.
    pub back_slice: Option<String>,
    pub fire_mode: FireMode,
    pub edge_threshold: f32,
    pub retract_delta: f32,
    pub trigger_close_pct: f32,
    pub trigger_open_pct: f32,
    pub fire_debounce_ms: u32,
    pub release_grace_ms: u32,
    pub aim_dwell_ms: u32,
    pub nav_dwell_ms: u32,
    pub wheel_min_open_ms: u32,
    /// How far the stick may wander before it counts as pushed.
    pub deadzone: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning {
            movement_stick: Stick::Left,
            opposing_stick: Stick::Right,
            back_slice: None,
            fire_mode: FireMode::Dwell,
            edge_threshold: DEFAULT_EDGE_THRESHOLD,
            retract_delta: DEFAULT_RETRACT_DELTA,
            trigger_close_pct: DEFAULT_TRIGGER_CLOSE_PCT,
            trigger_open_pct: DEFAULT_TRIGGER_OPEN_PCT,
            fire_debounce_ms: 0,
            release_grace_ms: 0,
            aim_dwell_ms: 0,
            nav_dwell_ms: 0,
            wheel_min_open_ms: 0,
            deadzone: 0.0,
        }
    }
}

impl Tuning {
    /// **ESTABLISHED** (`0x10145a540`): the `controller_tuning` table over the
    /// defaults above.
    pub fn read(doc: &Value) -> Tuning {
        let table = doc.get(FILE).unwrap_or(&Value::Null);
        let mut tuning = Tuning::default();
        if let Some(stick) = table.get("movement_stick").and_then(Value::as_str) {
            tuning.movement_stick = Stick::parse(stick).unwrap_or(Stick::Left);
            // UNKNOWN: the binary reads `opposing_stick` from the file too; when
            // it is absent the other stick is the natural partner.
            tuning.opposing_stick = tuning.movement_stick.opposing();
        }
        if let Some(stick) = table.get("opposing_stick").and_then(Value::as_str) {
            tuning.opposing_stick = Stick::parse(stick).unwrap_or_else(|| tuning.movement_stick.opposing());
        }
        tuning.back_slice = table.get("back_slice").and_then(Value::as_str).map(str::to_string);
        if let Some(mode) = table.get("fire_mode").and_then(Value::as_str) {
            tuning.fire_mode = FireMode::parse(mode).unwrap_or_default();
        }
        for (key, field) in [
            ("edge_threshold", &mut tuning.edge_threshold),
            ("retract_delta", &mut tuning.retract_delta),
            ("trigger_close_pct", &mut tuning.trigger_close_pct),
            ("trigger_open_pct", &mut tuning.trigger_open_pct),
            ("deadzone", &mut tuning.deadzone),
        ] {
            if let Some(value) = table.f32_of(key) {
                *field = value;
            }
        }
        for (key, field) in [
            ("fire_debounce_ms", &mut tuning.fire_debounce_ms),
            ("release_grace_ms", &mut tuning.release_grace_ms),
            ("aim_dwell_ms", &mut tuning.aim_dwell_ms),
            ("nav_dwell_ms", &mut tuning.nav_dwell_ms),
            ("wheel_min_open_ms", &mut tuning.wheel_min_open_ms),
        ] {
            if let Some(value) = table.u64_of(key) {
                *field = value as u32;
            }
        }
        tuning
    }

    /// The same, from the file's text.
    pub fn parse(text: &str) -> Result<Tuning, String> {
        Ok(Tuning::read(&text::parse_toml(text)?))
    }

    /// The trigger travel squeezed into the range the pad reports in.
    ///
    /// **ESTABLISHED** (`0x10145b48c`): `f32::clamp` between the open and close
    /// percentages — hence the panic the binary carries for `min > max`.
    pub fn trigger(&self, pct: f32) -> f32 {
        pct.clamp(self.trigger_open_pct, self.trigger_close_pct)
    }

    /// Whether the trigger counts as held at `pct`.
    ///
    /// The raw travel is compared, not the clamped one, so a trigger that has
    /// not moved at all is not held.
    pub fn held(&self, pct: f32) -> bool {
        pct >= self.trigger_open_pct
    }

    /// The aim dwell, as a duration.
    pub fn aim_dwell(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.aim_dwell_ms as u64)
    }

    /// The navigation dwell, as a duration.
    pub fn nav_dwell(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.nav_dwell_ms as u64)
    }

    /// The shortest time the wheel stays open, as a duration.
    pub fn wheel_min_open(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.wheel_min_open_ms as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_binarys_own_numbers() {
        let tuning = Tuning::default();
        assert_eq!(tuning.edge_threshold, 150.0);
        assert_eq!(tuning.retract_delta, 150.0);
        assert_eq!(tuning.trigger_close_pct, 300.0);
        assert_eq!(tuning.trigger_open_pct, 40.0);
        assert_eq!(tuning.movement_stick, Stick::Left);
        assert_eq!(tuning.opposing_stick, Stick::Right);
        assert_eq!(tuning.fire_mode, FireMode::Dwell);
    }

    #[test]
    fn the_trigger_clamps_between_its_two_percentages() {
        let tuning = Tuning::default();
        assert_eq!(tuning.trigger(0.0), 40.0);
        assert_eq!(tuning.trigger(100.0), 100.0);
        assert_eq!(tuning.trigger(400.0), 300.0);
        assert!(!tuning.held(0.0));
        assert!(tuning.held(50.0));
    }

    #[test]
    fn the_table_overrides_the_defaults() {
        let text = "\
[controller_tuning]
movement_stick = \"right\"
fire_mode = \"release\"
edge_threshold = 90.0
aim_dwell_ms = 250
wheel_min_open_ms = 120
deadzone = 0.2
";
        let tuning = Tuning::parse(text).unwrap();
        assert_eq!(tuning.movement_stick, Stick::Right);
        // the opposing stick follows the movement stick when the file is silent
        assert_eq!(tuning.opposing_stick, Stick::Left);
        assert_eq!(tuning.fire_mode, FireMode::Release);
        assert_eq!(tuning.edge_threshold, 90.0);
        assert_eq!(tuning.retract_delta, 150.0);
        assert_eq!(tuning.aim_dwell(), std::time::Duration::from_millis(250));
        assert_eq!(tuning.wheel_min_open(), std::time::Duration::from_millis(120));
        assert_eq!(tuning.deadzone, 0.2);
    }

    #[test]
    fn sticks_and_modes_spell_themselves() {
        assert_eq!(Stick::parse(" left "), Some(Stick::Left));
        assert_eq!(Stick::parse("up"), None);
        assert_eq!(Stick::Left.opposing(), Stick::Right);
        assert_eq!(FireMode::parse("dwell"), Some(FireMode::Dwell));
        assert_eq!(FireMode::Release.as_str(), "release");
    }
}
