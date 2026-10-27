//! Command line: subcommand dispatch exactly as `hydra::main` does it.
//!
//! # Recovered shape
//!
//! `hydra::main` (`0x10021d750` … `0x10021e474`) collects
//! `std::env::args().skip(1)` into a `Vec<String>` (`0x10021dac8`
//! `std::env::args` + `0x10021dadc` `SpecFromIterNested<Vec<String>>::from_iter`,
//! and the `add x0, x19, #0x18` at `0x10021dd78` proves the first token is the
//! subcommand, i.e. argv[0] was skipped) and then dispatches on
//! **`tokens[0].len()` first**, comparing bytes only after the length matches:
//!
//! | length | byte compare | word | target |
//! |---|---|---|---|
//! | `0` (`cbz`) | — | *(no arguments)* | `hydra::gui::run` — the hub (`0x10021dd40`) |
//! | `3` | `cmp x9,#3` @`0x10021dd48` | `gui` | `connect::Target::many` + `gui::run` (`0x10021deb8`, `0x10021e144`) |
//! | `4` | `mov w9,#0x6c70` / `movk #0x7961` @`0x10021dd58` | `play` | `connect::Target::from_args` + `Runtime::block_on` (`0x10021dd78`, `0x10021ddc4`) |
//! | `5` | `mov w11,#0x6f6c` / `movk #0x6967` + `ldrb #4` @`0x10021ddd4` | `login` | `connect::Target::from_args` + `Runtime::block_on` (`0x10021dee8`, `0x10021e108`) |
//! | `6` | `mov w11,#0x6d69` / `movk #0x6f70` + `ldrh #4` @`0x10021de2c` | `import` | `0x10021e0a8` (an `import` future on the runtime) |
//! | `6` | `mov w10,#0x6572` / `movk #0x6c70` + `ldrh #4` @`0x10021de4c` | `replay` | [`crate::replay`] (`0x10021de70`, `0x10021de84`) |
//! | `6` | `mov w10,#0x6573` / `movk #0x7672` + `ldrb #4` @`0x10021ddf4` | `serve` | `hydra::serve` (`0x10021de1c`) |
//! | `7` | `mov w10,#0x6f6c`/`movk #0x6b6f` + `ldur w8,[x8,#3]` = `kout` @`0x10021dc74` | `lookout` | `seed` (`cmp x8,#4` + `movk #0x6465`/`mov w9,#0x6573`, `0x10021dca8`) → `hydra::lookout_seed` (`0x10021dcc4`) |
//! | anything else | — | — | `0x10021df18` → the usage text + exit 1 |
//!
//! `replay` with no file argument is *also* the usage path: `cmp x1, #1;
//! b.eq 0x10021df18` at `0x10021de64` (the same guard exists for `lookout`,
//! `0x10021df10`).
//!
//! # Additions in this reconstruction
//!
//! The reference build has **no** `--help` and no `--version`: `--help` is five
//! bytes, matches none of the lengths above, and therefore prints the usage text
//! and exits 1 like any other unknown argument.  This crate keeps that behaviour
//! for unknown arguments (usage on **stderr**, exit 1) and additionally
//! recognises `--help`/`-h`/`help` (usage on **stdout**, exit 0),
//! `--version`/`-V`/`version` (`hydra <version>`, exit 0) and the
//! `--replay <file>` spelling of the gate (`hydra replay <file>` is the
//! recovered one; both reach the same code) so the binary is usable without a
//! window.  The usage text itself is the recovered literal (see [`USAGE`]).

use crate::replay::Show;

/// The `--help` / usage text, byte-for-byte from the recovered literal
/// (`__const` `0x10206f8ac`, 848 bytes, loaded for the usage message at
/// `hydra::main` `0x10021df30`..`0x10021df40`).
pub const USAGE: &str = concat!(
    "usage:\n",
    "  hydra                                                             the hub\n",
    "  hydra login <character> [--account <account>] [--game <code>]   raw stream\n",
    "  hydra play  <character> [--account <account>] [--game <code>]   readable text, `;status`\n",
    "  hydra gui   [<character>... | <character> [--account <account>] [--game <code>]]   the hub\n",
    "  hydra serve [<character>... | <character> [--account <account>] [--game <code>]] [--detach] [--in <dir>] [--web <port>]\n",
    "                                                                    resting, no window\n",
    "  hydra replay <file.bytes|file.xml> [--summary | --text]\n",
    "  hydra import <file.bytes|folder>...                               recordings into the ledger: hunts and experience\n",
    "  hydra lookout seed                                                the player logs taught to the lookout, Hydra stopped",
);

/// The version reported by `--version`.
///
/// `hydra_update::VERSION` is the recovered constant
/// (`semver::Version::parse("0.1.0-beta.7")` in `hydra::update::keep`,
/// `0x10009e744`); this crate repeats the string because Phase 2 may not depend
/// on `hydra_update`.  `Cargo.toml` carries the same version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// One of the command lines the reference build understands.
///
/// Every variant is recognised; what each one *runs* is set out in `NOTES.md`
/// §8 (`login` and `play` log in for real since Phase 6, `serve` is still the
/// Lich side only, the hub window and the ledger are still reported).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// No arguments at all: the hub window (`hydra::gui::run`).
    Hub,
    /// `hydra gui [<character>...]`
    Gui { targets: Vec<String> },
    /// `hydra play <character> [--account ..] [--game ..]`
    Play { targets: Vec<String> },
    /// `hydra login <character> [--account ..] [--game ..]`
    Login { targets: Vec<String> },
    /// `hydra serve [...] [--detach] [--in <dir>] [--web <port>]`
    Serve { args: Vec<String> },
    /// `hydra replay <file.bytes|file.xml> [--summary | --text]`, also accepted
    /// as `hydra --replay <file>`
    Replay { file: String, show: Show },
    /// `hydra import <file.bytes|folder>...`
    Import { paths: Vec<String> },
    /// `hydra lookout seed`
    LookoutSeed,
    /// `hydra --help` (an addition; see the module docs).
    Help,
    /// `hydra --version` (an addition; see the module docs).
    Version,
    /// Anything else — including `replay`/`lookout` without their argument.
    /// The usage text goes to stderr and the exit status is 1.
    Usage,
}

impl Command {
    /// Dispatch exactly like `hydra::main` (`0x10021dc40` … `0x10021df48`).
    ///
    /// `tokens` are the arguments *after* the program name, i.e.
    /// `std::env::args().skip(1)`.
    pub fn parse(tokens: &[String]) -> Command {
        let Some(first) = tokens.first() else {
            // `cbz x1, 0x10021dd14` @`0x10021dc40`: no arguments → the hub.
            return Command::Hub;
        };
        let rest = tokens[1..].to_vec();
        match first.as_str() {
            "gui" => Command::Gui { targets: rest },
            "play" => Command::Play { targets: rest },
            "login" => Command::Login { targets: rest },
            "serve" => Command::Serve { args: rest },
            // `cmp x1, #1; b.eq usage` — replay needs its file.
            "replay" if !rest.is_empty() => Command::Replay {
                file: rest[0].clone(),
                show: Show::from_args(tokens),
            },
            // Addition: the `--replay <file>` spelling of the same command.
            "--replay" if !rest.is_empty() => Command::Replay {
                file: rest[0].clone(),
                show: Show::from_args(tokens),
            },
            "import" => Command::Import { paths: rest },
            // `lookout` + a 4-byte `seed`: anything else is the usage path.
            "lookout" if rest.first().map(String::as_str) == Some("seed") => Command::LookoutSeed,
            // Additions; the reference build treated these as unknown.
            "--help" | "-h" | "help" => Command::Help,
            "--version" | "-V" | "version" => Command::Version,
            _ => Command::Usage,
        }
    }

    /// How to name this command line in a diagnostic.
    pub fn label(&self) -> &'static str {
        match self {
            Command::Hub => "hydra (the hub)",
            Command::Gui { .. } => "hydra gui",
            Command::Play { .. } => "hydra play",
            Command::Login { .. } => "hydra login",
            Command::Serve { .. } => "hydra serve",
            Command::Replay { .. } => "hydra replay",
            Command::Import { .. } => "hydra import",
            Command::LookoutSeed => "hydra lookout seed",
            Command::Help => "hydra --help",
            Command::Version => "hydra --version",
            Command::Usage => "hydra",
        }
    }

    /// True for the two commands after which `hydra::main` shuts the runtime
    /// down with `Runtime::shutdown_background` (`0x10021dfe0`, reached from the
    /// `login`/`play` byte compares at `0x10021df64`..`0x10021df88`) instead of
    /// letting it drop at the end of `main`.
    pub fn shuts_the_runtime_down(&self) -> bool {
        matches!(self, Command::Play { .. } | Command::Login { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(words: &[&str]) -> Vec<String> {
        words.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_arguments_is_the_hub() {
        assert_eq!(Command::parse(&[]), Command::Hub);
    }

    #[test]
    fn each_recovered_subcommand_word_is_recognised() {
        assert_eq!(
            Command::parse(&toks(&["gui"])),
            Command::Gui { targets: vec![] }
        );
        assert_eq!(
            Command::parse(&toks(&["gui", "Aelith"])),
            Command::Gui {
                targets: vec!["Aelith".into()]
            }
        );
        assert_eq!(
            Command::parse(&toks(&["play", "Aelith", "--game", "GSF"])),
            Command::Play {
                targets: vec!["Aelith".into(), "--game".into(), "GSF".into()]
            }
        );
        assert_eq!(
            Command::parse(&toks(&["login", "Aelith"])),
            Command::Login {
                targets: vec!["Aelith".into()]
            }
        );
        assert_eq!(
            Command::parse(&toks(&["serve", "--detach"])),
            Command::Serve {
                args: vec!["--detach".into()]
            }
        );
        assert_eq!(
            Command::parse(&toks(&["import", "a.bytes"])),
            Command::Import {
                paths: vec!["a.bytes".into()]
            }
        );
        assert_eq!(
            Command::parse(&toks(&["lookout", "seed"])),
            Command::LookoutSeed
        );
    }

    #[test]
    fn replay_takes_its_file_and_its_flags() {
        assert_eq!(
            Command::parse(&toks(&["replay", "f.xml"])),
            Command::Replay {
                file: "f.xml".into(),
                show: Show::Default
            }
        );
        assert_eq!(
            Command::parse(&toks(&["replay", "f.xml", "--text"])),
            Command::Replay {
                file: "f.xml".into(),
                show: Show::Text
            }
        );
        assert_eq!(
            Command::parse(&toks(&["replay", "f.xml", "--summary"])),
            Command::Replay {
                file: "f.xml".into(),
                show: Show::Summary
            }
        );
    }

    #[test]
    fn the_dash_spelling_of_replay_is_the_same_command() {
        assert_eq!(
            Command::parse(&toks(&["--replay", "f.xml"])),
            Command::parse(&toks(&["replay", "f.xml"]))
        );
        assert_eq!(
            Command::parse(&toks(&["--replay", "f.xml", "--summary"])),
            Command::Replay {
                file: "f.xml".into(),
                show: Show::Summary
            }
        );
        assert_eq!(Command::parse(&toks(&["--replay"])), Command::Usage);
    }

    #[test]
    fn missing_arguments_and_unknown_words_fall_to_the_usage_path() {
        assert_eq!(Command::parse(&toks(&["replay"])), Command::Usage);
        assert_eq!(Command::parse(&toks(&["lookout"])), Command::Usage);
        assert_eq!(Command::parse(&toks(&["lookout", "sow"])), Command::Usage);
        assert_eq!(Command::parse(&toks(&["bogus"])), Command::Usage);
        // Mixed case is not a subcommand: the binary's `bcmp` is exact.
        assert_eq!(Command::parse(&toks(&["Replay", "f.xml"])), Command::Usage);
        // A prefix is not a subcommand either (length is compared first).
        assert_eq!(Command::parse(&toks(&["replays", "f.xml"])), Command::Usage);
    }

    #[test]
    fn help_and_version_are_recognised_although_the_binary_lacked_them() {
        assert_eq!(Command::parse(&toks(&["--help"])), Command::Help);
        assert_eq!(Command::parse(&toks(&["-h"])), Command::Help);
        assert_eq!(Command::parse(&toks(&["help"])), Command::Help);
        assert_eq!(Command::parse(&toks(&["--version"])), Command::Version);
        assert_eq!(Command::parse(&toks(&["-V"])), Command::Version);
        assert_eq!(Command::parse(&toks(&["version"])), Command::Version);
    }

    #[test]
    fn the_usage_text_is_the_848_byte_literal() {
        assert_eq!(USAGE.len(), 848);
        assert!(USAGE.starts_with("usage:\n  hydra  "));
        assert!(USAGE.ends_with("Hydra stopped"));
        assert_eq!(USAGE.lines().count(), 10);
    }

    #[test]
    fn only_login_and_play_shut_the_runtime_down() {
        assert!(Command::parse(&toks(&["play", "x"])).shuts_the_runtime_down());
        assert!(Command::parse(&toks(&["login", "x"])).shuts_the_runtime_down());
        for word in [&["gui"][..], &["serve"][..], &["replay", "f"][..], &[][..]] {
            assert!(!Command::parse(&toks(word)).shuts_the_runtime_down());
        }
    }

    #[test]
    fn the_version_is_the_recovered_one() {
        assert_eq!(VERSION, "0.1.0-beta.7");
    }
}
