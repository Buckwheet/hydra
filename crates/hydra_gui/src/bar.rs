//! The status bar: what it says, where it sits, how it is fitted and how it is
//! painted.
//!
//! Recovered from `hydra_gui::bar` (addresses are unslid VAs):
//!
//! | symbol | addr | status |
//! |---|---|---|
//! | `Bar::look` | `0x100708334` | partly |
//! | `Bar::said` | `0x100708760` | full |
//! | `Bar::parts` | `0x100708868` | partly |
//! | `Bar::words` | `0x100708dd0` | full |
//! | `Bar::behind` | `0x100709280` | partly |
//! | `Bar::fitted` | `0x1007093d8` | full (layout maths) |
//! | `<Bar as egui::widgets::Widget>::ui` | `0x1006f4920` | the `egui` widget impl |
//! | `Serialize for Place` | `0x100707c04` | variant names |
//! | `Serialize for Says` | `0x100707a14` | field names/order |
//! | `Serialize for Look` | `0x1007076c0` | field names/order |
//!
//! Phase 4 replaced the [`crate::seam`] painter trait with the real
//! `egui::Painter`: [`Bar::fitted`] measures the caption with the live
//! `TextStyle::Body` (`TextStyle::resolve` @`0x100709488` — the static it reads
//! has variant tag `1`, i.e. `Body`), and [`Bar::write`] hands the recovered
//! primitives to that painter. `Bar` also implements `egui::Widget`, which is
//! what the binary does (`<Bar as egui::widgets::Widget>::ui` @`0x1006f4920`).

pub mod stack;
pub mod vessel;

use crate::json::{self, Value};
use crate::seam::{rect_from_min_max, Color32, Color32Ext, Painter, Rect, Vec2};

/// Where a bar is drawn against its area.
///
/// Names and tag values come from
/// `<... Deserialize for Place>::deserialize::__FieldVisitor::visit_str`
/// @`0x1006d1e70`: length-6 names `inside` (tag 6), `corner` (tag 5),
/// `hidden` (tag 0); length-5 names `above` (1), `below` (2), `right` (4) and
/// (by elimination) `left` (3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Place {
    #[default]
    Hidden = 0,
    Above = 1,
    Below = 2,
    Left = 3,
    Right = 4,
    Corner = 5,
    Inside = 6,
}

impl Place {
    pub const EVERY: [Place; 7] = [
        Place::Hidden,
        Place::Above,
        Place::Below,
        Place::Left,
        Place::Right,
        Place::Corner,
        Place::Inside,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Place::Hidden => "hidden",
            Place::Above => "above",
            Place::Below => "below",
            Place::Left => "left",
            Place::Right => "right",
            Place::Corner => "corner",
            Place::Inside => "inside",
        }
    }

    pub fn of_name(name: &str) -> Option<Place> {
        Place::EVERY.into_iter().find(|p| p.name() == name)
    }

    /// True for the places that run along an edge horizontally, so the *height*
    /// of the bar is what the caption eats into.
    ///
    /// `bar::Bar::fitted` @`0x100709510` tests the bit mask `0x26` = bits 1, 2
    /// and 5, i.e. `Above | Below | Corner`.
    pub fn across(self) -> bool {
        matches!(self, Place::Above | Place::Below | Place::Corner)
    }

    /// True for the places that run vertically: `Bar::fitted` @`0x100709544`
    /// tests the mask `0x18` = bits 3 and 4, i.e. `Left | Right`.
    pub fn down(self) -> bool {
        matches!(self, Place::Left | Place::Right)
    }
}

/// What a bar writes inside itself.
///
/// Field names and declaration order are the literals handed to
/// `SerializeMap::serialize_entry` by `Serialize for Says` @`0x100707a14`
/// (`label`, `numbers`, `percent`, `words`, at offsets 0, 1, 2, 3).
// UNKNOWN: the four fields are one byte each and serialise through four
// distinct monomorphisations; their exact integer type could not be pinned
// down, so they are modelled as plain flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Says {
    pub label: bool,
    pub numbers: bool,
    pub percent: bool,
    pub words: bool,
}

impl Says {
    pub fn to_json(self) -> Value {
        json::obj([
            ("label", Value::Bool(self.label)),
            ("numbers", Value::Bool(self.numbers)),
            ("percent", Value::Bool(self.percent)),
            ("words", Value::Bool(self.words)),
        ])
    }

    pub fn from_json(v: &Value) -> Option<Says> {
        Some(Says {
            label: v.get("label").and_then(Value::as_bool).unwrap_or(false),
            numbers: v.get("numbers").and_then(Value::as_bool).unwrap_or(false),
            percent: v.get("percent").and_then(Value::as_bool).unwrap_or(false),
            words: v.get("words").and_then(Value::as_bool).unwrap_or(false),
        })
    }
}

/// How a bar is painted.
///
/// Field names and their serialisation order are recovered from
/// `Serialize for Look` @`0x1007076c0`: `fills`, `place`, `says`, `color`,
/// `overlay`, `background`, `track`, `fill_image`, `ring`, `clock`,
/// `threshold`, `text_color`, `outline_color`, `outline`, `second_emblem`.
/// `fills` and `place` are `Place`-valued (one byte each, at struct offsets
/// `0x83` and `0x84` — see `Bar::look`, which copies the pair across with a
/// single `strh`).
// UNKNOWN: the *type* of each field past `place` could not be recovered from
// the shared serializer thunks; the colour fields are `Color32` (the recovered
// code forces alpha to `0xff`), `fill_image`/`second_emblem` hold asset names,
// and the rest are flags / an optional threshold.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub fills: Place,
    pub place: Place,
    pub says: Says,
    pub color: Color32,
    pub overlay: bool,
    pub background: Color32,
    pub track: Color32,
    pub fill_image: Option<String>,
    pub ring: bool,
    pub clock: bool,
    pub threshold: Option<f32>,
    pub text_color: Color32,
    pub outline_color: Color32,
    pub outline: bool,
    pub second_emblem: Option<String>,
    /// Which emblem the bar draws.
    ///
    /// `Bar::look` clamps the written index with `min(_, 4)`
    /// (`csel w0, w0, w1(=4), lo` @`0x10070869c`), so there are five slots.
    // UNKNOWN: the field's serialised name (it is not one of the fifteen
    // `serialize_entry` literals, so it is probably part of the `says`/`color`
    // group); it is kept here because the recovered clamp is real.
    pub emblem: usize,
}

impl Default for Look {
    fn default() -> Self {
        Look {
            fills: Place::Hidden,
            place: Place::Hidden,
            says: Says::default(),
            color: Color32::WHITE,
            overlay: false,
            background: Color32::BLACK,
            track: Color32::TRANSPARENT,
            fill_image: None,
            ring: false,
            clock: false,
            threshold: None,
            text_color: Color32::WHITE,
            outline_color: Color32::BLACK,
            outline: false,
            second_emblem: None,
            emblem: 0,
        }
    }
}

impl Look {
    /// The value a full bar is scaled by. `Bar::behind` @`0x1007092c4` loads
    /// `0x42c80000` = `100.0f32` and divides the clamped percentage by it.
    pub const FULL: f32 = 100.0;

    /// The largest emblem index: `min(_, 4)` @`0x10070869c`.
    pub const EMBLEM_MAX: usize = 4;

    pub fn to_json(&self) -> Value {
        fn color(c: Color32) -> Value {
            Value::Num(c.to_u32() as f64)
        }
        fn opt_str(s: &Option<String>) -> Value {
            match s {
                Some(s) => Value::Str(s.clone()),
                None => Value::Null,
            }
        }
        json::obj([
            ("fills", Value::Str(self.fills.name().into())),
            ("place", Value::Str(self.place.name().into())),
            ("says", self.says.to_json()),
            ("color", color(self.color)),
            ("overlay", Value::Bool(self.overlay)),
            ("background", color(self.background)),
            ("track", color(self.track)),
            ("fill_image", opt_str(&self.fill_image)),
            ("ring", Value::Bool(self.ring)),
            ("clock", Value::Bool(self.clock)),
            (
                "threshold",
                match self.threshold {
                    Some(t) => Value::Num(t as f64),
                    None => Value::Null,
                },
            ),
            ("text_color", color(self.text_color)),
            ("outline_color", color(self.outline_color)),
            ("outline", Value::Bool(self.outline)),
            ("second_emblem", opt_str(&self.second_emblem)),
        ])
    }

    pub fn from_json(v: &Value) -> Option<Look> {
        fn color(v: &Value) -> Color32 {
            <Color32 as Color32Ext>::from_u32(v.as_f64().unwrap_or(0.0) as u32)
        }
        fn opt_str(v: Option<&Value>) -> Option<String> {
            v.and_then(Value::as_str).map(str::to_string)
        }
        let mut look = Look::default();
        if let Some(p) = v.get("fills").and_then(Value::as_str) {
            look.fills = Place::of_name(p).unwrap_or(Place::Hidden);
        }
        if let Some(p) = v.get("place").and_then(Value::as_str) {
            look.place = Place::of_name(p).unwrap_or(Place::Hidden);
        }
        if let Some(s) = v.get("says") {
            look.says = Says::from_json(s).unwrap_or_default();
        }
        if let Some(c) = v.get("color") {
            look.color = color(c);
        }
        look.overlay = v.get("overlay").and_then(Value::as_bool).unwrap_or(false);
        if let Some(c) = v.get("background") {
            look.background = color(c);
        }
        if let Some(c) = v.get("track") {
            look.track = color(c);
        }
        look.fill_image = opt_str(v.get("fill_image"));
        look.ring = v.get("ring").and_then(Value::as_bool).unwrap_or(false);
        look.clock = v.get("clock").and_then(Value::as_bool).unwrap_or(false);
        look.threshold = v.get("threshold").and_then(Value::as_f32);
        if let Some(c) = v.get("text_color") {
            look.text_color = color(c);
        }
        if let Some(c) = v.get("outline_color") {
            look.outline_color = color(c);
        }
        look.outline = v.get("outline").and_then(Value::as_bool).unwrap_or(false);
        look.second_emblem = opt_str(v.get("second_emblem"));
        Some(look)
    }
}

/// Extra text a bar shows while a hunt is on file.
// UNKNOWN: `Bar::words` @`0x100708e00` tests the two words at struct offsets
// `0x130`/`0x138` and, when either is present, formats the caption with the
// recovered literal "{} {} : hunt #{} on file spans the same".  The two words
// are modelled as the place name and the hunt number.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hunt {
    pub place: String,
    pub number: u32,
}

/// The font the caption is drawn with.
///
/// `Bar::fitted` resolves `TextStyle::Body` against the *live* style
/// (`TextStyle::resolve` @`0x100709488`); `Bar::write` only has a painter, so
/// the same style is resolved once against `Style::default()` — which at this
/// egui rev is `FontId::proportional(13.0)`.
pub fn body_font() -> egui::FontId {
    static BODY: std::sync::OnceLock<egui::FontId> = std::sync::OnceLock::new();
    BODY.get_or_init(|| egui::TextStyle::Body.resolve(&egui::Style::default()))
        .clone()
}

/// One status bar.
///
/// Layout evidence (struct offsets in the original 368-byte `Bar`):
/// `0x130`/`0x138` hunt words, `0x140` "measured" flag, `0x144` the measured
/// [`Vec2`], `0x14c` a mode tag (compared with 2 by `Bar::look`/`behind`),
/// `0x15c` the fill percentage, `0x165` the fill [`Place`], `0x166` the bar
/// [`Place`].
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub look: Look,
    /// The caption's label, e.g. `HP`.
    pub label: String,
    /// 0.0 ..= 1.0.
    pub value: f32,
    /// Spelled-out text, shown when `look.says.words` is set.
    pub text: String,
    /// The fill percentage as stored (`0x15c`), clamped to 100 on use.
    pub fills: u8,
    pub place: Place,
    pub hunt: Option<Hunt>,
    /// `Bar::fitted` sets this together with [`Bar::size`] (`0x140`).
    pub measured: bool,
    pub size: Vec2,
}

impl Default for Bar {
    fn default() -> Self {
        Bar {
            look: Look::default(),
            label: String::new(),
            value: 0.0,
            text: String::new(),
            fills: 0,
            place: Place::Hidden,
            hunt: None,
            measured: false,
            size: Vec2::ZERO,
        }
    }
}

impl Bar {
    /// Padding added around the measured caption before it is taken off the
    /// available size: `Bar::fitted` @`0x1007094fc` adds `4.0` to both axes of
    /// the galley size (`fmov.2s v1, #4.00000000`).
    pub const PAD: f32 = 4.0;

    /// The smallest size `fitted` will report: `Bar::fitted` @`0x100709568`
    /// clamps with `movi.2s v1, #0x41, lsl #24` = `8.0`.
    pub const FLOOR: f32 = 8.0;

    /// The fill fraction, `fills / 100`, clamped to `0 ..= 1`.
    ///
    /// `Bar::behind` @`0x1007092b0` loads `0x15c`, clamps with
    /// `csel w8, w8, w9(=100), lo` and divides by 100.0.
    pub fn fraction(&self) -> f32 {
        let percent = self.fills.min(100) as f32;
        percent / Look::FULL
    }

    /// The part of `area` the fill covers.
    ///
    /// The same axis choice `Bar::behind` uses for its hit test
    /// (`0x1007092d4`…`0x100709334`): `Above` fills from the bottom edge up,
    /// `Below` from the top down, `Right` from the left edge across and every
    /// other place (`Left` above all) from the right edge back.
    pub fn fill_rect(&self, area: Rect) -> Rect {
        let fill = self.fraction();
        let (min, max) = (Vec2::new(area.min.x, area.min.y), Vec2::new(area.max.x, area.max.y));
        match self.place {
            Place::Above => rect_from_min_max(
                Vec2::new(min.x, max.y - area.height() * fill),
                max,
            ),
            Place::Below => rect_from_min_max(
                min,
                Vec2::new(max.x, min.y + area.height() * fill),
            ),
            Place::Right => rect_from_min_max(
                min,
                Vec2::new(min.x + area.width() * fill, max.y),
            ),
            _ => rect_from_min_max(
                Vec2::new(max.x - area.width() * fill, min.y),
                max,
            ),
        }
    }

    /// Apply a written look over this bar's own look and return the result.
    ///
    /// Recovered from `Bar::look` @`0x100708334`: the original copies the live
    /// look out of `self`, overlays the written fields (colours are re-packed
    /// with a forced `0xff` alpha — @`0x10070863c`, and the emblem index is
    /// clamped to 4 — @`0x100708698`), stores the written `fills`/`place` pair
    /// back into `self` (`strh w9, [x0, #0x165]` @`0x100708344`) and returns a
    /// copy of the result.
    // UNKNOWN: which of the two `Place` bytes is `fills` and which is `place`.
    pub fn look(&mut self, over: &Look) -> Look {
        // Base: the live look (`ldp q0, q1, [x0]` … @0x100708354).
        let mut look = self.look.clone();
        // The written half supplies the `fills`/`place` pair and its colours,
        // each re-packed with a forced `0xff` alpha.
        look.fills = over.fills;
        look.place = over.place;
        look.color = over.color.opaque();
        look.background = over.background.opaque();
        look.track = over.track.opaque();
        look.text_color = over.text_color.opaque();
        look.outline_color = over.outline_color.opaque();
        look.emblem = over.emblem.min(Look::EMBLEM_MAX);
        self.look = look.clone();
        self.place = look.place;
        look
    }

    /// The caption as two halves: the label half and the value half.
    ///
    /// `Bar::parts` @`0x100708868` builds a `Vec<String>` of pieces, joins each
    /// group, and returns the pair; `Bar::said` @`0x100708760` then joins the
    /// two with a single space.
    // UNKNOWN: the exact split between the halves; the recovered code joins two
    // `String`s and each is itself produced by a `join`.
    pub fn parts(&self) -> (String, String) {
        let mut head: Vec<String> = Vec::new();
        let mut tail: Vec<String> = Vec::new();
        if self.look.says.label && !self.label.is_empty() {
            head.push(self.label.clone());
        }
        if self.look.says.numbers {
            tail.push(format!("{}", self.value));
        }
        if self.look.says.percent {
            tail.push(format!("{}%", (self.value * Look::FULL).round() as i32));
        }
        if self.look.says.words && !self.text.is_empty() {
            tail.push(self.text.clone());
        }
        (head.join(" "), tail.join(" "))
    }

    /// The caption: the two halves joined by a single space.
    ///
    /// `Bar::said` @`0x100708760` calls `join_generic_copy` with the literal
    /// `" "` (1 byte) at `0x102144b98`.
    pub fn said(&self) -> String {
        let (head, tail) = self.parts();
        match (head.is_empty(), tail.is_empty()) {
            (true, true) => String::new(),
            (true, false) => tail,
            (false, true) => head,
            (false, false) => format!("{head} {tail}"),
        }
    }

    /// What the bar shows.
    ///
    /// `Bar::words` @`0x100708dd0`: the caption, or — when the hunt words at
    /// `0x130`/`0x138` are present — `"{} {} : hunt #{} on file spans the
    /// same"` (recovered from the format pieces at `0x103e21840`/`0x103e2184a`).
    pub fn words(&self) -> String {
        match &self.hunt {
            None => self.said(),
            Some(hunt) => format!(
                "{} {} : hunt #{} on file spans the same",
                self.said(),
                hunt.place,
                hunt.number
            ),
        }
    }

    /// Fit the bar inside the space `ui` offers, given its caption.
    ///
    /// Recovered from `Bar::fitted` @`0x1007093d8`:
    ///
    /// * take `ui.available_size()` (`bl egui::ui::Ui::available_size`
    ///   @`0x100709404`);
    /// * the caption is `parts().0` for [`Place::Corner`] and [`Bar::words`]
    ///   otherwise (`cmp w8, #5` @`0x100709410`);
    /// * only places 1..=5 measure anything (`sub w9, w8, #1; cmp w9, #4`);
    /// * the caption is laid out with `TextStyle::Body` and its **height** is
    ///   read (`ldp d9, d8, [galley, #0x30]` @`0x1007094d4`), then
    ///   [`Bar::PAD`] is added and taken off the *height* for
    ///   `Above | Below | Corner` and off the *width* for `Left | Right`;
    /// * the result is clamped to [`Bar::FLOOR`] on both axes, stored, and the
    ///   bar is returned.
    pub fn fitted(mut self, ui: &mut egui::Ui) -> Bar {
        let available = ui.available_size();
        let caption = if self.place == Place::Corner {
            self.parts().0
        } else {
            self.words()
        };
        let mut size = available;
        let measures = matches!(
            self.place,
            Place::Above | Place::Below | Place::Left | Place::Right | Place::Corner
        );
        if measures && !caption.is_empty() {
            // `TextStyle::resolve` @0x100709488 then the layout call
            // @0x1007094cc; only `galley.size().y` is used.
            let font_id = egui::TextStyle::Body.resolve(ui.style());
            let galley = ui
                .painter()
                .layout(caption, font_id, self.look.text_color, f32::INFINITY);
            let eats = galley.size().y + Bar::PAD;
            if self.place.across() {
                size.y = available.y - eats;
            } else {
                size.x = available.x - eats;
            }
        }
        self.measured = true;
        self.size = size.max(Vec2::splat(Bar::FLOOR));
        self
    }

    /// Is `at` behind the filled part of the bar inside `area`?
    ///
    /// Recovered from `Bar::behind` @`0x100709280`: the fill fraction comes from
    /// [`Bar::fraction`], `Left | Right` fills shrink the *width* and the other
    /// places shrink the *height*, and a `fills` of `Corner` grows the area by
    /// half the bar's thickness before the test (`fmul s3, s3, #0.5`
    /// @`0x100709300`).
    // UNKNOWN: the original also threads a fourth argument through unchanged
    // (`mov x0, x2` @`0x1007092a8`); it is modelled here as a bool.
    pub fn behind(&self, area: &mut Rect, at: Vec2) -> bool {
        let fraction = self.fraction();
        if self.look.fills == Place::Corner {
            let half = (area.width().min(area.height()) * 0.5).abs();
            area.min = egui::Pos2::new(area.min.x - half, area.min.y - half);
            area.max = egui::Pos2::new(area.max.x + half, area.max.y + half);
        }
        if fraction <= 0.0 {
            return false;
        }
        let band = |lo: f32, hi: f32, v: f32| v >= lo && v <= hi;
        match self.place {
            Place::Above => {
                let edge = area.max.y - area.height() * fraction;
                band(area.min.x, area.max.x, at.x) && band(edge, area.max.y, at.y)
            }
            Place::Below => {
                let edge = area.min.y + area.height() * fraction;
                band(area.min.x, area.max.x, at.x) && band(area.min.y, edge, at.y)
            }
            Place::Right => {
                let edge = area.min.x + area.width() * fraction;
                band(area.min.y, area.max.y, at.y) && band(area.min.x, edge, at.x)
            }
            _ => {
                let edge = area.max.x - area.width() * fraction;
                band(area.min.y, area.max.y, at.y) && band(edge, area.max.x, at.x)
            }
        }
    }

    /// Paint the bar into `area`.
    ///
    /// `Bar::write` @`0x100708f04` was not reconstructed instruction by
    /// instruction; this paints the recovered fields — `track`, `background`,
    /// the fill (`color`, [`Bar::fill_rect`]), `outline`/`outline_color`,
    /// `ring`, the emblem (`min(index, 4)`) and the caption in `text_color` —
    /// through the real `egui::Painter`.
    pub fn write(&self, painter: &Painter, area: Rect) {
        let look = &self.look;
        if look.track.a() > 0 {
            painter.rect_filled(area, egui::CornerRadius::ZERO, look.track);
        }
        painter.rect_filled(area, egui::CornerRadius::ZERO, look.background);
        let filled = self.fill_rect(area);
        if filled.width() > 0.0 && filled.height() > 0.0 {
            painter.rect_filled(filled, egui::CornerRadius::ZERO, look.color);
        }
        if look.outline {
            painter.rect_stroke(
                area,
                egui::CornerRadius::ZERO,
                egui::Stroke::new(1.0, look.outline_color),
                egui::StrokeKind::Inside,
            );
        }
        if look.ring {
            painter.rect_stroke(
                area.shrink(1.0),
                egui::CornerRadius::ZERO,
                egui::Stroke::new(2.0, look.color),
                egui::StrokeKind::Inside,
            );
        }
        self.emblem(painter, area);
        let caption = self.words();
        if !caption.is_empty() {
            painter.text(
                area.center(),
                egui::Align2::CENTER_CENTER,
                caption,
                body_font(),
                look.text_color,
            );
        }
    }

    /// Paint the emblem disc(s).
    ///
    /// The recovered look clamps the emblem index with `min(_, 4)`
    /// @`0x10070869c`, i.e. five slots.
    // UNKNOWN: where each slot sits and how big the emblem is. The disc is
    // drawn at the bar's left edge, one diameter further right per slot, with
    // `second_emblem` (when present) mirrored at the right edge.
    pub fn emblem(&self, painter: &Painter, area: Rect) {
        let radius = (area.height() * 0.5 - 1.0).max(1.0);
        let slot = self.look.emblem.min(Look::EMBLEM_MAX) as f32;
        let inset = radius + 1.0 + slot * (radius * 2.0 + 2.0);
        let x = (area.min.x + inset).min(area.max.x - radius);
        painter.circle_filled(
            egui::Pos2::new(x, area.center().y),
            radius,
            self.look.text_color,
        );
        if self.look.second_emblem.is_some() {
            painter.circle_filled(
                egui::Pos2::new(area.max.x - inset.max(radius + 1.0), area.center().y),
                radius,
                self.look.text_color,
            );
        }
    }
}

impl egui::Widget for Bar {
    /// `<Bar as egui::widgets::Widget>::ui` @`0x1006f4920`: fit the bar to the
    /// space the `Ui` offers, take that space, and paint.
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let bar = self.fitted(ui);
        let (rect, response) = ui.allocate_exact_size(bar.size, egui::Sense::click());
        bar.write(ui.painter(), rect);
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless;
    use crate::seam::rect;

    fn caption_bar() -> Bar {
        Bar {
            look: Look {
                says: Says {
                    label: true,
                    percent: true,
                    ..Says::default()
                },
                place: Place::Above,
                ..Look::default()
            },
            label: "HP".into(),
            value: 0.5,
            place: Place::Above,
            ..Bar::default()
        }
    }

    /// The caption's measured height in the current `Ui` — the same quantity
    /// `Bar::fitted` takes off the bar (`galley.size().y`).
    fn caption_height(ui: &mut egui::Ui, bar: &Bar) -> f32 {
        let font_id = egui::TextStyle::Body.resolve(ui.style());
        let galley = ui.painter().layout(
            bar.words(),
            font_id,
            bar.look.text_color,
            f32::INFINITY,
        );
        galley.size().y
    }

    #[test]
    fn place_names_round_trip() {
        for place in Place::EVERY {
            assert_eq!(Place::of_name(place.name()), Some(place));
        }
        assert_eq!(Place::of_name("nope"), None);
        assert_eq!(Place::default(), Place::Hidden);
    }

    #[test]
    fn place_axis_masks_match_the_recovered_immediates() {
        // 0x26 == bits 1|2|5, 0x18 == bits 3|4.
        let across: u32 = Place::EVERY
            .into_iter()
            .filter(|p| p.across())
            .map(|p| 1 << p as u32)
            .sum();
        let down: u32 = Place::EVERY
            .into_iter()
            .filter(|p| p.down())
            .map(|p| 1 << p as u32)
            .sum();
        assert_eq!(across, 0x26);
        assert_eq!(down, 0x18);
    }

    #[test]
    fn fraction_clamps_at_one_hundred_percent() {
        let mut bar = Bar::default();
        bar.fills = 40;
        assert_eq!(bar.fraction(), 0.4);
        bar.fills = 100;
        assert_eq!(bar.fraction(), 1.0);
        bar.fills = 250;
        assert_eq!(bar.fraction(), 1.0);
    }

    #[test]
    fn said_parts_and_words() {
        let bar = caption_bar();
        assert_eq!(bar.parts(), ("HP".to_string(), "50%".to_string()));
        assert_eq!(bar.said(), "HP 50%");
        assert_eq!(bar.words(), "HP 50%");

        let mut hunting = bar.clone();
        hunting.hunt = Some(Hunt {
            place: "the hills".into(),
            number: 7,
        });
        assert_eq!(
            hunting.words(),
            "HP 50% the hills : hunt #7 on file spans the same"
        );
    }

    #[test]
    fn said_drops_empty_halves() {
        let mut bar = Bar::default();
        bar.look.says.label = true;
        bar.label = "MP".into();
        assert_eq!(bar.said(), "MP");
        bar.label.clear();
        assert_eq!(bar.said(), "");
    }

    #[test]
    fn fitted_takes_the_caption_out_of_the_height_for_above() {
        let mut bar = caption_bar();
        let (_, out) = headless::frame(&egui::Context::default(), Vec2::new(300.0, 40.0), |ui| {
            let eats = caption_height(ui, &bar) + Bar::PAD;
            assert!(eats > Bar::PAD, "the real font gives a positive height");
            let fitted = bar.clone().fitted(ui);
            assert!(fitted.measured);
            assert_eq!(fitted.size.x, 300.0);
            assert_eq!(fitted.size.y, 40.0 - eats);
            bar = fitted;
        });
        assert!(bar.measured);
        assert!(bar.size.y < 40.0);
        assert!(out.shapes.is_empty(), "fitted paints nothing");
    }

    #[test]
    fn fitted_takes_the_caption_out_of_the_width_for_left() {
        let mut bar = caption_bar();
        bar.look.place = Place::Left;
        bar.place = Place::Left;
        headless::frame(&egui::Context::default(), Vec2::new(300.0, 40.0), |ui| {
            let eats = caption_height(ui, &bar) + Bar::PAD;
            let fitted = bar.clone().fitted(ui);
            assert_eq!(fitted.size.x, 300.0 - eats);
            assert_eq!(fitted.size.y, 40.0);
        });
    }

    #[test]
    fn fitted_never_goes_below_the_floor() {
        let mut bar = caption_bar();
        bar.place = Place::Left;
        headless::frame(&egui::Context::default(), Vec2::new(12.0, 100.0), |ui| {
            let fitted = bar.clone().fitted(ui);
            // 12 - (caption + 4) is negative, so the width is the 8.0 floor.
            assert_eq!(fitted.size.x, Bar::FLOOR);
            assert_eq!(fitted.size.y, 100.0);
        });
    }

    #[test]
    fn fitted_leaves_hidden_and_inside_alone() {
        for place in [Place::Hidden, Place::Inside] {
            let mut bar = caption_bar();
            bar.place = place;
            bar.look.place = place;
            headless::frame(&egui::Context::default(), Vec2::new(300.0, 40.0), |ui| {
                let fitted = bar.clone().fitted(ui);
                assert_eq!(fitted.size.x, 300.0, "{place:?}");
                assert_eq!(fitted.size.y, 40.0, "{place:?}");
                assert!(fitted.measured);
            });
        }
    }

    #[test]
    fn fitted_measures_only_the_label_half_for_corner() {
        let mut bar = caption_bar();
        bar.place = Place::Corner;
        bar.look.place = Place::Corner;
        headless::frame(&egui::Context::default(), Vec2::new(300.0, 40.0), |ui| {
            // `Corner` measures `parts().0` (the label half), one line of text.
            let font_id = egui::TextStyle::Body.resolve(ui.style());
            let galley = ui.painter().layout(
                "HP".to_string(),
                font_id,
                bar.look.text_color,
                f32::INFINITY,
            );
            let fitted = bar.clone().fitted(ui);
            assert_eq!(fitted.size.y, 40.0 - (galley.size().y + Bar::PAD));
            assert_eq!(fitted.size.x, 300.0);
        });
    }

    #[test]
    fn behind_hits_the_filled_end() {
        let mut bar = Bar::default();
        bar.fills = 50;
        bar.place = Place::Above;
        let mut area = rect(Vec2::ZERO, Vec2::new(100.0, 20.0));
        // Above fills from the far edge inwards: y in [10, 20].
        assert!(bar.behind(&mut area, Vec2::new(50.0, 15.0)));
        assert!(!bar.behind(&mut area, Vec2::new(50.0, 5.0)));
    }

    #[test]
    fn behind_grows_a_corner_fill_area_by_half_the_thickness() {
        let mut bar = Bar::default();
        bar.fills = 100;
        bar.look.fills = Place::Corner;
        let mut area = rect(Vec2::ZERO, Vec2::new(20.0, 40.0));
        bar.behind(&mut area, Vec2::ZERO);
        // min(width, height) / 2 = 10 on every side.
        assert_eq!(area.min, egui::Pos2::new(-10.0, -10.0));
        assert_eq!(area.max, egui::Pos2::new(30.0, 50.0));
    }

    #[test]
    fn fill_rect_matches_the_hit_test() {
        // The painted fill and `behind` must agree, or the hit test lies.
        let area = rect(Vec2::new(10.0, 20.0), Vec2::new(100.0, 40.0));
        for place in Place::EVERY {
            let mut bar = Bar {
                fills: 25,
                place,
                ..Bar::default()
            };
            // The `Corner` *fill* place grows the hit area (tested separately);
            // this test is about the painted band versus the plain hit test.
            bar.look.fills = Place::Hidden;
            let filled = bar.fill_rect(area);
            let inside = filled.center();
            let outside = match place {
                Place::Above => egui::Pos2::new(inside.x, area.min.y + 1.0),
                Place::Below => egui::Pos2::new(inside.x, area.max.y - 1.0),
                Place::Right => egui::Pos2::new(area.max.x - 1.0, inside.y),
                _ => egui::Pos2::new(area.min.x + 1.0, inside.y),
            };
            assert!(
                bar.behind(&mut area.clone(), Vec2::new(inside.x, inside.y)),
                "{place:?} fills where it says"
            );
            assert!(
                !bar.behind(&mut area.clone(), Vec2::new(outside.x, outside.y)),
                "{place:?} does not fill the far end"
            );
        }
    }

    #[test]
    fn look_forces_an_opaque_alpha_and_keeps_the_live_fields() {
        let mut bar = Bar::default();
        bar.look.ring = true;
        bar.look.threshold = Some(0.5);
        let over = Look {
            color: Color32::from_rgba_premultiplied(1, 2, 3, 0x40),
            place: Place::Below,
            emblem: 9,
            ..Look::default()
        };
        let applied = bar.look(&over);
        assert_eq!(applied.color, Color32::from_rgb(1, 2, 3));
        assert_eq!(bar.look.color, Color32::from_rgb(1, 2, 3));
        assert_eq!(bar.place, Place::Below);
        // The emblem index is clamped with `min(_, 4)` @0x10070869c.
        assert_eq!(applied.emblem, Look::EMBLEM_MAX);
        // Fields the written half does not carry keep their live values.
        assert!(applied.ring);
        assert_eq!(applied.threshold, Some(0.5));
    }

    #[test]
    fn write_paints_the_background_the_fill_and_the_caption() {
        let mut bar = caption_bar();
        bar.fills = 40;
        let area = rect(Vec2::new(10.0, 20.0), Vec2::new(100.0, 10.0));
        let (_, out) = headless::one_frame(|ui| {
            bar.write(ui.painter(), area);
        });
        let fills = headless::rect_fills(&out);
        assert_eq!(fills.len(), 2, "background then fill");
        // The background covers the whole bar, in `look.background`.
        assert_eq!(fills[0].rect, area);
        assert_eq!(fills[0].fill, Color32::BLACK);
        // The fill is the 40 % band, in `look.color`, full width.
        assert_eq!(fills[1].fill, Color32::WHITE);
        assert_eq!(fills[1].rect.width(), 100.0);
        assert_eq!(fills[1].rect.height(), 4.0);
        // `Above` fills from the bottom edge up.
        assert_eq!(fills[1].rect.min.y, 26.0);
        assert_eq!(fills[1].rect.max.y, 30.0);
        // The caption is the recovered `words()`.
        assert_eq!(headless::texts(&out), vec!["HP 50%".to_string()]);
    }

    #[test]
    fn write_paints_the_track_outline_and_ring_when_asked() {
        let mut bar = caption_bar();
        bar.fills = 100;
        bar.look.track = Color32::from_rgb(1, 2, 3);
        bar.look.outline = true;
        bar.look.ring = true;
        let area = rect(Vec2::ZERO, Vec2::new(60.0, 20.0));
        let (_, out) = headless::one_frame(|ui| {
            bar.write(ui.painter(), area);
        });
        let fills = headless::rect_fills(&out);
        // track, background, fill.
        assert_eq!(fills.len(), 3);
        assert_eq!(fills[0].fill, Color32::from_rgb(1, 2, 3));
        // `outline` + `ring` are two stroked rects (a stroke keeps the fill).
        let strokes = headless::shapes(&out)
            .into_iter()
            .filter_map(|shape| match shape {
                egui::Shape::Rect(rect) if rect.stroke.width > 0.0 => Some(rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(strokes.len(), 2);
        assert_eq!(strokes[0].rect, area);
        assert_eq!(strokes[0].stroke.color, Color32::BLACK);
        assert_eq!(strokes[1].rect, area.shrink(1.0));
        assert_eq!(strokes[1].stroke.color, Color32::WHITE);
    }

    #[test]
    fn the_emblem_steps_across_the_bar_and_clamps_at_four() {
        let mut bar = caption_bar();
        bar.fills = 0;
        let area = rect(Vec2::ZERO, Vec2::new(200.0, 20.0));
        let mut centres = Vec::new();
        for slot in 0..=Look::EMBLEM_MAX + 1 {
            bar.look.emblem = slot;
            let (_, out) = headless::one_frame(|ui| {
                bar.write(ui.painter(), area);
            });
            let circles = headless::circles(&out);
            assert_eq!(circles.len(), 1, "one emblem disc");
            assert_eq!(circles[0].radius, 9.0);
            assert_eq!(circles[0].center.y, 10.0);
            centres.push(circles[0].center.x);
        }
        assert_eq!(centres[0], 10.0);
        assert_eq!(centres[1], 30.0);
        assert_eq!(centres[4], 90.0);
        // Slot 5 is the same as slot 4: the recovered `min(_, 4)` clamp.
        assert_eq!(centres[5], centres[4]);
    }

    #[test]
    fn the_widget_takes_the_fitted_space_and_paints_it() {
        let mut bar = caption_bar();
        bar.fills = 100;
        let (response, out) = headless::frame(
            &egui::Context::default(),
            Vec2::new(120.0, 30.0),
            |ui| ui.add(bar.clone()),
        );
        // `fitted` took the caption off the height, and the widget took it.
        assert!(response.rect.height() < 30.0);
        assert_eq!(response.rect.width(), 120.0);
        assert_eq!(response.rect.min, egui::Pos2::ZERO);
        let fills = headless::rect_fills(&out);
        assert_eq!(fills[0].rect, response.rect);
        assert_eq!(fills[1].rect.height(), response.rect.height());
    }

    #[test]
    fn look_json_round_trips() {
        let look = Look {
            fills: Place::Below,
            place: Place::Right,
            says: Says {
                label: true,
                words: true,
                ..Says::default()
            },
            color: Color32::from_rgb(10, 20, 30),
            overlay: true,
            background: Color32::from_rgb(1, 2, 3),
            track: Color32::from_rgb(4, 5, 6),
            fill_image: Some("built-in:orb_background".into()),
            ring: true,
            clock: false,
            threshold: Some(0.25),
            text_color: Color32::from_rgb(7, 8, 9),
            outline_color: Color32::from_rgb(11, 12, 13),
            outline: true,
            second_emblem: None,
            // UNKNOWN: the emblem index is not one of the recovered serialised
            // fields, so it does not survive the round trip either.
            emblem: 0,
        };
        let text = look.to_json().write();
        let back = Look::from_json(&json::parse(&text).unwrap()).unwrap();
        assert_eq!(back, look);
        assert!(text.starts_with(r#"{"fills":"below","place":"right","says":{"label":true"#));
    }
}
