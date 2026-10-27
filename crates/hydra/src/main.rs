//! `hydra` — the Hydra GemStone IV front end, recovered from the reference
//! arm64 build and running headless.
//!
//! # The recovered startup chain
//!
//! `_main` (`0x100221d1c`, the 64-byte standard Rust shim) puts the address of
//! `hydra::main` (`0x10021d750`) in `x8` and calls
//! `std::rt::lang_start_internal`, so `hydra::main` is an ordinary `fn main`
//! that *returns its exit status in `x0`* (`mov x0, x19` at `0x10021dcec`,
//! `mov w19, #1` / `mov w19, #0` on the two paths).  Rust's `fn main` cannot
//! return an `i32`, so [`main`] wraps [`hydra_main`] in
//! [`std::process::ExitCode`]; the statuses are unchanged (0 success, 1
//! anything else).
//!
//! `hydra::main` (`0x10021d750` … `0x10021e474`, 3364 bytes) runs, in order:
//!
//! 1. [`started::begin`] — `0x10021d844`, the recorded launch instant;
//! 2. [`startup::prepare`] — the `.app` bundle / `HOME` / data-folder prologue
//!    at `0x10021d790` … `0x10021dab8` (see that module); a message here is
//!    printed to stderr (`0x10021dbf8`) and exits 1;
//! 3. `std::env::args().skip(1)` → `Vec<String>` (`0x10021dac4`/`0x10021dae0`);
//! 4. the **multi-threaded tokio runtime**,
//!    `Builder::new_multi_thread()` + `enable_all()` + `build()`
//!    (`0x10021dae4`, `enable_all`'s two driver flags at `0x10021dae8`,
//!    `Builder::build` at `0x10021db54`); a build error is printed as
//!    `cannot start the async runtime: {err}` (`0x103e253cf`, `0x10021db88`)
//!    and exits 1.  The runtime is built *before* the subcommand is inspected,
//!    so even a replay pays for it;
//! 5. the subcommand dispatch (`cli::Command::parse`, `0x10021dc40` …), which
//!    for `hydra replay` ends in [`replay::replay_show`] (`0x10021de84`), for
//!    `login`/`play` in `Runtime::block_on(hydra::connect::run(..))`
//!    (`0x10021ddc4` / `0x10021e108`), for `gui` and the bare hub in
//!    `hydra::gui::run` (`0x10021dd40`, `0x10021e144` — the eframe boundary),
//!    for `serve` in `hydra::serve` (`0x10021de1c`), for `lookout seed` in
//!    `hydra::lookout_seed` (`0x10021dcc4`), and for anything else in the
//!    usage text + exit 1 (`0x10021df18`);
//! 6. `Runtime::shutdown_background` for `login`/`play` only (`0x10021dfe0`);
//! 7. if the subcommand produced a message, it goes to stderr (`{msg}` + `\n`,
//!    `0x103e21acf`) and the status is 1 (`0x10021dffc`..`0x10021e040`).
//!
//! # What this phase runs
//!
//! `hydra replay <file>` (the phase gate), `hydra --help`, `hydra --version`
//! and `hydra gui …` (the hub window, through eframe's wgpu renderer — see
//! [`gui`]).  Every other subcommand is *recognised* — the recovered word table
//! and the usage text are exact — and then reports what it still needs, on
//! stderr, with status 1.  Nothing is stubbed out silently: see `NOTES.md` for
//! the per-command state and for the GUI boundary.
//!
//! The window is *built* as an [`hydra_gui::app::Launch`] value first, which
//! touches no device and no event loop, so the tests can assert the recovered
//! plan (`"Hydra"`, 660 × 520, `Renderer::Wgpu`) and never open a window.  Only
//! [`gui`] hands that value to eframe.

#![allow(dead_code)]

mod cli;
mod render;
mod replay;
mod session;
mod source;
mod started;
mod startup;

use std::process::ExitCode;

use cli::Command;

fn main() -> ExitCode {
    ExitCode::from(hydra_main())
}

/// `hydra::main` (`0x10021d750`), returning the exit status the reference build
/// hands back to `lang_start_internal`.
fn hydra_main() -> u8 {
    started::begin();

    if let Err(message) = startup::prepare() {
        eprintln!("{message}");
        return 1;
    }

    let tokens: Vec<String> = std::env::args().skip(1).collect();
    let command = Command::parse(&tokens);

    // Always built, and always before the dispatch — `0x10021dae4` sits above
    // the first subcommand compare at `0x10021dc4c`.
    let runtime = match runtime() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("cannot start the async runtime: {err}");
            return 1;
        }
    };

    let outcome = run(&command);

    if command.shuts_the_runtime_down() {
        // `Runtime::shutdown_background` at `0x10021dfe0`.
        runtime.shutdown_background();
    } else {
        drop(runtime);
    }

    match outcome {
        Some(message) => {
            eprintln!("{message}");
            1
        }
        None => 0,
    }
}

/// The recovered runtime: `Builder::new_multi_thread().enable_all().build()`
/// (`0x10021dae4` … `0x10021db54`).
fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
}

/// Do what the subcommand asks.  `Some(message)` means "print this to stderr and
/// exit 1", which is how `hydra::main` reports every failure it does not panic
/// on (`0x10021dffc`).
fn run(command: &Command) -> Option<String> {
    match command {
        Command::Replay { file, show } => replay::replay_show(file, *show).err(),
        // Additions: the reference build had neither.
        Command::Help => {
            // The same text, on stdout, with status 0.  The reference build
            // printed it to stderr and exited 1 (`0x10021df2c` … `0x10021e024`).
            println!("{}", cli::USAGE);
            None
        }
        Command::Version => {
            println!("hydra {}", cli::VERSION);
            None
        }
        // The usage path: `hydra::main` puts the 848-byte literal into the
        // message slot and falls through to the stderr print.
        Command::Usage => Some(cli::USAGE.to_string()),
        // `hydra gui …` → `connect::Target::many` + `hydra::gui::run`
        // (`0x10021deb8`, `0x10021e144`).
        Command::Gui { targets } => gui(targets),
        // The bare hub reaches the same `gui::run` in the recovered dispatch
        // (`0x10021dd40`), but this headless phase keeps it reporting rather
        // than opening a window; only the explicit `gui` word launches.
        Command::Hub
        | Command::Play { .. }
        | Command::Login { .. }
        | Command::Serve { .. }
        | Command::Import { .. }
        | Command::LookoutSeed => Some(not_wired(command)),
    }
}

/// `hydra gui …` — build the recovered hub window and hand it to eframe.
///
/// The build half ( [`gui_launch`] ) is a pure value: `NativeOptions` with the
/// recovered `"Hydra"` title, the `"hub"` placement's 660 × 520 size, the
/// 256 × 256 badge icon, `egui_wgpu::WgpuConfiguration::default()` and the
/// wgpu renderer, plus the boxed [`hydra_gui::app::Hub`].  Only
/// [`hydra_gui::app::Launch::run`] starts the platform event loop, and nothing
/// in this crate's tests calls it.
fn gui(targets: &[String]) -> Option<String> {
    let launch = gui_launch(targets);
    let (width, height) = launch.size();
    eprintln!(
        "hydra gui: opening the hub window {:?} ({width:.0}x{height:.0}, wgpu) for {} target(s)",
        launch.title,
        targets.len()
    );
    match launch.run() {
        Ok(()) => None,
        Err(err) => Some(format!("cannot open the hub window: {err}")),
    }
}

/// The window value `hydra gui` builds, split out so the tests can assert the
/// recovered plan without ever launching it.
fn gui_launch(targets: &[String]) -> hydra_gui::app::Launch {
    let data_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let character = targets.first().cloned().unwrap_or_default();
    let start = hydra_gui::app::Start::new(character, data_dir);
    hydra_gui::app::build(&start, false)
}

/// Why a recognised subcommand does not run in this phase.
fn not_wired(command: &Command) -> String {
    let needs = match command {
        Command::Hub => {
            "the window (egui/eframe: hydra_gui, hydra_ui, hydra_board, gs_render)"
        }
        Command::Play { .. } | Command::Login { .. } => {
            "a live game connection (hydra_table's connector, hydra_net's login, the Lich relay's keyed handshake)"
        }
        Command::Serve { .. } => {
            "the resting/serve window (hydra_web, hydra_table) and the Lich relay's keyed handshake"
        }
        Command::Import { .. } => "the ledger (hydra_import, hydra_ledger)",
        Command::LookoutSeed => "the data folder and the player logs (hydra_table, hydra_net)",
        _ => "nothing: it is wired",
    };
    format!(
        "{}: not wired up in this headless phase; it needs {needs}. \
         See crates/hydra/NOTES.md. `hydra replay <file>` is available.",
        command.label()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::Show;

    #[test]
    fn the_usage_path_is_the_recovered_message() {
        assert_eq!(run(&Command::Usage).unwrap(), cli::USAGE);
    }

    #[test]
    fn help_and_version_succeed_without_a_message() {
        assert_eq!(run(&Command::Help), None);
        assert_eq!(run(&Command::Version), None);
    }

    #[test]
    fn a_replay_failure_is_a_message() {
        let outcome = run(&Command::Replay {
            file: "/nonexistent/hydra/fixture.xml".into(),
            show: Show::Default,
        });
        let message = outcome.expect("a message");
        assert!(message.starts_with("/nonexistent/hydra/fixture.xml: "));
    }

    #[test]
    fn every_unwired_command_says_what_it_needs() {
        for command in [
            Command::Hub,
            Command::Play { targets: vec![] },
            Command::Login { targets: vec![] },
            Command::Serve { args: vec![] },
            Command::Import { paths: vec![] },
            Command::LookoutSeed,
        ] {
            let message = run(&command).expect("a message");
            assert!(message.contains("not wired up in this headless phase"), "{message}");
            assert!(message.contains("NOTES.md"), "{message}");
        }
        assert!(not_wired(&Command::Hub).contains("eframe"));
        assert!(not_wired(&Command::LookoutSeed).contains("hydra_table"));
    }

    #[test]
    fn the_gui_subcommand_builds_the_recovered_hub_window() {
        // `gui_launch` is the half of `gui` that builds the value; it must never
        // start an event loop, so this test can assert the recovered plan.
        let launch = gui_launch(&["Aelith".into()]);
        assert_eq!(launch.title, "Hydra");
        assert_eq!(launch.size(), (660.0, 520.0));
        assert_eq!(launch.options.renderer, eframe::Renderer::Wgpu);
        assert_eq!(launch.options.viewport.title.as_deref(), Some("Hydra"));
        let icon = launch.options.viewport.icon.as_ref().expect("the badge icon");
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
    }

    #[test]
    fn the_runtime_is_the_recovered_builder() {
        let runtime = runtime().expect("a runtime");
        // `enable_all()` really turned the time driver on.
        let elapsed = runtime.block_on(async {
            let start = tokio::time::Instant::now();
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            start.elapsed()
        });
        assert!(elapsed >= std::time::Duration::from_millis(1), "{elapsed:?}");
        runtime.shutdown_background();
    }
}
