//! Known GemStone IV protocol tag names.
//!
//! Recovered from `tags::is_known` (`0x101c3603c`, 7224 bytes).  The binary
//! holds the vocabulary as a **sorted** `[&str; 128]`: a 16-byte-stride
//! `(ptr, len)` table at `0x1044a7810` over the string pool at `0x103bf8ee8`,
//! looked up with a fully unrolled, branchless binary search (seven probes at
//! index offsets `0x20/0x10/0x08/0x04/0x02/0x01` around the midpoint `65`,
//! each comparing `min(len)` bytes with `bcmp` and then the lengths — `str`'s
//! `Ord`).  Only `core`/`alloc` code is called; there is no hash table and no
//! `phf` involvement, and the table below is *not* a PHF set.
//!
//! This file is that table verbatim (verified entry-for-entry against the
//! binary) plus `[T]::binary_search`, which is the same algorithm.

/// The complete tag vocabulary from the binary, in table order.
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
/// The name is matched exactly (case-sensitive), as the binary's `bcmp`
/// comparison does.  A leading `</`/`<` or trailing `>` must already be
/// stripped: callers normally use [`crate::text::tag_name`] first.
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

    /// The exact table at `0x1044a7810` in the arm64 binary, in table order.
    ///
    /// Order and spelling matter: `is_known` is a binary search, and the
    /// parser's tag dispatch relies on the names being exactly these.
    const RECOVERED_TABLE: [&str; 17] = [
        "FEStart FEVersion LaunchURL LichWebUI PantheonStatus a action annotate app",
        "b br c castTime celebration checkBox clearContainer clearDialogData",
        "clearDynaStream clearStream cli closeButton closeContainers closeDialog",
        "closedialog cmdButton cmdlist cmdtimestamp compDef compass component",
        "container continuation crtrStatus d deleteContainer description dialogData",
        "dir dropDownBox dynaStream editBox egMerchant endSetup exists exits",
        "exposeContainer exposeDialog exposeStream extra flag forcesave",
        "getSkinVersion group hScrollBar hostile i image indicator inv",
        "inventoryManager inventoryViewItem label launchURL left link macros map",
        "mapInfo menu menuImage menuLink mi mode monopolize name nav nomenu",
        "noverbupdates objective objectives openDialog opendialog output palette",
        "playerID players popBold popInputState popStream popup preset presets",
        "progressBar prompt pulse pushBold pushInputState pushStream radio resource",
        "result reward right room roomDesc roommeta roundTime sagaImage sagaimage",
        "sentSettings sep settings settingsInfo skin spell stream streamBox",
        "streamId streamWindow string style switchQuickBar task timer tipInfo",
        "upDownEditBox updateverbs vScrollBar",
    ];

    #[test]
    fn table_matches_the_recovered_binary_snapshot() {
        let recovered: Vec<&str> = RECOVERED_TABLE
            .iter()
            .flat_map(|chunk| chunk.split(' '))
            .collect();
        assert_eq!(recovered.len(), 128);
        assert_eq!(KNOWN_TAGS.to_vec(), recovered);
    }

    #[test]
    fn every_table_entry_is_recognised_and_nothing_else_is() {
        for tag in KNOWN_TAGS {
            assert!(is_known(tag), "{tag} should be known");
        }
        // `bcmp` is exact: case, padding and prefixes are all misses.
        assert!(!is_known(" pushBold"));
        assert!(!is_known("pushBold "));
        assert!(!is_known("PushBold"));
        assert!(!is_known("pushBol"));
        assert!(!is_known("pushBoldd"));
        assert!(!is_known("<pushBold/>"));
        assert!(!is_known(""));
    }

    #[test]
    fn known_tags_iterates_the_whole_table() {
        assert_eq!(known_tags().count(), 128);
        assert_eq!(known_tags().next(), Some("FEStart"));
        assert_eq!(known_tags().last(), Some("vScrollBar"));
    }
}
