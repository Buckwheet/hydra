//! Fonts.
//!
//! Recovered field names (serde pool at 0x103993400) for the theme file's
//! `[type]` section:
//!
//! ```text
//! ui_font ui_size story_font story_size mono_font mono_size line_spacing
//! fallbacks
//! ```
//!
//! and `Face { font, size }` from the same pool.  `Face::under` @0x101951438
//! shows the in-memory layout: an `Option<String>` at `+0x00` (niche
//! `isize::MIN`), a `u32` weight at `+0x18` and an `f32` size at `+0x1c`.

use crate::layer::Over;
use crate::toml::Value;

/// One font setting.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Face {
    /// The family name; `None` inherits from the layer underneath.
    pub font: Option<String>,
    /// The weight.  `// UNKNOWN:` the binary only tests it for zero
    /// ("unset"), so a plain `u32` is the faithful model.
    pub weight: u32,
    /// The point size; `0.0` means "unset" (the binary branches on the word at
    /// `+0x18`, not on this).
    pub size: f32,
}

impl Face {
    /// `@0x101951438` — `self` laid over `other`.
    ///
    /// The recovered logic picks the font name from `other` when it has one and
    /// from `self` otherwise, and takes the weight and size from whichever face
    /// supplied the name (`csel x8, x20, x21, ne` on the `+0x18` word).
    pub fn under(&self, other: &Face) -> Face {
        let (font, weight, size) = if other.font.is_some() {
            (other.font.clone(), other.weight, other.size)
        } else if self.font.is_some() {
            (self.font.clone(), self.weight, self.size)
        } else {
            (None, 0, 0.0)
        };
        Face { font, weight, size }
    }

    /// `true` when this face sets nothing.
    pub fn is_empty(&self) -> bool {
        self.font.is_none() && self.weight == 0 && self.size == 0.0
    }
}

/// A resolved set of faces.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Type {
    pub ui: Face,
    pub story: Face,
    pub mono: Face,
    pub line_spacing: f32,
    pub fallbacks: Vec<String>,
}

impl Type {
    /// `@0x1019514fc` — every font family the theme needs, in order.
    ///
    /// The binary builds the list from the three faces and then extends it with
    /// `fallbacks`; a face with no name contributes nothing.
    pub fn fonts(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(3 + self.fallbacks.len());
        for face in [&self.ui, &self.story, &self.mono] {
            if let Some(name) = &face.font {
                if !out.contains(name) {
                    out.push(name.clone());
                }
            }
        }
        for name in &self.fallbacks {
            if !out.contains(name) {
                out.push(name.clone());
            }
        }
        out
    }
}

/// The theme file's `[type]` section.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TypeFile {
    pub ui_font: Option<String>,
    pub ui_size: Option<f32>,
    pub story_font: Option<String>,
    pub story_size: Option<f32>,
    pub mono_font: Option<String>,
    pub mono_size: Option<f32>,
    pub line_spacing: Option<f32>,
    pub fallbacks: Option<Vec<String>>,
}

impl TypeFile {
    /// `@0x1019515f0` — overlay `self` on top of `under`.
    pub fn over(&self, under: &TypeFile) -> TypeFile {
        TypeFile {
            ui_font: self.ui_font.clone().or_else(|| under.ui_font.clone()),
            ui_size: self.ui_size.or(under.ui_size),
            story_font: self.story_font.clone().or_else(|| under.story_font.clone()),
            story_size: self.story_size.or(under.story_size),
            mono_font: self.mono_font.clone().or_else(|| under.mono_font.clone()),
            mono_size: self.mono_size.or(under.mono_size),
            line_spacing: self.line_spacing.or(under.line_spacing),
            fallbacks: self.fallbacks.clone().or_else(|| under.fallbacks.clone()),
        }
    }

    /// `@0x101951e8c` — nothing set.
    pub fn is_empty(&self) -> bool {
        *self == TypeFile::default()
    }

    /// `@0x101951f84` — only the fields that differ from `base`.
    pub fn differing(&self, base: &TypeFile) -> TypeFile {
        let f = |a: Option<f32>, b: Option<f32>| if a == b { None } else { a };
        let s = |a: &Option<String>, b: &Option<String>| {
            if a == b {
                None
            } else {
                a.clone()
            }
        };
        TypeFile {
            ui_font: s(&self.ui_font, &base.ui_font),
            ui_size: f(self.ui_size, base.ui_size),
            story_font: s(&self.story_font, &base.story_font),
            story_size: f(self.story_size, base.story_size),
            mono_font: s(&self.mono_font, &base.mono_font),
            mono_size: f(self.mono_size, base.mono_size),
            line_spacing: f(self.line_spacing, base.line_spacing),
            fallbacks: if self.fallbacks == base.fallbacks {
                None
            } else {
                self.fallbacks.clone()
            },
        }
    }

    /// Resolve this layer over `under` into a [`Type`].
    pub fn resolve(&self, under: &TypeFile) -> Type {
        let merged = self.over(under);
        Type {
            ui: Face {
                font: merged.ui_font,
                weight: 0,
                size: merged.ui_size.unwrap_or(0.0),
            },
            story: Face {
                font: merged.story_font,
                weight: 0,
                size: merged.story_size.unwrap_or(0.0),
            },
            mono: Face {
                font: merged.mono_font,
                weight: 0,
                size: merged.mono_size.unwrap_or(0.0),
            },
            line_spacing: merged.line_spacing.unwrap_or(0.0),
            fallbacks: merged.fallbacks.unwrap_or_default(),
        }
    }
}

impl Over for TypeFile {
    fn over(&self, under: &Self) -> Self {
        TypeFile::over(self, under)
    }
    fn differing(&self, base: &Self) -> Self {
        TypeFile::differing(self, base)
    }
    fn is_empty(&self) -> bool {
        TypeFile::is_empty(self)
    }
    fn from_value(_v: &Value) -> Option<Self> {
        None
    }
    fn to_value(&self) -> Value {
        Value::Table(Default::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_under_prefers_the_other_when_it_has_a_name() {
        let a = Face {
            font: Some("Iosevka".into()),
            weight: 400,
            size: 14.0,
        };
        let b = Face {
            font: None,
            weight: 0,
            size: 0.0,
        };
        // `a.under(b)`: `b` has no name, so `a` supplies everything.
        assert_eq!(a.under(&b), a);
        // `b.under(a)`: `a` has a name, so `a` wins.
        assert_eq!(b.under(&a), a);
        // Two named faces: the one underneath wins.
        let c = Face {
            font: Some("Noto Sans".into()),
            weight: 700,
            size: 18.0,
        };
        assert_eq!(a.under(&c), c);
        // No names at all.
        assert_eq!(b.under(&b), Face::default());
    }

    #[test]
    fn over_and_differing() {
        let base = TypeFile {
            ui_font: Some("Noto Sans".into()),
            ui_size: Some(14.0),
            story_font: Some("Noto Serif".into()),
            line_spacing: Some(1.2),
            ..TypeFile::default()
        };
        let top = TypeFile {
            ui_size: Some(16.0),
            ..TypeFile::default()
        };
        let merged = top.over(&base);
        assert_eq!(merged.ui_font.as_deref(), Some("Noto Sans"));
        assert_eq!(merged.ui_size, Some(16.0));
        assert_eq!(merged.story_font.as_deref(), Some("Noto Serif"));

        let diff = merged.differing(&base);
        assert_eq!(diff.ui_size, Some(16.0));
        assert_eq!(diff.ui_font, None);
        assert_eq!(diff.story_font, None);
        assert_eq!(diff.line_spacing, None);
    }

    #[test]
    fn empty_and_resolve() {
        assert!(TypeFile::default().is_empty());
        assert!(!TypeFile {
            ui_size: Some(0.0),
            ..TypeFile::default()
        }
        .is_empty());

        let layer = TypeFile {
            ui_font: Some("Inter".into()),
            ui_size: Some(15.0),
            mono_font: Some("Iosevka".into()),
            fallbacks: Some(vec!["Noto Sans".into(), "Inter".into()]),
            ..TypeFile::default()
        };
        let t = layer.resolve(&TypeFile::default());
        assert_eq!(t.ui.font.as_deref(), Some("Inter"));
        assert_eq!(t.ui.size, 15.0);
        assert_eq!(t.mono.font.as_deref(), Some("Iosevka"));
        assert_eq!(t.story.font, None);
        // `fonts` de-duplicates and keeps the face order.
        assert_eq!(t.fonts(), vec!["Inter", "Iosevka", "Noto Sans"]);
    }

    #[test]
    fn fonts_of_an_empty_type_is_empty() {
        assert!(Type::default().fonts().is_empty());
    }
}
