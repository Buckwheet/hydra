//! The right-click object menu.
//!
//! Recovered literals: `"object-menu"` (the menu's element id), `"target clear"`,
//! `"category"` and `"command"` (0x1039929c4 / 0x10399473d).  The drop glue
//! names `MenuGroup`, so a menu is a `Vec<MenuGroup>`.

use hydra_protocol::frame::payload::{Link, LinkTarget};

/// One group of menu entries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuGroup {
    /// The group's title (the game's `category`).
    pub title: String,
    /// `(label, command)` pairs.
    pub items: Vec<(String, String)>,
}

/// The element id the menu is drawn under.
pub const ELEMENT: &str = "object-menu";

/// The command that clears the current target.
pub const TARGET_CLEAR: &str = "target clear";

/// `@0x101937d04` — the command a link sends when clicked.
pub fn link_command(link: &Link) -> Option<String> {
    Some(match &link.target {
        LinkTarget::Command { cmd } => cmd.clone(),
        LinkTarget::Href { href } => href.clone(),
        LinkTarget::Object { exist, noun } => match noun {
            Some(noun) => format!("look at {noun}"),
            None => format!("look at #{exist}"),
        },
    })
}

/// `@0x101937c80` — the groups offered for an object.
///
/// The recovered function reads the `<menu>` payload's `category`/`command`
/// attributes; here the groups are derived from the link plus the standard
/// tail entries.
pub fn object_menu(link: &Link) -> Vec<MenuGroup> {
    let mut groups = Vec::new();
    if let LinkTarget::Object { noun, .. } = &link.target {
        let name = noun.clone().unwrap_or_default();
        let mut items = vec![
            (format!("Look at {name}"), format!("look at {name}")),
            (format!("Get {name}"), format!("get {name}")),
        ];
        if let Some(cmd) = link_command(link) {
            items.push((name.clone(), cmd));
        }
        groups.push(MenuGroup {
            title: "object".into(),
            items,
        });
        groups.push(MenuGroup {
            title: "target".into(),
            items: vec![(TARGET_CLEAR.into(), TARGET_CLEAR.into())],
        });
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_come_from_the_link() {
        assert_eq!(
            link_command(&Link::command("look")).as_deref(),
            Some("look")
        );
        let obj = Link::object("123", Some("gem"));
        assert_eq!(link_command(&obj).as_deref(), Some("look at gem"));
        let bare = Link::object("123", None::<String>);
        assert_eq!(link_command(&bare).as_deref(), Some("look at #123"));
    }

    #[test]
    fn menus_are_built_for_objects() {
        let groups = object_menu(&Link::object("1", Some("gem")));
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].title, "object");
        assert!(groups[0].items.iter().any(|(_, c)| c == "get gem"));
        assert_eq!(groups[1].items[0].1, TARGET_CLEAR);
        // A plain command link has no object menu.
        assert!(object_menu(&Link::command("x")).is_empty());
    }
}
