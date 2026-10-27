//! The story window: scrollback, streams, highlights and prompts.
//!
//! Recovered symbol sizes: `Story::painted` 9.3 KB, `Story::hear` 8.1 KB,
//! `Story::alert` 4.4 KB, `Story::world` 3.8 KB, `Story::push` 3.2 KB,
//! `Story::alerts_within` 1.4 KB, `Story::settle` 696 B, `Story::see_at` 488 B,
//! `Story::typing` 488 B, `Story::tell` 452 B, `Story::highlight_with` 576 B,
//! `Story::scrollback` 276 B, `Story::missed` 112 B, `Story::see` 108 B.
//! `Prompts::{line, draws}` are 100 B and 148 B.
//!
//! The drop glue names `Shown`, `Heard` and `Call`, and the literals mention
//! `"lookout"` and `"combat"` (0x10399299e) — the two side panels a line can be
//! copied to.

pub mod scrollback;
pub mod stamp;
pub mod streams;

use std::collections::VecDeque;

use hydra_protocol::runs::Runs;

use crate::highlight::matcher::Matcher;
use crate::run::{StyledRun, RunLink};
use crate::theme::{Palette, Token};

/// A line as the window shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shown {
    /// The styled runs.
    pub runs: Vec<StyledRun>,
    /// When the line arrived (game seconds, when known).
    pub at: Option<u32>,
    /// The stream it belongs to, if any.
    pub stream: Option<String>,
}

impl Shown {
    /// The plain text of the line.
    pub fn plain(&self) -> String {
        StyledRun::plain(&self.runs)
    }

    /// The first link on the line.
    pub fn link(&self) -> Option<&RunLink> {
        self.runs.iter().find_map(|r| r.link.as_ref())
    }
}

/// A line the highlighter caught.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Heard {
    /// The rule that fired.
    pub rule: String,
    /// The text that matched.
    pub text: String,
    /// The `sound` the rule asked for, if any.
    pub sound: Option<String>,
}

/// One of the side panels a line can be copied to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    /// The lookout panel.
    Lookout,
    /// The combat panel.
    Combat,
}

impl Call {
    pub fn name(self) -> &'static str {
        match self {
            Call::Lookout => "lookout",
            Call::Combat => "combat",
        }
    }

    pub fn named(name: &str) -> Option<Call> {
        Some(match name {
            "lookout" => Call::Lookout,
            "combat" => Call::Combat,
            _ => return None,
        })
    }
}

/// The prompt the game is waiting at.
///
/// `Prompts::draws` @0x10185af30 compares the trimmed prompt against the one it
/// last drew and returns `true` when it changed; `Prompts::line` @0x10185aecc
/// notes whether any prompt line had content, which suppresses the redraw.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prompts {
    last: Option<String>,
    drawn: bool,
}

impl Prompts {
    pub fn new() -> Prompts {
        Prompts::default()
    }

    /// `@0x10185aecc` — note whether any of `lines` has content.
    pub fn line(&mut self, lines: &[String]) {
        self.drawn = lines.iter().any(|l| !l.trim().is_empty()) || self.drawn;
    }

    /// `@0x10185af30` — does `prompt` need drawing?
    ///
    /// `false` when the trimmed prompt equals the last one and nothing new
    /// arrived; otherwise the prompt is remembered and `true` is returned.
    pub fn draws(&mut self, prompt: &str) -> bool {
        let trimmed = prompt.trim();
        let same = self
            .last
            .as_deref()
            .map(|last| last == trimmed)
            .unwrap_or(false);
        let needs = !(same && !self.drawn);
        if needs {
            self.last = Some(trimmed.to_string());
        }
        self.drawn = false;
        needs
    }

    /// The prompt last drawn.
    pub fn shown(&self) -> Option<&str> {
        self.last.as_deref()
    }
}

/// The main story scrollback.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Story {
    lines: VecDeque<Shown>,
    /// How many lines to keep.
    pub keep: usize,
    /// Lines the highlighter caught.
    pub heard: Vec<Heard>,
    /// Lines that arrived while the window was scrolled back.
    pub missed: usize,
    /// Whether the window is following the tail.
    pub typing: bool,
}

impl Story {
    /// The default scrollback depth.
    pub const KEEP: usize = 5000;

    pub fn new() -> Story {
        Story {
            lines: VecDeque::new(),
            keep: Story::KEEP,
            heard: Vec::new(),
            missed: 0,
            typing: true,
        }
    }

    /// `true` when nothing has been seen.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The lines, oldest first.
    pub fn lines(&self) -> Vec<&Shown> {
        self.lines.iter().collect()
    }

    /// The last `n` lines as plain text.
    pub fn text(&self, n: usize) -> Vec<String> {
        let start = self.lines.len().saturating_sub(n);
        self.lines.iter().skip(start).map(|l| l.plain()).collect()
    }

    /// `@0x101852c80` — the scrollback as a whole.
    pub fn scrollback(&self) -> &VecDeque<Shown> {
        &self.lines
    }

    /// `@0x101853548` — append a plain line.
    pub fn see(&mut self, text: impl Into<String>) -> &Shown {
        let runs = vec![StyledRun {
            text: text.into(),
            token: Some(Token::Text),
            color: Token::Text.bare(),
            bold: false,
            link: None,
        }];
        self.push(Shown {
            runs,
            at: None,
            stream: None,
        })
    }

    /// `@0x101855568` — append a line, trimming to [`Story::keep`].
    pub fn push(&mut self, line: Shown) -> &Shown {
        self.lines.push_back(line);
        if self.keep > 0 && self.lines.len() > self.keep {
            let drop = self.lines.len() - self.keep;
            for _ in 0..drop {
                self.lines.pop_front();
            }
        }
        if !self.typing {
            self.missed += 1;
        }
        self.lines.back().unwrap()
    }

    /// `@0x1018535b4` — append a protocol `Runs` line.
    pub fn hear(&mut self, runs: &Runs, palette: &Palette) -> &Shown {
        let line = Shown {
            runs: crate::run::game_line(runs, palette),
            at: None,
            stream: None,
        };
        self.push(line)
    }

    /// `@0x101853308` — apply the highlight rules to the newest line.
    pub fn highlight_with(&mut self, matcher: &Matcher) -> Vec<Heard> {
        let Some(line) = self.lines.back() else {
            return Vec::new();
        };
        let text = line.plain();
        let caught = matcher.check(&text);
        self.heard.extend(caught.iter().cloned());
        caught
    }

    /// `@0x1018561ec` — send a line to a side panel.
    pub fn tell(&mut self, which: Call, text: impl Into<String>) -> &Shown {
        let runs = vec![StyledRun {
            text: text.into(),
            token: Some(match which {
                Call::Lookout => Token::Lookout,
                Call::Combat => Token::Creature,
            }),
            color: match which {
                Call::Lookout => Token::Lookout.bare(),
                Call::Combat => Token::Creature.bare(),
            },
            bold: false,
            link: None,
        }];
        self.push(Shown {
            runs,
            at: None,
            stream: None,
        })
    }

    /// `@0x1018563b0` — raise an alert line.
    pub fn alert(&mut self, text: impl Into<String>) -> &Shown {
        let runs = vec![StyledRun {
            text: text.into(),
            token: Some(Token::Warning),
            color: Token::Warning.bare(),
            bold: true,
            link: None,
        }];
        self.push(Shown {
            runs,
            at: None,
            stream: None,
        })
    }

    /// `@0x101852d94` — the alerts in the last `secs` of game time.
    pub fn alerts_within(&self, now: u32, secs: u32) -> Vec<&Shown> {
        self.lines
            .iter()
            .filter(|l| l.runs.iter().any(|r| r.token == Some(Token::Warning)))
            .filter(|l| match l.at {
                Some(at) => now.saturating_sub(at) <= secs,
                None => false,
            })
            .collect()
    }

    /// `@0x1018574b0` — the world panel's copy of the story.
    pub fn world(&self, _palette: &Palette) -> Vec<(Token, String)> {
        self.lines
            .iter()
            .map(|l| {
                let token = l.runs.first().and_then(|r| r.token).unwrap_or(Token::Text);
                (token, StyledRun::plain(&l.runs))
            })
            .collect()
    }

    /// `@0x1018583d4` — how many lines arrived while scrolled back.
    pub fn missed(&self) -> usize {
        self.missed
    }

    /// `@0x1018583d4` — look at a line by index from the end.
    pub fn see_at(&self, back: usize) -> Option<&Shown> {
        let len = self.lines.len();
        if back >= len {
            None
        } else {
            self.lines.get(len - 1 - back)
        }
    }

    /// `@0x1018585bc` — stop following the tail.
    pub fn settle(&mut self) {
        self.typing = false;
    }

    /// `@0x101858874` — follow the tail again and clear the missed count.
    pub fn typing(&mut self) {
        self.typing = true;
        self.missed = 0;
    }

    /// `@0x101858a5c` — everything the window draws, coloured.
    pub fn painted(&self, palette: &Palette) -> Vec<(Token, String)> {
        let mut out = Vec::with_capacity(self.lines.len());
        for line in &self.lines {
            for run in &line.runs {
                let token = run.token.unwrap_or(Token::Text);
                let _ = palette.get(token);
                out.push((token, run.text.clone()));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Palette;

    #[test]
    fn calls_are_the_two_side_panels() {
        assert_eq!(Call::named("lookout"), Some(Call::Lookout));
        assert_eq!(Call::named("combat"), Some(Call::Combat));
        assert_eq!(Call::named("nope"), None);
        assert_eq!(Call::Combat.name(), "combat");
    }

    #[test]
    fn prompts_only_redraw_when_they_change() {
        let mut p = Prompts::new();
        assert!(p.draws("s>"));
        // Same prompt, nothing new: no redraw.
        assert!(!p.draws(" s> "));
        // A new prompt redraws.
        assert!(p.draws("You have a new prompt."));
        assert_eq!(p.shown(), Some("You have a new prompt."));
        // A new line forces a redraw even when the prompt is unchanged.
        p.line(&["something".to_string()]);
        assert!(p.draws("You have a new prompt."));
    }

    #[test]
    fn prompt_lines_without_content_do_not_force_a_redraw() {
        let mut p = Prompts::new();
        p.draws("s>");
        p.line(&["   ".to_string(), String::new()]);
        assert!(!p.draws("s>"));
    }

    #[test]
    fn pushing_trims_to_keep() {
        let mut s = Story::new();
        s.keep = 3;
        for i in 0..5 {
            s.see(i.to_string());
        }
        assert_eq!(s.text(10), vec!["2", "3", "4"]);
    }

    #[test]
    fn alerts_and_missed_lines() {
        let mut s = Story::new();
        s.alert("danger");
        s.see("plain");
        assert_eq!(s.missed(), 0);
        s.settle();
        s.see("while scrolled");
        assert_eq!(s.missed(), 1);
        s.typing();
        assert_eq!(s.missed(), 0);
    }

    #[test]
    fn see_at_walks_backwards() {
        let mut s = Story::new();
        s.see("a");
        s.see("b");
        assert_eq!(s.see_at(0).unwrap().plain(), "b");
        assert_eq!(s.see_at(1).unwrap().plain(), "a");
        assert!(s.see_at(2).is_none());
    }

    #[test]
    fn telling_uses_the_panel_token() {
        let mut s = Story::new();
        s.tell(Call::Lookout, "invaders");
        assert_eq!(s.lines()[0].runs[0].token, Some(Token::Lookout));
        s.tell(Call::Combat, "a troll");
        assert_eq!(s.lines()[1].runs[0].token, Some(Token::Creature));
    }

    #[test]
    fn hearing_a_protocol_run_list() {
        let p = Palette::bare();
        let runs = Runs::from_vec(vec![hydra_protocol::runs::Run::new("hello")]);
        let mut s = Story::new();
        s.hear(&runs, &p);
        assert_eq!(s.text(1), vec!["hello"]);
        assert_eq!(s.painted(&p).len(), 1);
    }
}
