//! Importing a Wrayth / StormFront skin as a Hydra theme.
//!
//! Recovered message and name literals (0x103992048 and 0x10399213f):
//!
//! ```text
//! "it is not a Wrayth skin"
//! "it has no SkinData: not a Wrayth skin"
//! "it is not a Wrayth settings file; "
//! "Despana"                      "Say what to call it."
//! "presetRoomName" "presetBold" "presetSpeech" "presetWhisper"
//! "presetThought" "presetWatching" "presetLinkUnderline"
//! "presetSelectedLink" "presetCommand"
//! "LeftPanel" "expandPanel" "Background"
//! ```
//!
//! A Wrayth skin is a small XML document (`<skin>` with a `<SkinData>` child
//! holding `<color id=".." value=".."/>` entries and a `<preset id=".."`
//! table).  The binary does not pull in an XML parser for this: it scans for
//! the tags and attributes it needs, which is what [`read_skin`] does too.
//! Treat the scanner as a documented seam — it accepts the same attribute
//! spellings the recovered literals name and nothing else.

use crate::theme::harmony::{Recipe, Scheme};
use crate::theme::file::{Chosen, File, RecipeFile};
use crate::theme::{parse_hex, rgb, Rgb, Token};

/// A preset name a Wrayth skin can carry, with the Hydra token it maps to.
///
/// Order and spelling are the recovered literals.
pub const PRESET_MAP: [(&str, Token); 9] = [
    ("presetRoomName", Token::RoomName),
    ("presetBold", Token::LineStrong),
    ("presetSpeech", Token::Speech),
    ("presetWhisper", Token::Whisper),
    ("presetThought", Token::Thought),
    ("presetWatching", Token::Watching),
    ("presetLinkUnderline", Token::Link),
    ("presetSelectedLink", Token::SelectedLink),
    ("presetCommand", Token::Command),
];

/// A parsed Wrayth skin.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skin {
    /// The skin's `name` attribute, if it had one.
    pub name: String,
    /// `id -> colour`, from the `<color>` elements.
    pub colors: Vec<(String, Rgb)>,
    /// `preset id -> colour`, from the `<preset>` elements.
    pub presets: Vec<(String, Rgb)>,
}

/// `@0x101846744` — read a Wrayth skin.
///
/// Errors use the recovered messages verbatim: a document without `<skin>` is
/// "it is not a Wrayth skin", and one without `<SkinData>` is
/// "it has no SkinData: not a Wrayth skin".
pub fn read_skin(text: &str) -> Result<Skin, String> {
    if !text.contains("<skin") {
        return Err("it is not a Wrayth skin".to_string());
    }
    if !text.contains("<SkinData") {
        return Err("it has no SkinData: not a Wrayth skin".to_string());
    }
    let mut skin = Skin::default();
    if let Some(tag) = text.split("<skin").nth(1) {
        let head = &tag[..tag.find('>').unwrap_or(tag.len())];
        skin.name = attribute(head, "name").unwrap_or_default();
    }
    // `<color id="x" value="#rrggbb"/>`, case-insensitive on the tag.
    for (tag, close) in tags(text) {
        let name = tag.to_ascii_lowercase();
        if name == "color" {
            if let (Some(id), Some(v)) = (attribute(&close, "id"), attribute(&close, "value")) {
                if let Some(c) = parse_hex(&v) {
                    skin.colors.push((id, c));
                }
            }
        } else if name == "preset" {
            if let (Some(id), Some(v)) = (attribute(&close, "id"), attribute(&close, "value")) {
                if let Some(c) = parse_hex(&v) {
                    skin.presets.push((id, c));
                }
            }
        }
    }
    Ok(skin)
}

/// Every `<tag ...>` in `text` as `(tag name, attribute text)`.
///
/// Deliberately tiny: Wrayth skins are flat and well-formed enough for this.
fn tags(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        rest = &rest[open + 1..];
        let end = match rest.find('>') {
            Some(e) => e,
            None => break,
        };
        let inside = &rest[..end];
        rest = &rest[end + 1..];
        if inside.starts_with('/') || inside.starts_with('?') || inside.starts_with('!') {
            continue;
        }
        let name: String = inside
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != '/')
            .collect();
        out.push((name, inside.to_string()));
    }
    out
}

fn attribute(tag: &str, key: &str) -> Option<String> {
    let pat = format!("{key}=");
    let mut rest = tag;
    loop {
        let at = rest.find(&pat)?;
        // Only accept `key=` when it is at a word boundary.
        let before_ok = at == 0
            || rest[..at]
                .chars()
                .last()
                .map(|c| c.is_whitespace() || c == '"')
                .unwrap_or(true);
        let after = &rest[at + pat.len()..];
        if before_ok {
            let quote = after.chars().next()?;
            if quote == '"' || quote == '\'' {
                let body = &after[1..];
                let end = body.find(quote)?;
                return Some(body[..end].to_string());
            }
            let end = after
                .find(|c: char| c.is_whitespace())
                .unwrap_or(after.len());
            return Some(after[..end].to_string());
        }
        rest = &rest[at + pat.len()..];
    }
}

/// `@0x101844074` — the token pins a Wrayth skin implies.
///
/// A pin is a token whose colour the skin states outright; the recovered code
/// walks [`PRESET_MAP`] in order and takes the first matching preset.
pub fn pins_from_wrayth(skin: &Skin) -> Vec<(Token, Rgb)> {
    let mut out = Vec::new();
    for (preset, token) in PRESET_MAP {
        if let Some((_, c)) = skin.presets.iter().find(|(id, _)| id == preset) {
            out.push((token, *c));
        }
    }
    out
}

/// `@0x101844d34` — the settings presets a skin declares, as
/// `(preset name, token name)` pairs the settings page lists.
pub fn settings_presets(skin: &Skin) -> Vec<(String, String)> {
    skin.presets
        .iter()
        .filter_map(|(id, c)| {
            PRESET_MAP
                .iter()
                .find(|(name, _)| name == id)
                .map(|(_, token)| (id.clone(), token.name().to_string()))
                .or_else(|| {
                    // Unknown presets still show up, keyed by their own name.
                    let _ = c;
                    Some((id.clone(), id.trim_start_matches("preset").to_lowercase()))
                })
        })
        .collect()
}

/// `@0x1018454c0` — import a Wrayth skin as a Hydra theme file.
///
/// `name` overrides the skin's own name (the "Say what to call it." prompt);
/// when it is `None` the skin's `name` attribute is used.
pub fn import(text: &str, name: Option<&str>) -> Result<File, String> {
    let skin = read_skin(text)?;
    let pins = pins_from_wrayth(&skin);
    let mut names = std::collections::BTreeMap::new();
    for (token, c) in &pins {
        names.insert(token.name().to_string(), *c);
    }
    // The background the skin declares, if any.
    let background = skin
        .colors
        .iter()
        .find(|(id, _)| id.eq_ignore_ascii_case("Background"))
        .map(|(_, c)| *c)
        .unwrap_or(Token::Canvas.bare());
    let seed = pins
        .iter()
        .find(|(t, _)| *t == Token::RoomName)
        .map(|(_, c)| *c)
        .unwrap_or_else(|| rgb(0xd7, 0xad, 0x63));
    Ok(File {
        chosen: Chosen {
            name: Some(
                name.map(|s| s.to_string())
                    .unwrap_or_else(|| skin.name.clone()),
            ),
            ..Chosen::default()
        },
        names: Some(names),
        recipe: Some(RecipeFile {
            seed: Some(seed),
            background: Some(background),
            scheme: Some(Scheme::Analogous),
            variance: Some(Recipe::default().variance),
            contrast: None,
            separation: None,
            room_spread: None,
        }),
        ..File::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKIN: &str = r##"<?xml version="1.0"?>
<skin name="Drake">
  <SkinData>
    <color id="Background" value="#0b0b0b"/>
    <color id="Text" value="#c8c8c8"/>
    <preset id="presetRoomName" value="#63add7"/>
    <preset id="presetSpeech" value="#f0eee8"/>
    <preset id="mystery" value="#123456"/>
  </SkinData>
</skin>
"##;

    #[test]
    fn reads_colors_and_presets() {
        let skin = read_skin(SKIN).unwrap();
        assert_eq!(skin.name, "Drake");
        assert_eq!(skin.colors.len(), 2);
        assert_eq!(skin.colors[0], ("Background".into(), rgb(0x0b, 0x0b, 0x0b)));
        assert_eq!(skin.colors[1], ("Text".into(), rgb(0xc8, 0xc8, 0xc8)));
        assert_eq!(skin.presets.len(), 3);
    }

    #[test]
    fn rejects_non_skins() {
        assert_eq!(
            read_skin("<html></html>").unwrap_err(),
            "it is not a Wrayth skin"
        );
        assert_eq!(
            read_skin("<skin name=\"x\"></skin>").unwrap_err(),
            "it has no SkinData: not a Wrayth skin"
        );
    }

    #[test]
    fn pins_follow_the_preset_map() {
        let skin = read_skin(SKIN).unwrap();
        let pins = pins_from_wrayth(&skin);
        assert_eq!(pins.len(), 2);
        assert_eq!(pins[0], (Token::RoomName, rgb(0x63, 0xad, 0xd7)));
        assert_eq!(pins[1], (Token::Speech, rgb(0xf0, 0xee, 0xe8)));
    }

    #[test]
    fn settings_presets_name_every_preset() {
        let skin = read_skin(SKIN).unwrap();
        let list = settings_presets(&skin);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0], ("presetRoomName".into(), "room_name".into()));
        // An unknown preset falls back to its stripped, lowercased name.
        assert_eq!(list[2], ("mystery".into(), "mystery".into()));
    }

    #[test]
    fn import_builds_a_theme_file() {
        let file = import(SKIN, None).unwrap();
        assert_eq!(file.chosen.name.as_deref(), Some("Drake"));
        let names = file.names.unwrap();
        assert_eq!(names.get("room_name"), Some(&rgb(0x63, 0xad, 0xd7)));
        let recipe = file.recipe.unwrap();
        assert_eq!(recipe.background, Some(rgb(0x0b, 0x0b, 0x0b)));
        assert_eq!(recipe.seed, Some(rgb(0x63, 0xad, 0xd7)));
        assert_eq!(recipe.scheme, Some(Scheme::Analogous));

        let renamed = import(SKIN, Some("Mine")).unwrap();
        assert_eq!(renamed.chosen.name.as_deref(), Some("Mine"));
    }
}
