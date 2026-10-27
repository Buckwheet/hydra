//! The room graph a survey walk builds up, and the compass step that walks it.
//!
//! Evidence (addresses are unslid VAs; see `NOTES.md` for the method):
//!
//! * [`step`] `@0x1013cafb8` — a match on the direction's byte length (1, 2 and
//!   4), with the deltas loaded from tables at `0x1020734a8` (`e` = `(1,0)`),
//!   `0x1020784a0` (`s` = `(0,1)`), `0x102181c30` (`ne` = `(1,-1)`) and
//!   `0x102157030` (`sw` = `(-1,1)`), plus `movi` immediates for the rest. The
//!   result is written as `Some` at `+0` of a 16-byte sret, a pair of `i32` at
//!   `+4` and a third `i32` at `+0xc` (`up` = `1`, `down` = `-1`).
//! * [`Graph`] — `drop_in_place<Graph>` `@0x1013cbf2c` drops a
//!   `Vec<(String, String, String)>` at `+0x00` (72-byte elements), a hash table
//!   at `+0x18` and a `BTreeMap` at `+0x48`; `from_records` stores a `u32` at
//!   `+0x60`. `Graph::untried` reads the map's root at `+0x48`, its height at
//!   `+0x50` and its length at `+0x58`.
//! * [`Known`] — `drop_in_place<Known>` `@0x1013cc9dc`: a `String` at `+0x00`, a
//!   [`Seen`] at `+0x18`, a `Vec<String>` at `+0x78`, an `Option<Via>` at
//!   `+0x90` (niche in the first `String`'s capacity slot), a 16-byte
//!   `Option<(i32, i32, i32)>` at `+0xc0` and a `BTreeSet`-shaped map at
//!   `+0xd0` (its node's children start at `+0x118`, i.e. its value type is
//!   zero-sized). 232 bytes in all.
//! * [`Seen`] — `drop_in_place<Seen>` `@0x1013cbe58`: two 1-byte-element
//!   `Vec`s, a `Vec<String>` and an `Option<String>`; 0x60 bytes.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::log::{Body, EdgeResult, Record, Via};

/// The step a direction word takes on the map.
///
/// The three `i32`s come out of [`step`]'s sret: `(dx, dy)` for the compass
/// grid and a third for the levels `up` and `down` climb.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Step {
    pub dx: i32,
    pub dy: i32,
    pub dz: i32,
}

/// The compass step for a direction word, if it is one.
///
/// `@0x1013cafb8`. Exactly these words are recognised — `n s e w`, `ne se sw
/// nw`, `up`, `down`; every other input (including `north`, and every 3-byte
/// word) is `None`, because the function dispatches on the byte length alone.
pub fn step(dir: &str) -> Option<Step> {
    let step = match dir {
        "n" => Step {
            dx: 0,
            dy: -1,
            dz: 0,
        },
        "s" => Step {
            dx: 0,
            dy: 1,
            dz: 0,
        },
        "e" => Step {
            dx: 1,
            dy: 0,
            dz: 0,
        },
        "w" => Step {
            dx: -1,
            dy: 0,
            dz: 0,
        },
        "ne" => Step {
            dx: 1,
            dy: -1,
            dz: 0,
        },
        "se" => Step {
            dx: 1,
            dy: 1,
            dz: 0,
        },
        "sw" => Step {
            dx: -1,
            dy: 1,
            dz: 0,
        },
        "nw" => Step {
            dx: -1,
            dy: -1,
            dz: 0,
        },
        "up" => Step {
            dx: 0,
            dy: 0,
            dz: 1,
        },
        "down" => Step {
            dx: 0,
            dy: 0,
            dz: -1,
        },
        _ => return None,
    };
    Some(step)
}

/// What a survey has seen in a room.
///
/// The shape is recovered from the binary (0x60 bytes: two `Vec`s of a
/// one-byte, allocation-free element at `+0x00` and `+0x18`, a `Vec<String>` at
/// `+0x30` and an `Option<String>` at `+0x48`), but the crate that owned the
/// type never receives or returns one, and its field names are not in the
/// binary. The names here are ours; `// UNKNOWN:` in `NOTES.md`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Seen {
    /// One-byte marks (the first `Vec`).
    pub marks: Vec<u8>,
    /// One-byte marks (the second `Vec`).
    pub flags: Vec<u8>,
    /// Text lines seen in the room.
    pub lines: Vec<String>,
    /// The last thing seen, when it is a single word.
    pub last: Option<String>,
}

/// What the walk knows about one room.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Known {
    /// The room's session id — the key the room is filed under.
    pub sid: String,
    /// What has been seen in the room so far.
    pub seen: Seen,
    /// The room's exits, as the game names them (`north`, `northeast`, ...).
    pub exits: Vec<String>,
    /// How the walk got here, when the log recorded it.
    pub via: Option<Via>,
    /// The room's map cell and level, when the game sent one.
    pub pos: Option<(i32, i32, i32)>,
    /// The exits already attempted.
    pub tried: BTreeSet<String>,
}

/// The rooms a survey log has walked, and the moves between them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Graph {
    /// Every move that arrived: `(from sid, direction, to sid)`.
    pub edges: Vec<(String, String, String)>,
    /// Map cell → level, from the rooms' `pos`.
    pub levels: HashMap<(i32, i32), i32>,
    /// The rooms, by session id.
    pub rooms: BTreeMap<String, Known>,
    /// The highest number any `sid` in the log spelled.
    pub highest: u32,
}

impl Graph {
    /// Build the graph from a log's records.
    ///
    /// `@0x1013cb194`. The loop is recovered in outline: it walks the records in
    /// order and switches on the body's variant, which the binary reads as a
    /// 64-bit discriminant at `+0x60` of each 232-byte `Record`:
    ///
    /// * **room** (`Some` = `1`): the `sid` is parsed as a number after leading
    ///   `s` bytes are skipped (`str::trim_start_matches('s')` + `from_str` at
    ///   `0x1013cb2e8`), and the largest such number is kept; a `pos` inserts the
    ///   cell into the graph's hash table (key = the `(x, y)` pair, value = the
    ///   level); the room is cloned into a [`Known`] and inserted into the
    ///   `BTreeMap` under its `sid` (`BTreeMap::insert` at `0x1013c0b18`).
    /// * **verb**: the lines are classified; a verb whose lines read as movement
    ///   is skipped, otherwise the text is kept with the room.
    /// * **edge**: the room the move started from is looked up by `from`, the
    ///   attempted command is matched against the room's exits
    ///   (`0x1013cb43c`), and the outcome is read from the record (`0x1013cb45c`
    ///   loads the result byte; `0` = arrived, `1` = failed, `2` = stayed).
    /// * **event**: nothing.
    ///
    /// `// UNKNOWN:` what exactly the failed and stayed paths store; this build
    /// records the attempt in [`Known::tried`] and files the move in
    /// [`Graph::edges`] only when it arrived.
    pub fn from_records(records: &[Record]) -> Graph {
        let mut graph = Graph::default();
        for record in records {
            match &record.body {
                Body::Room(room) => {
                    if let Some(number) = sid_number(&room.sid) {
                        graph.highest = graph.highest.max(number);
                    }
                    if let Some((x, y, level)) = room.pos {
                        graph.levels.insert((x, y), level);
                    }
                    // A room seen twice keeps what was already tried there.
                    let mut known = graph.rooms.remove(&room.sid).unwrap_or_default();
                    known.sid = room.sid.clone();
                    known.exits = room.exits.clone();
                    known.via = room.via.clone();
                    known.pos = room.pos;
                    graph.rooms.insert(room.sid.clone(), known);
                }
                Body::Verb(verb) => {
                    // A verb that moved the character is an edge in disguise.
                    if verb.lines.iter().any(|line| reads_as_movement(line)) {
                        continue;
                    }
                    if let Some(known) = graph.rooms.get_mut(&verb.sid) {
                        known.seen.lines.extend(verb.lines.iter().cloned());
                        known.seen.last = Some(verb.verb.clone());
                    }
                }
                Body::Edge(edge) => {
                    let Some(known) = graph.rooms.get_mut(&edge.from) else {
                        continue;
                    };
                    let attempted = match &edge.dir {
                        Some(dir) => dir.clone(),
                        None => edge.cmd.clone(),
                    };
                    // The binary only reads the outcome once the command is one
                    // of the room's known exits.
                    if !known.exits.is_empty() && !known.exits.iter().any(|exit| *exit == attempted || *exit == edge.cmd) {
                        continue;
                    }
                    known.tried.insert(attempted.clone());
                    if edge.result == EdgeResult::Arrived {
                        if let Some(to) = &edge.to {
                            graph
                                .edges
                                .push((edge.from.clone(), attempted, to.clone()));
                        }
                    }
                }
                Body::Event(_) => {}
            }
        }
        graph
    }

    /// What the walk knows about the room filed under `sid`.
    ///
    /// `@0x1013cba80`: a `BTreeMap::get` over the graph's map, walking down from
    /// the root node (`+0x48`, its height at `+0x50`) and comparing keys with
    /// `memcmp`, so the answer is `None` for an unknown room.
    pub fn room(&self, sid: &str) -> Option<&Known> {
        self.rooms.get(sid)
    }

    /// How many exits the walk has not yet attempted.
    ///
    /// `@0x1013cbb5c`: for every known room the binary iterates the room's
    /// `Vec<String>` of exits and tests each against the room's set of tried
    /// names (a `BTreeSet`, whose node's children begin at `+0x118`), counting
    /// the ones the set does not hold, and returns the sum as a `usize` (the
    /// empty graph returns `0`).
    pub fn untried(&self) -> usize {
        self.rooms
            .values()
            .map(|known| {
                known
                    .exits
                    .iter()
                    .filter(|exit| !known.tried.contains(*exit))
                    .count()
            })
            .sum()
    }
}

/// The number a room `sid` spells: the binary skips leading `s` bytes and then
/// reads decimal digits (`0x1013cb2e8`, with the overflow checks at
/// `0x1013cb574`).
fn sid_number(sid: &str) -> Option<u32> {
    sid.trim_start_matches('s').parse::<u32>().ok()
}

/// Whether a verb's answer looks like the character moving.
///
/// The binary calls `hydra_model::movement::classify` (`0x101b94dd0`) per line
/// and keeps the verb when the classification is one of its "movement" codes
/// (`0x17`, or `8`/`9`) — see `0x1013cb510`. `// UNKNOWN:` those codes and that
/// classifier; a line whose last word is a compass word is used here, which also
/// accepts the long spellings (`north`, `northeast`, ...) that [`step`] alone
/// does not.
fn reads_as_movement(line: &str) -> bool {
    line.split_whitespace()
        .next_back()
        .map(|word| {
            let word = word.trim_matches(|c: char| !c.is_ascii_alphabetic());
            compass_step(&word.to_ascii_lowercase()).is_some()
        })
        .unwrap_or(false)
}

/// [`step`] for the long spellings as well as the short ones.
fn compass_step(word: &str) -> Option<Step> {
    let short = match word {
        "north" => "n",
        "south" => "s",
        "east" => "e",
        "west" => "w",
        "northeast" => "ne",
        "southeast" => "se",
        "southwest" => "sw",
        "northwest" => "nw",
        other => other,
    };
    step(short)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::{parse, EdgeRecord, EventRecord, Happening, RoomRecord, VerbRecord};

    fn line(body: &str) -> Record {
        let text = format!(
            r#"{{"t":"1","run":"run-1","char":"Ashryn","server":"Prime",{}}}"#,
            body
        );
        parse(&text).unwrap()
    }

    fn room(sid: &str, exits: &[&str], pos: Option<(i32, i32, i32)>, via: Option<(&str, &str)>) -> Record {
        let exits: Vec<String> = exits.iter().map(|exit| format!("{:?}", exit)).collect();
        let pos = match pos {
            Some((x, y, z)) => format!(r#","pos":[{},{},{}]"#, x, y, z),
            None => String::new(),
        };
        let via = match via {
            Some((from, cmd)) => format!(r#","via":{{"from":{:?},"cmd":{:?}}}"#, from, cmd),
            None => String::new(),
        };
        line(&format!(
            r#""kind":"room","sid":{:?},"title":"T","desc":"D","paths":"","compass":[],"exits":[{}],"roommeta":{{}},"animals":[],"tracked":[]{}{}"#,
            sid,
            exits.join(","),
            pos,
            via
        ))
    }

    fn edge(from: &str, cmd: &str, dir: Option<&str>, to: Option<&str>, result: &str) -> Record {
        let dir = match dir {
            Some(dir) => format!(r#","dir":{:?}"#, dir),
            None => String::new(),
        };
        let to = match to {
            Some(to) => format!(r#","to":{:?}"#, to),
            None => String::new(),
        };
        line(&format!(
            r#""kind":"edge","from":{:?},"cmd":{:?}{}{},"rt":null,"result":{:?},"lines":[]"#,
            from, cmd, dir, to, result
        ))
    }

    fn verb(sid: &str, verb: &str, lines: &[&str]) -> Record {
        let lines: Vec<String> = lines.iter().map(|text| format!("{:?}", text)).collect();
        line(&format!(
            r#""kind":"verb","sid":{:?},"verb":{:?},"lines":[{}],"rt":0,"cast_rt":0,"prompt_closed":false"#,
            sid,
            verb,
            lines.join(",")
        ))
    }

    fn event(sid: &str, what: &str) -> Record {
        line(&format!(
            r#""kind":"event","sid":{:?},"what":{:?}"#,
            sid, what
        ))
    }

    #[test]
    fn step_reads_the_compass_table() {
        for (word, want) in [
            ("n", (0, -1, 0)),
            ("s", (0, 1, 0)),
            ("e", (1, 0, 0)),
            ("w", (-1, 0, 0)),
            ("ne", (1, -1, 0)),
            ("se", (1, 1, 0)),
            ("sw", (-1, 1, 0)),
            ("nw", (-1, -1, 0)),
            ("up", (0, 0, 1)),
            ("down", (0, 0, -1)),
        ] {
            assert_eq!(
                step(word),
                Some(Step {
                    dx: want.0,
                    dy: want.1,
                    dz: want.2
                }),
                "{}",
                word
            );
        }
    }

    #[test]
    fn step_rejects_what_the_binary_rejects() {
        // The match is on the byte length alone: words and 3-byte input fall out.
        for word in ["north", "south", "northeast", "", "u", "ne ", "NE", "dn", "out"] {
            assert_eq!(step(word), None, "{}", word);
        }
    }

    #[test]
    fn steps_add_up_to_a_cell() {
        let mut cell = (0, 0, 0);
        for word in ["n", "ne", "e", "se", "s", "sw", "w", "nw"] {
            let step = step(word).unwrap();
            cell.0 += step.dx;
            cell.1 += step.dy;
            cell.2 += step.dz;
        }
        assert_eq!(cell, (0, 0, 0), "a full turn of the compass comes home");
        let up = step("up").unwrap();
        let down = step("down").unwrap();
        assert_eq!((up.dz, down.dz), (1, -1));
        assert_eq!((up.dx, up.dy, down.dx, down.dy), (0, 0, 0, 0));
    }

    #[test]
    fn builds_rooms_exits_and_positions() {
        let records = [
            room("s1710", &["north", "down"], Some((12, -4, 0)), Some(("s1709", "south"))),
            room("s2318", &["south", "up"], Some((12, -5, 0)), Some(("s1710", "north"))),
        ];
        let graph = Graph::from_records(&records);
        assert_eq!(graph.rooms.len(), 2);
        assert_eq!(graph.highest, 2318);

        let first = graph.room("s1710").expect("known room");
        assert_eq!(first.sid, "s1710");
        assert_eq!(first.exits, ["north", "down"]);
        assert_eq!(
            first.via,
            Some(Via {
                from: "s1709".to_owned(),
                cmd: "south".to_owned()
            })
        );
        assert_eq!(first.pos, Some((12, -4, 0)));
        assert!(first.tried.is_empty());

        assert_eq!(graph.levels.get(&(12, -4)), Some(&0));
        assert_eq!(graph.levels.get(&(12, -5)), Some(&0));
        assert_eq!(graph.levels.len(), 2);

        assert!(graph.room("s9999").is_none());
        assert_eq!(graph.untried(), 4, "nothing attempted yet");
    }

    #[test]
    fn an_arrived_edge_is_filed_and_marks_the_exit_tried() {
        let records = [
            room("s1710", &["north", "down"], None, None),
            edge("s1710", "north", Some("north"), Some("s2318"), "arrived"),
        ];
        let graph = Graph::from_records(&records);
        assert_eq!(
            graph.edges,
            [(
                "s1710".to_owned(),
                "north".to_owned(),
                "s2318".to_owned()
            )]
        );
        let known = graph.room("s1710").unwrap();
        assert_eq!(known.exits, ["north", "down"]);
        assert!(known.tried.contains("north"));
        assert_eq!(graph.untried(), 1, "only `down` is still untried");
    }

    #[test]
    fn failed_and_stayed_edges_are_tried_but_not_edges() {
        let records = [
            room("s1710", &["north"], None, None),
            edge("s1710", "north", Some("north"), None, "failed"),
            edge("s1710", "north", Some("north"), None, "stayed"),
        ];
        let graph = Graph::from_records(&records);
        assert!(graph.edges.is_empty(), "no move arrived");
        assert!(graph.room("s1710").unwrap().tried.contains("north"));
        assert_eq!(graph.untried(), 0);
    }

    #[test]
    fn an_edge_from_an_unknown_room_is_ignored() {
        let records = [
            room("s1710", &["north"], None, None),
            edge("s1700", "north", Some("north"), Some("s1710"), "arrived"),
        ];
        let graph = Graph::from_records(&records);
        assert!(graph.edges.is_empty());
        assert_eq!(graph.untried(), 1);
    }

    #[test]
    fn an_edge_that_is_not_an_exit_is_ignored() {
        let records = [
            room("s1710", &["north"], None, None),
            edge("s1710", "sing", Some("sing"), Some("s1"), "arrived"),
        ];
        let graph = Graph::from_records(&records);
        assert!(graph.edges.is_empty());
        assert!(graph.room("s1710").unwrap().tried.is_empty());
    }

    #[test]
    fn untried_counts_only_the_exits_no_edge_attempted() {
        let records = [
            room("s1", &["north", "south", "east"], None, None),
            room("s2", &["up"], None, None),
            edge("s1", "north", Some("north"), Some("s2"), "arrived"),
            edge("s1", "south", Some("south"), None, "failed"),
        ];
        let graph = Graph::from_records(&records);
        // s1 still has `east`; s2 has its `up`.
        assert_eq!(graph.untried(), 2);
    }

    #[test]
    fn a_revisited_room_keeps_what_was_tried_and_takes_the_new_exits() {
        let records = [
            room("s1", &["north"], None, None),
            edge("s1", "north", Some("north"), Some("s2"), "arrived"),
            room("s1", &["north", "down"], None, None),
        ];
        let graph = Graph::from_records(&records);
        let known = graph.room("s1").unwrap();
        assert_eq!(known.exits, ["north", "down"]);
        assert!(known.tried.contains("north"));
        assert_eq!(graph.untried(), 1);
    }

    #[test]
    fn verbs_are_kept_with_the_room_unless_they_moved() {
        let records = [
            room("s1", &["north"], None, None),
            verb("s1", "llook", &["You see a fountain."]),
            verb("s1", "lnorth", &["You walk north."]),
            verb("s9", "lwave", &["You wave."]),
        ];
        let graph = Graph::from_records(&records);
        let known = graph.room("s1").unwrap();
        assert_eq!(known.seen.lines, ["You see a fountain."]);
        assert_eq!(known.seen.last.as_deref(), Some("llook"));
        assert!(graph.room("s9").is_none(), "no room to hang the verb on");
    }

    #[test]
    fn events_leave_the_graph_alone() {
        let records = [
            room("s1", &["north"], None, None),
            event("s1", "died"),
        ];
        let graph = Graph::from_records(&records);
        assert_eq!(graph.rooms.len(), 1);
        assert_eq!(graph.untried(), 1);
    }

    #[test]
    fn rooms_are_ordered_by_sid() {
        let records = [
            room("s30", &[], None, None),
            room("s4", &[], None, None),
            room("s100", &[], None, None),
        ];
        let graph = Graph::from_records(&records);
        let sids: Vec<&str> = graph.rooms.keys().map(String::as_str).collect();
        assert_eq!(sids, ["s100", "s30", "s4"], "a BTreeMap orders by bytes");
    }

    #[test]
    fn an_empty_log_makes_an_empty_graph() {
        let graph = Graph::from_records(&[]);
        assert_eq!(graph, Graph::default());
        assert_eq!(graph.untried(), 0);
        assert!(graph.room("s1").is_none());
    }

    #[test]
    fn a_sid_without_a_number_still_files_the_room() {
        let graph = Graph::from_records(&[room("wehnimer", &["north"], None, None)]);
        assert_eq!(graph.highest, 0);
        assert!(graph.room("wehnimer").is_some());
    }

    #[test]
    fn the_record_types_round_trip_through_the_builders() {
        // The builders above only exist for the tests; make sure they really
        // produced the record kinds they were asked for.
        assert!(matches!(room("s1", &[], None, None).body, Body::Room(boxed) if boxed.sid == "s1"));
        assert!(matches!(edge("s1", "n", None, None, "arrived").body, Body::Edge(EdgeRecord { result: EdgeResult::Arrived, .. })));
        assert!(matches!(verb("s1", "v", &[]).body, Body::Verb(VerbRecord { .. })));
        assert!(matches!(event("s1", "died").body, Body::Event(EventRecord { what: Happening::Died, .. })));
        let _: RoomRecord = RoomRecord::default();
    }
}
