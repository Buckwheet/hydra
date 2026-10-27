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
