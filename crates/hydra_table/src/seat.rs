//! Stand-ins for `hydra_board`'s seat and session board.
//!
//! `hydra_board` — `sessions::Sessions`, `sessions::Glance`, `seat::Seat` — is
//! **not** part of this reconstruction, but `hydra_table` uses it constantly:
//!
//! ```text
//! 0x1013eacf0  hydra_board::sessions::Sessions::glance       (now)   -> Glance
//! 0x1013e9cc4  hydra_board::sessions::Sessions::open_windows  (kind, id)
//! 0x1013ea648  hydra_board::sessions::Sessions::close         ()
//! 0x1013e651c  hydra_board::seat::Seat::keep_window           (bool)
//! ```
//!
//! The two types below carry exactly what those call sites read: a session's
//! account/character/game (`Glance`'s 0x30-byte elements hold two `String`s, the
//! `drop_in_place<hydra_board::sessions::Glance>` symbol at `0x101296590`), and a
//! seat's name, id (`Table::show` reads `[seat, #0xaa0]`) and kept-window flag.
//!
//! **UNKNOWN**: everything else about the board.  The methods here keep the
//! recovered *effects* that `hydra_table` can observe (who is live, which window
//! is open) and do no drawing.

use std::sync::Mutex;
use std::time::Instant;

/// One live session, as `Sessions::glance` reports it.
///
/// **ESTABLISHED**: the element stride is `0x30` and it holds a `String` at `+0`
/// and another at `+0x18` (`0x101292560`, `0x101292574`), which the reward claim
/// code turns into `(String, String)` pairs
/// (`drop_in_place<Vec<(String, String)>>`, `0x101297a4c`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Live {
    /// The account the session logged in as.
    pub account: String,
    /// The character it is playing.
    pub character: String,
    /// The game code, when the board knows it.
    pub game_code: String,
}

/// A seat in the hub: one character's window.
///
/// **ESTABLISHED**: `Table::show` reads the id at `[seat, #0xaa0]` and calls
/// `Seat::keep_window(seat, true)`; the name is the seat's identity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seat {
    /// The character this seat shows.
    pub name: String,
    /// The seat's id, read at `+0xaa0`.
    pub id: u32,
    /// Whether the window is kept open (`keep_window`).
    pub kept: bool,
}

/// The session board.
///
/// **ESTABLISHED**: 0x40 bytes (`Table::with_game_runtime` copies exactly 0x40
/// bytes at `+0`), and the three methods called above.  **INFERRED**: the live
/// session list is held here.
#[derive(Debug, Default)]
pub struct Sessions {
    live: Mutex<Vec<Live>>,
    /// Which window kinds are open, as `open_windows` last set them.
    pub windows: Mutex<Vec<u32>>,
}

impl Sessions {
    /// A board with no sessions.
    pub fn new() -> Sessions {
        Sessions::default()
    }

    /// The sessions live at `now`.
    ///
    /// **ESTABLISHED** (`0x1013eacf0`): takes `Instant::now` and answers a
    /// `Glance`, which is dropped right after it is read
    /// (`drop_in_place<hydra_board::sessions::Glance>`).
    pub fn glance(&self, _now: Instant) -> Vec<Live> {
        self.live.lock().unwrap().clone()
    }

    /// Record a session as live.
    pub fn add(&self, live: Live) {
        self.live.lock().unwrap().push(live);
    }

    /// Close every session (`0x1013ea648`).
    pub fn close(&self) {
        self.live.lock().unwrap().clear();
    }

    /// Open the windows of `kind`, with `id` in front (`0x1013e9cc4`).
    ///
    /// **ESTABLISHED**: the call passes `2` and the seat id; **UNKNOWN**: what the
    /// board did with them, so the kinds are recorded here.
    pub fn open_windows(&self, kind: u32, id: u32) -> Vec<u32> {
        let mut windows = self.windows.lock().unwrap();
        windows.push(kind);
        windows.push(id);
        windows.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_board_reports_what_was_added() {
        let sessions = Sessions::new();
        assert!(sessions.glance(Instant::now()).is_empty());
        sessions.add(Live {
            account: "main".to_string(),
            character: "Aelith".to_string(),
            game_code: "GS4".to_string(),
        });
        let live = sessions.glance(Instant::now());
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].character, "Aelith");
        sessions.close();
        assert!(sessions.glance(Instant::now()).is_empty());
    }

    #[test]
    fn a_window_kind_and_id_are_recorded() {
        let sessions = Sessions::new();
        assert_eq!(sessions.open_windows(2, 4312), vec![2, 4312]);
    }
}
