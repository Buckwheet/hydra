//! Window shape settings: corner style, density, buttons and tabs.
//!
//! Recovered from the theme file's `[shape]` section.  The field names come
//! from the serde field-visitor string pool at 0x103991abf:
//!
//! ```text
//! flat outlined filled  tight normal roomy  plain separated boxed
//! corners stroke window_margin density scrollbar shadow pane_gap
//! pane_padding bar_thickness title_height title_align buttons tabs
//! frame_thickness
//! ```
//!
//! and the type names sit next to them at 0x103992048: `Buttons Density
//! TabStyle ShapeFile TitleAlign CornersFile`.

use crate::layer::Over;
use crate::toml::Value;

/// Corner treatment: `flat` / `outlined` / `filled`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corners {
    Flat,
    Outlined,
    Filled,
}

/// Layout density: `tight` / `normal` / `roomy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Density {
    Tight,
    Normal,
    Roomy,
}

/// How adjacent buttons sit: `plain` / `separated` / `boxed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Buttons {
    Plain,
    Separated,
    Boxed,
}

/// Title alignment.
///
/// `// UNKNOWN:` no `TitleAlign` parse function survived in the symbol table;
/// the three names are the ones the theme writer emits for `title_align`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleAlign {
    Left,
    Centre,
    Right,
}

/// Where a pane's tab strip is drawn: `plain` / `separated` / `boxed` /
/// `filled`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabStyle {
    Plain,
    Separated,
    Boxed,
    Filled,
}

impl TabStyle {
    /// `@0x101843be4` — the style a name selects.
    ///
    /// The binary matches on the length (`9`, `6`, `5`) and then on the bytes,
    /// returning the discriminant directly and `4` (out of range, i.e. "no
    /// match") for anything else.  `Option` here keeps callers honest about
    /// that `4`.
    pub fn named(name: &str) -> Option<TabStyle> {
        Some(match name {
            "plain" => TabStyle::Plain,
            "separated" => TabStyle::Separated,
            "boxed" => TabStyle::Boxed,
            "filled" => TabStyle::Filled,
            _ => return None,
        })
    }

    /// The name [`TabStyle::named`] accepts.
    pub fn name(self) -> &'static str {
        match self {
            TabStyle::Plain => "plain",
            TabStyle::Separated => "separated",
            TabStyle::Boxed => "boxed",
            TabStyle::Filled => "filled",
        }
    }

    /// The discriminant the binary returns.
    pub fn index(self) -> u8 {
        match self {
            TabStyle::Plain => 0,
            TabStyle::Separated => 1,
            TabStyle::Boxed => 2,
            TabStyle::Filled => 3,
        }
    }
}

macro_rules! named_enum {
    ($t:ty, [$($name:literal => $variant:path),* $(,)?]) => {
        impl $t {
            /// The name the theme file uses.
            pub fn name(self) -> &'static str {
                match self { $($variant => $name),* }
            }
            /// Parse a name.
            pub fn named(name: &str) -> Option<Self> {
                Some(match name { $($name => $variant),*, _ => return None })
            }
        }
        impl $crate::layer::Over for $t {
            fn over(&self, _under: &Self) -> Self { *self }
            fn differing(&self, _base: &Self) -> Self { *self }
            fn is_empty(&self) -> bool { false }
            fn from_value(v: &$crate::toml::Value) -> Option<Self> { <$t>::named(v.as_str()?) }
            fn to_value(&self) -> $crate::toml::Value {
                $crate::toml::Value::Str(self.name().to_string())
            }
        }
    };
}

named_enum!(Corners, ["flat" => Corners::Flat, "outlined" => Corners::Outlined, "filled" => Corners::Filled]);
named_enum!(Density, ["tight" => Density::Tight, "normal" => Density::Normal, "roomy" => Density::Roomy]);
named_enum!(Buttons, ["plain" => Buttons::Plain, "separated" => Buttons::Separated, "boxed" => Buttons::Boxed]);
named_enum!(TitleAlign, ["left" => TitleAlign::Left, "centre" => TitleAlign::Centre, "right" => TitleAlign::Right]);

/// The `[shape.corners]` sub-table.
///
/// `// UNKNOWN:` `CornersFile` is one of the four theme types the binary names
/// but no field-name array for it survives; only its existence and the three
/// style words are attested, so this is a newtype over [`Corners`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CornersFile(pub Option<Corners>);

impl Over for CornersFile {
    fn over(&self, under: &Self) -> Self {
        CornersFile(self.0.or(under.0))
    }
    fn differing(&self, base: &Self) -> Self {
        CornersFile(if self.0 == base.0 { None } else { self.0 })
    }
    fn is_empty(&self) -> bool {
        self.0.is_none()
    }
    fn from_value(v: &Value) -> Option<Self> {
        Corners::from_value(v).map(|c| CornersFile(Some(c)))
    }
    fn to_value(&self) -> Value {
        match self.0 {
            Some(c) => c.to_value(),
            None => Value::Str(String::new()),
        }
    }
}

/// The theme file's `[shape]` section — fourteen optional fields.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShapeFile {
    pub corners: CornersFile,
    pub stroke: Option<f32>,
    pub window_margin: Option<f32>,
    pub density: Option<Density>,
    pub scrollbar: Option<f32>,
    pub shadow: Option<f32>,
    pub pane_gap: Option<f32>,
    pub pane_padding: Option<f32>,
    pub bar_thickness: Option<f32>,
    pub title_height: Option<f32>,
    pub title_align: Option<TitleAlign>,
    pub buttons: Option<Buttons>,
    pub tabs: Option<TabStyle>,
    pub frame_thickness: Option<f32>,
}

impl ShapeFile {
    /// `@0x101843ce8` — overlay `self` on top of `under`, field by field.
    pub fn over(&self, under: &ShapeFile) -> ShapeFile {
        ShapeFile {
            corners: self.corners.over(&under.corners),
            stroke: self.stroke.or(under.stroke),
            window_margin: self.window_margin.or(under.window_margin),
            density: self.density.or(under.density),
            scrollbar: self.scrollbar.or(under.scrollbar),
            shadow: self.shadow.or(under.shadow),
            pane_gap: self.pane_gap.or(under.pane_gap),
            pane_padding: self.pane_padding.or(under.pane_padding),
            bar_thickness: self.bar_thickness.or(under.bar_thickness),
            title_height: self.title_height.or(under.title_height),
            title_align: self.title_align.or(under.title_align),
            buttons: self.buttons.or(under.buttons),
            tabs: self.tabs.or(under.tabs),
            frame_thickness: self.frame_thickness.or(under.frame_thickness),
        }
    }

    /// `@0x101843ef4` — only the fields that differ from `base`.
    pub fn differing(&self, base: &ShapeFile) -> ShapeFile {
        let d = |a: Option<f32>, b: Option<f32>| if a == b { None } else { a };
        ShapeFile {
            corners: self.corners.differing(&base.corners),
            stroke: d(self.stroke, base.stroke),
            window_margin: d(self.window_margin, base.window_margin),
            density: if self.density == base.density { None } else { self.density },
            scrollbar: d(self.scrollbar, base.scrollbar),
            shadow: d(self.shadow, base.shadow),
            pane_gap: d(self.pane_gap, base.pane_gap),
            pane_padding: d(self.pane_padding, base.pane_padding),
            bar_thickness: d(self.bar_thickness, base.bar_thickness),
            title_height: d(self.title_height, base.title_height),
            title_align: if self.title_align == base.title_align {
                None
            } else {
                self.title_align
            },
            buttons: if self.buttons == base.buttons { None } else { self.buttons },
            tabs: if self.tabs == base.tabs { None } else { self.tabs },
            frame_thickness: d(self.frame_thickness, base.frame_thickness),
        }
    }

    /// `true` when nothing at all is set.
    pub fn is_empty(&self) -> bool {
        *self == ShapeFile::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_style_names_round_trip() {
        let cases = [
            ("plain", TabStyle::Plain),
            ("separated", TabStyle::Separated),
            ("boxed", TabStyle::Boxed),
            ("filled", TabStyle::Filled),
        ];
        for (i, (name, style)) in cases.into_iter().enumerate() {
            assert_eq!(TabStyle::named(name), Some(style));
            assert_eq!(style.name(), name);
            assert_eq!(style.index() as usize, i);
        }
        // The binary answers `4` (out of range) for anything else.
        assert_eq!(TabStyle::named(""), None);
        assert_eq!(TabStyle::named("Plain"), None);
        assert_eq!(TabStyle::named("boxed "), None);
    }

    #[test]
    fn the_three_style_triples() {
        assert_eq!(Corners::named("flat"), Some(Corners::Flat));
        assert_eq!(Corners::named("outlined"), Some(Corners::Outlined));
        assert_eq!(Corners::named("filled"), Some(Corners::Filled));
        assert_eq!(Density::named("tight"), Some(Density::Tight));
        assert_eq!(Density::named("normal"), Some(Density::Normal));
        assert_eq!(Density::named("roomy"), Some(Density::Roomy));
        assert_eq!(Buttons::named("plain"), Some(Buttons::Plain));
        assert_eq!(Buttons::named("separated"), Some(Buttons::Separated));
        assert_eq!(Buttons::named("boxed"), Some(Buttons::Boxed));
        assert_eq!(TitleAlign::named("left"), Some(TitleAlign::Left));
        assert_eq!(TitleAlign::named("centre"), Some(TitleAlign::Centre));
        assert_eq!(TitleAlign::named("right"), Some(TitleAlign::Right));
        assert_eq!(Corners::named("boxed"), None);
    }

    #[test]
    fn over_prefers_the_top_layer() {
        let base = ShapeFile {
            stroke: Some(1.0),
            density: Some(Density::Normal),
            ..ShapeFile::default()
        };
        let top = ShapeFile {
            stroke: Some(2.0),
            ..ShapeFile::default()
        };
        let merged = top.over(&base);
        assert_eq!(merged.stroke, Some(2.0));
        assert_eq!(merged.density, Some(Density::Normal));
        assert_eq!(merged.window_margin, None);
    }

    #[test]
    fn differing_keeps_only_the_changes() {
        let base = ShapeFile {
            stroke: Some(1.0),
            title_height: Some(18.0),
            tabs: Some(TabStyle::Boxed),
            ..ShapeFile::default()
        };
        let next = ShapeFile {
            stroke: Some(1.0),
            title_height: Some(20.0),
            tabs: Some(TabStyle::Boxed),
            ..ShapeFile::default()
        };
        let diff = next.differing(&base);
        assert_eq!(diff.stroke, None);
        assert_eq!(diff.title_height, Some(20.0));
        assert_eq!(diff.tabs, None);
        assert_eq!(diff.pane_gap, None);
    }

    #[test]
    fn empty_shape_file_is_the_default() {
        assert!(ShapeFile::default().is_empty());
        assert!(!ShapeFile {
            stroke: Some(0.0),
            ..ShapeFile::default()
        }
        .is_empty());
    }
}
