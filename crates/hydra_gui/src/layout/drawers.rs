//! The four edge drawers, and the zones they carve out of the window.
//!
//! Recovered from `hydra_gui::layout::drawers` (unslid VAs):
//!
//! | symbol | addr | status |
//! |---|---|---|
//! | `Drawers::is_default` | `0x1007cf5a4` | **full** |
//! | `Serialize for Drawer` | `0x1007ced10` | **full** (names + offsets) |
//! | `Serialize for Drawers` | `0x1007cee28` | **full** (names + offsets) |
//! | `Serialize for Zone` | `0x1007ce860` | **full** (variant names) |
//! | `Deserialize for Mode` | `0x1007ce910` | fieldless enum |
//! | `Zones::of` | `0x1007cef40` | partly |
//! | `Zones::fit` | `0x1007cf34c` | partly |
//! | `Zones::keep` | `0x1007cf3f8` | partly |
//! | `Zones::inside` | `0x1007cf478` | partly |

use crate::json::{self, Value};
use crate::seam::Vec2;

/// A drawer's edge.
///
/// `Serialize for Zone` @`0x1007ce860` writes the unit variant name — the
/// literals `main` (`0x102157570`), `top` (`0x102157574`), `bottom`
/// (`0x102157577`), `left` (`0x10215757d`), `right` (`0x102157581`) — and the
/// discriminant is the byte at `+0` of a zone record (`ldrb w9, [x0]`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Zone {
    #[default]
    Main = 0,
    Top = 1,
    Bottom = 2,
    Left = 3,
    Right = 4,
}

impl Zone {
    pub const EVERY: [Zone; 5] = [Zone::Main, Zone::Top, Zone::Bottom, Zone::Left, Zone::Right];

    pub fn name(self) -> &'static str {
        match self {
            Zone::Main => "main",
            Zone::Top => "top",
            Zone::Bottom => "bottom",
            Zone::Left => "left",
            Zone::Right => "right",
        }
    }

    pub fn of_name(name: &str) -> Option<Zone> {
        Zone::EVERY.into_iter().find(|z| z.name() == name)
    }

    /// The zone a drawer edge belongs to.
    pub fn of_index(index: usize) -> Zone {
        match index {
            0 => Zone::Top,
            1 => Zone::Bottom,
            2 => Zone::Left,
            3 => Zone::Right,
            _ => Zone::Main,
        }
    }
}

/// How an open drawer takes room.
///
/// The three names come from the drawer options captions next to
/// `play::options::window::drawers_set` @`0x10081d644`: "Drawn, with its
/// windows.  Drag a window into an open drawer to keep it there." followed by
/// "Pushing: it takes its room from the main area, whose windows move over."
/// and "Clipping: it lies over the main area, …".  The serde ids `pushing` and
/// `clipping` are the literals at `0x10217d7df` and `0x10217e6c0`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Drawn = 0,
    Pushing = 1,
    Clipping = 2,
}

impl Mode {
    pub const EVERY: [Mode; 3] = [Mode::Drawn, Mode::Pushing, Mode::Clipping];

    pub fn name(self) -> &'static str {
        match self {
            Mode::Drawn => "drawn",
            Mode::Pushing => "pushing",
            Mode::Clipping => "clipping",
        }
    }

    pub fn of_name(name: &str) -> Option<Mode> {
        Mode::EVERY.into_iter().find(|m| m.name() == name)
    }
}

/// One drawer.
///
/// `Serialize for Drawer` @`0x1007ced10` emits, in this order (serde declaration
/// order), `open` (value at `+0x8`), `size` (`+0x0`), `mode` (`+0x9`) and
/// `opacity` (`+0x4`) — so the layout is `size: f32` at `0`, `opacity: f32` at
/// `4`, `open: bool` at `8`, `mode: Mode` at `9`, 12 bytes total.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawer {
    pub open: bool,
    pub size: f32,
    pub mode: Mode,
    pub opacity: f32,
}

impl Drawer {
    /// The smallest size the recovered code will honour:
    /// `Zones::of` @`0x1007cef98` clamps with `fmaxnm s7, s3, s2` where
    /// `mov w8, #0x42200000` = `40.0`.
    pub const FLOOR: f32 = 40.0;

    pub fn to_json(self) -> Value {
        json::obj([
            ("open", Value::Bool(self.open)),
            ("size", Value::Num(self.size as f64)),
            ("mode", Value::Str(self.mode.name().into())),
            ("opacity", Value::Num(self.opacity as f64)),
        ])
    }

    pub fn from_json(v: &Value) -> Drawer {
        let mut drawer = Drawer::default();
        if let Some(open) = v.get("open").and_then(Value::as_bool) {
            drawer.open = open;
        }
        if let Some(size) = v.get("size").and_then(Value::as_f32) {
            drawer.size = size;
        }
        if let Some(mode) = v.get("mode").and_then(Value::as_str) {
            drawer.mode = Mode::of_name(mode).unwrap_or_default();
        }
        if let Some(opacity) = v.get("opacity").and_then(Value::as_f32) {
            drawer.opacity = opacity;
        }
        drawer
    }

    /// The size this drawer actually takes, or `0.0` when it is shut.
    ///
    /// `Zones::of` @`0x1007cef68`: the `open` byte must be `1`
    /// (`cmp w8, #0x1; b.ne`), the size must be finite (`and w8, w8,
    /// #0x7fffffff; cmp w8, #0x7f7fffff; b.gt`), and the result is clamped up to
    /// [`Drawer::FLOOR`].
    pub fn extent(&self) -> f32 {
        if !self.open || !self.size.is_finite() {
            return 0.0;
        }
        self.size.max(Drawer::FLOOR)
    }
}

impl Default for Drawer {
    /// The top and bottom drawers are 160 px, the left and right 240 px, all
    /// shut and fully opaque.
    ///
    /// Recovered from `Drawers::is_default` @`0x1007cf5a4`, which compares the
    /// eight floats of a `Drawers` against the static table at `0x102156cc0`:
    /// `q1 = [160.0, 1.0, 160.0, 1.0]`, `q2 = [240.0, 1.0, 240.0, 1.0]`.
    fn default() -> Self {
        Drawer {
            open: false,
            size: 160.0,
            mode: Mode::Drawn,
            opacity: 1.0,
        }
    }
}

/// The four drawers.
///
/// `Serialize for Drawers` @`0x1007cee28` emits `top` (value at `+0x00`),
/// `bottom` (`+0x0c`), `left` (`+0x18`) and `right` (`+0x24`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawers {
    pub top: Drawer,
    pub bottom: Drawer,
    pub left: Drawer,
    pub right: Drawer,
}

impl Default for Drawers {
    fn default() -> Self {
        Drawers {
            top: Drawer::default(),
            bottom: Drawer::default(),
            left: Drawer {
                size: 240.0,
                ..Drawer::default()
            },
            right: Drawer {
                size: 240.0,
                ..Drawer::default()
            },
        }
    }
}

impl Drawers {
    pub fn every(&self) -> [Drawer; 4] {
        [self.top, self.bottom, self.left, self.right]
    }

    pub fn edges(&self) -> [(&'static str, Drawer); 4] {
        [
            ("top", self.top),
            ("bottom", self.bottom),
            ("left", self.left),
            ("right", self.right),
        ]
    }

    /// True when nothing has been changed from [`Drawers::default`].
    ///
    /// `Drawers::is_default` @`0x1007cf5a4`:
    ///
    /// * the `mode` bytes at `0x9`, `0x15`, `0x21` and `0x2d` and the `open`
    ///   bytes at `0x8`, `0x14`, `0x20` and `0x2c` are OR-ed together and must
    ///   all be zero (`orr`/`tst w8, #0xff` @`0x1007cf618`);
    /// * the eight floats are compared with the static default vectors
    ///   (`fcmeq.4s` against `0x102156cc0`/`0x102156cd0`), masked with the
    ///   lane-bit table at `0x102156ce0` (`01 00 02 00 … 80 00`) and reduced
    ///   with `addv.8h`;
    /// * the tail byte at `0x2d` (the right drawer's mode) is checked last
    ///   (`ldrb w8, [x0, #0x2d]; cmp w8, #0x0; cset w0, eq`).
    pub fn is_default(&self) -> bool {
        *self == Drawers::default()
    }

    pub fn to_json(self) -> Value {
        json::obj([
            ("top", self.top.to_json()),
            ("bottom", self.bottom.to_json()),
            ("left", self.left.to_json()),
            ("right", self.right.to_json()),
        ])
    }

    pub fn from_json(v: &Value) -> Drawers {
        let mut drawers = Drawers::default();
        if let Some(top) = v.get("top") {
            drawers.top = Drawer::from_json(top);
        }
        if let Some(bottom) = v.get("bottom") {
            drawers.bottom = Drawer::from_json(bottom);
        }
        if let Some(left) = v.get("left") {
            drawers.left = Drawer::from_json(left);
        }
        if let Some(right) = v.get("right") {
            drawers.right = Drawer::from_json(right);
        }
        drawers
    }
}

/// One carved-out zone: which edge it is, and its extent along that edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub zone: Zone,
    /// The offset of the zone's inner edge (`(f32, f32)` at `+0x4` of the
    /// recovered 20-byte record).
    pub at: Vec2,
    /// The size it takes (`(f32, f32)` at `+0xc`).
    pub size: Vec2,
}

/// The zones the drawers leave for the main area.
///
/// Layout recovered from `Zones::fit`/`keep`/`inside` (`0x1007cf34c`,
/// `0x1007cf3f8`, `0x1007cf478`): a `Vec<Placed>` at `+0x00` of 20-byte records
/// (`ldrb w11, [x9, #0x10]; add x9, x9, #0x14` in `fit`), a fallback pair at
/// `+0x18` and a per-axis scale pair at `+0x28`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Zones {
    pub placed: Vec<Placed>,
    pub fallback: Vec2,
    pub scale: Vec2,
    /// The space left for the main area.
    pub main: Vec2,
}

impl Zones {
    /// Room the drawers must leave free before they may grow: the recovered code
    /// subtracts 60 from the height (`mov w9, #-0x3d900000` = `-60.0`
    /// @`0x1007cefdc`) and 144 from the width (`mov w11, #-0x3d100000` =
    /// `-144.0` @`0x1007cf088`).
    pub const KEEP_HEIGHT: f32 = 60.0;
    pub const KEEP_WIDTH: f32 = 144.0;

    /// Build the zones from the drawers and the window size.
    ///
    /// Recovered steps of `Zones::of` @`0x1007cef40`:
    ///
    /// 1. each open drawer's extent is clamped up to [`Drawer::FLOOR`];
    /// 2. `room = max(height - 60, 0)`; if `top + bottom > room` and
    ///    `top + bottom > 0`, both are scaled by `room / (top + bottom)`;
    /// 3. the same for `left + right` against `max(width - 144, 0)`;
    /// 4. the zones are pushed in the order `Top`, `Bottom`, `Left`, `Right`,
    ///    then the main area is what is left;
    /// 5. the per-axis scale is `inner / outer` with `1.0` wherever the divisor
    ///    is not positive (`fcmgt.4s v0, v2, #0.0; bsl` @`0x1007cf2d8`).
    pub fn of(drawers: &Drawers, size: Vec2) -> Zones {
        let (mut top, mut bottom) = (drawers.top.extent(), drawers.bottom.extent());
        let (mut left, mut right) = (drawers.left.extent(), drawers.right.extent());

        let room_y = (size.y - Zones::KEEP_HEIGHT).max(0.0);
        let take_y = top + bottom;
        if take_y > room_y && take_y > 0.0 {
            let scale = room_y / take_y;
            top *= scale;
            bottom *= scale;
        }
        let room_x = (size.x - Zones::KEEP_WIDTH).max(0.0);
        let take_x = left + right;
        if take_x > room_x && take_x > 0.0 {
            let scale = room_x / take_x;
            left *= scale;
            right *= scale;
        }

        let main = Vec2::new((size.x - left - right).max(0.0), (size.y - top - bottom).max(0.0));
        let placed = vec![
            Placed {
                zone: Zone::Top,
                at: Vec2::new(0.0, top),
                size: Vec2::new(size.x, top),
            },
            Placed {
                zone: Zone::Bottom,
                at: Vec2::new(0.0, size.y - bottom),
                size: Vec2::new(size.x, bottom),
            },
            Placed {
                zone: Zone::Left,
                at: Vec2::new(left, 0.0),
                size: Vec2::new(left, size.y),
            },
            Placed {
                zone: Zone::Right,
                at: Vec2::new(size.x - right, 0.0),
                size: Vec2::new(right, size.y),
            },
            Placed {
                zone: Zone::Main,
                at: Vec2::new(left, top),
                size: main,
            },
        ];
        let scale = Vec2::new(
            if size.x > 0.0 { main.x / size.x } else { 1.0 },
            if size.y > 0.0 { main.y / size.y } else { 1.0 },
        );
        Zones {
            placed,
            fallback: Vec2::ZERO,
            scale,
            main,
        }
    }

    /// The placed zone with this tag, if any.
    ///
    /// `Zones::keep` @`0x1007cf3f8` walks the 20-byte records comparing the tag
    /// byte with the requested zone and returns the matching record's `at`/
    /// `size`; `Main` (tag 0) short-circuits to the stored fallback pair.
    pub fn keep(&self, zone: Zone) -> Option<Placed> {
        if zone == Zone::Main {
            return Some(Placed {
                zone,
                at: Vec2::ZERO,
                size: self.main,
            });
        }
        self.placed.iter().copied().find(|p| p.zone == zone)
    }

    /// Where the main area ends up, inside the window.
    pub fn main_rect(&self) -> crate::seam::Rect {
        crate::seam::Rect::from_min_size(
            Vec2::new(
                self.keep(Zone::Left).map(|p| p.size.x).unwrap_or(0.0),
                self.keep(Zone::Top).map(|p| p.size.y).unwrap_or(0.0),
            ),
            self.main,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_recovered_table() {
        let drawers = Drawers::default();
        assert_eq!(drawers.top.size, 160.0);
        assert_eq!(drawers.bottom.size, 160.0);
        assert_eq!(drawers.left.size, 240.0);
        assert_eq!(drawers.right.size, 240.0);
        for drawer in drawers.every() {
            assert!(!drawer.open);
            assert_eq!(drawer.mode, Mode::Drawn);
            assert_eq!(drawer.opacity, 1.0);
        }
        assert!(drawers.is_default());
    }

    #[test]
    fn is_default_notices_every_field() {
        let base = Drawers::default();

        let mut open = base;
        open.top.open = true;
        assert!(!open.is_default());

        let mut sized = base;
        sized.left.size = 200.0;
        assert!(!sized.is_default());

        let mut opacity = base;
        opacity.right.opacity = 0.5;
        assert!(!opacity.is_default());

        let mut mode = base;
        mode.bottom.mode = Mode::Clipping;
        assert!(!mode.is_default());
    }

    #[test]
    fn drawer_extent_needs_open_and_is_floored() {
        let mut drawer = Drawer::default();
        assert_eq!(drawer.extent(), 0.0);
        drawer.open = true;
        assert_eq!(drawer.extent(), 160.0);
        drawer.size = 10.0;
        assert_eq!(drawer.extent(), Drawer::FLOOR);
        drawer.size = f32::NAN;
        assert_eq!(drawer.extent(), 0.0);
        drawer.size = f32::INFINITY;
        assert_eq!(drawer.extent(), 0.0);
    }

    #[test]
    fn zones_keep_sixty_of_height_and_a_hundred_and_forty_four_of_width() {
        let mut drawers = Drawers::default();
        drawers.top.open = true;
        drawers.bottom.open = true;
        drawers.left.open = true;
        drawers.right.open = true;
        // 160 + 160 = 320 fits in 1000 - 60, 240 + 240 = 480 fits in 1600 - 144.
        let zones = Zones::of(&drawers, Vec2::new(1600.0, 1000.0));
        assert_eq!(zones.keep(Zone::Top).unwrap().size.y, 160.0);
        assert_eq!(zones.keep(Zone::Bottom).unwrap().size.y, 160.0);
        assert_eq!(zones.keep(Zone::Left).unwrap().size.x, 240.0);
        assert_eq!(zones.keep(Zone::Right).unwrap().size.x, 240.0);
        assert_eq!(zones.main, Vec2::new(1600.0 - 480.0, 1000.0 - 320.0));
    }

    #[test]
    fn zones_scale_the_drawers_down_proportionally() {
        let mut drawers = Drawers::default();
        drawers.top.open = true;
        drawers.bottom.open = true;
        // room = 200 - 60 = 140, take = 320 -> both scale by 140/320.
        let zones = Zones::of(&drawers, Vec2::new(800.0, 200.0));
        let take = zones.keep(Zone::Top).unwrap().size.y + zones.keep(Zone::Bottom).unwrap().size.y;
        assert!((take - 140.0).abs() < 1e-4, "{take}");
        assert_eq!(zones.main.y, 200.0 - 140.0);
    }

    #[test]
    fn zones_reports_the_main_rect_inside_the_drawers() {
        let mut drawers = Drawers::default();
        drawers.top.open = true;
        drawers.left.open = true;
        let zones = Zones::of(&drawers, Vec2::new(1000.0, 800.0));
        let rect = zones.main_rect();
        assert_eq!(rect.min, Vec2::new(240.0, 160.0));
        assert_eq!(rect.size(), Vec2::new(1000.0 - 240.0, 800.0 - 160.0));
    }

    #[test]
    fn shut_drawers_take_nothing() {
        let zones = Zones::of(&Drawers::default(), Vec2::new(1000.0, 800.0));
        assert_eq!(zones.main, Vec2::new(1000.0, 800.0));
        assert_eq!(zones.scale, Vec2::new(1.0, 1.0));
        for placed in &zones.placed[..4] {
            assert_eq!(placed.size.x == 0.0 || placed.size.y == 0.0, true);
        }
    }

    #[test]
    fn zone_and_mode_names_round_trip() {
        for zone in Zone::EVERY {
            assert_eq!(Zone::of_name(zone.name()), Some(zone));
        }
        for mode in Mode::EVERY {
            assert_eq!(Mode::of_name(mode.name()), Some(mode));
        }
        assert_eq!(Mode::default(), Mode::Drawn);
        assert_eq!(Mode::of_name("pushing"), Some(Mode::Pushing));
    }

    #[test]
    fn drawers_json_round_trips() {
        let mut drawers = Drawers::default();
        drawers.top.open = true;
        drawers.right.mode = Mode::Pushing;
        drawers.left.opacity = 0.25;
        let text = drawers.to_json().write();
        assert_eq!(
            text,
            r#"{"top":{"open":true,"size":160,"mode":"drawn","opacity":1},"bottom":{"open":false,"size":160,"mode":"drawn","opacity":1},"left":{"open":false,"size":240,"mode":"drawn","opacity":0.25},"right":{"open":false,"size":240,"mode":"pushing","opacity":1}}"#
        );
        let back = Drawers::from_json(&json::parse(&text).unwrap());
        assert_eq!(back, drawers);
        assert!(!back.is_default());
    }

    #[test]
    fn missing_keys_keep_the_defaults() {
        let drawers = Drawers::from_json(&json::parse(r#"{"top":{"open":true}}"#).unwrap());
        assert!(drawers.top.open);
        assert_eq!(drawers.top.size, 160.0);
        assert_eq!(drawers.left, Drawers::default().left);
    }
}
