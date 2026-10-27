//! The `inv` command: load an inventory and wait for the game to answer.
//!
//! Recovered from `hydra_table::inventory_command` (`0x101237e64`, `0x10123846c`).
//! `Load::start` is 55 KB of inlined `hydra_model` inventory parsing over a
//! `tokio` state machine; what survives cleanly is the protocol it speaks: it
//! sends `inv` and then collects the `<inv>` frames until the game is done.

use crate::travel_command;

/// The command the game is asked for.
///
/// **ESTABLISHED** (`inventory_command::Load::start`, `0x10123846c`, and
/// `inventory_command::family`, `0x1012386bc`): the family is `inv`, and the load
/// is the bare word.
pub const COMMAND: &str = "inv";

/// The line that ends an inventory listing.
///
/// **ESTABLISHED** (`Load::heard`, `0x101237e64`): the height/weight/purse summary
/// line the game prints after the items; **UNKNOWN**: the exact matcher, which is
/// inlined into 1.5 KB of comparisons, so the caller's parser decides.
pub const DONE: &str = "";

/// An inventory load in progress.
///
/// **ESTABLISHED**: the load keeps the lines it has heard (`Load::heard`) and the
/// command it sent; the parsed inventory itself is `hydra_model`'s.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Load {
    /// The lines heard so far.
    pub heard: Vec<String>,
    /// Whether the load has finished.
    pub done: bool,
}

impl Load {
    /// The command that starts a load.
    ///
    /// **ESTABLISHED** (`Load::start`, `0x10123846c`): the load sends
    /// [`COMMAND`] and then reads frames until [`DONE`].
    ///
    /// **Departure**: the original ran this on the session's `tokio` actor and
    /// parsed the frames as they arrived; this returns the command to send and an
    /// empty load for the caller to feed.
    pub fn start() -> (String, Load) {
        (COMMAND.to_string(), Load::default())
    }

    /// Take one line of the answer.
    ///
    /// **ESTABLISHED** (`Load::heard`, `0x101237e64`): every line is kept, and the
    /// load is done when the closing line arrives.
    ///
    /// **UNKNOWN**: the closing line's matcher is inlined in the binary, so a line
    /// that tripwire's the summary is left to the caller: this marks the load done
    /// only for the empty [`DONE`] sentinel the caller can pass in.
    pub fn heard(&mut self, line: &str) -> bool {
        self.heard.push(line.to_string());
        let done = !DONE.is_empty() && line.trim() == DONE;
        self.done |= done;
        done
    }

    /// The line `.go2` uses to talk about the load this character is running.
    ///
    /// **ESTABLISHED**: `travel_command::CANNOT_WALK` is what a walk says when the
    /// character it would move is busy, which is what a running load makes it.
    pub fn busy(&self) -> Option<&'static str> {
        if self.done {
            None
        } else {
            Some(travel_command::CANNOT_WALK)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_load_starts_with_the_inv_command() {
        let (command, load) = Load::start();
        assert_eq!(command, "inv");
        assert!(load.heard.is_empty());
        assert!(!load.done);
    }

    #[test]
    fn the_lines_are_kept_in_order() {
        let (_, mut load) = Load::start();
        assert!(!load.heard("You are holding a brass ring."));
        assert!(!load.heard("You have 10 coins."));
        assert_eq!(load.heard.len(), 2);
        assert_eq!(load.heard[0], "You are holding a brass ring.");
        assert_eq!(load.busy(), Some(travel_command::CANNOT_WALK));
    }
}
