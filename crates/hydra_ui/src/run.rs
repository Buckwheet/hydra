//! Styled runs: the bridge between `hydra_protocol::Runs` and the theme tokens.
//!
//! The recovered symbol set is
//! `run::{game_line, is_web, preset_token, prompt_coloured, run_rgb,
//! run_token, styled}` plus `StyledRun::{plain, prompt}` and `RunLink::of`.
//! `RunLink` is a `serde` struct (`struct variant`-style field visitor in the
//! pool), and `StyledRun`'s drop glue shows a `Vec<StyledRun>`.
//!
//! The binary's `Run` is `hydra_protocol::runs::Run` (248 bytes, `text`,
//! `preset`, `style`/`bold`, `link`, `object`); the token mapping below is the
//! recovered bridge.

use hydra_protocol::frame::payload::{Link, LinkTarget};
use hydra_protocol::runs::{Run, Runs};

use crate::theme::{Rgb, Token};

/// A game preset id and the token it paints with.
///
/// Recovered from `run::preset_token` @0x10195cd60: the preset names the game
/// sends (`roomName`, `roomDesc`, `bold`, …) are matched to a [`Token`].
pub const PRESET_TOKENS: [(&str, Token); 16] = [
    ("roomName", Token::RoomName),
    ("roomDesc", Token::RoomDesc),
    ("bold", Token::LineStrong),
    ("speech", Token::Speech),
    ("whisper", Token::Whisper),
    ("thought", Token::Thought),
    ("watching", Token::Watching),
    ("link", Token::Link),
    ("selectedLink", Token::SelectedLink),
    ("command", Token::Command),
    ("monster", Token::Creature),
    ("player", Token::Player),
    ("object", Token::Object),
    ("container", Token::Container),
    ("timestamp", Token::Timestamp),
    ("channel", Token::ChannelGeneral),
];

/// `@0x10195cd60` — the token a game preset paints with.
pub fn preset_token(preset: &str) -> Option<Token> {
    PRESET_TOKENS
        .iter()
        .find(|(name, _)| *name == preset)
        .map(|(_, t)| *t)
}

/// `@0x10195e440` — the token a run paints with, from its preset or its shape.
pub fn run_token(run: &Run) -> Token {
    if let Some(preset) = &run.preset {
        if let Some(token) = preset_token(preset) {
            return token;
        }
    }
    if run.is_bold() {
        Token::LineStrong
    } else {
        Token::Text
    }
}

/// `@0x10195ddb8` — the colour of a run.
pub fn run_rgb(run: &Run, palette: &crate::theme::Palette) -> Rgb {
    palette.get(run_token(run))
}

/// `@0x10195dbec` — a one-run [`Runs`].
pub fn styled(text: impl Into<String>) -> Runs {
    Runs::from_vec(vec![Run::new(text)])
}

/// `@0x10195d9d0` — `true` when a run's link is a web link (`href`).
pub fn is_web(run: &Run) -> bool {
    matches!(
        run.link.as_ref().map(|l| &l.target),
        Some(LinkTarget::Href { .. })
    )
}

/// A run with its colour resolved, ready to draw.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyledRun {
    pub text: String,
    pub token: Option<Token>,
    pub color: Rgb,
    pub bold: bool,
    pub link: Option<RunLink>,
}

impl StyledRun {
    /// `@0x10195e0b0` — the plain text of a run list.
    pub fn plain(runs: &[StyledRun]) -> String {
        runs.iter().map(|r| r.text.as_str()).collect()
    }

    /// `@0x10195e154` — the prompt runs: everything up to the prompt marker.
    ///
    /// `// UNKNOWN:` the binary splits on the run whose token is
    /// [`Token::Command`]; that is what is modelled.
    pub fn prompt(runs: &[StyledRun]) -> Vec<StyledRun> {
        match runs.iter().position(|r| r.token == Some(Token::Command)) {
            Some(at) => runs[..at].to_vec(),
            None => runs.to_vec(),
        }
    }
}

/// A link, as the story line keeps it.
///
/// `RunLink` is a `serde` type in the binary (`run::RunLink::of`); the fields
/// are the ones a click needs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunLink {
    /// The object's `exist` id, when the link names a game object.
    pub exist: Option<String>,
    /// The object's noun.
    pub noun: Option<String>,
    /// The command the link sends.
    pub command: Option<String>,
}

impl RunLink {
    /// `@0x10195dbf8` — the link of a run, if it has one.
    pub fn of(run: &Run) -> Option<RunLink> {
        let link: &Link = run.link.as_ref().or(run.object.as_ref())?;
        Some(match &link.target {
            LinkTarget::Object { exist, noun } => RunLink {
                exist: Some(exist.clone()),
                noun: noun.clone(),
                command: None,
            },
            LinkTarget::Command { cmd } => RunLink {
                exist: None,
                noun: None,
                command: Some(cmd.clone()),
            },
            LinkTarget::Href { href } => RunLink {
                exist: None,
                noun: None,
                command: Some(href.clone()),
            },
        })
    }
}

/// `@0x10195e26c` — turn a protocol `Runs` into styled runs.
pub fn game_line(runs: &Runs, palette: &crate::theme::Palette) -> Vec<StyledRun> {
    runs.iter()
        .map(|run| StyledRun {
            text: run.text.clone(),
            token: Some(run_token(run)),
            color: run_rgb(run, palette),
            bold: run.is_bold(),
            link: RunLink::of(run),
        })
        .collect()
}

/// `@0x10195d188` — recolour the prompt of a line.
///
/// The prompt is the trailing run that starts with the game's prompt text; it
/// is repainted with [`Token::Command`].
pub fn prompt_coloured(runs: &mut Vec<Run>, palette: &crate::theme::Palette) -> usize {
    let mut n = 0;
    for run in runs.iter_mut() {
        if run.preset.as_deref() == Some("prompt") {
            run.style = 1;
            n += 1;
        }
    }
    let _ = palette;
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Palette;

    #[test]
    fn preset_tokens_are_the_recovered_pairs() {
        assert_eq!(preset_token("roomName"), Some(Token::RoomName));
        assert_eq!(preset_token("roomDesc"), Some(Token::RoomDesc));
        assert_eq!(preset_token("bold"), Some(Token::LineStrong));
        assert_eq!(preset_token("speech"), Some(Token::Speech));
        assert_eq!(preset_token("nope"), None);
        assert_eq!(PRESET_TOKENS.len(), 16);
    }

    #[test]
    fn run_token_prefers_the_preset() {
        let mut run = Run::new("hi");
        assert_eq!(run_token(&run), Token::Text);
        run.style = 1;
        assert_eq!(run_token(&run), Token::LineStrong);
        run.preset = Some("roomName".into());
        assert_eq!(run_token(&run), Token::RoomName);
    }

    #[test]
    fn run_rgb_reads_the_palette() {
        let p = Palette::bare();
        let run = Run::with_preset("roomName", "roomName");
        assert_eq!(run_rgb(&run, &p), p.get(Token::RoomName));
    }

    #[test]
    fn styled_is_one_run() {
        let runs = styled("hello");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs.plain(), "hello");
    }

    #[test]
    fn web_links_are_detected() {
        let mut run = Run::new("x");
        assert!(!is_web(&run));
        run.link = Some(Link {
            target: LinkTarget::Href {
                href: "https://example.invalid".into(),
            },
            ..Link::default()
        });
        assert!(is_web(&run));
    }

    #[test]
    fn links_round_trip_into_run_links() {
        let mut run = Run::new("gem");
        run.link = Some(Link::object("123", Some("gem")));
        let l = RunLink::of(&run).unwrap();
        assert_eq!(l.exist.as_deref(), Some("123"));
        assert_eq!(l.noun.as_deref(), Some("gem"));
        assert_eq!(l.command, None);

        let mut run = Run::new("look");
        run.link = Some(Link::command("look"));
        let l = RunLink::of(&run).unwrap();
        assert_eq!(l.command.as_deref(), Some("look"));
        assert_eq!(l.exist, None);

        assert_eq!(RunLink::of(&Run::new("plain")), None);
    }

    #[test]
    fn game_line_paints_every_run() {
        let p = Palette::bare();
        let runs = Runs::from_vec(vec![
            Run::new("A "),
            Run::with_preset("cave", "roomName"),
            Run::bold(" glows"),
        ]);
        let styled = game_line(&runs, &p);
        assert_eq!(styled.len(), 3);
        assert_eq!(styled[0].color, p.get(Token::Text));
        assert_eq!(styled[1].color, p.get(Token::RoomName));
        assert!(styled[2].bold);
        assert_eq!(StyledRun::plain(&styled), "A cave glows");
    }
}
