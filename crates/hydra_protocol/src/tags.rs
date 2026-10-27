//! Known GemStone IV protocol tag names.
//!
//! Recovered from the 128-entry PHF set used by `tags::is_known` in the
//! binary.  A sorted table plus binary search is behaviourally equivalent to
//! the PHF lookup (and avoids a dependency).

/// The complete tag vocabulary from the binary's PHF set.
///
/// Sorted with `sort` order (`str`'s byte ordering); `is_known` relies on it.
static KNOWN_TAGS: [&str; 128] = [
    "FEStart",
    "FEVersion",
    "LaunchURL",
    "LichWebUI",
    "PantheonStatus",
    "a",
    "action",
    "annotate",
    "app",
    "b",
    "br",
    "c",
    "castTime",
    "celebration",
    "checkBox",
    "clearContainer",
    "clearDialogData",
    "clearDynaStream",
    "clearStream",
    "cli",
    "closeButton",
    "closeContainers",
    "closeDialog",
    "closedialog",
    "cmdButton",
    "cmdlist",
    "cmdtimestamp",
    "compDef",
    "compass",
    "component",
    "container",
    "continuation",
    "crtrStatus",
    "d",
    "deleteContainer",
    "description",
    "dialogData",
    "dir",
    "dropDownBox",
    "dynaStream",
    "editBox",
    "egMerchant",
    "endSetup",
    "exists",
    "exits",
    "exposeContainer",
    "exposeDialog",
    "exposeStream",
    "extra",
    "flag",
    "forcesave",
    "getSkinVersion",
    "group",
    "hScrollBar",
    "hostile",
    "i",
    "image",
    "indicator",
    "inv",
    "inventoryManager",
    "inventoryViewItem",
    "label",
    "launchURL",
    "left",
    "link",
    "macros",
    "map",
    "mapInfo",
    "menu",
    "menuImage",
    "menuLink",
    "mi",
    "mode",
    "monopolize",
    "name",
    "nav",
    "nomenu",
    "noverbupdates",
    "objective",
    "objectives",
    "openDialog",
    "opendialog",
    "output",
    "palette",
    "playerID",
    "players",
    "popBold",
    "popInputState",
    "popStream",
    "popup",
    "preset",
    "presets",
    "progressBar",
    "prompt",
    "pulse",
    "pushBold",
    "pushInputState",
    "pushStream",
    "radio",
    "resource",
    "result",
    "reward",
    "right",
    "room",
    "roomDesc",
    "roommeta",
    "roundTime",
    "sagaImage",
    "sagaimage",
    "sentSettings",
    "sep",
    "settings",
    "settingsInfo",
    "skin",
    "spell",
    "stream",
    "streamBox",
    "streamId",
    "streamWindow",
    "string",
    "style",
    "switchQuickBar",
    "task",
    "timer",
    "tipInfo",
    "upDownEditBox",
    "updateverbs",
    "vScrollBar",
];

/// Returns true when `tag` is one of the protocol's known tag names.
///
/// The name is matched exactly (case-sensitive), as in the original PHF
/// lookup.  A leading `</`/`<` or trailing `>` must already be stripped:
/// callers normally use [`crate::text::tag_name`] first.
pub fn is_known(tag: &str) -> bool {
    KNOWN_TAGS.binary_search(&tag).is_ok()
}

/// Iterate over the known tag vocabulary (mainly useful for diagnostics).
pub fn known_tags() -> impl Iterator<Item = &'static str> {
    KNOWN_TAGS.iter().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_tags_are_present() {
        for tag in [
            "a",
            "d",
            "pushBold",
            "popBold",
            "prompt",
            "room",
            "roomDesc",
            "inv",
            "stream",
            "style",
            "dialogData",
            "settings",
            "compass",
            "exits",
        ] {
            assert!(is_known(tag), "{tag} should be known");
        }
    }

    #[test]
    fn unknown_tags_are_rejected() {
        for tag in [
            "",
            "notATag",
            "PUSHBOLD",
            "<pushBold/>",
            "pushbold",
            "streamx",
        ] {
            assert!(!is_known(tag), "{tag:?} should not be known");
        }
    }

    #[test]
    fn table_is_sorted_and_complete() {
        assert_eq!(KNOWN_TAGS.len(), 128);
        assert!(KNOWN_TAGS.windows(2).all(|w| w[0] < w[1]));
    }
}
