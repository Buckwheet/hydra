//! Window frames: how the chrome eats into a window's rectangle.
//!
//! Recovered from four functions:
//!
//! * `frame::Measures::parse` @0x1019472b8 — `toml::from_str` of a 4-field
//!   struct; the field names come from the literal at 0x1039be7ee,
//!   `bar overhang corner run` (and serde's `struct Measures with 4 elements`).
//! * `frame::lay` @0x101946044 — the scaled frame metrics.
//! * `frame::folder` @0x101946fc0 — splits a path on `/`.
//! * `frame::list` @0x101946890 — groups paths by their folder.
//!
//! Phase 4 added the drawing layer: [`framed`] projects [`lay`] onto an
//! `egui::Rect`, [`painted`] draws the bar, the outline and the carved runs, and
//! [`titled`] puts a title in the bar.

use egui::{Align2, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};

use crate::paint;
use crate::theme::{Palette, Token};
use crate::toml::{self, Table, Value};

/// The frame metrics, in points at scale 1.
///
/// Field order in memory: `bar` at `+0x00`, `run` at `+0x04`, `overhang` at
/// `+0x08`, `corner` at `+0x0c` (from the `ldp s2, s1, [x0]` /
/// `ldr s9, [x0, #0x8]` / `ldr s11, [x0, #0xc]` in `frame::lay`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Measures {
    /// Height of the title bar.
    pub bar: f32,
    /// How far the frame overhangs the client area.
    pub overhang: f32,
    /// Corner radius.
    pub corner: f32,
    /// Length of a carved run along an edge.
    pub run: f32,
}

impl Measures {
    /// `@0x1019472b8` — read a `[frame]` table.
    ///
    /// The binary feeds the text to `toml::from_str`; the recovered error path
    /// rejects non-finite numbers, so `inf`/`nan` are refused here too.
    pub fn parse(src: &str) -> Result<Measures, String> {
        let doc = toml::parse(src).map_err(|e| e.to_string())?;
        // Accept both a bare table and one nested under `[frame]`.
        let table: &Table = match doc.get("frame") {
            Some(Value::Table(t)) => t,
            _ => &doc,
        };
        let mut m = Measures::default();
        for (field, slot) in [
            ("bar", &mut m.bar),
            ("overhang", &mut m.overhang),
            ("corner", &mut m.corner),
            ("run", &mut m.run),
        ] {
            match table.get(field) {
                Some(v) => {
                    let f = v
                        .as_f64()
                        .ok_or_else(|| format!("`{field}` is not a number"))?;
                    if !f.is_finite() {
                        return Err(format!("`{field}` is not a finite number"));
                    }
                    *slot = f as f32;
                }
                None => return Err(format!("missing `{field}`")),
            }
        }
        Ok(m)
    }

    /// Render back to a `[frame]` table body.
    pub fn to_toml(&self) -> String {
        let mut t = Table::new();
        t.insert("bar".into(), Value::Float(self.bar as f64));
        t.insert("overhang".into(), Value::Float(self.overhang as f64));
        t.insert("corner".into(), Value::Float(self.corner as f64));
        t.insert("run".into(), Value::Float(self.run as f64));
        toml::to_string(&t)
    }
}

/// A rectangle in points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The frame metrics scaled to a window.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frames {
    /// The scaled bar height.
    pub bar: f32,
    /// The scaled corner radius.
    pub corner: f32,
    /// The scaled run length.
    pub run: f32,
    /// The rectangle left for the client area.
    pub inner: Size,
}

/// `@0x101946044` — scale the measures and carve the client area out of `size`.
///
/// The recovered guards are: `overhang > 0`, `scale > 0`, `corner > 0` and
/// `size.w > 0`; otherwise the layout degenerates and the client area is the
/// whole rectangle.  The scale factor is `scale / size.w` and every measure is
/// multiplied by it, with the run length floored at `1.0`
/// (`fmaxnm s14, s1, #1.0`).
pub fn lay(measures: &Measures, scale: f32, size: Size) -> Frames {
    if measures.overhang <= 0.0 || scale <= 0.0 || measures.corner <= 0.0 || size.w <= 0.0 {
        return Frames {
            bar: 0.0,
            corner: 0.0,
            run: 0.0,
            inner: size,
        };
    }
    let k = scale / size.w;
    let bar = (measures.bar * k).max(0.0);
    let corner = (measures.corner * k).max(0.0);
    let run = (measures.run * k).max(1.0);
    let inset = measures.overhang * k;
    Frames {
        bar,
        corner,
        run,
        inner: Size {
            x: size.x + inset,
            y: size.y + bar + inset,
            w: (size.w - inset * 2.0).max(0.0),
            h: (size.h - bar - inset * 2.0).max(0.0),
        },
    }
}

/// `@0x101946fc0` — the folder part of a path (everything before the last
/// `/`), or `""` when there is no separator.
///
/// The binary builds a `char` searcher for `'/'` and walks the matches.
pub fn folder(path: &str) -> &str {
    match path.rfind('/') {
        Some(at) => &path[..at],
        None => "",
    }
}

// ---------------------------------------------------------------------------
// Phase 4: the same maths in `egui::Rect` terms, and the painting that goes
// with it.  `lay` stays the single source of truth — [`framed`] only converts
// its arguments and its result, and [`painted`] draws what it returns.
// ---------------------------------------------------------------------------

impl Size {
    /// The rectangle as an [`egui::Rect`].
    pub fn rect(&self) -> Rect {
        Rect::from_min_size(Pos2::new(self.x, self.y), Vec2::new(self.w, self.h))
    }

    /// A [`Size`] from an [`egui::Rect`] — the argument `lay` needs.
    pub fn of(rect: Rect) -> Size {
        Size {
            x: rect.min.x,
            y: rect.min.y,
            w: rect.width(),
            h: rect.height(),
        }
    }
}

impl Frames {
    /// The client area as an [`egui::Rect`].
    pub fn inner_rect(&self) -> Rect {
        self.inner.rect()
    }

    /// `true` when [`lay`] refused to carve a frame (its four guards failed).
    pub fn is_degenerate(&self) -> bool {
        self.corner <= 0.0 && self.bar <= 0.0 && self.run <= 0.0
    }
}

/// A frame laid over a rectangle: the outer box, the client box and the bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Framed {
    /// The recovered metrics (bar height, corner radius, run length, client
    /// box) — exactly what [`lay`] returned.
    pub frames: Frames,
    /// The whole window rectangle, as given.
    pub outer: Rect,
    /// The client area: [`Frames::inner_rect`].
    pub inner: Rect,
    /// The title bar, across the top of `outer`.  Empty when `bar == 0`.
    pub bar: Rect,
}

impl Default for Framed {
    fn default() -> Framed {
        Framed {
            frames: Frames::default(),
            outer: Rect::NOTHING,
            inner: Rect::NOTHING,
            bar: Rect::NOTHING,
        }
    }
}

/// [`lay`] in `egui::Rect` terms.
///
/// The degenerate case is not papered over: when `lay` returns its identity
/// (the four guards failed) `inner == outer` and `bar` is empty, and
/// [`painted`] then draws nothing.
pub fn framed(measures: &Measures, scale: f32, rect: Rect) -> Framed {
    let frames = lay(measures, scale, Size::of(rect));
    let inner = frames.inner_rect();
    let bar = Rect::from_min_size(rect.min, Vec2::new(rect.width(), frames.bar));
    Framed {
        frames,
        outer: rect,
        inner,
        bar,
    }
}

/// What [`painted`] drew.
#[derive(Clone, Debug, PartialEq)]
pub struct Painted {
    /// The title bar, when it has a height.
    pub bar: Option<Rect>,
    /// The outline of the outer box, when the frame is not degenerate.
    pub outline: Option<Rect>,
    /// The four carved runs, one per edge (top, bottom, left, right).
    pub runs: Vec<[Pos2; 2]>,
    /// The client area — where a caller puts its content.
    pub inner: Rect,
}

impl Default for Painted {
    fn default() -> Painted {
        Painted {
            bar: None,
            outline: None,
            runs: Vec::new(),
            inner: Rect::NOTHING,
        }
    }
}

/// Paint a frame: the bar, the outline and the four carved runs.
///
/// Colours are the recovered tokens, used exactly:
///
/// * the bar is [`Token::Raised`] (`#464646` in the built-in palette);
/// * the outline is a one-point stroke in [`Token::Line`] (`#3c3c3c`);
/// * the carved runs are [`Token::LineStrong`] (`#969696`).
///
/// The run geometry is `lay`'s: a run is `frames.run` long (never shorter than
/// the recovered `1.0` floor) and starts `frames.corner` in from each corner of
/// the client edge.  `// UNKNOWN:` which edge of the window the original carves
/// the runs from is not in the disassembly; the four client edges are used.
pub fn painted(painter: &Painter, framed: &Framed, palette: &Palette) -> Painted {
    let mut out = Painted {
        inner: framed.inner,
        ..Painted::default()
    };
    if framed.frames.is_degenerate() {
        return out;
    }
    let corner = framed.frames.corner;
    let run = framed.frames.run;

    if framed.frames.bar > 0.0 {
        painter.rect_filled(framed.bar, corner, paint::ink(palette, Token::Raised));
        out.bar = Some(framed.bar);
    }
    painter.rect_stroke(
        framed.outer,
        corner,
        Stroke::new(1.0, paint::ink(palette, Token::Line)),
        StrokeKind::Inside,
    );
    out.outline = Some(framed.outer);

    // The runs start `corner` in from each corner and are `run` long.
    let strong = Stroke::new(1.0, paint::ink(palette, Token::LineStrong));
    let left = framed.outer.left();
    let right = framed.outer.right();
    let top = framed.inner.top();
    let bottom = framed.outer.bottom();
    let segments = [
        [
            Pos2::new(left + corner, top),
            Pos2::new(left + corner + run, top),
        ],
        [
            Pos2::new(left + corner, bottom),
            Pos2::new(left + corner + run, bottom),
        ],
        [
            Pos2::new(left, top + corner),
            Pos2::new(left, top + corner + run),
        ],
        [
            Pos2::new(right, top + corner),
            Pos2::new(right, top + corner + run),
        ],
    ];
    for segment in segments {
        painter.line_segment(segment, strong);
        out.runs.push(segment);
    }
    out
}

/// Paint a title into the bar, centred or aligned as the theme's
/// `[shape] title_align` asks.
///
/// `TitleAlign` is one of `left`/`centre`/`right` (`theme::shape`); a frame with
/// no bar has nowhere to put the text, so nothing is drawn.
pub fn titled(
    painter: &Painter,
    framed: &Framed,
    palette: &Palette,
    font: FontId,
    title: &str,
    align: crate::theme::shape::TitleAlign,
) -> Option<Rect> {
    if framed.frames.bar <= 0.0 || title.is_empty() {
        return None;
    }
    if align == crate::theme::shape::TitleAlign::Centre {
        // `Align2::anchor_size` centres the galleys on the bar.
        return Some(painter.text(
            framed.bar.center(),
            Align2::CENTER_CENTER,
            title,
            font,
            paint::ink(palette, Token::Text),
        ));
    }
    let (pos, anchor) = if align == crate::theme::shape::TitleAlign::Right {
        (
            Pos2::new(framed.bar.right() - 2.0, framed.bar.center().y),
            Align2::RIGHT_CENTER,
        )
    } else {
        (
            Pos2::new(framed.bar.left() + 2.0, framed.bar.center().y),
            Align2::LEFT_CENTER,
        )
    };
    Some(painter.text(pos, anchor, title, font, paint::ink(palette, Token::Text)))
}

/// One folder of frame files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listed {
    /// The folder, as [`folder`] returns it.
    pub folder: String,
    /// The file names inside it, sorted.
    pub names: Vec<String>,
}

/// `@0x101946890` — group a list of paths by folder.
pub fn list(paths: &[String]) -> Vec<Listed> {
    let mut out: Vec<Listed> = Vec::new();
    for path in paths {
        let dir = folder(path).to_string();
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        match out.iter_mut().find(|l| l.folder == dir) {
            Some(entry) => entry.names.push(name),
            None => out.push(Listed {
                folder: dir,
                names: vec![name],
            }),
        }
    }
    for entry in &mut out {
        entry.names.sort();
        entry.names.dedup();
    }
    out.sort_by(|a, b| a.folder.cmp(&b.folder));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_four_measures() {
        let m = Measures::parse("bar = 18.0\noverhang = 2.0\ncorner = 6.0\nrun = 4.0\n").unwrap();
        assert_eq!(
            m,
            Measures {
                bar: 18.0,
                overhang: 2.0,
                corner: 6.0,
                run: 4.0
            }
        );
        // The `[frame]` wrapper works too.
        let m = Measures::parse("[frame]\nbar = 1.0\noverhang = 1.0\ncorner = 1.0\nrun = 1.0\n")
            .unwrap();
        assert_eq!(m.bar, 1.0);
    }

    #[test]
    fn parse_rejects_missing_and_non_finite() {
        assert!(Measures::parse("bar = 1.0\n").is_err());
        assert!(Measures::parse("bar = inf\noverhang = 1.0\ncorner = 1.0\nrun = 1.0\n").is_err());
        assert!(Measures::parse("bar = \"x\"\noverhang = 1.0\ncorner = 1.0\nrun = 1.0\n").is_err());
    }

    #[test]
    fn measures_round_trip() {
        let m = Measures {
            bar: 20.0,
            overhang: 3.0,
            corner: 8.0,
            run: 5.0,
        };
        assert_eq!(Measures::parse(&m.to_toml()).unwrap(), m);
    }

    #[test]
    fn lay_scales_and_carves() {
        let m = Measures {
            bar: 20.0,
            overhang: 2.0,
            corner: 4.0,
            run: 0.1,
        };
        let size = Size {
            x: 0.0,
            y: 0.0,
            w: 400.0,
            h: 300.0,
        };
        let f = lay(&m, 400.0, size);
        assert_eq!(f.bar, 20.0);
        assert_eq!(f.corner, 4.0);
        // The run is floored at 1.0.
        assert_eq!(f.run, 1.0);
        assert_eq!(f.inner.x, 2.0);
        assert_eq!(f.inner.y, 22.0);
        assert_eq!(f.inner.w, 396.0);
        assert_eq!(f.inner.h, 276.0);
    }

    #[test]
    fn lay_degenerates_without_measures() {
        let size = Size {
            x: 1.0,
            y: 2.0,
            w: 3.0,
            h: 4.0,
        };
        let f = lay(&Measures::default(), 1.0, size);
        assert_eq!(f.inner, size);
        assert_eq!(f.bar, 0.0);
        // A zero width does the same.
        let f = lay(
            &Measures {
                bar: 1.0,
                overhang: 1.0,
                corner: 1.0,
                run: 1.0,
            },
            1.0,
            Size::default(),
        );
        assert_eq!(f.inner, Size::default());
    }

    #[test]
    fn folder_of_paths() {
        assert_eq!(folder("a/b/c.toml"), "a/b");
        assert_eq!(folder("c.toml"), "");
        assert_eq!(folder("/c.toml"), "");
        assert_eq!(folder("a//c.toml"), "a/");
    }

    #[test]
    fn listing_groups_by_folder() {
        let paths = vec![
            "b/two.toml".to_string(),
            "a/one.toml".to_string(),
            "a/another.toml".to_string(),
            "top.toml".to_string(),
        ];
        let groups = list(&paths);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].folder, "");
        assert_eq!(groups[0].names, vec!["top.toml"]);
        assert_eq!(groups[1].folder, "a");
        assert_eq!(groups[1].names, vec!["another.toml", "one.toml"]);
        assert_eq!(groups[2].folder, "b");
    }
}

/// Phase 4 — the drawing tests.  Every one runs one headless `egui` frame with a
/// fixed `RawInput` (a window never opens) and asserts on the shapes that came
/// out, or on the rectangles the painting reports.
#[cfg(test)]
mod drawing {
    use super::*;
    use crate::theme::Palette;
    use egui::{Color32, Shape};

    /// Run one frame with an 800×600 screen.
    fn frame(mut ui: impl FnMut(&mut egui::Ui)) -> egui::FullOutput {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)));
        ctx.run_ui(input, ui)
    }

    fn rects(out: &egui::FullOutput) -> Vec<egui::epaint::RectShape> {
        out.shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Rect(r) => Some(r.clone()),
                _ => None,
            })
            .collect()
    }

    fn segments(out: &egui::FullOutput) -> Vec<([Pos2; 2], egui::Stroke)> {
        out.shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::LineSegment { points, stroke } => Some((*points, *stroke)),
                _ => None,
            })
            .collect()
    }

    fn texts(out: &egui::FullOutput) -> Vec<(String, Color32)> {
        out.shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Text(t) => {
                    let painted = t
                        .galley
                        .job
                        .sections
                        .first()
                        .map(|s| s.format.color)
                        .unwrap_or(Color32::PLACEHOLDER);
                    let colour = if painted == Color32::PLACEHOLDER {
                        t.fallback_color
                    } else {
                        painted
                    };
                    Some((t.galley.text().to_string(), colour))
                }
                _ => None,
            })
            .collect()
    }

    const OUTER: Rect = Rect {
        min: Pos2 { x: 10.0, y: 20.0 },
        max: Pos2 { x: 410.0, y: 320.0 },
    };

    fn measures() -> Measures {
        Measures {
            bar: 20.0,
            overhang: 2.0,
            corner: 4.0,
            run: 8.0,
        }
    }

    #[test]
    fn framed_is_lay_in_rect_terms() {
        let f = framed(&measures(), 400.0, OUTER);
        let laid = lay(
            &measures(),
            400.0,
            Size {
                x: 10.0,
                y: 20.0,
                w: 400.0,
                h: 300.0,
            },
        );
        assert_eq!(f.frames, laid);
        assert_eq!(f.outer, OUTER);
        // k = 400/400 = 1, so every measure is itself.
        assert_eq!(f.frames.bar, 20.0);
        assert_eq!(f.frames.corner, 4.0);
        assert_eq!(f.frames.run, 8.0);
        // The client box: 2 in from the sides, bar + 2 from the top.
        assert_eq!(
            f.inner,
            Rect::from_min_size(Pos2::new(12.0, 42.0), Vec2::new(396.0, 276.0))
        );
        assert_eq!(
            f.bar,
            Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(400.0, 20.0))
        );
        assert!(!f.frames.is_degenerate());
        // `Size` round-trips through `Rect`.
        assert_eq!(Size::of(OUTER).rect(), OUTER);
    }

    #[test]
    fn framed_keeps_the_run_floor() {
        let mut m = measures();
        m.run = 0.1;
        let f = framed(&m, 400.0, OUTER);
        assert_eq!(f.frames.run, 1.0);
        // A zero width degenerates: the client area is the whole rectangle.
        let f = framed(&measures(), 400.0, Rect::from_min_size(Pos2::ZERO, Vec2::ZERO));
        assert!(f.frames.is_degenerate());
        assert_eq!(f.inner, Rect::from_min_size(Pos2::ZERO, Vec2::ZERO));
    }

    #[test]
    fn the_frame_paints_bar_outline_and_four_runs() {
        let palette = Palette::bare();
        let mut seen = Painted::default();
        let out = frame(|ui| {
            let f = framed(&measures(), 400.0, OUTER);
            seen = painted(ui.painter(), &f, &palette);
        });

        // The reported geometry is `lay`'s.
        assert_eq!(seen.inner, Rect::from_min_size(Pos2::new(12.0, 42.0), Vec2::new(396.0, 276.0)));
        assert_eq!(seen.bar, Some(Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(400.0, 20.0))));
        assert_eq!(seen.outline, Some(OUTER));

        // Two rectangles: the bar filled with `raised`, the outline in `line`.
        let rects = rects(&out);
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].rect, seen.bar.unwrap());
        assert_eq!(rects[0].fill, Color32::from_rgb(0x46, 0x46, 0x46));
        assert_eq!(rects[0].stroke.width, 0.0);
        assert_eq!(rects[1].rect, OUTER);
        assert_eq!(rects[1].fill, Color32::TRANSPARENT);
        assert_eq!(rects[1].stroke.color, Color32::from_rgb(0x3c, 0x3c, 0x3c));

        // Four carved runs, `line_strong`, `run` long, `corner` in from the corner.
        let runs = segments(&out);
        assert_eq!(runs.len(), 4);
        for (_, stroke) in &runs {
            assert_eq!(stroke.color, Color32::from_rgb(0x96, 0x96, 0x96));
            assert_eq!(stroke.width, 1.0);
        }
        // top: from (left+corner, inner.top) to (left+corner+run, inner.top)
        assert_eq!(
            runs[0].0,
            [Pos2::new(14.0, 42.0), Pos2::new(22.0, 42.0)]
        );
        // bottom: along the outer box's bottom edge.
        assert_eq!(
            runs[1].0,
            [Pos2::new(14.0, 320.0), Pos2::new(22.0, 320.0)]
        );
        // left and right: down from the bar, `corner` in.
        assert_eq!(runs[2].0, [Pos2::new(10.0, 46.0), Pos2::new(10.0, 54.0)]);
        assert_eq!(runs[3].0, [Pos2::new(410.0, 46.0), Pos2::new(410.0, 54.0)]);
        assert_eq!(seen.runs.len(), 4);
    }

    #[test]
    fn a_degenerate_frame_paints_nothing_at_all() {
        let palette = Palette::bare();
        let mut seen = Painted::default();
        let out = frame(|ui| {
            // `Measures::default()` fails all four of `lay`'s guards.
            let f = framed(&Measures::default(), 1.0, OUTER);
            seen = painted(ui.painter(), &f, &palette);
        });
        assert!(seen.bar.is_none());
        assert!(seen.outline.is_none());
        assert!(seen.runs.is_empty());
        assert_eq!(seen.inner, OUTER);
        assert!(out.shapes.is_empty());
    }

    #[test]
    fn the_title_sits_in_the_bar_where_the_theme_asks() {
        let palette = Palette::bare();
        let mut left = None;
        let out = frame(|ui| {
            let f = framed(&measures(), 400.0, OUTER);
            left = titled(
                ui.painter(),
                &f,
                &palette,
                egui::FontId::proportional(12.0),
                "Hydra",
                crate::theme::shape::TitleAlign::Left,
            );
        });
        let text = texts(&out);
        assert_eq!(text.len(), 1);
        assert_eq!(text[0].0, "Hydra");
        assert_eq!(text[0].1, Color32::from_rgb(0xb4, 0xb4, 0xb4));
        // Left-aligned: two points in from the bar's left edge, vertically centred.
        assert_eq!(left.unwrap().min.x, 12.0);
        assert_eq!(left.unwrap().left(), 12.0);

        // Centre and right alignments land elsewhere in the same bar.
        let mut centre = None;
        frame(|ui| {
            let f = framed(&measures(), 400.0, OUTER);
            centre = titled(
                ui.painter(),
                &f,
                &palette,
                egui::FontId::proportional(12.0),
                "Hydra",
                crate::theme::shape::TitleAlign::Centre,
            );
        });
        assert!(centre.unwrap().center().x > left.unwrap().center().x);
        // A frame with no bar has nowhere to put a title.
        let mut none = Some(Rect::NOTHING);
        frame(|ui| {
            let f = framed(&Measures::default(), 1.0, OUTER);
            none = titled(
                ui.painter(),
                &f,
                &palette,
                egui::FontId::proportional(12.0),
                "Hydra",
                crate::theme::shape::TitleAlign::Left,
            );
        });
        assert_eq!(none, None);
    }
}

