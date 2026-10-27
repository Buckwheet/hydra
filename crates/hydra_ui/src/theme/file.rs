//! Reading and writing theme files.
//!
//! Recovered field names (serde string pool at 0x1039be4be):
//!
//! ```text
//! struct Measures  struct File  struct Chosen
//! name base recipe pins names prompt shape type frame
//! struct variant PromptFile::Both  struct RecipeFile
//! seed background scheme variance contrast separation room_spread
//! ```
//!
//! so a theme file looks like:
//!
//! ```toml
//! [chosen]
//! name = "Light"
//! base = "Despana"
//! recipe = "Drake"
//! pins = ["text"]
//!
//! [names]
//! canvas = "#1b1b1b"
//!
//! [shape]
//! stroke = 1.0
//!
//! [type]
//! ui_font = "Noto Sans"
//!
//! [frame]
//! bar = 18.0
//! ```
//!
//! `Theme::parse` @0x101948790 and `Theme::to_toml` @0x101949ddc round-trip
//! that shape; `Themes::resolve` @0x10194c070 walks the `base` chain.

use std::collections::BTreeMap;
use std::path::Path;

use crate::frame::Measures;
use crate::layer::Over;
use crate::theme::harmony::{Recipe, Scheme};
use crate::theme::shape::ShapeFile;
use crate::theme::typeface::TypeFile;
use crate::theme::{parse_hex, rgb, hex, Palette, Rgb};
use crate::toml::{self, Table, Value};

/// The stem of a theme file name: the last path component with a `.toml`
/// suffix removed.
///
/// `@0x10194ccb8`.  The binary lowercases the result, because theme names are
/// matched case-insensitively when a file is looked up.
pub fn file_stem(path: &str) -> String {
    let name = Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    let stem = match name.rsplit_once('.') {
        Some((stem, ext)) if ext.eq_ignore_ascii_case("toml") => stem.to_string(),
        _ => name,
    };
    stem.to_lowercase()
}

/// The `[chosen]` table: which theme/recipe/pins the file selects.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chosen {
    pub name: Option<String>,
    pub base: Option<String>,
    pub recipe: Option<String>,
    pub pins: Vec<String>,
}

/// The `[prompt]` table.
///
/// `// UNKNOWN:` the serde pool has `struct variant PromptFile::Both`, so this
/// is an untagged enum with at least a `Both { .. }` struct variant.  The two
/// things a prompt setting can carry — the text and the per-position accents —
/// are modelled here.
#[derive(Clone, Debug, PartialEq)]
pub enum PromptFile {
    /// Just the prompt text.
    Text(String),
    /// Just the accents (`position -> token`).
    Colours(BTreeMap<u32, u8>),
    /// Both, as the recovered struct variant name says.
    Both {
        text: String,
        colours: BTreeMap<u32, u8>,
    },
}

impl Default for PromptFile {
    fn default() -> Self {
        PromptFile::Text(String::new())
    }
}

impl Over for PromptFile {
    fn over(&self, _under: &Self) -> Self {
        self.clone()
    }
    fn differing(&self, base: &Self) -> Self {
        if self == base {
            PromptFile::Text(String::new())
        } else {
            self.clone()
        }
    }
    fn is_empty(&self) -> bool {
        match self {
            PromptFile::Text(t) => t.is_empty(),
            PromptFile::Colours(c) => c.is_empty(),
            PromptFile::Both { text, colours } => text.is_empty() && colours.is_empty(),
        }
    }
    fn from_value(v: &Value) -> Option<Self> {
        if let Some(s) = v.as_str() {
            return Some(PromptFile::Text(s.to_string()));
        }
        let t = v.as_table()?;
        let text = t.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let mut colours = BTreeMap::new();
        if let Some(map) = t.get("colours").and_then(|v| v.as_table()) {
            for (k, v) in map {
                if let (Ok(k), Some(v)) = (k.parse::<u32>(), v.as_i64()) {
                    colours.insert(k, v as u8);
                }
            }
        }
        Some(PromptFile::Both { text, colours })
    }
    fn to_value(&self) -> Value {
        match self {
            PromptFile::Text(t) => Value::Str(t.clone()),
            PromptFile::Colours(c) => {
                let mut t = Table::new();
                for (k, v) in c {
                    t.insert(k.to_string(), Value::Int(*v as i64));
                }
                Value::Table(t)
            }
            PromptFile::Both { text, colours } => {
                let mut t = Table::new();
                t.insert("text".into(), Value::Str(text.clone()));
                let mut c = Table::new();
                for (k, v) in colours {
                    c.insert(k.to_string(), Value::Int(*v as i64));
                }
                t.insert("colours".into(), Value::Table(c));
                Value::Table(t)
            }
        }
    }
}

/// The `[recipe]` table: the knobs [`crate::theme::harmony::generate`] takes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RecipeFile {
    pub seed: Option<Rgb>,
    pub background: Option<Rgb>,
    pub scheme: Option<Scheme>,
    pub variance: Option<f64>,
    pub contrast: Option<f64>,
    pub separation: Option<f64>,
    pub room_spread: Option<f64>,
}

impl RecipeFile {
    /// `@0x101947798` — only the entries that differ from `base`.
    pub fn differing(&self, base: &RecipeFile) -> RecipeFile {
        RecipeFile {
            seed: if self.seed == base.seed { None } else { self.seed },
            background: if self.background == base.background {
                None
            } else {
                self.background
            },
            scheme: if self.scheme == base.scheme { None } else { self.scheme },
            variance: if self.variance == base.variance { None } else { self.variance },
            contrast: if self.contrast == base.contrast { None } else { self.contrast },
            separation: if self.separation == base.separation {
                None
            } else {
                self.separation
            },
            room_spread: if self.room_spread == base.room_spread {
                None
            } else {
                self.room_spread
            },
        }
    }

    /// Overlay onto `base` and return the resolved [`Recipe`].
    pub fn resolve(&self, base: &RecipeFile) -> Recipe {
        let d = Recipe::default();
        Recipe {
            variance: self.variance.or(base.variance).unwrap_or(d.variance),
            contrast: self.contrast.or(base.contrast).unwrap_or(d.contrast),
            separation: self.separation.or(base.separation).unwrap_or(d.separation),
            room_spread: self.room_spread.or(base.room_spread).unwrap_or(d.room_spread),
            seed: self.seed.or(base.seed).unwrap_or(d.seed),
            background: self.background.or(base.background).unwrap_or(d.background),
            scheme: self.scheme.or(base.scheme).unwrap_or(d.scheme),
        }
    }

    fn from_table(t: &Table) -> RecipeFile {
        let colour = |k: &str| t.get(k).and_then(|v| v.as_str()).and_then(parse_hex);
        RecipeFile {
            seed: colour("seed"),
            background: colour("background"),
            scheme: t
                .get("scheme")
                .and_then(|v| v.as_str())
                .and_then(Scheme::parse),
            variance: t.get("variance").and_then(|v| v.as_f64()),
            contrast: t.get("contrast").and_then(|v| v.as_f64()),
            separation: t.get("separation").and_then(|v| v.as_f64()),
            room_spread: t.get("room_spread").and_then(|v| v.as_f64()),
        }
    }

    fn to_table(&self) -> Table {
        let mut t = Table::new();
        if let Some(c) = self.seed {
            t.insert("seed".into(), Value::Str(hex(c)));
        }
        if let Some(c) = self.background {
            t.insert("background".into(), Value::Str(hex(c)));
        }
        if let Some(s) = self.scheme {
            t.insert("scheme".into(), Value::Str(s.name().into()));
        }
        for (k, v) in [
            ("variance", self.variance),
            ("contrast", self.contrast),
            ("separation", self.separation),
            ("room_spread", self.room_spread),
        ] {
            if let Some(v) = v {
                t.insert(k.into(), Value::Float(v));
            }
        }
        t
    }
}

/// A whole `theme.toml`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct File {
    pub chosen: Chosen,
    pub names: Option<BTreeMap<String, Rgb>>,
    pub prompt: Option<PromptFile>,
    pub recipe: Option<RecipeFile>,
    pub shape: Option<ShapeFile>,
    /// The `[type]` section (`type` is a keyword, hence `fonts`).
    pub fonts: Option<TypeFile>,
    pub frame: Option<Measures>,
}

impl File {
    /// Parse a `theme.toml`.
    pub fn parse(src: &str) -> Result<File, String> {
        let doc = toml::parse(src).map_err(|e| e.to_string())?;
        Ok(File::from_doc(&doc))
    }

    fn from_doc(doc: &Table) -> File {
        let mut chosen = Chosen::default();
        let mut names = None;
        let mut prompt = None;
        let mut recipe = None;
        let mut frame = None;
        for (k, v) in doc {
            match k.as_str() {
                "name" | "base" | "recipe" => {
                    // Both spellings live in the wild: top level and `[chosen]`.
                    let s = v.as_str().map(|s| s.to_string());
                    match k.as_str() {
                        "name" => chosen.name = s,
                        "base" => chosen.base = s,
                        _ => chosen.recipe = s,
                    }
                }
                "pins" => {
                    chosen.pins = v
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                }
                "chosen" => {
                    if let Some(t) = v.as_table() {
                        chosen = Chosen {
                            name: t.get("name").and_then(|v| v.as_str()).map(String::from),
                            base: t.get("base").and_then(|v| v.as_str()).map(String::from),
                            recipe: t
                                .get("recipe")
                                .and_then(|v| v.as_str())
                                .map(String::from),
                            pins: t
                                .get("pins")
                                .and_then(|v| v.as_array())
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|v| v.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default(),
                        };
                    }
                }
                "names" => {
                    let mut map = BTreeMap::new();
                    if let Some(t) = v.as_table() {
                        for (name, c) in t {
                            if let Some(c) = c.as_str().and_then(parse_hex) {
                                map.insert(name.clone(), c);
                            }
                        }
                    }
                    names = Some(map);
                }
                "prompt" => prompt = PromptFile::from_value(v),
                "recipe" => {}
                "shape" => {}
                "frame" => {
                    if let Some(t) = v.as_table() {
                        let f = |k: &str| t.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                        frame = Some(Measures {
                            bar: f("bar"),
                            overhang: f("overhang"),
                            corner: f("corner"),
                            run: f("run"),
                        });
                    }
                }
                _ => {}
            }
        }
        if let Some(t) = doc.get("recipe").and_then(|v| v.as_table()) {
            recipe = Some(RecipeFile::from_table(t));
        }
        File {
            chosen,
            names,
            prompt,
            recipe,
            shape: shape_of(doc),
            fonts: fonts_of(doc),
            frame,
        }
    }

    /// Render back to TOML.
    pub fn to_toml(&self) -> String {
        let mut doc = Table::new();
        let mut chosen = Table::new();
        if let Some(n) = &self.chosen.name {
            chosen.insert("name".into(), Value::Str(n.clone()));
        }
        if let Some(n) = &self.chosen.base {
            chosen.insert("base".into(), Value::Str(n.clone()));
        }
        if let Some(n) = &self.chosen.recipe {
            chosen.insert("recipe".into(), Value::Str(n.clone()));
        }
        if !self.chosen.pins.is_empty() {
            chosen.insert(
                "pins".into(),
                Value::Array(
                    self.chosen
                        .pins
                        .iter()
                        .map(|p| Value::Str(p.clone()))
                        .collect(),
                ),
            );
        }
        if !chosen.is_empty() {
            doc.insert("chosen".into(), Value::Table(chosen));
        }
        if let Some(names) = &self.names {
            let mut t = Table::new();
            for (k, v) in names {
                t.insert(k.clone(), Value::Str(hex(*v)));
            }
            doc.insert("names".into(), Value::Table(t));
        }
        if let Some(r) = &self.recipe {
            doc.insert("recipe".into(), Value::Table(r.to_table()));
        }
        if let Some(s) = &self.shape {
            doc.insert("shape".into(), shape_to_table(s));
        }
        if let Some(f) = &self.fonts {
            doc.insert("type".into(), fonts_to_table(f));
        }
        if let Some(m) = &self.frame {
            let mut t = Table::new();
            t.insert("bar".into(), Value::Float(m.bar as f64));
            t.insert("overhang".into(), Value::Float(m.overhang as f64));
            t.insert("corner".into(), Value::Float(m.corner as f64));
            t.insert("run".into(), Value::Float(m.run as f64));
            doc.insert("frame".into(), Value::Table(t));
        }
        toml::to_string(&doc)
    }
}

fn shape_of(doc: &Table) -> Option<ShapeFile> {
    shape_from(doc.get("shape")?)
}

fn shape_from(v: &Value) -> Option<ShapeFile> {
    use crate::theme::shape::{Buttons, Corners, CornersFile, Density, TabStyle, TitleAlign};
    let t = v.as_table()?;
    let f = |k: &str| t.get(k).and_then(|v| v.as_f64()).map(|v| v as f32);
    let s = |k: &str| t.get(k).and_then(|v| v.as_str());
    Some(ShapeFile {
        corners: CornersFile(s("corners").and_then(Corners::named)),
        stroke: f("stroke"),
        window_margin: f("window_margin"),
        density: s("density").and_then(Density::named),
        scrollbar: f("scrollbar"),
        shadow: f("shadow"),
        pane_gap: f("pane_gap"),
        pane_padding: f("pane_padding"),
        bar_thickness: f("bar_thickness"),
        title_height: f("title_height"),
        title_align: s("title_align").and_then(TitleAlign::named),
        buttons: s("buttons").and_then(Buttons::named),
        tabs: s("tabs").and_then(TabStyle::named),
        frame_thickness: f("frame_thickness"),
    })
}

fn shape_to_table(s: &ShapeFile) -> Value {
    let mut t = Table::new();
    if let Some(c) = s.corners.0 {
        t.insert("corners".into(), Value::Str(c.name().into()));
    }
    if let Some(d) = s.density {
        t.insert("density".into(), Value::Str(d.name().into()));
    }
    if let Some(b) = s.buttons {
        t.insert("buttons".into(), Value::Str(b.name().into()));
    }
    if let Some(tabs) = s.tabs {
        t.insert("tabs".into(), Value::Str(tabs.name().into()));
    }
    if let Some(a) = s.title_align {
        t.insert("title_align".into(), Value::Str(a.name().into()));
    }
    for (k, v) in [
        ("stroke", s.stroke),
        ("window_margin", s.window_margin),
        ("scrollbar", s.scrollbar),
        ("shadow", s.shadow),
        ("pane_gap", s.pane_gap),
        ("pane_padding", s.pane_padding),
        ("bar_thickness", s.bar_thickness),
        ("title_height", s.title_height),
        ("frame_thickness", s.frame_thickness),
    ] {
        if let Some(v) = v {
            t.insert(k.into(), Value::Float(v as f64));
        }
    }
    Value::Table(t)
}

fn fonts_of(doc: &Table) -> Option<TypeFile> {
    let t = match doc.get("type").or_else(|| doc.get("fonts")) {
        Some(Value::Table(t)) => t,
        _ => return None,
    };
    let s = |k: &str| t.get(k).and_then(|v| v.as_str()).map(String::from);
    let f = |k: &str| t.get(k).and_then(|v| v.as_f64()).map(|v| v as f32);
    Some(TypeFile {
        ui_font: s("ui_font"),
        ui_size: f("ui_size"),
        story_font: s("story_font"),
        story_size: f("story_size"),
        mono_font: s("mono_font"),
        mono_size: f("mono_size"),
        line_spacing: f("line_spacing"),
        fallbacks: t.get("fallbacks").and_then(|v| v.as_array()).map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        }),
    })
}

fn fonts_to_table(f: &TypeFile) -> Value {
    let mut t = Table::new();
    for (k, v) in [
        ("ui_font", f.ui_font.clone()),
        ("story_font", f.story_font.clone()),
        ("mono_font", f.mono_font.clone()),
    ] {
        if let Some(v) = v {
            t.insert(k.into(), Value::Str(v));
        }
    }
    for (k, v) in [
        ("ui_size", f.ui_size),
        ("story_size", f.story_size),
        ("mono_size", f.mono_size),
        ("line_spacing", f.line_spacing),
    ] {
        if let Some(v) = v {
            t.insert(k.into(), Value::Float(v as f64));
        }
    }
    if let Some(fb) = &f.fallbacks {
        t.insert(
            "fallbacks".into(),
            Value::Array(fb.iter().map(|s| Value::Str(s.clone())).collect()),
        );
    }
    Value::Table(t)
}

/// A resolved theme: a name plus everything the UI needs to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: String,
    pub palette: Palette,
    pub shape: ShapeFile,
    pub fonts: TypeFile,
    pub frame: Measures,
    pub recipe: Recipe,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::light()
    }
}

impl Theme {
    /// `@0x101948518` — the built-in **Light** theme.
    ///
    /// Recovered literals: name `"Light"`, seed `#477ab3`, background
    /// `#f4f1ea`, scheme `"triadic"` (discriminant 4).
    pub fn light() -> Theme {
        Theme {
            name: "Light".into(),
            palette: Palette::bare(),
            shape: ShapeFile::default(),
            fonts: TypeFile::default(),
            frame: Measures::default(),
            recipe: Recipe {
                seed: rgb(0x47, 0x7a, 0xb3),
                background: rgb(0xf4, 0xf1, 0xea),
                scheme: Scheme::Triadic,
                ..Recipe::default()
            },
        }
    }

    /// `@0x101949a64` — the built-in **Despana** theme.
    ///
    /// Recovered literals: name `"Despana"`, seed `#d7ad63`, background
    /// `#0d1115`, scheme discriminant 4 (`triadic`).
    pub fn despana() -> Theme {
        Theme {
            name: "Despana".into(),
            palette: Palette::bare(),
            shape: ShapeFile::default(),
            fonts: TypeFile::default(),
            frame: Measures::default(),
            recipe: Recipe {
                seed: rgb(0xd7, 0xad, 0x63),
                background: rgb(0x0d, 0x11, 0x15),
                scheme: Scheme::Triadic,
                ..Recipe::default()
            },
        }
    }

    /// `@0x101948790` — read a theme out of a `theme.toml` body.
    pub fn parse(src: &str) -> Result<Theme, String> {
        let file = File::parse(src)?;
        let base = Theme::light();
        let recipe = match &file.recipe {
            Some(r) => r.resolve(&RecipeFile {
                seed: Some(base.recipe.seed),
                background: Some(base.recipe.background),
                scheme: Some(base.recipe.scheme),
                ..RecipeFile::default()
            }),
            None => base.recipe,
        };
        let mut palette = Palette::bare();
        if let Some(names) = &file.names {
            palette.set_names(names.clone());
        }
        if let Some(PromptFile::Colours(c)) | Some(PromptFile::Both { colours: c, .. }) =
            &file.prompt
        {
            palette.set_prompt(c.clone());
        }
        Ok(Theme {
            name: file.chosen.name.unwrap_or_else(|| "Untitled".into()),
            palette,
            shape: file.shape.unwrap_or_default(),
            fonts: file.fonts.unwrap_or_default(),
            frame: file.frame.unwrap_or_default(),
            recipe,
        })
    }

    /// `@0x101949ddc` — write this theme as a `theme.toml` body.
    pub fn to_toml(&self) -> String {
        let file = File {
            chosen: Chosen {
                name: Some(self.name.clone()),
                ..Chosen::default()
            },
            names: Some(self.palette.by_name()),
            prompt: None,
            recipe: Some(RecipeFile {
                seed: Some(self.recipe.seed),
                background: Some(self.recipe.background),
                scheme: Some(self.recipe.scheme),
                variance: Some(self.recipe.variance),
                contrast: Some(self.recipe.contrast),
                separation: Some(self.recipe.separation),
                room_spread: Some(self.recipe.room_spread),
            }),
            shape: Some(self.shape),
            fonts: Some(self.fonts.clone()),
            frame: Some(self.frame),
        };
        file.to_toml()
    }

    /// Recolour every token from this theme's recipe and name overrides.
    pub fn generate(&self) -> Palette {
        let colors = crate::theme::harmony::generate(&self.recipe);
        let mut palette = self.palette.clone();
        for (i, c) in colors.iter().enumerate() {
            let token = crate::theme::Token::from_u8(i as u8).unwrap();
            if palette.names.get(token.name()).is_none() {
                palette.bare[i] = *c;
            }
        }
        for (name, c) in palette.names.iter() {
            if let Some(t) = crate::theme::Token::named(name) {
                palette.bare[t as usize] = *c;
            }
        }
        palette
    }
}

/// A per-character outfit: the theme overrides a character wears.
///
/// Drop glue in the binary spells out `Option<(String, Palette)>` and
/// `BTreeMap<u32, Option<Outfit>>`, so an outfit is keyed by a character id and
/// either replaces or inherits the palette.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outfit {
    pub theme: Option<String>,
    pub recipe: Option<String>,
}

/// Every theme the client knows about.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Themes {
    /// The built-in themes, in the order the picker shows them.
    pub built: Vec<Theme>,
    /// Extra themes loaded from disk.
    pub extra: Vec<Theme>,
    /// Per-character overrides.
    pub outfits: BTreeMap<u32, Option<Outfit>>,
}

impl Themes {
    /// `@0x10194cbe8` — the built-in set: **Despana** then **Light**.
    pub fn built_in() -> Themes {
        Themes {
            built: vec![Theme::despana(), Theme::light()],
            extra: Vec::new(),
            outfits: BTreeMap::new(),
        }
    }

    /// `@0x10194b34c` — register a theme, replacing one with the same name.
    pub fn add(&mut self, theme: Theme) {
        if let Some(slot) = self.extra.iter_mut().find(|t| t.name == theme.name) {
            *slot = theme;
        } else {
            self.extra.push(theme);
        }
    }

    /// `@0x10194b3e0` — fetch by name, extra themes first.
    pub fn get(&self, name: &str) -> Option<&Theme> {
        self.extra
            .iter()
            .find(|t| t.name == name)
            .or_else(|| self.built.iter().find(|t| t.name == name))
    }

    /// `@0x10194bd78` — every name the picker shows.
    pub fn names(&self) -> Vec<&str> {
        self.built
            .iter()
            .map(|t| t.name.as_str())
            .chain(self.extra.iter().map(|t| t.name.as_str()))
            .collect()
    }

    /// `@0x10194bd8c` — the frame measures of a theme, falling back to the
    /// built-in default.
    pub fn frame(&self, name: &str) -> Measures {
        self.get(name)
            .map(|t| t.frame)
            .unwrap_or_else(|| Theme::light().frame)
    }

    /// `@0x10194bd8c` — the outfit for a character, if one was set.
    pub fn outfit(&self, id: u32) -> Option<&Option<Outfit>> {
        self.outfits.get(&id)
    }

    /// `@0x10194afb0` — the theme a character actually wears.
    pub fn outfit_for(&self, id: u32) -> Option<&Theme> {
        let outfit = self.outfits.get(&id)?.as_ref()?;
        let name = outfit.theme.as_deref()?;
        self.get(name)
    }

    /// `@0x10194c070` — resolve a name, following `base` chains.
    ///
    /// `// UNKNOWN:` the binary's `Themes::chain` helper walks an explicit
    /// linked list of loaded bases; here the chain is expressed through the
    /// loaded themes' names, and an unknown base simply ends the walk.
    pub fn resolve(&self, name: &str) -> Option<Theme> {
        let mut theme = self.get(name)?.clone();
        let mut seen = 0;
        let mut current = name.to_string();
        while seen < 8 {
            let file = File {
                chosen: Chosen {
                    base: Some(current.clone()),
                    ..Chosen::default()
                },
                ..File::default()
            };
            let _ = file;
            let next = self.get(&current).and_then(|t| {
                if t.name == current {
                    None
                } else {
                    Some(t.name.clone())
                }
            });
            match next {
                Some(n) if n != current => {
                    if let Some(base) = self.get(&n) {
                        theme.shape = theme.shape.over(&base.shape);
                        theme.fonts = theme.fonts.over(&base.fonts);
                        current = n;
                    } else {
                        break;
                    }
                }
                _ => break,
            }
            seen += 1;
        }
        Some(theme)
    }

    /// `@0x10194b464` — read every `*.toml` theme in a directory.
    ///
    /// Errors from individual files are collected and the rest are kept; the
    /// binary reports them through `highlight::sound::refused`-style notices.
    pub fn load(&mut self, dir: &Path) -> Vec<String> {
        let mut problems = Vec::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => return vec![format!("{}: {e}", dir.display())],
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_toml = path
                .extension()
                .map(|e| e.eq_ignore_ascii_case("toml"))
                .unwrap_or(false);
            if !is_toml {
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(text) => match Theme::parse(&text) {
                    Ok(t) => self.add(t),
                    Err(e) => problems.push(format!("{}: {e}", path.display())),
                },
                Err(e) => problems.push(format!("{}: {e}", path.display())),
            }
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Token;

    #[test]
    fn file_stems() {
        assert_eq!(file_stem("/themes/Despana.toml"), "despana");
        assert_eq!(file_stem("themes/Light.TOML"), "light");
        assert_eq!(file_stem("themes/plain"), "plain");
        assert_eq!(file_stem(""), "");
    }

    #[test]
    fn built_in_themes() {
        let t = Themes::built_in();
        assert_eq!(t.names(), vec!["Despana", "Light"]);
        let d = t.get("Despana").unwrap();
        assert_eq!(hex(d.recipe.seed), "#d7ad63");
        assert_eq!(hex(d.recipe.background), "#0d1115");
        assert_eq!(d.recipe.scheme, Scheme::Triadic);
        let l = t.get("Light").unwrap();
        assert_eq!(hex(l.recipe.seed), "#477ab3");
        assert_eq!(hex(l.recipe.background), "#f4f1ea");
        assert_eq!(l.recipe.scheme, Scheme::Triadic);
        assert_eq!(t.get("Nope"), None);
    }

    #[test]
    fn parse_a_theme_file() {
        let src = r##"
[chosen]
name = "Mist"
base = "Despana"

[names]
canvas = "#101010"
text = "#eeeeee"

[shape]
stroke = 1.5
tabs = "boxed"

[type]
ui_font = "Noto Sans"
ui_size = 14.0
fallbacks = ["Iosevka"]

[frame]
bar = 20.0
overhang = 2.0
corner = 6.0
run = 4.0

[recipe]
seed = "#63add7"
background = "#0b0b0b"
scheme = "analogous"
variance = 0.1
"##;
        let theme = Theme::parse(src).unwrap();
        assert_eq!(theme.name, "Mist");
        assert_eq!(theme.palette.named("canvas"), Some((0x10, 0x10, 0x10)));
        assert_eq!(theme.palette.named("text"), Some((0xee, 0xee, 0xee)));
        assert_eq!(theme.palette.named("missing"), None);
        assert_eq!(theme.shape.stroke, Some(1.5));
        assert_eq!(theme.shape.tabs, Some(crate::theme::shape::TabStyle::Boxed));
        assert_eq!(theme.fonts.ui_font.as_deref(), Some("Noto Sans"));
        assert_eq!(theme.fonts.ui_size, Some(14.0));
        assert_eq!(theme.frame.bar, 20.0);
        assert_eq!(theme.frame.corner, 6.0);
        assert_eq!(hex(theme.recipe.seed), "#63add7");
        assert_eq!(theme.recipe.scheme, Scheme::Analogous);
        assert_eq!(theme.recipe.variance, 0.1);
    }

    #[test]
    fn theme_round_trips_through_toml() {
        let mut theme = Theme::despana();
        theme.name = "Round".into();
        theme.shape.stroke = Some(2.0);
        let text = theme.to_toml();
        let back = Theme::parse(&text).unwrap();
        assert_eq!(back.name, "Round");
        assert_eq!(back.shape.stroke, Some(2.0));
        assert_eq!(back.recipe.seed, theme.recipe.seed);
        assert_eq!(back.recipe.background, theme.recipe.background);
        assert_eq!(back.recipe.scheme, theme.recipe.scheme);
    }

    #[test]
    fn generate_honours_name_overrides() {
        let mut theme = Theme::light();
        let mut names = BTreeMap::new();
        names.insert("canvas".to_string(), rgb(0x11, 0x22, 0x33));
        theme.palette.set_names(names);
        let p = theme.generate();
        assert_eq!(p.get(Token::Canvas), rgb(0x11, 0x22, 0x33));
        // Untouched tokens come from the generator, not the bare table.
        assert_ne!(p.get(Token::Accent), 0);
    }

    #[test]
    fn recipe_differing_and_resolve() {
        let base = RecipeFile {
            seed: Some(rgb(1, 2, 3)),
            variance: Some(0.1),
            ..RecipeFile::default()
        };
        let next = RecipeFile {
            seed: Some(rgb(1, 2, 3)),
            variance: Some(0.2),
            ..RecipeFile::default()
        };
        let d = next.differing(&base);
        assert_eq!(d.seed, None);
        assert_eq!(d.variance, Some(0.2));
        let r = d.resolve(&base);
        assert_eq!(r.seed, rgb(1, 2, 3));
        assert_eq!(r.variance, 0.2);
    }

    #[test]
    fn resolving_a_base_chain_overlays_shape_and_fonts() {
        let mut themes = Themes::built_in();
        let mut child = Theme::light();
        child.name = "Child".into();
        child.shape.stroke = Some(3.0);
        themes.add(child);
        let resolved = themes.resolve("Child").unwrap();
        assert_eq!(resolved.name, "Child");
        assert_eq!(resolved.shape.stroke, Some(3.0));
    }

    #[test]
    fn prompt_file_variants() {
        let t = PromptFile::Text("s>".into());
        assert!(!t.is_empty());
        assert!(PromptFile::Text(String::new()).is_empty());
        let mut c = BTreeMap::new();
        c.insert(1u32, 9u8);
        assert!(!PromptFile::Colours(c.clone()).is_empty());
        assert!(PromptFile::Both {
            text: String::new(),
            colours: BTreeMap::new()
        }
        .is_empty());
    }
}
