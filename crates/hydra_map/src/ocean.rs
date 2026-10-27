//! The ocean: the sea grid the ships sail and the ports beside it.
//!
//! The data is the tab-separated pair of tables the binary carries. Two
//! literals pin the columns down (`0x102422e7b`, `0x102422e31`):
//!
//! ```text
//! uid  ocean  sea  col  row  n  ne  e  se  s  sw  w  nw      (13 fields)
//! 7116001  The Tenebrous Cauldron  North Blue  10  53  7116224  7116225 ...
//! 7142002  Sleeping Drake Harbor  Sleeping Drake Harbor, Pier One   (3 fields)
//! ```
//!
//! [`Heading`] is the eight-way compass, clockwise from north; `Heading::named`
//! (`0x1015a6c50`) accepts the full name, the two-letter abbreviation and the
//! single letter, after `trim()` and `to_ascii_lowercase()`. Each heading has a
//! cell delta, read out of the two `i32` tables `runs_true` uses
//! (`0x102422c28`, `0x102422c48`):
//!
//! ```text
//! n (0,-1)  ne (1,-1)  e (1,0)  se (1,1)  s (0,1)  sw (-1,1)  w (-1,0)  nw (-1,-1)
//! ```

use std::collections::HashMap;
use std::sync::OnceLock;

/// One of the eight sailing directions, clockwise from north.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Heading {
    #[default]
    North = 0,
    Northeast = 1,
    East = 2,
    Southeast = 3,
    South = 4,
    Southwest = 5,
    West = 6,
    Northwest = 7,
}

/// The cell deltas, indexed by [`Heading`] (`0x102422c28` and `0x102422c48`).
const DELTA: [(i32, i32); 8] = [
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
];

const NAMES: [&str; 8] = [
    "north",
    "northeast",
    "east",
    "southeast",
    "south",
    "southwest",
    "west",
    "northwest",
];

impl Heading {
    pub const ALL: [Heading; 8] = [
        Heading::North,
        Heading::Northeast,
        Heading::East,
        Heading::Southeast,
        Heading::South,
        Heading::Southwest,
        Heading::West,
        Heading::Northwest,
    ];

    pub fn of(ix: u8) -> Option<Heading> {
        Heading::ALL.get(ix as usize).copied()
    }

    /// The full name, as the tables spell it.
    pub fn name(self) -> &'static str {
        NAMES[self as usize]
    }

    /// The ship's-heading delta in grid cells: column first, then row, with the
    /// row growing southwards.
    pub fn delta(self) -> (i32, i32) {
        DELTA[self as usize]
    }

    /// The direction across the compass: `north` ↔ `south`, and so on. This is
    /// the `eor #4` `runs_true` applies to the heading before reading a room's
    /// neighbour list.
    pub fn opposite(self) -> Heading {
        Heading::of(self as u8 ^ 4).expect("heading ^ 4 is a heading")
    }

    /// @0x1015a6c50 — `Option<Heading>` with the niche tag `8` for `None`
    /// (`mov w21, #8`).
    ///
    /// The recovered arms, by trimmed length and text:
    ///
    /// | len | accepted |
    /// |---|---|
    /// | 1 | `n` `e` `s` `w` |
    /// | 2 | `ne` `se` `sw` `nw` |
    /// | 3 | — (falls through to `None`) |
    /// | 4 | `east` `west` |
    /// | 5 | `north` `south` |
    /// | 9 | `northeast` `southeast` `southwest` `northwest` |
    ///
    /// Anything else is `None`.
    pub fn named(text: &str) -> Option<Heading> {
        let t = text.trim().to_ascii_lowercase();
        match t.len() {
            1 => match t.as_str() {
                "n" => Some(Heading::North),
                "e" => Some(Heading::East),
                "s" => Some(Heading::South),
                "w" => Some(Heading::West),
                _ => None,
            },
            2 => match t.as_str() {
                "ne" => Some(Heading::Northeast),
                "se" => Some(Heading::Southeast),
                "sw" => Some(Heading::Southwest),
                "nw" => Some(Heading::Northwest),
                _ => None,
            },
            4 => match t.as_str() {
                "east" => Some(Heading::East),
                "west" => Some(Heading::West),
                _ => None,
            },
            5 => match t.as_str() {
                "north" => Some(Heading::North),
                "south" => Some(Heading::South),
                _ => None,
            },
            9 => match t.as_str() {
                "northeast" => Some(Heading::Northeast),
                "southeast" => Some(Heading::Southeast),
                "southwest" => Some(Heading::Southwest),
                "northwest" => Some(Heading::Northwest),
                _ => None,
            },
            _ => None,
        }
    }
}

/// One cell of open sea.
///
/// The field offsets are the binary's: the eight neighbours are the first 128
/// bytes (`runs_true` indexes them as `room + heading * 16`, each an
/// `Option<u64>` whose tag word is `1` for `Some`), the uid is at `+0xb0` and
/// the cell at `+0xb8`/`+0xbc`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OceanRoom {
    /// `n ne e se s sw w nw`, in that order.
    pub neighbours: [Option<u64>; 8],
    /// The `ocean` column.
    pub title: String,
    /// The `sea` column.
    pub sea: String,
    /// The room's own uid (`+0xb0`).
    pub uid: u64,
    /// Column (`+0xb8`) and row (`+0xbc`).
    pub col: i32,
    pub row: i32,
}

impl OceanRoom {
    /// `room.neighbours[heading]`.
    pub fn neighbour(&self, heading: Heading) -> Option<u64> {
        self.neighbours[heading as usize]
    }

    pub fn cell(&self) -> (i32, i32) {
        (self.col, self.row)
    }
}

/// One port: the sea it sits on and the room that stands for it.
///
/// Offsets from `Ocean::bound`/`port_named`/`port_at`: the name is a `String` at
/// `+0x10` and the number a `u64` at `+0x40`; 72 bytes in all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Port {
    /// **UNKNOWN** — the 16 bytes at `+0x00` are never read by any of the
    /// recovered `Ocean` methods. Kept so the shape stays honest.
    pub reserved: [u64; 2],
    /// The port's name (`+0x10`); `port_named` matches this case-insensitively.
    pub name: String,
    /// The sea the port is on. **INFERRED** from the pier table's middle column
    /// (`+0x28`, a `String` that no stub function reads).
    pub sea: String,
    /// What `port_at` matches: the pier room's uid (`+0x40`).
    pub number: u64,
}

/// One pier: its title and the room it stands in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pier {
    /// `piers_titled` matches this (`+0x00`, a `String`).
    pub title: String,
    /// The pier's room uid (`+0x18`).
    pub room: u64,
}

/// What `Ocean::bound` resolves a name or a number to.
///
/// Layout from the sret writes at `0x1015a66a4`..`0x1015a69ac`: a `String`, then
/// the number, then a flag. `None` is the `isize::MIN` niche in the string.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bound {
    /// The port's name, or the room's uid in decimal when a number was given.
    pub name: String,
    pub number: u64,
    /// `true` when the input matched a port's name, `false` when it was a
    /// number.
    pub named: bool,
}

/// The whole ocean.
#[derive(Clone, Debug, Default)]
pub struct Ocean {
    /// Every sea cell, in table order (`+0x00`).
    pub rooms: Vec<OceanRoom>,
    /// Every pier (`+0x30`); element size 32.
    pub piers: Vec<Pier>,
    /// Every port (`+0x18`); element size 72.
    pub ports: Vec<Port>,
    rooms_by_id: HashMap<u64, usize>,
    by_cell: HashMap<(i32, i32), usize>,
    /// pier room uid → index into [`Ocean::ports`] (`+0xa8`).
    pier_port: HashMap<u64, usize>,
    /// pier title → index into [`Ocean::ports`] (`+0xd8`), keyed by the
    /// lower-cased title so lookups are case-insensitive.
    titled_port: HashMap<String, usize>,
}

impl Ocean {
    /// Build an ocean from sea cells alone (no ports or piers). The index
    /// constructor `read` uses, exposed for callers and tests.
    pub fn from_rooms(rooms: Vec<OceanRoom>) -> Ocean {
        let mut ocean = Ocean::default();
        for room in rooms {
            ocean.push_room(room);
        }
        ocean
    }

    fn push_room(&mut self, room: OceanRoom) {
        let ix = self.rooms.len();
        self.rooms_by_id.entry(room.uid).or_insert(ix);
        self.by_cell.entry((room.col, room.row)).or_insert(ix);
        self.rooms.push(room);
    }

    /// The cell at `(col, row)` — `Ocean::at` (`0x1015a4c90`): a
    /// `HashMap<(i32,i32), usize>` at `+0x78`, then `&self.rooms[index]`.
    pub fn at(&self, cell: (i32, i32)) -> Option<&OceanRoom> {
        let ix = *self.by_cell.get(&cell)?;
        self.rooms.get(ix)
    }

    /// The room with this uid — `Ocean::room` (`0x1015a6570`): the
    /// `HashMap<u64, usize>` at `+0x48`.
    pub fn room(&self, uid: u64) -> Option<&OceanRoom> {
        let ix = *self.rooms_by_id.get(&uid)?;
        self.rooms.get(ix)
    }

    /// The port with this number — `Ocean::port_at` (`0x1015a6a10`): a linear
    /// walk of `self.ports` comparing `Port::number`.
    pub fn port_at(&self, number: u64) -> Option<&Port> {
        self.ports.iter().find(|p| p.number == number)
    }

    /// @0x1015a4638 — three passes over `self.ports`, each comparing the
    /// lower-cased input with the lower-cased port name:
    ///
    /// 1. equal;
    /// 2. the port name *starts with* the input;
    /// 3. any word of the port name (split on spaces and apostrophes) equals the
    ///    input.
    ///
    /// An empty input never matches.
    pub fn port_named(&self, name: &str) -> Option<&Port> {
        let needle = name.trim().to_lowercase();
        if needle.is_empty() {
            return None;
        }
        let lowered: Vec<String> = self.ports.iter().map(|p| p.name.to_lowercase()).collect();
        if let Some(i) = lowered.iter().position(|n| *n == needle) {
            return self.ports.get(i);
        }
        if let Some(i) = lowered.iter().position(|n| n.starts_with(&needle)) {
            return self.ports.get(i);
        }
        lowered
            .iter()
            .position(|n| {
                n.split([' ', '\''])
                    .any(|word| !word.is_empty() && word == needle)
            })
            .and_then(|i| self.ports.get(i))
    }

    /// The port of the pier standing in room `pier` — `Ocean::port_of_pier`
    /// (`0x1015a4a74`): the `HashMap<u64, usize>` at `+0xa8`, then
    /// `&self.ports[index]`.
    pub fn port_of_pier(&self, pier: u64) -> Option<&Port> {
        let ix = *self.pier_port.get(&pier)?;
        self.ports.get(ix)
    }

    /// The port of the pier called `title` — `Ocean::port_of_pier_titled`
    /// (`0x1015a4b64`): the `HashMap<String, usize>` at `+0xd8`.
    pub fn port_of_pier_titled(&self, title: &str) -> Option<&Port> {
        let ix = *self.titled_port.get(&title.to_lowercase())?;
        self.ports.get(ix)
    }

    /// The piers of the port numbered `number`, as pier room uids.
    ///
    /// **Partly recovered.** `Ocean::piers_of` (`0x1015a6a44`) returns the
    /// `HashMap` iterator at `+0xa8` with the key captured, i.e. it walks
    /// `pier → port` pairs and keeps the ones belonging to the argument. The
    /// answer is therefore the same set of pier uids; whether the original
    /// yields the uids or the `Port`s could not be read off the iterator type,
    /// so this yields the uids (which is what [`Ocean::piers_titled`] yields
    /// too).
    pub fn piers_of(&self, number: u64) -> impl Iterator<Item = u64> + '_ {
        let ports = &self.ports;
        self.pier_port.iter().filter_map(move |(&pier, &ix)| {
            let port = ports.get(ix)?;
            (port.number == number).then_some(pier)
        })
    }

    /// The piers whose title contains `title`, as pier room uids, ascending.
    ///
    /// **Partly recovered.** @0x1015a4970: `self.piers.iter()` is filtered by
    /// the needle, the `room` of each survivor collected into a `Vec<u64>`, and
    /// the vector `sort_unstable()`ed (the large-arm call at `0x1015a4a48` is
    /// `slice::sort::unstable::ipnsort`). The comparison inside the filter
    /// closure was not dumped; a case-insensitive substring test is the sensible
    /// reading of a "titled" lookup (`port_named` also folds case).
    pub fn piers_titled(&self, title: &str) -> Vec<u64> {
        let needle = title.to_lowercase();
        let mut found: Vec<u64> = self
            .piers
            .iter()
            .filter(|p| p.title.to_lowercase().contains(&needle))
            .map(|p| p.room)
            .collect();
        found.sort_unstable();
        found
    }

    /// @0x1015a6660 — a port name, or a room number with an optional `u`
    /// prefix and an optional sign.
    ///
    /// * `port_named(text)` → `Bound { name: port.name, number: port.number,
    ///   named: true }`;
    /// * else the text is parsed as an `i64` (the unrolled decimal reader at
    ///   `0x1015a6734`) and `room(n)` looked up → `Bound { name: format!("{}",
    ///   room.uid), number: n, named: false }`;
    /// * else `None`.
    ///
    /// An empty or sign-only input is `None` (`0x1015a66fc`).
    pub fn bound(&self, text: &str) -> Option<Bound> {
        if let Some(port) = self.port_named(text) {
            return Some(Bound {
                name: port.name.clone(),
                number: port.number,
                named: true,
            });
        }
        let number = crate::room::parse_number(text)?;
        let uid = u64::try_from(number).ok()?;
        let room = self.room(uid)?;
        Some(Bound {
            name: format!("{}", room.uid),
            number: uid,
            named: false,
        })
    }

    /// @0x1015a6a78 — may the ship sail from `room` in `heading`?
    ///
    /// Three conditions, in this order, all read off the instruction stream:
    ///
    /// 1. `room.neighbours[heading]` is `Some(uid)` (the tag word at
    ///    `room + heading*16` must be `1`);
    /// 2. the room with that uid sits at exactly `room.cell + heading.delta()`
    ///    (`ldp w12, w14, [neighbour, #0xb8]` against `col + dcol`, `row +
    ///    drow`);
    /// 3. that room's own neighbour in the *opposite* heading (`eor #4`, the
    ///    `TABLE[h ^ 4]` lookup) is `Some(room.uid)`.
    ///
    /// Anything else is `false`, including an unknown heading and an unknown
    /// neighbour uid.
    pub fn runs_true(&self, room: &OceanRoom, heading: Heading) -> bool {
        let Some(uid) = room.neighbour(heading) else {
            return false;
        };
        let Some(next) = self.room(uid) else {
            return false;
        };
        let (dcol, drow) = heading.delta();
        if next.col != room.col + dcol || next.row != room.row + drow {
            return false;
        }
        next.neighbour(heading.opposite()) == Some(room.uid)
    }

    /// @0x1015a4d90 — build the ocean from its two tables.
    ///
    /// **Partly recovered.** The function is a `lines()` loop splitting on tabs
    /// and parsing decimals with an unrolled reader; the column names come from
    /// the two literals quoted at the top of this module. What could not be
    /// established is how the two tables are told apart inside one text: this
    /// build dispatches on the field count (13 = sea cell, 3 = pier, anything
    /// else is skipped), and skips a header line whose first field is not a
    /// number. The four `HashMap`s `read` builds are `rooms_by_id`, `by_cell`,
    /// `pier_port` and `titled_port`, which is what the getters above use.
    pub fn read(text: &str) -> Ocean {
        let mut ocean = Ocean::default();
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            let f: Vec<&str> = line.split('\t').collect();
            match f.len() {
                13 => {
                    let (Some(uid), Some(col), Some(row)) =
                        (parse_u64(f[0]), parse_i32(f[3]), parse_i32(f[4]))
                    else {
                        continue; // a header line
                    };
                    let mut neighbours = [None; 8];
                    for (h, slot) in neighbours.iter_mut().enumerate() {
                        *slot = parse_u64(f[5 + h]);
                    }
                    let ix = ocean.rooms.len();
                    ocean.rooms.push(OceanRoom {
                        neighbours,
                        title: f[1].to_string(),
                        sea: f[2].to_string(),
                        uid,
                        col,
                        row,
                    });
                    ocean.rooms_by_id.entry(uid).or_insert(ix);
                    ocean.by_cell.entry((col, row)).or_insert(ix);
                }
                3 => {
                    let (Some(room), sea, title) = (parse_u64(f[0]), f[1], f[2]) else {
                        continue;
                    };
                    ocean.piers.push(Pier {
                        title: title.to_string(),
                        room,
                    });
                    // One port per (sea, pier room) pair. The pier's own room
                    // number stands for the port, which is what `port_at`
                    // matches and what the piers are grouped by.
                    let ix = ocean.ports.len();
                    if !ocean.ports.iter().any(|p| p.number == room) {
                        ocean.ports.push(Port {
                            reserved: [0; 2],
                            name: sea.to_string(),
                            sea: sea.to_string(),
                            number: room,
                        });
                    }
                    let port_ix = ocean
                        .ports
                        .iter()
                        .position(|p| p.number == room)
                        .unwrap_or(ix);
                    ocean.pier_port.insert(room, port_ix);
                    ocean
                        .titled_port
                        .entry(title.to_lowercase())
                        .or_insert(port_ix);
                }
                _ => {}
            }
        }
        ocean
    }
}

static OCEAN: OnceLock<Ocean> = OnceLock::new();

/// The process-wide ocean.
///
/// @0x1015a6c10: `static OCEAN: OnceLock<Ocean>` (`hydra_map::ocean::ocean::OCEAN`
/// at `0x104586e18`, state word at `+0x40`) and the accessor returns
/// `&'static Ocean`.
///
/// **UNKNOWN** — the initialiser. The original parsed the ocean tables out of
/// the embedded data blob; that blob is not reproducible here, so the first call
/// parses [`DEFAULT_OCEAN`] (empty) unless [`set_ocean`] installed one first.
pub fn ocean() -> &'static Ocean {
    OCEAN.get_or_init(|| Ocean::read(DEFAULT_OCEAN))
}

/// The text [`ocean`] parses on first use.
pub const DEFAULT_OCEAN: &str = "";

/// Install the ocean the process will use. Only valid before [`ocean`] is first
/// called; afterwards the installed value wins and `Err` is returned with the
/// ocean that is already in place.
pub fn set_ocean(text: &str) -> Result<(), &'static Ocean> {
    OCEAN.set(Ocean::read(text)).map_err(|_| ocean())
}

fn parse_u64(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    s.parse().ok()
}

fn parse_i32(s: &str) -> Option<i32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    s.parse().ok()
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A sea-cell line: `uid  ocean  sea  col  row  n ne e se s sw w nw`.
    fn cell(uid: u64, col: i32, row: i32, neighbours: [Option<u64>; 8]) -> String {
        let mut out = format!("{uid}\tThe Tenebrous Cauldron\tNorth Blue\t{col}\t{row}");
        for n in neighbours {
            out.push('\t');
            if let Some(uid) = n {
                out.push_str(&uid.to_string());
            }
        }
        out.push('\n');
        out
    }

    /// Four sea cells (a 2x2 block at columns 10..=11, rows 52..=53) plus three
    /// piers. `7116001` and `7116002` are east/west of each other with both
    /// links present; `7116005` is north of `7116001` with no reverse link.
    fn table() -> String {
        let mut text = String::from(
            "uid\tocean\tsea\tcol\trow\tn\tne\te\tse\ts\tsw\tw\tnw\n",
        );
        // order: n ne e se s sw w nw
        text.push_str(&cell(
            7116001,
            10,
            53,
            [Some(7116005), None, Some(7116002), None, None, None, None, None],
        ));
        text.push_str(&cell(
            7116002,
            11,
            53,
            [None, None, None, None, None, None, Some(7116001), None],
        ));
        text.push_str(&cell(7116003, 12, 53, [None; 8]));
        text.push_str(&cell(7116005, 10, 52, [None, None, None, None, None, None, None, None]));
        text.push_str("\n");
        text.push_str("7142002\tSleeping Drake Harbor\tSleeping Drake Harbor, Pier One\n");
        text.push_str("7142003\tSleeping Drake Harbor\tSleeping Drake Harbor, Pier Two\n");
        text.push_str("7140118\tBrisker's Cove\tComyner's Docks, Pier 4\n");
        text
    }

    fn ocean() -> Ocean {
        Ocean::read(&table())
    }

    #[test]
    fn heading_names_cover_all_eight_and_their_abbreviations() {
        for h in Heading::ALL {
            assert_eq!(Heading::named(h.name()), Some(h));
            assert_eq!(Heading::named(&h.name().to_uppercase()), Some(h));
        }
        // The two-letter forms, which are not simply the first two letters of
        // the long names.
        for (two, h) in [
            ("ne", Heading::Northeast),
            ("se", Heading::Southeast),
            ("sw", Heading::Southwest),
            ("nw", Heading::Northwest),
        ] {
            assert_eq!(Heading::named(two), Some(h), "{two}");
        }
        // A long name truncated to two letters is not an abbreviation.
        assert_eq!(Heading::named("no"), None);
        assert_eq!(Heading::named("  n "), Some(Heading::North));
        assert_eq!(Heading::named("N"), Some(Heading::North));
        assert_eq!(Heading::named("nne"), None);
        assert_eq!(Heading::named(""), None);
        assert_eq!(Heading::named("northeasterly"), None);
    }

    #[test]
    fn heading_deltas_and_opposites_are_the_recovered_tables() {
        assert_eq!(Heading::North.delta(), (0, -1));
        assert_eq!(Heading::Northeast.delta(), (1, -1));
        assert_eq!(Heading::East.delta(), (1, 0));
        assert_eq!(Heading::Southeast.delta(), (1, 1));
        assert_eq!(Heading::South.delta(), (0, 1));
        assert_eq!(Heading::Southwest.delta(), (-1, 1));
        assert_eq!(Heading::West.delta(), (-1, 0));
        assert_eq!(Heading::Northwest.delta(), (-1, -1));
        assert_eq!(Heading::North.opposite(), Heading::South);
        assert_eq!(Heading::East.opposite(), Heading::West);
        assert_eq!(Heading::Northeast.opposite(), Heading::Southwest);
        assert_eq!(Heading::Northwest.opposite(), Heading::Southeast);
    }

    #[test]
    fn the_table_parses_into_cells_with_eight_neighbours() {
        let o = ocean();
        assert_eq!(o.rooms.len(), 4);
        let r = o.room(7116001).unwrap();
        assert_eq!(r.title, "The Tenebrous Cauldron");
        assert_eq!(r.sea, "North Blue");
        assert_eq!(r.cell(), (10, 53));
        assert_eq!(r.neighbour(Heading::North), Some(7116005));
        assert_eq!(r.neighbour(Heading::East), Some(7116002));
        assert_eq!(r.neighbour(Heading::South), None);
        assert_eq!(o.at((12, 53)).unwrap().uid, 7116003);
        assert!(o.at((99, 99)).is_none());
    }

    #[test]
    fn runs_true_needs_the_neighbour_the_cell_and_the_reverse_link() {
        let o = ocean();
        let a = o.room(7116001).unwrap();
        // East: 7116002 is at (11,53), and 7116002's west neighbour is 7116001.
        assert!(o.runs_true(a, Heading::East));
        // North: 7116005 is at (10,52) but does not list us as its south
        // neighbour, so the run does not hold.
        assert!(!o.runs_true(a, Heading::North));
        // South: no neighbour at all.
        assert!(!o.runs_true(a, Heading::South));
    }

    #[test]
    fn bound_takes_a_port_name_or_a_number() {
        let o = ocean();
        let by_name = o.bound("Sleeping Drake Harbor").unwrap();
        assert_eq!(by_name.name, "Sleeping Drake Harbor");
        assert_eq!(by_name.number, 7142002);
        assert!(by_name.named);

        let by_number = o.bound("u7116003").unwrap();
        assert_eq!(by_number.name, "7116003");
        assert_eq!(by_number.number, 7116003);
        assert!(!by_number.named);

        assert_eq!(o.bound("7116003").unwrap().number, 7116003);
        assert!(o.bound("nowhere at all").is_none());
        assert!(o.bound("").is_none());
    }

    #[test]
    fn port_named_matches_exactly_by_prefix_and_by_word() {
        let o = ocean();
        assert_eq!(o.port_named("Brisker's Cove").unwrap().number, 7140118);
        assert_eq!(o.port_named("  brisker's cove ").unwrap().number, 7140118);
        assert_eq!(o.port_named("Brisker").unwrap().number, 7140118);
        assert_eq!(o.port_named("Cove").unwrap().number, 7140118);
        assert!(o.port_named("").is_none());
        assert!(o.port_named("nothing").is_none());
    }

    #[test]
    fn piers_are_grouped_by_port_and_found_by_title() {
        let o = ocean();
        let mut piers: Vec<u64> = o.piers_of(7142002).collect();
        piers.sort_unstable();
        assert_eq!(piers, vec![7142002]);
        assert_eq!(o.piers_titled("pier two"), vec![7142003]);
        assert_eq!(
            o.piers_titled("Comyner"),
            vec![7140118],
            "title match is a substring test"
        );
        assert_eq!(o.port_of_pier(7142002).unwrap().name, "Sleeping Drake Harbor");
        assert_eq!(o.port_at(7142003).unwrap().name, "Sleeping Drake Harbor");
        assert_eq!(
            o.port_of_pier_titled("Sleeping Drake Harbor, Pier One")
                .unwrap()
                .number,
            7142002
        );
    }
}
