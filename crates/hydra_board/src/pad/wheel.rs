//! The controller wheel: its geometry, its file format and the view built from
//! it.
//!
//! The pure geometry is recovered exactly:
//!
//!  * [`lay_out`] turns the per-slice `(sized, span)` pairs into the angle at
//!    the centre of each slice, filling the circle to 360 degrees (evidence:
//!    `0x101459e40`).
//!  * [`seat_at_angle`] turns a direction into the index of the slice under it
//!    (`0x101457ab8`).
//!
//! The rest of the module is a working model of the binary's state; where a
//! field's meaning could not be established it is marked `UNKNOWN:` and the
//! accessor is conservative.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::text::{self, Value};

/// No slice is thinner than this, whatever the file says.
///
/// **ESTABLISHED** (`0x101459ebc`, `0x101459f20`): `fmov.4s v0, #30.0` and the
/// `fmaxnm` that follows each span.
pub const MIN_SPAN: f32 = 30.0;

/// A full turn, in degrees.
///
/// **ESTABLISHED** (`0x10145a0fc`): `0x43b40000`.
pub const FULL_TURN: f32 = 360.0;

/// The tolerance under which a total counts as already being a full turn.
///
/// **ESTABLISHED** (`0x10145a2d4`): `0x3a83126f`.
pub const FULL_TURN_SLACK: f32 = 0.001;

/// Degrees per radian, as the binary's literal `0x42652ee1`.
///
/// **ESTABLISHED** (`0x101457ad8`, `0x101457adc`): `mov w8,#0x2ee1` /
/// `movk w8,#0x4265,lsl 16`.
pub const DEGREES_PER_RADIAN: f32 = 57.295_776;

/// The key that holds the wheels.
pub const FILE: &str = "controller_wheels";

/// The key that holds the per-wheel metadata.
pub const META_FILE: &str = "controller_wheels_meta";

/// The name of the wheel used when nothing else is chosen.
///
/// **ESTABLISHED** (`0x101459114`): the key `"default"` (length 7).
pub const DEFAULT_WHEEL: &str = "default";

/// The label of the slice that goes back up a level.
///
/// **ESTABLISHED** (`0x1014586a8`, `0x101458350`): the six bytes `"back"`.
pub const BACK: &str = "back";

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/// How wide each slice is, in degrees; the total is always a full turn.
///
/// `weights[i].0` says whether the slice has a span of its own; `weights[i].1`
/// is that span.  Slices without one share what is left of the circle equally.
///
/// **ESTABLISHED** (`0x101459e40`):
///
///  1. each span is clamped to at least [`MIN_SPAN`];
///  2. `sum` is the total of the *sized* spans and `unsized` the count of the
///     rest; each unsized slice then gets `((360 - sum) / unsized).max(0)`;
///  3. if the whole total is at most `f32::EPSILON` (the literal `0x34000000`
///     at `0x10145a2b8`) every slice gets `360 / count` instead;
///  4. if the total is more than [`FULL_TURN_SLACK`] away from 360 every span is
///     scaled by `360 / total`;
///  5. the returned angles are the *centres* of the slices, starting at
///     `start_angle - spans[0] / 2`.
pub fn spans(weights: &[(bool, f32)]) -> Vec<f32> {
    if weights.is_empty() {
        return Vec::new();
    }
    let clamped: Vec<f32> = weights
        .iter()
        .map(|(_, span)| {
            // `fminnm`/`fmaxnm`: a NaN span becomes MIN_SPAN, as `f32::max` does.
            span.max(MIN_SPAN)
        })
        .collect();

    let mut sum = -0.0f32;
    for (i, (sized, _)) in weights.iter().enumerate() {
        if *sized {
            sum += clamped[i];
        }
    }
    let unsized_count = weights.iter().filter(|(sized, _)| !*sized).count();
    let share = if unsized_count != 0 {
        ((FULL_TURN - sum) / unsized_count as f32).max(0.0)
    } else {
        0.0
    };

    let mut spans: Vec<f32> = weights
        .iter()
        .enumerate()
        .map(|(i, (sized, _))| if *sized { clamped[i] } else { share })
        .collect();

    let mut total = -0.0f32;
    for span in &spans {
        total += *span;
    }
    if total > f32::EPSILON {
        if (total - FULL_TURN).abs() > FULL_TURN_SLACK {
            let scale = FULL_TURN / total;
            for span in &mut spans {
                *span *= scale;
            }
        }
    } else {
        let each = FULL_TURN / spans.len() as f32;
        for span in &mut spans {
            *span = each;
        }
    }

    spans
}

/// The same, as the angle at the centre of each slice.
///
/// The centres are `spans[0] / 2` apart from `start_angle` and each later one is
/// half of the two neighbouring spans further round, which is exactly what
/// contiguous slices give (`0x10145a46c`: `spans[0] * -0.5` added to the start
/// angle, then a running total).
pub fn lay_out(weights: &[(bool, f32)], start_angle: f32) -> Vec<f32> {
    let spans = spans(weights);
    let Some(first) = spans.first() else {
        return Vec::new();
    };
    let mut angle = start_angle - first * 0.5;
    let mut out = Vec::with_capacity(spans.len());
    for span in spans {
        out.push(angle + span * 0.5);
        angle += span;
    }
    out
}

/// Which slice of `seats` a direction is over.
///
/// `seats[i]` is `(centre, span)` in degrees, in the order [`lay_out`] returns
/// them.  The answer is `Some(index)` when the direction lands in a slice and
/// `None` when there are no slices at all.
///
/// `sin` and `cos` are the direction's sine and cosine: the one caller
/// (`hydra_gui`'s wheel editor, `0x1006b50c8`) passes what `sincosf` just
/// returned.
///
/// **ESTABLISHED** (`0x101457ab8`): `atan2f(sin, cos)` (the `atan2f` stub at
/// `0x101f8443c`) is turned into degrees with [`DEGREES_PER_RADIAN`], the first
/// seat's angle is subtracted, the result is folded into `[0, 360)` with
/// `fmodf` (stub `0x101f84718`) plus 360 when negative, and then the seats'
/// spans are added up until the angle falls inside one.
pub fn seat_at_angle(seats: &[[f32; 2]], sin: f32, cos: f32) -> Option<usize> {
    if seats.is_empty() {
        return None;
    }
    let turned = sin.atan2(cos) * DEGREES_PER_RADIAN;
    let mut angle = turned - seats[0][0];
    angle %= FULL_TURN;
    if angle < 0.0 {
        angle += FULL_TURN;
    }
    let mut edge = 0.0f32;
    for (i, seat) in seats.iter().enumerate() {
        edge += seat[1];
        if angle < edge {
            return Some(i);
        }
    }
    Some(seats.len() - 1)
}

// ---------------------------------------------------------------------------
// The file format
// ---------------------------------------------------------------------------

/// One slice of a wheel.
///
/// The nine field names come from the `__FieldVisitor` at `0x10145db28`:
/// `label`(0), `command`(1), `color`(2), `span`(3), `inner`(4), `slices`(5),
/// `start`(6), `back`(7), `fire_type`(8) — the `deserialize` glue spelled
/// `struct Slice with 9 elements`.  The element is 144 bytes; `slices` is the
/// `Vec<Slice>` at `+0x30` (used by [`Wheels::level`]), `span` is the
/// `Option<f32>` at `+0x78` and `start` the `Option<f32>` at `+0x80` (used by
/// [`Wheels::start_of`]).
///
/// UNKNOWN: `inner` and `back` are only ever seen as single values; they are
/// modelled as option values without further semantics.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Slice {
    pub label: Option<String>,
    pub command: Option<String>,
    pub color: Option<String>,
    /// The slice's own angular width, when it has one; slices without a span
    /// share the rest of the circle.
    pub span: Option<f32>,
    pub inner: Option<String>,
    /// The sub-wheel this slice opens.
    pub slices: Vec<Slice>,
    /// Where the slice's centre sits, when it is pinned.
    pub start: Option<f32>,
    pub back: Option<String>,
    pub fire_type: Option<String>,
}

impl Slice {
    /// The slice that goes back up a level: a bare [`BACK`] label.
    ///
    /// **ESTABLISHED** (`0x1014586a8`): a 6-byte allocation holding `"back"`.
    pub fn back() -> Slice {
        Slice {
            label: Some(BACK.to_string()),
            ..Slice::default()
        }
    }

    /// Is this the way back?
    pub fn is_back(&self) -> bool {
        self.label.as_deref() == Some(BACK)
    }

    /// Read a slice out of a parsed document.
    pub fn read(value: &Value) -> Slice {
        let mut slice = Slice::default();
        match value {
            Value::Str(label) => {
                slice.label = Some(label.clone());
            }
            Value::Table(_) => {
                slice.label = value
                    .get("label")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                slice.command = value
                    .get("command")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                slice.color = value
                    .get("color")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                slice.span = value.f32_of("span");
                slice.start = value.f32_of("start");
                slice.inner = value
                    .get("inner")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                slice.back = value.get("back").and_then(Value::as_str).map(str::to_string);
                slice.fire_type = value
                    .get("fire_type")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if let Some(children) = value.get("slices").and_then(Value::as_array) {
                    slice.slices = children.iter().map(Slice::read).collect();
                }
            }
            _ => {}
        }
        slice
    }

    /// Write a slice back out in the same shape.
    pub fn write(&self) -> Value {
        let mut map = BTreeMap::new();
        if let Some(label) = &self.label {
            map.insert("label".to_string(), Value::Str(label.clone()));
        }
        if let Some(command) = &self.command {
            map.insert("command".to_string(), Value::Str(command.clone()));
        }
        if let Some(color) = &self.color {
            map.insert("color".to_string(), Value::Str(color.clone()));
        }
        if let Some(span) = self.span {
            map.insert("span".to_string(), Value::Float(span as f64));
        }
        if let Some(inner) = &self.inner {
            map.insert("inner".to_string(), Value::Str(inner.clone()));
        }
        if !self.slices.is_empty() {
            map.insert(
                "slices".to_string(),
                Value::Array(self.slices.iter().map(Slice::write).collect()),
            );
        }
        if let Some(start) = self.start {
            map.insert("start".to_string(), Value::Float(start as f64));
        }
        if let Some(back) = &self.back {
            map.insert("back".to_string(), Value::Str(back.clone()));
        }
        if let Some(fire_type) = &self.fire_type {
            map.insert("fire_type".to_string(), Value::Str(fire_type.clone()));
        }
        Value::Table(map)
    }

    /// The pair [`lay_out`] wants: `(has a span, the span)`.
    ///
    /// **ESTABLISHED** (`0x1014584c0`): the flag is read from `+0x78` and the
    /// float from `+0x7c`, i.e. the `Option<f32>` `span`.
    pub fn weight(&self) -> (bool, f32) {
        match self.span {
            Some(span) => (true, span),
            None => (false, 0.0),
        }
    }
}

/// Per-wheel metadata (`controller_wheels_meta`).
///
/// **PARTLY** (`0x101459dd4`): the first field is the `Option<f32>` at `+0x0`
/// that [`Wheels::start_of`] reads when the path is empty — the wheel's own
/// start angle.  UNKNOWN: the remaining fields; only `start` is used here.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meta {
    pub start: Option<f32>,
}

impl Meta {
    pub fn read(value: &Value) -> Meta {
        Meta {
            start: value.f32_of("start"),
        }
    }

    pub fn write(&self) -> Value {
        let mut map = BTreeMap::new();
        if let Some(start) = self.start {
            map.insert("start".to_string(), Value::Float(start as f64));
        }
        Value::Table(map)
    }
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/// The laid-out wheel: the slices, the path that got here and the angles.
///
/// **ESTABLISHED** shape (`0x1014583e8`): three `Vec`s in a row — the `Slice`s,
/// 16-byte `(kind, index)` pairs, and the `f32` angles [`lay_out`] produced.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct View {
    /// The slices on this level, in file order.
    pub slices: Vec<Slice>,
    /// One entry per slice: the `(kind, index)` pair [`View::real`] reshapes.
    pub path: Vec<(usize, usize)>,
    /// The angle at the centre of each slice, in degrees.
    pub angles: Vec<f32>,
}

impl View {
    /// Build a view over `slices`, laying them out from `start` degrees.
    ///
    /// When `wants_back` is set, the level is not called `"none"` and no slice
    /// is already a way back, a [`Slice::back`] is prepended.
    ///
    /// **ESTABLISHED** (`0x1014583e8`): the "is there already a way back" test
    /// (`0x10145850c`) and the appended six-byte `"back"` (`0x1014586a8`).
    pub fn build(slices: &[Slice], wants_back: bool, name: &str, start: f32) -> View {
        let mut slices: Vec<Slice> = slices.to_vec();
        if wants_back && name != "none" && !slices.iter().any(|s| s.is_back()) {
            slices.insert(0, Slice::back());
        }
        let weights: Vec<(bool, f32)> = slices.iter().map(Slice::weight).collect();
        let angles = lay_out(&weights, start);
        let path = (0..slices.len()).map(|i| (1, i)).collect();
        View {
            slices,
            path,
            angles,
        }
    }

    /// The `(kind, index)` pair behind `i`, with the kind flipped.
    ///
    /// **ESTABLISHED** (`0x1014583c0`): `eor x0, kind, #1`; out of range gives
    /// the sentinel `2`.
    pub fn real(&self, i: usize) -> (usize, usize) {
        match self.path.get(i) {
            Some((kind, index)) => (kind ^ 1, *index),
            None => (2, 0),
        }
    }

    /// The slice at `i`, when it is a leaf the pointer can pick.
    ///
    /// A slice is not a leaf when its pair's kind is zero, when it has
    /// sub-slices of its own, or when it is the way back.
    ///
    /// **ESTABLISHED** (`0x101458304`), including the `"back"` comparison.
    pub fn leaf(&self, i: usize) -> Option<&Slice> {
        let (kind, _) = self.path.get(i).copied()?;
        if kind == 0 {
            return None;
        }
        let slice = self.slices.get(i)?;
        if !slice.slices.is_empty() || slice.is_back() {
            return None;
        }
        Some(slice)
    }

    /// The seats [`seat_at_angle`] wants: `(centre, span)` per slice.
    pub fn seats(&self) -> Vec<[f32; 2]> {
        let weights: Vec<(bool, f32)> = self.slices.iter().map(Slice::weight).collect();
        let spans = spans(&weights);
        self.angles
            .iter()
            .zip(spans)
            .map(|(angle, span)| [*angle, span])
            .collect()
    }

    /// Which slice a direction's sine and cosine are over.
    pub fn over(&self, sin: f32, cos: f32) -> Option<usize> {
        seat_at_angle(&self.seats(), sin, cos)
    }
}

// ---------------------------------------------------------------------------
// An open wheel
// ---------------------------------------------------------------------------

/// A wheel that is on screen right now.
///
/// **PARTLY** (`0x10145804c`): the constructor takes the wheel's label and one
/// flag and zeroes the rest; the label is the `String` at `+0x20`, `Vec`s sit at
/// `+0x0` and `+0x38`, two `Duration`s at `+0x50` and `+0x58` (the second starts
/// at one second — `0x3b9aca00` nanos — the wheel's `wheel_min_open_ms`) and the
/// flag at `+0x64`.  UNKNOWN: which of the two `Vec`s is the aimed path; they
/// are modelled as `path` and `picks`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Open {
    /// The slices aimed at so far, outermost first.
    pub path: Vec<usize>,
    /// The index of the slice under the pointer.
    pub index: usize,
    /// The wheel's label.
    pub label: String,
    /// One `(kind, index)` per pick made while the wheel has been open.
    pub picks: Vec<(usize, usize)>,
    /// How long the aim must rest before it fires (`aim_dwell_ms`).
    pub dwell: Duration,
    /// How long the wheel stays open at least (`wheel_min_open_ms`).
    pub min_open: Duration,
    /// When the aim last moved.
    pub aimed_at: Option<Instant>,
    /// A count the binary keeps at `+0x60`.
    pub counted: u32,
    /// The flag the constructor was given (`+0x64`).
    pub nav: bool,
}

impl Open {
    /// **ESTABLISHED** (`0x10145804c`): a label and one flag; the rest empty.
    pub fn new(label: &str, nav: bool) -> Open {
        Open {
            path: Vec::new(),
            index: 0,
            label: label.to_string(),
            picks: Vec::new(),
            dwell: Duration::from_secs(1),
            min_open: Duration::from_secs(1),
            aimed_at: None,
            counted: 0,
            nav,
        }
    }

    /// Aim at `index`, returning the dwell that still has to pass before it
    /// fires.
    ///
    /// **PARTLY** (`0x101457b60`): the binary measures
    /// `Instant::saturating_duration_since` against the dwell and grows a
    /// `picks` vector; the exact rule it applies when the aim changes is
    /// UNKNOWN, so this restarts the wait on a change and reports the time
    /// still owed otherwise.
    pub fn aim(&mut self, index: usize, now: Instant) -> Duration {
        if index != self.index || self.aimed_at.is_none() {
            self.index = index;
            self.aimed_at = Some(now);
            self.counted = 0;
            return self.dwell;
        }
        let waited = self
            .aimed_at
            .map(|at| now.saturating_duration_since(at))
            .unwrap_or_default();
        match waited.checked_sub(self.dwell) {
            Some(_) => {
                self.counted = self.counted.saturating_add(1);
                Duration::ZERO
            }
            None => self.dwell - waited,
        }
    }

    /// The index due south of the centre.
    ///
    /// **PARTLY** (`0x101458100`): the binary reads its index fields and tests
    /// them against the view; the rule beyond that is UNKNOWN, so this is the
    /// identity on the recorded index, with `None` when nothing is open.
    pub fn south(&self, view: &View) -> Option<usize> {
        if self.path.is_empty() && self.picks.is_empty() {
            return None;
        }
        if view.slices.is_empty() {
            return None;
        }
        Some(self.index.min(view.slices.len() - 1))
    }
}

// ---------------------------------------------------------------------------
// The whole set of wheels
// ---------------------------------------------------------------------------

/// Every wheel in `controller_wheels`, plus its metadata.
///
/// **ESTABLISHED** (the drop glue at `0x101430c18`, `0x101430b48`):
/// `BTreeMap<String, Vec<Slice>>` followed by `BTreeMap<String, Meta>`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Wheels {
    pub wheels: BTreeMap<String, Vec<Slice>>,
    pub meta: BTreeMap<String, Meta>,
}

impl Wheels {
    /// Read the wheels out of a parsed document.
    ///
    /// **ESTABLISHED** (`0x101459038`): the standalone `"default"` key becomes
    /// the wheel called `default`, `controller_wheels` holds the named wheels
    /// and `controller_wheels_meta` their metadata; a bad wheel is reported as
    /// `format!("the {} wheel: {}", name, error)` and a bad metadata entry as
    /// `format!("the {} wheel's settings: {}", name, error)` (literals at
    /// `0x103e6a1be`).
    pub fn read(doc: &Value) -> Result<Wheels, String> {
        let mut wheels = Wheels::default();
        if let Some(default) = doc.get(DEFAULT_WHEEL) {
            wheels.wheels.insert(
                DEFAULT_WHEEL.to_string(),
                read_slices(default).map_err(|e| format!("the {DEFAULT_WHEEL} wheel: {e}"))?,
            );
        }
        if let Some(named) = doc.get(FILE).and_then(Value::as_table) {
            for (name, value) in named {
                let slices = read_slices(value).map_err(|e| format!("the {name} wheel: {e}"))?;
                wheels.wheels.insert(name.clone(), slices);
            }
        }
        if let Some(named) = doc.get(META_FILE).and_then(Value::as_table) {
            for (name, value) in named {
                wheels.meta.insert(name.clone(), Meta::read(value));
            }
        }
        Ok(wheels)
    }

    /// The same, straight from a file's text.
    pub fn parse(text: &str) -> Result<Wheels, String> {
        let doc = text::parse_toml(text)?;
        Wheels::read(&doc)
    }

    /// The slices on `name`, or the default wheel when there is no such wheel.
    pub fn of<'a>(&'a self, name: &str) -> Option<&'a [Slice]> {
        self.wheels
            .get(name)
            .or_else(|| self.wheels.get(DEFAULT_WHEEL))
            .map(Vec::as_slice)
    }

    /// Walk `path` down `name` and give back the slices found there.
    ///
    /// **ESTABLISHED** (`0x101459b00`): a missing wheel or an index out of
    /// range gives `None`.
    pub fn level(&self, name: &str, path: &[usize]) -> Option<&[Slice]> {
        let mut here = self.of(name)?;
        for index in path {
            let slice = here.get(*index)?;
            here = &slice.slices;
        }
        Some(here)
    }

    /// The `start` angle recorded for the last slice of `path`, or for the wheel
    /// itself when `path` is empty.
    ///
    /// **ESTABLISHED** (`0x101459c24`): an absent `start` reads as `0.0`.
    pub fn start_of(&self, name: &str, path: &[usize]) -> f32 {
        let Some((last, rest)) = path.split_last() else {
            return self
                .meta
                .get(name)
                .and_then(|meta| meta.start)
                .unwrap_or(0.0);
        };
        let Some(here) = self.level(name, rest) else {
            return 0.0;
        };
        here.get(*last)
            .and_then(|slice| slice.start)
            .unwrap_or(0.0)
    }

    /// Lay `name` out at `path`, ready to draw.
    pub fn view(&self, name: &str, path: &[usize], wants_back: bool) -> View {
        let slices = self.level(name, path).unwrap_or(&[]);
        let start = self.start_of(name, path);
        View::build(slices, wants_back, name, start)
    }

    /// Merge another set of wheels over this one.
    ///
    /// **PARTLY** (`0x101458c78`): the binary clones both maps and inserts each
    /// entry of `other` over the matching one here.
    pub fn over(&mut self, other: &Wheels) {
        for (name, slices) in &other.wheels {
            self.wheels.insert(name.clone(), slices.clone());
        }
        for (name, meta) in &other.meta {
            self.meta.insert(name.clone(), meta.clone());
        }
    }

    /// Write the wheels back out as a document.
    pub fn write(&self) -> Value {
        let mut named = BTreeMap::new();
        for (name, slices) in &self.wheels {
            if name == DEFAULT_WHEEL {
                continue;
            }
            named.insert(
                name.clone(),
                Value::Array(slices.iter().map(Slice::write).collect()),
            );
        }
        let mut metas = BTreeMap::new();
        for (name, meta) in &self.meta {
            metas.insert(name.clone(), meta.write());
        }
        let mut doc = BTreeMap::new();
        if let Some(default) = self.wheels.get(DEFAULT_WHEEL) {
            doc.insert(
                DEFAULT_WHEEL.to_string(),
                Value::Array(default.iter().map(Slice::write).collect()),
            );
        }
        doc.insert(FILE.to_string(), Value::Table(named));
        doc.insert(META_FILE.to_string(), Value::Table(metas));
        Value::Table(doc)
    }
}

fn read_slices(value: &Value) -> Result<Vec<Slice>, String> {
    match value {
        Value::Array(items) => Ok(items.iter().map(Slice::read).collect()),
        Value::Table(_) | Value::Str(_) => Ok(vec![Slice::read(value)]),
        _ => Err("expected a list of slices".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn lay_out_ignores_an_empty_wheel() {
        assert!(lay_out(&[], 0.0).is_empty());
    }

    #[test]
    fn lay_out_clamps_thin_slices_to_thirty_degrees() {
        // Two sized slices of 10 and 50 degrees: the first becomes 30, so the
        // total is 80 and everything is scaled by 360/80 = 4.5.
        assert_eq!(spans(&[(true, 10.0), (true, 50.0)]), vec![135.0, 225.0]);
        let angles = lay_out(&[(true, 10.0), (true, 50.0)], 0.0);
        // The first centre is the start angle; the next is half of both spans on.
        assert!(close(angles[0], 0.0), "{angles:?}");
        assert!(close(angles[1], 180.0), "{angles:?}");
    }

    #[test]
    fn lay_out_shares_the_remainder_between_unsized_slices() {
        // One sized slice of 180 and two without a span: 90 each.
        assert_eq!(
            spans(&[(true, 180.0), (false, 0.0), (false, 0.0)]),
            vec![180.0, 90.0, 90.0]
        );
        let angles = lay_out(&[(true, 180.0), (false, 0.0), (false, 0.0)], 0.0);
        assert_eq!(angles.len(), 3);
        assert!(close(angles[0], 0.0), "{angles:?}");
        assert!(close(angles[1], 135.0), "{angles:?}");
        assert!(close(angles[2], 225.0), "{angles:?}");
    }

    #[test]
    fn lay_out_falls_back_to_equal_shares() {
        // No sized slice and nothing to share out: every slice is 360 / n.
        let angles = lay_out(&[(false, 0.0); 4], 0.0);
        for (i, angle) in angles.iter().enumerate() {
            assert!(close(*angle, 90.0 * i as f32), "{angles:?}");
        }
    }

    #[test]
    fn lay_out_keeps_a_whole_circle_alone() {
        // 120 + 120 + 120 is already 360, so nothing is rescaled.
        assert_eq!(
            spans(&[(true, 120.0), (true, 120.0), (true, 120.0)]),
            vec![120.0, 120.0, 120.0]
        );
        let angles = lay_out(&[(true, 120.0), (true, 120.0), (true, 120.0)], 0.0);
        assert!(close(angles[0], 0.0), "{angles:?}");
        assert!(close(angles[1], 120.0), "{angles:?}");
        assert!(close(angles[2], 240.0), "{angles:?}");
    }

    #[test]
    fn lay_out_starts_at_the_given_angle() {
        let angles = lay_out(&[(true, 360.0)], 90.0);
        assert_eq!(angles.len(), 1);
        assert!(close(angles[0], 90.0), "{angles:?}");
    }

    /// The sine and cosine of a direction in degrees.
    fn dir(degrees: f32) -> (f32, f32) {
        let radians = degrees.to_radians();
        (radians.sin(), radians.cos())
    }

    #[test]
    fn seat_at_angle_finds_the_slice_under_a_direction() {
        // Four quarters centred at 0/90/180/270 degrees.
        let seats = [
            [0.0f32, 90.0],
            [90.0, 90.0],
            [180.0, 90.0],
            [270.0, 90.0],
        ];
        for (want, degrees) in [(0, 10.0), (1, 100.0), (2, 190.0), (3, 280.0)] {
            let (sin, cos) = dir(degrees);
            assert_eq!(seat_at_angle(&seats, sin, cos), Some(want), "{degrees}");
        }
        let (sin, cos) = dir(0.0);
        assert_eq!(seat_at_angle(&[], sin, cos), None);
    }

    #[test]
    fn seat_at_angle_wraps_past_the_last_slice() {
        let seats = [[0.0f32, 180.0], [180.0, 180.0]];
        let (sin, cos) = dir(350.0);
        assert_eq!(seat_at_angle(&seats, sin, cos), Some(1));
    }

    #[test]
    fn every_slice_field_survives_a_round_trip() {
        let text = "\
default = [
  { label = \"look\", command = \"look\", color = \"blue\", span = 90.0, fire_type = \"tap\" },
  { label = \"ring\", slices = [ { label = \"one\", start = 10.0 } ] },
  \"plain\",
]
";
        let wheels = Wheels::parse(text).unwrap();
        let slices = wheels.of(DEFAULT_WHEEL).unwrap();
        assert_eq!(slices.len(), 3);
        assert_eq!(slices[0].label.as_deref(), Some("look"));
        assert_eq!(slices[0].command.as_deref(), Some("look"));
        assert_eq!(slices[0].color.as_deref(), Some("blue"));
        assert_eq!(slices[0].span, Some(90.0));
        assert_eq!(slices[0].fire_type.as_deref(), Some("tap"));
        assert_eq!(slices[1].slices.len(), 1);
        assert_eq!(slices[1].slices[0].start, Some(10.0));
        assert_eq!(slices[2].label.as_deref(), Some("plain"));
        // And back out again.
        let again = Wheels::read(&wheels.write()).unwrap();
        assert_eq!(again, wheels);
    }

    #[test]
    fn level_and_start_of_walk_the_path() {
        let text = "\
[controller_wheels]
main = [ { label = \"ring\", start = 45.0, slices = [ { label = \"deep\", start = 90.0 } ] } ]
";
        let wheels = Wheels::parse(text).unwrap();
        assert_eq!(wheels.level("main", &[]).unwrap().len(), 1);
        assert_eq!(wheels.level("main", &[0]).unwrap().len(), 1);
        assert_eq!(wheels.level("main", &[0, 0]).unwrap().len(), 0);
        assert!(wheels.level("main", &[9]).is_none());
        assert!(wheels.level("nope", &[]).is_none());
        assert_eq!(wheels.start_of("main", &[]), 0.0);
        assert_eq!(wheels.start_of("main", &[0]), 45.0);
        assert_eq!(wheels.start_of("main", &[0, 0]), 90.0);
    }

    #[test]
    fn start_of_reads_the_wheel_metadata() {
        let text = "\
[controller_wheels]
main = []
[controller_wheels_meta.main]
start = 270.0
";
        let wheels = Wheels::parse(text).unwrap();
        assert_eq!(wheels.start_of("main", &[]), 270.0);
    }

    #[test]
    fn a_bad_wheel_is_named_in_the_error() {
        let doc = text::parse_toml("[controller_wheels]\nmain = 3\n").unwrap();
        let err = Wheels::read(&doc).unwrap_err();
        assert!(err.contains("main"), "{err}");
    }

    #[test]
    fn view_build_adds_a_back_slice() {
        let slices = vec![Slice {
            label: Some("look".into()),
            span: Some(180.0),
            ..Slice::default()
        }];
        let view = View::build(&slices, true, "main", 0.0);
        assert_eq!(view.slices.len(), 2);
        assert_eq!(view.slices[0].label.as_deref(), Some(BACK));
        assert_eq!(view.angles.len(), 2);
        // A way back is not a leaf.
        assert!(view.leaf(0).is_none());
        assert_eq!(view.leaf(1).map(|s| s.label.as_deref()), Some(Some("look")));
        // The name "none" suppresses the way back.
        assert_eq!(View::build(&slices, true, "none", 0.0).slices.len(), 1);
        assert_eq!(View::build(&slices, false, "main", 0.0).slices.len(), 1);
    }

    #[test]
    fn real_flips_the_kind_and_sentinelises_a_miss() {
        let view = View::build(&[Slice::default()], false, "main", 0.0);
        assert_eq!(view.path, vec![(1, 0)]);
        assert_eq!(view.real(0), (0, 0));
        assert_eq!(view.real(5), (2, 0));
    }

    #[test]
    fn seats_cover_the_whole_circle() {
        let slices = vec![
            Slice {
                span: Some(180.0),
                ..Slice::default()
            },
            Slice {
                span: Some(180.0),
                ..Slice::default()
            },
        ];
        let view = View::build(&slices, false, "main", 0.0);
        let seats = view.seats();
        assert_eq!(seats.len(), 2);
        let total: f32 = seats.iter().map(|s| s[1]).sum();
        assert!(close(total, 360.0), "{seats:?}");
        // And the hit test agrees with the layout.
        let (sin, cos) = dir(10.0);
        assert_eq!(view.over(sin, cos), Some(0));
    }

    #[test]
    fn open_starts_at_one_second() {
        let open = Open::new("main", true);
        assert_eq!(open.label, "main");
        assert_eq!(open.dwell, Duration::from_secs(1));
        assert_eq!(open.min_open, Duration::from_secs(1));
        assert!(open.nav);
        assert!(open.picks.is_empty());
    }

    #[test]
    fn aim_restarts_the_dwell_when_the_slice_changes() {
        let mut open = Open::new("main", false);
        let start = Instant::now();
        assert_eq!(open.aim(1, start), Duration::from_secs(1));
        assert_eq!(open.index, 1);
        // Same slice, no time gone by: the whole dwell is still owed.
        assert_eq!(open.aim(1, start), Duration::from_secs(1));
        // Two seconds later the dwell has long passed.
        let later = start + Duration::from_secs(2);
        assert_eq!(open.aim(1, later), Duration::ZERO);
        // A different slice starts again.
        assert_eq!(open.aim(2, later), Duration::from_secs(1));
    }

    #[test]
    fn over_merges_one_set_of_wheels_into_another() {
        let mut base = Wheels::parse("[controller_wheels]\na = []\n").unwrap();
        let over = Wheels::parse("[controller_wheels]\nb = []\n").unwrap();
        base.over(&over);
        assert!(base.wheels.contains_key("a"));
        assert!(base.wheels.contains_key("b"));
    }
}
