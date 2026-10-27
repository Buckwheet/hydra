//! Working a course across the ocean.
//!
//! Both entry points return `Option<`[`Course`]`>` and share one search: a
//! shortest-path tree over the state `(room, heading)`, where `heading` is the
//! direction the ship is holding, plus one extra state per room (`8`) for "not
//! holding any heading yet". That ninth state is visible in `course_holding`
//! itself: the distance table is allocated as `len * 36` bytes
//! (`0x1015a7350`: `mov w8, #0x24` … `madd x8, x21, x8, x19`), i.e. nine `i32`
//! per room, and the entry point seeds `dist[from][held] = 0` with
//! `str wzr, [x8, x20, lsl #2]`, where `x20 = arg2 & 0xff`.
//!
//! The frontier is a binary heap of 24-byte `(cost, room, heading)` records
//! (`0x1015a73bc`..`0x1015a7404` is the sift-up), and the answer is reassembled
//! from the predecessor chain into
//!
//! ```text
//! Course { runs: Vec<(u32, Heading)>, rooms: Vec<u64> }
//! ```
//!
//! 48 bytes at the sret: an 8-byte run `(count: u32, heading: u8)` repeated, and
//! a `Vec<u64>` of room uids. `runs` is a run-length encoding of the headings
//! between consecutive rooms — the merge loop at `0x1015a78b8` increments the
//! previous run's count when the heading is unchanged, and starts a new run with
//! count `1` otherwise.
//!
//! **Partly recovered.** What is *not* recoverable from the instruction stream
//! is the cost the search minimises: `course` is 16 936 bytes, almost all of it
//! inlined search, and its comparison closure was not dumped. This build
//! minimises `(moves, turns, ...)`, which reproduces the shape of the answer (a
//! `runs` list of straight sails plus one `rooms` entry per move) without
//! claiming to be the original metric.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::ocean::{Heading, Ocean};

/// The heading state that means "no heading held yet". It is one past the last
/// [`Heading`] and is the reason the distance table is nine slots wide.
const FREE: u8 = 8;

/// A course across the water.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Course {
    /// `(how many moves, heading)` for each straight run, in order.
    pub runs: Vec<(u32, Heading)>,
    /// The rooms the course passes through, starting at the origin.
    pub rooms: Vec<u64>,
}

impl Course {
    /// The headings, one per move, in order.
    pub fn headings(&self) -> Vec<Heading> {
        let mut out = Vec::new();
        for (count, h) in &self.runs {
            for _ in 0..*count {
                out.push(*h);
            }
        }
        out
    }

    /// The total number of moves.
    pub fn length(&self) -> usize {
        self.runs.iter().map(|(c, _)| *c as usize).sum()
    }
}

/// The search's cost: moves first, then how often the ship came about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Cost {
    moves: u32,
    turns: u32,
}

impl Ocean {
    /// @0x1015a7a90 — a course from room `from` to room `to` with no heading
    /// held at the start (the `FREE` state).
    pub fn course(&self, from: u64, to: u64) -> Option<Course> {
        self.course_holding(from, None, to)
    }

    /// @0x1015a6fc8 — a course from `from` to `to` starting with `held` already
    /// set: the ship keeps to that heading until it comes about.
    ///
    /// The original's arguments are `(&self, from: u64, held, to: u64)`: the
    /// origin and the destination are both looked up in the
    /// `HashMap<u64, usize>` at `+0x48` (`0x1015a700c`, `0x1015a70b4`), and the
    /// third is masked to a byte (`and x20, x23, #0xff`, `0x1015a7344`) and used
    /// as the second index of the distance row. `None` is that ninth state.
    pub fn course_holding(&self, from: u64, held: Option<Heading>, to: u64) -> Option<Course> {
        let index: std::collections::HashMap<u64, usize> =
            self.rooms.iter().enumerate().map(|(i, r)| (r.uid, i)).collect();
        let start = *index.get(&from)?;
        let goal = *index.get(&to)?;
        let held = held.map(|h| h as u8).unwrap_or(FREE);

        let width = 9usize;
        let n = self.rooms.len();
        let mut dist = vec![Cost { moves: u32::MAX, turns: u32::MAX }; n * width];
        let mut prev: Vec<Option<(usize, u8)>> = vec![None; n * width];
        let mut heap: BinaryHeap<Reverse<(Cost, u8, usize)>> = BinaryHeap::new();

        dist[start * width + held as usize] = Cost { moves: 0, turns: 0 };
        heap.push(Reverse((Cost { moves: 0, turns: 0 }, held, start)));

        while let Some(Reverse((cost, h, room_ix))) = heap.pop() {
            let slot = room_ix * width + h as usize;
            if cost > dist[slot] {
                continue;
            }
            let room = &self.rooms[room_ix];
            // A ship that is already under way must use the heading it holds
            // for its first move; after that it may come about. Without this the
            // ninth state would be unobservable: a turn costs less than a move,
            // so every course would start in whatever heading it liked.
            let held_first = room_ix == start && held != FREE;
            for next_h in 0..8u8 {
                if held_first && next_h != held {
                    continue;
                }
                let heading = Heading::of(next_h).expect("0..8 is a heading");
                if !self.runs_true(room, heading) {
                    continue;
                }
                let Some(uid) = room.neighbour(heading) else {
                    continue;
                };
                let Some(&next_ix) = index.get(&uid) else {
                    continue;
                };
                let turn = u32::from(h != FREE && h != next_h);
                let next = Cost {
                    moves: cost.moves + 1,
                    turns: cost.turns + turn,
                };
                let next_slot = next_ix * width + next_h as usize;
                if next < dist[next_slot] {
                    dist[next_slot] = next;
                    prev[next_slot] = Some((room_ix, h));
                    heap.push(Reverse((next, next_h, next_ix)));
                }
            }
        }

        // The cheapest state at the destination, ties going to the lowest
        // heading index (which is the order the search walks them in).
        let mut best: Option<(Cost, u8)> = None;
        for h in 0..width as u8 {
            let cost = dist[goal * width + h as usize];
            if cost.moves == u32::MAX {
                continue;
            }
            if best.map_or(true, |(b, _)| cost < b) {
                best = Some((cost, h));
            }
        }
        let (_, goal_h) = best?;

        // Walk the predecessor chain back to the start, collecting the heading
        // held when each room was reached (the origin has none).
        let mut steps: Vec<(usize, u8)> = vec![(goal, goal_h)];
        let mut at = (goal, goal_h);
        while let Some((room_ix, h)) = prev[at.0 * width + at.1 as usize] {
            steps.push((room_ix, h));
            at = (room_ix, h);
        }
        steps.reverse();

        let mut course = Course::default();
        for (i, (room_ix, h)) in steps.iter().enumerate() {
            course.rooms.push(self.rooms[*room_ix].uid);
            // The origin has no arrival heading; every later room has one, and
            // it is the heading of the move that arrived.
            if i == 0 || *h == FREE {
                continue;
            }
            let heading = Heading::of(*h).expect("0..8 is a heading");
            match course.runs.last_mut() {
                Some((count, last)) if *last == heading => *count += 1,
                _ => course.runs.push((1, heading)),
            }
        }
        Some(course)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocean::OceanRoom;

    /// A 3x3 block of open sea on rows 10..=12, columns 10..=12, wired up so
    /// every neighbour link is symmetric. Uids are `row * 100 + col`, so
    /// `1012` is row 10, column 12.
    fn sea() -> Ocean {
        let uid_at = |col: i32, row: i32| -> Option<u64> {
            if (10..=12).contains(&col) && (10..=12).contains(&row) {
                Some(row as u64 * 100 + col as u64)
            } else {
                None
            }
        };
        let mut rooms = Vec::new();
        for row in 10..=12 {
            for col in 10..=12 {
                let mut neighbours = [None; 8];
                for h in Heading::ALL {
                    let (dc, dr) = h.delta();
                    neighbours[h as usize] = uid_at(col + dc, row + dr);
                }
                rooms.push(OceanRoom {
                    neighbours,
                    title: "Open Sea".into(),
                    sea: "Test Sea".into(),
                    uid: row as u64 * 100 + col as u64,
                    col,
                    row,
                });
            }
        }
        Ocean::from_rooms(rooms)
    }

    #[test]
    fn runs_true_holds_across_the_grid() {
        let o = sea();
        let r = o.room(1010).unwrap();
        assert!(o.runs_true(r, Heading::East));
        assert!(o.runs_true(r, Heading::Southeast));
        assert!(!o.runs_true(r, Heading::North), "no cell north of row 10");
        assert!(!o.runs_true(o.room(1212).unwrap(), Heading::South));
    }

    #[test]
    fn a_straight_course_is_one_run() {
        let o = sea();
        let c = o.course(1010, 1012).unwrap();
        assert_eq!(c.rooms, vec![1010, 1011, 1012]);
        assert_eq!(c.runs, vec![(2, Heading::East)]);
        assert_eq!(c.length(), 2);
        assert_eq!(c.headings(), vec![Heading::East, Heading::East]);
    }

    #[test]
    fn a_diagonal_course_is_also_one_run() {
        let o = sea();
        let c = o.course(1010, 1212).unwrap();
        assert_eq!(c.rooms, vec![1010, 1111, 1212]);
        assert_eq!(c.runs, vec![(2, Heading::Southeast)]);
    }

    #[test]
    fn holding_a_heading_forces_the_first_move() {
        let o = sea();
        // From (10,10) to (11,11) east is held, so the course must go east
        // first and then come about.
        let c = o.course_holding(1010, Some(Heading::East), 1111).unwrap();
        assert_eq!(c.rooms, vec![1010, 1011, 1111]);
        assert_eq!(c.runs, vec![(1, Heading::East), (1, Heading::South)]);
        assert_eq!(c.length(), 2);

        // The same trip with no heading held is a single south-east run.
        let free = o.course(1010, 1111).unwrap();
        assert_eq!(free.runs, vec![(1, Heading::Southeast)]);
    }

    #[test]
    fn an_off_grid_destination_has_no_course() {
        let o = sea();
        assert!(o.course(1010, 9999).is_none());
        assert!(o.course(9999, 1010).is_none());
        // Every corner of the block is reachable from every other.
        for from in [1010u64, 1012, 1210, 1212] {
            for to in [1010u64, 1012, 1210, 1212] {
                assert!(o.course(from, to).is_some(), "{from} -> {to}");
            }
        }
    }
}
