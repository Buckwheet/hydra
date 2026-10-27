//! Importing Hydra/GemStone session recordings.
//!
//! Recovered from the arm64 Mach-O binary: `instance_of` (`0x10026368c`),
//! `attribute_of` (`0x10026380c`), `gather` (`0x100263f0c`), `name_of`
//! (`0x10026413c`), `span_of` (`0x100264410`) and `sessions` (`0x100264708`),
//! plus the `Session` record layout from `drop_in_place<hydra_import::Session>`
//! (`0x100264c80`).  `NOTES.md` carries the function table, the recovered
//! literals and every uncertainty.
//!
//! A session is one recording: a `.bytes` wire log written by
//! `hydra_net::sink` (see `crates/hydra_net/NOTES.md`), rotated into
//! `<stem>-000.bytes`, `-001.bytes`, ...  [`sessions`] walks the given paths,
//! collects every `.bytes` file, groups the rotations of one recording
//! together and returns them oldest-first.

use std::path::{Path, PathBuf};

/// The tag prefix of the `<app ...>` record header.
const APP_TAG: &str = "<app ";
/// The tag prefix of the `<settingsInfo ...>` record header.
const SETTINGS_TAG: &str = "<settingsInfo ";
/// The attribute naming the game inside `<app ...>`.
const GAME_ATTR: &str = "game=";
/// The attribute naming the instance inside `<settingsInfo ...>`.
const INSTANCE_ATTR: &str = "instance=";
/// What [`span_of`] looks for: the timestamped prompt the server echoes.
const PROMPT_TAG: &str = "<prompt time=\"";

/// The short instance codes the original's static table mapped to display
/// names.
///
/// ESTABLISHED: the three static records at `0x1044a8680`, `0x1044a86a0` and
/// `0x1044a86c0` are `(&str, &str)` pairs holding `("GS3", "Prime")`,
/// `("GSF", "Shattered")` and `("GST", "Test")`; the pointer/length pair at
/// `+0x10` of each record is what `hydra_net::gemstone::endpoint::instance`
/// returned.  The rows are in the order the original compared them (`gs3`,
/// then `gsf`, then `gst`) and point at the run
/// `"GS3PrimeGSFShatteredGST"` at `0x103bf962d` plus `"Test"` at `0x10213f868`.
const INSTANCE_NAMES: [(&str, &str); 3] =
    [("GS3", "Prime"), ("GSF", "Shattered"), ("GST", "Test")];

/// The display name the original's static table gave a short instance code.
///
/// ESTABLISHED from `hydra_net::gemstone::endpoint::instance` (`0x101c44af8`):
/// the trimmed argument is compared against exactly three spellings —
/// `gs3`, `gsf` and `gst` — three ASCII bytes at a time and case-insensitively
/// (the `sub w, w, #0x41 / cmp #0x1a / csel w8, #0x20, wzr, lo` idiom), and the
/// table's second `&str` is returned; every other spelling falls to `mov x0, #0`
/// (`None`).  A longer or shorter string cannot match a three byte row, so the
/// length check the original does first is implied here.
fn display_name(code: &str) -> Option<&'static str> {
    INSTANCE_NAMES
        .iter()
        .find(|(short, _)| short.eq_ignore_ascii_case(code))
        .map(|&(_, name)| name)
}

/// One recorded session.
///
/// Layout established from the binary: four 24-byte fields, stride `0x60`
/// (`mov w9, #0x60` / `madd` when a session is stored at `0x1002647b0`, and
/// again when the caller walks the returned vector at `0x100005b70`):
///
/// | offset | field | recovered content |
/// | --- | --- | --- |
/// | `+0x00` | [`Session::name`] | [`name_of`]'s first string |
/// | `+0x18` | [`Session::stamp`] | [`name_of`]'s 19-byte tail string |
/// | `+0x30` | [`Session::files`] | `Vec<PathBuf>` of merged recordings |
/// | `+0x48` | [`Session::prefix`] | [`name_of`]'s `Option<String>` |
///
/// UNKNOWN: the original's field names (no debug info, and the mangled names
/// only ever say `hydra_import::Session`).  The names here are descriptive:
/// `name` + `stamp` are the pair reports print as `"{} {}"` (`0x103e21840`,
/// called with `&session[0x00]` and `&session[0x18]`), and `files` is the
/// rotation list.  `prefix` is the part of the file name before the first `_`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Session {
    /// The part of the file stem after its first `_`; with [`Session::stamp`]
    /// this is the identity the rotations are merged on.
    pub name: String,
    /// The 19 byte tail of the file stem, the rotation counter excluded.
    pub stamp: String,
    /// The `.bytes` recordings merged into this session, in sorted order.
    pub files: Vec<PathBuf>,
    /// The part of the file stem before its first `_`, absent when the stem
    /// has no `_` (and therefore no [`Session::name`] split).
    pub prefix: Option<String>,
}

impl Ord for Session {
    /// ESTABLISHED from the comparator inlined into
    /// `core::slice::sort::shared::smallsort::insert_tail` at `0x10026516c`:
    /// the string at `+0x18` ([`Session::stamp`]) is compared first, the string
    /// at `+0x00` ([`Session::name`]) second, and nothing else.  Both are
    /// compared as `str` (memcmp of the shorter length, then length tie-break).
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.stamp
            .cmp(&other.stamp)
            .then_with(|| self.name.cmp(&other.name))
    }
}

impl PartialOrd for Session {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// The instance (or game) a recording belongs to.
///
/// Recovered from `0x10026368c`: the `<app ... game="...">` attribute is tried
/// first and returned verbatim when it is non-empty; otherwise the
/// `<settingsInfo ... instance="...">` attribute is resolved through the
/// original's three row static table and the resulting display name is returned
/// (so `instance="GS3"` becomes `"Prime"`, `"GST"` becomes `"Test"` and
/// `"GSF"` becomes `"Shattered"`).
pub fn instance_of(text: &str) -> Option<String> {
    if let Some(game) = attribute_of(text, APP_TAG, GAME_ATTR) {
        if !game.is_empty() {
            return Some(game);
        }
        // An empty `game=""` is dropped and the settingsInfo path is tried
        // (`0x100263798`: free the string, then fall through).
    }
    let code = attribute_of(text, SETTINGS_TAG, INSTANCE_ATTR)?;
    let code = code.trim();
    // ESTABLISHED: the original's table — which lives in the sibling crate as
    // `hydra_net::gemstone::endpoint::instance` (`0x101c44af8`) — answers for
    // exactly `gs3`/`gsf`/`gst` with the display name.  The workspace's
    // reconstruction of that function returns an `Endpoint` (host/port/game)
    // for `GS4`/`GS3` instead, which cannot produce the display names, so the
    // recovered rows are applied here first.  See `NOTES.md` §1.1 and §4.
    if let Some(name) = display_name(code) {
        return Some(name.to_string());
    }
    // DEPARTURE: the original returned `None` for every other spelling.  The
    // recovered sibling table still knows `GS4` (and `GS3`), so a recording
    // that names one of those codes only in `<settingsInfo ...>` keeps the code
    // itself as its game instead of being skipped as "no game named".
    let endpoint = hydra_net::gemstone::endpoint::instance(code)?;
    Some(endpoint.game.to_string())
}

/// Recover a session identity from a `.bytes` recording's file name.
///
/// Recovered from `0x10026413c`.  On the stem (the file name without its
/// `.bytes` extension) the rule is:
///
/// * the last `-` must leave exactly three bytes behind it, and all three must
///   be ASCII digits (the rotation counter, `-000`, `-001`, ...);
/// * that `-` must be at least 20 bytes into the stem;
/// * exactly 20 bytes before it there must be another `-`;
/// * the 19 bytes between that second `-` and the counter's `-` are the
///   *stamp*, and byte 10 of the stamp must be `_`;
/// * everything before the second `-` is the *prefix*, which is split at its
///   first `_`: what follows the `_` is the [`Session::name`], what precedes it
///   is the [`Session::prefix`] (`None` when the prefix has no `_` at all, in
///   which case the whole prefix becomes the name).
///
/// The returned tuple is `(name, prefix, stamp)`; UNKNOWN: the original's
/// type/field names for that triple.
pub fn name_of(path: &Path) -> Option<(String, Option<String>, String)> {
    let stem = path.file_stem()?; // Path::file_stem
    let stem = stem.to_str()?; // core::str::from_utf8
    let dash = stem.rfind('-')?; // core::slice::memchr::memrchr(b'-')
    let suffix = stem.get(dash + 1..)?;
    if suffix.len() != 3 || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if dash < 20 {
        return None;
    }
    let before = dash - 20;
    if stem.as_bytes()[before] != b'-' {
        return None;
    }
    // The 19 bytes between the two dashes.  The original slices unconditionally
    // and panics through `core::str::slice_error_fail` on a non-char-boundary;
    // `get` returns `None` there instead (same answer for every valid file
    // name, no panic).
    let stamp = stem.get(before + 1..dash)?;
    if stamp.as_bytes().get(10) != Some(&b'_') {
        return None;
    }
    let prefix = stem.get(..before)?;
    Some(match prefix.split_once('_') {
        Some((before, after)) => (
            after.to_string(),
            Some(before.to_string()),
            stamp.to_string(),
        ),
        None => (prefix.to_string(), None, stamp.to_string()),
    })
}

/// The first and last `<prompt time="N">` timestamps in `text`.
///
/// Recovered from `0x100264410`.  The scan starts at the beginning of the text
/// and repeats from the end of each parsed number, so the values come from
/// successive prompts; the two `Option`s are `(first, last)` and both stay
/// `None` when the text is shorter than the 14 byte tag or holds no prompt with
/// a numeric time.  The numbers are parsed as `i64` (the inlined parser at
/// `0x100264618` handles a leading `-` and checks 64-bit overflow).
pub fn span_of(text: &str) -> (Option<i64>, Option<i64>) {
    let bytes = text.as_bytes();
    let mut first: Option<i64> = None;
    let mut last: Option<i64> = None;
    let mut from = 0usize;
    while from + PROMPT_TAG.len() <= bytes.len() {
        let Some(offset) = text[from..].find(PROMPT_TAG) else {
            break;
        };
        let value_start = from + offset + PROMPT_TAG.len();
        // The digits run up to the closing quote; anything else ends the value.
        let mut end = value_start;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        from = end;
        // UNKNOWN: the original only ever hands a bare digit run to the
        // inlined `i64::from_str`, so its sign handling is unreachable; a
        // failed parse (empty run) simply continues the scan.
        if let Ok(value) = text[value_start..end].parse::<i64>() {
            if first.is_none() {
                first = Some(value);
            }
            last = Some(value);
        }
    }
    (first, last)
}

/// Every session under `paths`, oldest first.
///
/// Recovered from `0x100264708`:
///
/// 1. [`gather`] every `.bytes` file below each path into one list;
/// 2. sort that list (stable `slice::sort`, so the merge below is
///    deterministic);
/// 3. for each file, [`name_of`] its path and either append it to the existing
///    session with the same `(name, stamp)` or start a new session;
/// 4. sort the sessions by [`Session`]'s `Ord` (stamp, then name).
///
/// Files whose name does not parse are skipped.  `paths` is INFERRED to be
/// `&[PathBuf]` (the element is a 24-byte owned string, read through its
/// `ptr`/`len` at `+0x08`/`+0x10` and fed straight to `gather`; `String` would
/// look identical in the disassembly).
pub fn sessions(paths: &[PathBuf]) -> Vec<Session> {
    let mut files: Vec<PathBuf> = Vec::new();
    for path in paths {
        gather(path, &mut files);
    }
    if files.len() >= 2 {
        files.sort();
    }

    let mut out: Vec<Session> = Vec::new();
    for file in files {
        let Some((name, prefix, stamp)) = name_of(&file) else {
            continue;
        };
        // The merge key is `(name, stamp)`: the stamp is checked second in the
        // original (`0x100264890` compares `+0x00`, then `0x1002648b0`
        // compares `+0x18`).
        match out
            .iter_mut()
            .find(|session| session.name == name && session.stamp == stamp)
        {
            Some(session) => session.files.push(file),
            None => out.push(Session {
                name,
                stamp,
                files: vec![file],
                prefix,
            }),
        }
    }
    if out.len() >= 2 {
        out.sort();
    }
    out
}

/// Collect every `.bytes` file at or below `path`.
///
/// Recovered from `0x100263f0c`: a directory is walked recursively
/// (`Path::is_dir`, `std::fs::read_dir`, `DirEntry::path`) and every entry is
/// gathered in turn; an unreadable directory (or an erroring entry) contributes
/// nothing.  Anything that is not a directory is kept only when its extension is
/// exactly `bytes` (`Path::extension`, compared byte for byte against the five
/// bytes loaded at `0x100264008`).
fn gather(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_dir() {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                gather(&entry.path(), files);
            }
        }
        return;
    }
    if path.extension().is_some_and(|ext| ext == "bytes") {
        files.push(path.to_path_buf());
    }
}

/// The value of `attribute` inside the first `<tag ...>` element of `text`.
///
/// Recovered from `0x10026380c`:
///
/// * find the first occurrence of `tag` in `text` (a byte-wise `memcmp` scan);
/// * the element interior ends at the next `>` (sliced with the char-boundary
///   checks the original performs);
/// * find `attribute` inside that interior with `str::find`;
/// * the character right after the attribute name is the quote (the original
///   accepts any `char`, not just `"`), and the value is everything up to the
///   next occurrence of that same character.
///
/// Returns `None` when any step fails, including a missing closing quote.
fn attribute_of(text: &str, tag: &str, attribute: &str) -> Option<String> {
    if tag.len() > text.len() {
        return None;
    }
    let at = text
        .as_bytes()
        .windows(tag.len())
        .position(|window| window == tag.as_bytes())?;
    let rest = text.get(at..)?;
    let end = rest.find('>')?;
    let inner = rest.get(..end)?;
    let after = inner.find(attribute)? + attribute.len();
    let quote = inner.get(after..)?.chars().next()?;
    let value = inner.get(after + quote.len_utf8()..)?;
    let close = value.find(quote)?;
    Some(value.get(..close)?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// A scratch directory that removes itself.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Scratch {
            let mut dir = std::env::temp_dir();
            dir.push(format!("hydra-import-{}-{}", std::process::id(), tag));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("scratch dir");
            Scratch(dir)
        }

        fn file(&self, name: &str, body: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, body).expect("write fixture");
            path
        }

        fn dir(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::create_dir_all(&path).expect("fixture dir");
            path
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    // ------------------------------------------------------- attribute_of

    #[test]
    fn attribute_is_taken_from_the_first_tag_and_stops_at_the_quote() {
        let text = r#"<app game="GS4" instance="GS3"><app game="DR">"#;
        assert_eq!(attribute_of(text, APP_TAG, GAME_ATTR).as_deref(), Some("GS4"));
        assert_eq!(
            attribute_of(text, APP_TAG, INSTANCE_ATTR).as_deref(),
            Some("GS3")
        );
    }

    #[test]
    fn attribute_needs_the_tag_the_attribute_and_a_closing_quote() {
        // tag missing
        assert_eq!(
            attribute_of(r#"<settingsInfo instance="GS3">"#, APP_TAG, GAME_ATTR),
            None
        );
        // attribute missing inside the element
        assert_eq!(
            attribute_of(r#"<app instance="GS3">"#, APP_TAG, GAME_ATTR),
            None
        );
        // element never closes
        assert_eq!(attribute_of(r#"<app game="GS4""#, APP_TAG, GAME_ATTR), None);
        // value never closes
        assert_eq!(attribute_of(r#"<app game="GS4>"#, APP_TAG, GAME_ATTR), None);
        // nothing follows the attribute name
        assert_eq!(attribute_of(r#"<app game="#, APP_TAG, GAME_ATTR), None);
        // tag longer than the text
        assert_eq!(attribute_of("", APP_TAG, GAME_ATTR), None);
    }

    #[test]
    fn attribute_takes_any_character_after_the_name_as_the_quote() {
        // The original decodes the character after the attribute name and
        // searches for the same character again, so `"` is not special.
        assert_eq!(
            attribute_of(r#"<app game=|GS4| other="x">"#, APP_TAG, GAME_ATTR).as_deref(),
            Some("GS4")
        );
        assert_eq!(
            attribute_of(r#"<app game=*GS4*x>"#, APP_TAG, GAME_ATTR).as_deref(),
            Some("GS4")
        );
        // A multi-byte quote is consumed by its UTF-8 length.
        assert_eq!(
            attribute_of("<app game=\u{a7}GS4\u{a7}>", APP_TAG, GAME_ATTR).as_deref(),
            Some("GS4")
        );
    }

    // ------------------------------------------------------- instance_of

    #[test]
    fn instance_of_prefers_the_app_game_attribute() {
        let text = r#"<app game="GS4" instance="GS3"><settingsInfo instance="GST">"#;
        assert_eq!(instance_of(text).as_deref(), Some("GS4"));
    }

    #[test]
    fn instance_of_falls_back_to_the_settings_info_instance() {
        assert_eq!(
            instance_of(r#"<settingsInfo instance="GS3">"#).as_deref(),
            Some("Prime")
        );
        // The code is trimmed before it is resolved, and the lookup is
        // case-insensitive (`GS3`, `gs3` and ` gs3 ` all land on Prime).
        assert_eq!(
            instance_of(r#"<settingsInfo instance=" gs3 ">"#).as_deref(),
            Some("Prime")
        );
    }

    #[test]
    fn instance_of_resolves_the_three_recovered_instance_codes() {
        // ESTABLISHED: the original's static table held exactly these three
        // rows (`0x1044a8680`/`0x1044a86a0`/`0x1044a86c0`, the strings
        // `"GS3PrimeGSFShatteredGST"` at `0x103bf962d` and `"Test"` at
        // `0x10213f868`), compared case-insensitively against the trimmed code.
        let cases = [
            ("GS3", "Prime"),
            ("gs3", "Prime"),
            (" gs3\t", "Prime"),
            ("GSF", "Shattered"),
            ("gsf", "Shattered"),
            (" GST ", "Test"),
            ("GST", "Test"),
            ("gst", "Test"),
        ];
        for (code, expected) in cases {
            let text = format!(r#"<settingsInfo instance="{code}">"#);
            assert_eq!(instance_of(&text).as_deref(), Some(expected), "{code}");
        }
    }

    #[test]
    fn display_name_only_knows_three_whole_codes() {
        for (code, expected) in INSTANCE_NAMES {
            assert_eq!(display_name(code), Some(expected));
            assert_eq!(display_name(&code.to_ascii_lowercase()), Some(expected));
        }
        // Exactly three bytes, no surrounding whitespace: trimming is
        // `instance_of`'s job, not the table's.
        for code in ["", "GS", "GS4", "GSTX", "GS3 ", " GSF", "gs4", "Test"] {
            assert_eq!(display_name(code), None, "{code}");
        }
    }

    #[test]
    fn instance_of_keeps_the_game_code_for_an_endpoint_only_spelling() {
        // DEPARTURE: the original answered only for gs3/gsf/gst and returned
        // `None` otherwise, so `instance="GS4"` alone was "no game named".
        // The recovered sibling table still knows GS4, so the code survives.
        assert_eq!(
            instance_of(r#"<settingsInfo instance="GS4">"#).as_deref(),
            Some("GS4")
        );
        assert_eq!(
            instance_of(r#"<settingsInfo instance=" gs4 ">"#).as_deref(),
            Some("GS4")
        );
    }

    #[test]
    fn instance_of_ignores_an_empty_game_attribute() {
        let text = r#"<app game=""><settingsInfo instance="GS3">"#;
        assert_eq!(instance_of(text).as_deref(), Some("Prime"));
    }

    #[test]
    fn instance_of_is_none_without_a_known_instance() {
        assert_eq!(instance_of(r#"<app game="">"#), None);
        assert_eq!(instance_of(r#"<settingsInfo instance="GSTX">"#), None);
        assert_eq!(instance_of(r#"<settingsInfo instance="GS3X">"#), None);
        assert_eq!(instance_of(r#"<settingsInfo instance="">"#), None);
        assert_eq!(instance_of("no records here"), None);
        // A short spelling that is neither a table row nor an endpoint.
        assert_eq!(instance_of(r#"<settingsInfo instance="GS">"#), None);
    }

    // ------------------------------------------------------- name_of

    #[test]
    fn name_of_splits_the_prefix_and_keeps_the_stamp() {
        // "Vaalor_GS4" + "-" + "2024-03-10_10-04-59" + "-" + "000"
        let path = Path::new("/logs/2024-03-10/Vaalor_GS4-2024-03-10_10-04-59-000.bytes");
        let (name, prefix, stamp) = name_of(path).expect("a session name");
        assert_eq!(name, "GS4");
        assert_eq!(prefix.as_deref(), Some("Vaalor"));
        assert_eq!(stamp, "2024-03-10_10-04-59");
    }

    #[test]
    fn name_of_without_an_underscore_in_the_prefix_keeps_it_whole() {
        let path = Path::new("/logs/VaalorGS4-2024-03-10_10-04-59-001.bytes");
        let (name, prefix, stamp) = name_of(path).expect("a session name");
        assert_eq!(name, "VaalorGS4");
        assert_eq!(prefix, None);
        assert_eq!(stamp, "2024-03-10_10-04-59");
    }

    #[test]
    fn name_of_requires_a_three_digit_rotation_counter() {
        // two digits
        assert_eq!(
            name_of(Path::new("Vaalor_GS4-2024-03-10_10-04-59-01.bytes")),
            None
        );
        // four digits
        assert_eq!(
            name_of(Path::new("Vaalor_GS4-2024-03-10_10-04-59-0000.bytes")),
            None
        );
        // not digits
        assert_eq!(
            name_of(Path::new("Vaalor_GS4-2024-03-10_10-04-59-abc.bytes")),
            None
        );
        assert_eq!(
            name_of(Path::new("Vaalor_GS4-2024-03-10_10-04-59-00a.bytes")),
            None
        );
    }

    #[test]
    fn name_of_requires_the_stamp_underscore_and_the_dash_before_it() {
        // '_' missing from the stamp
        assert_eq!(
            name_of(Path::new("Vaalor_GS4-2024-03-1010-04-59-000.bytes")),
            None
        );
        // '_' at the wrong place (offset 9 instead of 10)
        assert_eq!(
            name_of(Path::new("Vaalor_GS4-2024-03-1_10-04-59-000.bytes")),
            None
        );
        // no second dash: the 20th byte back is a digit
        assert_eq!(
            name_of(Path::new("Vaalor_GS42024-03-10_10-04-59-000.bytes")),
            None
        );
        // the stem is far too short to hold the stamp
        assert_eq!(name_of(Path::new("2024-03-10_10-04-59-000.bytes")), None);
        assert_eq!(name_of(Path::new("session-000.bytes")), None);
        assert_eq!(name_of(Path::new("")), None);
    }

    #[test]
    fn name_of_ignores_the_extension_and_the_directory() {
        let dotted = Path::new("/a/b/Vaalor_GS4-2024-03-10_10-04-59-000.bytes");
        let bare = Path::new("/a/b/Vaalor_GS4-2024-03-10_10-04-59-000");
        assert_eq!(name_of(dotted), name_of(bare));
    }

    // ------------------------------------------------------- span_of

    #[test]
    fn span_of_returns_the_first_and_last_prompt_time() {
        let text = concat!(
            "<prompt time=\"1700000000\">&gt;</prompt>\n",
            "<output>hello</output>\n",
            "<prompt time=\"1700000123\">&gt;</prompt>\n"
        );
        assert_eq!(span_of(text), (Some(1_700_000_000), Some(1_700_000_123)));
    }

    #[test]
    fn span_of_is_empty_for_short_or_promptless_text() {
        assert_eq!(span_of(""), (None, None));
        assert_eq!(span_of("<prompt time="), (None, None));
        assert_eq!(span_of("<app game=\"GS4\">\nno prompts\n"), (None, None));
    }

    #[test]
    fn span_of_skips_a_prompt_whose_time_is_not_a_number() {
        let text = concat!(
            "<prompt time=\"x\">",
            "<prompt time=\"42\">",
            "<prompt time=\"1700000000\">"
        );
        assert_eq!(span_of(text), (Some(42), Some(1_700_000_000)));
    }

    #[test]
    fn span_of_needs_the_full_tag_including_the_quote() {
        // The tag ends in `time="`, so a prompt without a quote is no match.
        assert_eq!(span_of("<prompt time=42>"), (None, None));
        assert_eq!(span_of("<prompt>"), (None, None));
    }

    // --------------------------------------------------- gather / sessions

    #[test]
    fn gather_collects_only_bytes_files_below_the_directory() {
        let scratch = Scratch::new("gather");
        let bytes = scratch.file("Vaalor_GS4-2024-03-10_10-04-59-000.bytes", "a");
        scratch.file("Vaalor_GS4-2024-03-10_10-04-59.log", "b");
        scratch.file("notes.txt", "c");
        scratch.dir("nested");
        let nested = scratch.0.join("nested/deep-000.bytes");
        fs::write(&nested, "d").expect("nested fixture");

        let mut files = Vec::new();
        gather(scratch.path(), &mut files);
        files.sort();
        assert_eq!(files, vec![bytes, nested]);
    }

    #[test]
    fn sessions_merges_the_rotations_of_one_recording() {
        let scratch = Scratch::new("merge");
        scratch.dir("2024-03-10");
        let first = scratch.file("2024-03-10/Vaalor_GS4-2024-03-10_10-04-59-000.bytes", "a");
        let second = scratch.file("2024-03-10/Vaalor_GS4-2024-03-10_10-04-59-001.bytes", "b");
        scratch.file("2024-03-10/Vaalor_GS4-2024-03-10_10-04-59.log", "c");

        let found = sessions(&[scratch.path().to_path_buf()]);
        assert_eq!(found.len(), 1, "{found:?}");
        let session = &found[0];
        assert_eq!(session.name, "GS4");
        assert_eq!(session.prefix.as_deref(), Some("Vaalor"));
        assert_eq!(session.stamp, "2024-03-10_10-04-59");
        assert_eq!(session.files, vec![first, second]);
    }

    #[test]
    fn sessions_keeps_recordings_apart_and_orders_them_by_stamp() {
        let scratch = Scratch::new("order");
        scratch.file("Vaalor_GS4-2024-03-10_20-00-00-000.bytes", "late");
        scratch.file("Vaalor_GS4-2024-03-10_10-04-59-000.bytes", "early");
        scratch.file("Dridarn_GS3-2024-03-10_12-00-00-000.bytes", "other");
        // Unparsable names are skipped outright.
        scratch.file("random-000.bytes", "skip");

        let found = sessions(&[scratch.path().to_path_buf()]);
        let labels: Vec<(&str, &str)> = found
            .iter()
            .map(|session| (session.name.as_str(), session.stamp.as_str()))
            .collect();
        assert_eq!(
            labels,
            vec![
                ("GS4", "2024-03-10_10-04-59"),
                ("GS3", "2024-03-10_12-00-00"),
                ("GS4", "2024-03-10_20-00-00"),
            ],
            "{found:?}"
        );
        assert!(found.iter().all(|session| session.files.len() == 1));
    }

    #[test]
    fn sessions_accepts_a_single_file_and_nothing_at_all() {
        let scratch = Scratch::new("single");
        let one = scratch.file("Vaalor_GS4-2024-03-10_10-04-59-000.bytes", "a");
        let found = sessions(&[one.clone()]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].files, vec![one]);
        assert!(sessions(&[]).is_empty());
        assert!(sessions(&[scratch.path().join("missing")]).is_empty());
    }

    #[test]
    fn sessions_merges_on_name_and_stamp_only() {
        let scratch = Scratch::new("key");
        // Same stamp and name, different prefix: one session, two files.
        scratch.file("Vaalor_GS4-2024-03-10_10-04-59-000.bytes", "a");
        scratch.file("Dridarn_GS4-2024-03-10_10-04-59-000.bytes", "b");
        let found = sessions(&[scratch.path().to_path_buf()]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].files.len(), 2);
        // The first file in sorted order decides the kept prefix.
        assert_eq!(found[0].prefix.as_deref(), Some("Dridarn"));
    }

    #[test]
    fn session_order_is_stamp_then_name() {
        let session = |name: &str, stamp: &str| Session {
            name: name.to_string(),
            stamp: stamp.to_string(),
            files: Vec::new(),
            prefix: None,
        };
        let mut sessions = vec![
            session("GS4", "2024-03-10_10-04-59"),
            session("GS3", "2024-03-10_10-04-59"),
            session("GS4", "2024-03-09_23-59-59"),
        ];
        sessions.sort();
        let order: Vec<(&str, &str)> = sessions
            .iter()
            .map(|s| (s.name.as_str(), s.stamp.as_str()))
            .collect();
        assert_eq!(
            order,
            vec![
                ("GS4", "2024-03-09_23-59-59"),
                ("GS3", "2024-03-10_10-04-59"),
                ("GS4", "2024-03-10_10-04-59"),
            ]
        );
        // The comparison ignores `files` and `prefix`.
        let mut a = session("GS4", "s");
        let mut b = session("GS4", "s");
        a.files.push(PathBuf::from("/x"));
        a.prefix = Some("Vaalor".to_string());
        b.prefix = None;
        assert_eq!(a.cmp(&b), std::cmp::Ordering::Equal);
    }
}
