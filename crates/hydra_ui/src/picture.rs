//! Picture references in game text (`<img src=".." alt="..">`).
//!
//! Recovered literals (0x103993219): `banner art flash anchor offset banner_fg
//! banner_bg` and `image connector strip interior-shelf interior-inline`
//! (0x103981108), plus `width height`.

/// What kind of picture a reference is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `image` — an inline picture.
    Image,
    /// `connector` — a map connector.
    Connector,
    /// `strip` — a horizontal strip of pictures.
    Strip,
    /// `interior-shelf`.
    InteriorShelf,
    /// `interior-inline`.
    InteriorInline,
}

impl Kind {
    /// The wire name.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Image => "image",
            Kind::Connector => "connector",
            Kind::Strip => "strip",
            Kind::InteriorShelf => "interior-shelf",
            Kind::InteriorInline => "interior-inline",
        }
    }

    /// Parse a wire name.
    pub fn named(name: &str) -> Option<Kind> {
        Some(match name {
            "image" => Kind::Image,
            "connector" => Kind::Connector,
            "strip" => Kind::Strip,
            "interior-shelf" => Kind::InteriorShelf,
            "interior-inline" => Kind::InteriorInline,
            _ => return None,
        })
    }
}

/// A picture reference.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Picture {
    pub src: String,
    pub alt: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// `@0x10193c338` — the kind of a picture reference.
///
/// 30 KB in the binary: it interprets the whole `<img>` grammar, including the
/// `banner`/`flash`/`anchor`/`offset` decorations.
pub fn kind_of(tag: &str) -> Kind {
    for kind in [
        Kind::Connector,
        Kind::Strip,
        Kind::InteriorShelf,
        Kind::InteriorInline,
        Kind::Image,
    ] {
        if tag.contains(kind.name()) {
            return kind;
        }
    }
    Kind::Image
}

/// `@0x10193b178` — the `src` of a picture reference.
pub fn source(tag: &str) -> Option<String> {
    attribute(tag, "src").or_else(|| attribute(tag, "href"))
}

/// `@0x10193bf88` — read a picture reference, refusing one wider than `max`.
pub fn within(tag: &str, max: u32) -> Option<Picture> {
    let src = source(tag)?;
    let width = attribute_u32(tag, "width");
    let height = attribute_u32(tag, "height");
    if let Some(w) = width {
        if w > max {
            return None;
        }
    }
    Some(Picture {
        src,
        alt: attribute(tag, "alt").unwrap_or_default(),
        width,
        height,
    })
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    hydra_protocol::text::attribute(tag, name)
}

fn attribute_u32(tag: &str, name: &str) -> Option<u32> {
    hydra_protocol::text::attribute_u32(tag, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip() {
        for kind in [
            Kind::Image,
            Kind::Connector,
            Kind::Strip,
            Kind::InteriorShelf,
            Kind::InteriorInline,
        ] {
            assert_eq!(Kind::named(kind.name()), Some(kind));
        }
        assert_eq!(Kind::named("nope"), None);
        assert_eq!(kind_of(r#"<img src="a" category="connector"/>"#), Kind::Connector);
        assert_eq!(kind_of(r#"<img src="a"/>"#), Kind::Image);
    }

    #[test]
    fn sources_and_sizes() {
        let tag = r#"<img src="map.png" alt="A map" width="120" height="80"/>"#;
        assert_eq!(source(tag).as_deref(), Some("map.png"));
        assert_eq!(within(tag, 200).unwrap().width, Some(120));
        // Too wide.
        assert!(within(tag, 100).is_none());
        // No src at all.
        assert!(within("<img/>", 100).is_none());
    }
}
