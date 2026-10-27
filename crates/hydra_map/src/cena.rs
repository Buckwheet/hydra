//! The room index, and the map file decoder.
//!
//! **`std`-only / scope departure.** The original crate did not own its room
//! model: it used the external `cena_map` crate. The surviving call sites name
//! these pieces of it —
//!
//! * `cena_map::map::Map::room(&self, id: RoomId) -> Option<&Room>` — called
//!   from `Ocean`'s helpers, `room::described` (`0x1015b11f8`) and
//!   `overrides::pocket`;
//! * `cena_map::map::Map::from_rooms(Vec<Room>) -> Map` — called from
//!   `overrides::applied` (`0x1015bf4d8`);
//! * `cena_map::map::Map::locate(&self, &Located) -> Option<RoomId>` — called
//!   from `follow::room_of` (`0x1015b357c`);
//! * `cena_map::locate::title_from_subtitle(&str) -> (&str, &str)` — called
//!   from `follow::room_of` (`0x1015b337c`);
//! * `cena_map::binary::decode::decode(&[u8]) -> Result<Map, E>` — called from
//!   `hydra_map::decode` (`0x1015bd87c`);
//! * `cena_map::Room`'s field offsets, read off `room::described` and
//!   `room::found`'s closure: id `+0x178` (`u32`), title `+0x160` (`String`),
//!   paths `+0x38/+0x40` (a `Vec` of 8-byte ids), areas `+0x50/+0x58` and
//!   specs `+0x68/+0x70` (`Vec<String>`), an optional note at `+0xd8/+0xe0`,
//!   and one exit list.
//!
//! `cena_map` is not part of this workspace, so this module carries the slice of
//! it that `hydra_map` actually uses. The field *names* below are ours; the
//! shapes and offsets are the binary's.

use std::collections::HashMap;

/// A room's map id. The stubs' `u32` arguments are these.
pub type RoomId = u32;

/// One exit from a room.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Exit {
    /// The direction word or command the game answers to (`"north"`, `"go gate"`).
    pub word: String,
    pub to: RoomId,
    /// `cena_map::exit::Cost::price`: the coin the exit charges, if any.
    pub price: Option<u32>,
    /// `cena_map::cond::Cond`: the condition the map attaches to the exit.
    pub when: Option<String>,
}

/// One map room.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Room {
    /// `cena_map::room::Room::id` — `+0x178`.
    pub id: RoomId,
    /// The room's own name — `+0x160`.
    pub title: String,
    /// The uid chain that leads here (innermost last) — `+0x38/+0x40`.
    pub paths: Vec<RoomId>,
    /// The area names the room sits in, innermost last — `+0x50/+0x58`.
    pub areas: Vec<String>,
    /// Free-form notes the map carries about the room — `+0x68/+0x70`.
    pub specs: Vec<String>,
    /// The room's subtitle / description line, when the map has one —
    /// `+0xd8/+0xe0`.
    pub note: Option<String>,
    pub exits: Vec<Exit>,
}

impl Room {
    pub fn exit(&self, word: &str) -> Option<&Exit> {
        self.exits.iter().find(|e| e.word == word)
    }

    /// The area the room is drawn in: the innermost of [`Room::areas`].
    pub fn area(&self) -> Option<&str> {
        self.areas.last().map(|s| s.as_str())
    }
}

/// What the game said about the room we are standing in.
///
/// `follow::room_of` builds this from `hydra_model`'s room block and hands it to
/// [`Map::locate`]: a room number parsed out of `"u12345"`, the title/subtitle
/// pair, the `"room exits"` component and the room's own styled text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Located {
    /// The number, when the game gave one (`Some(n)` whenever the parsed value
    /// is non-zero — see `follow::room_of`).
    pub number: Option<i64>,
    /// The title, as `title_from_subtitle` split it.
    pub title: Option<String>,
    /// The subtitle (the part after the comma of the game's room name).
    pub subtitle: Option<String>,
    /// The `"room exits"` component, when the room block carried one.
    pub exits: Option<String>,
}

/// `cena_map::locate::title_from_subtitle`: split a game room name into its
/// title and subtitle at the first `", "`.
///
/// **Partly recovered.** The call site (`follow::room_of`, `0x1015b337c`) passes
/// the room's styled text and uses both halves; the split point itself is inside
/// the external crate. The first `", "` is the only convention the map's titles
/// follow (every recovered title pair looks like
/// `"Sleeping Drake Harbor, Pier One"`), so that is what this does.
pub fn title_from_subtitle(name: &str) -> (String, String) {
    match name.find(", ") {
        Some(i) => (name[..i].to_string(), name[i + 2..].to_string()),
        None => (name.to_string(), String::new()),
    }
}

/// The room index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Map {
    rooms: Vec<Room>,
    by_id: HashMap<RoomId, usize>,
    /// `(title, subtitle)` folded to lower case → room.
    by_name: HashMap<String, usize>,
}

impl Map {
    /// `cena_map::map::Map::from_rooms`.
    pub fn from_rooms(rooms: Vec<Room>) -> Map {
        let mut by_id = HashMap::with_capacity(rooms.len());
        let mut by_name = HashMap::with_capacity(rooms.len());
        for (i, r) in rooms.iter().enumerate() {
            by_id.entry(r.id).or_insert(i);
            let mut key = r.title.trim().to_lowercase();
            if let Some(note) = &r.note {
                key.push_str(", ");
                key.push_str(&note.trim().to_lowercase());
            }
            by_name.entry(key).or_insert(i);
        }
        Map {
            rooms,
            by_id,
            by_name,
        }
    }

    /// `cena_map::map::Map::room`.
    pub fn room(&self, id: RoomId) -> Option<&Room> {
        self.by_id.get(&id).map(|&i| &self.rooms[i])
    }

    /// `cena_map::map::Map::ids_for_uid` (`0x101662d94`) — every room whose game
    /// number is `uid`.
    ///
    /// The original keeps a `u64 → ids` index beside the room list and answers
    /// with a slice; here the uid chain lives in [`Room::paths`] (its last
    /// element is the room's own game number — that is what
    /// [`crate::room::described`] formats as `" (u{})"`), so the answer is built
    /// from the room list. The two call sites in the binary are
    /// `room::found`'s numeric half and `walk::preflight::gigas_way`, both of
    /// which only want "the rooms with this number".
    pub fn ids_for_uid(&self, uid: i64) -> Vec<RoomId> {
        if !(0..=i64::from(u32::MAX)).contains(&uid) {
            return Vec::new();
        }
        let uid = uid as RoomId;
        self.rooms
            .iter()
            .filter(|room| room.paths.last() == Some(&uid))
            .map(|room| room.id)
            .collect()
    }

    pub fn rooms(&self) -> &[Room] {
        &self.rooms
    }

    pub fn len(&self) -> usize {
        self.rooms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }

    /// The room whose `title, subtitle` matches, case-insensitively.
    pub fn room_named(&self, title: &str, subtitle: &str) -> Option<&Room> {
        let mut key = title.trim().to_lowercase();
        if !subtitle.trim().is_empty() {
            key.push_str(", ");
            key.push_str(&subtitle.trim().to_lowercase());
        }
        self.by_name.get(&key).map(|&i| &self.rooms[i])
    }

    /// Where the character is standing.
    ///
    /// **Partly recovered.** `follow::room_of` (`0x1015b3304`) parses the room
    /// number, folds in `title_from_subtitle`, the `"room exits"` component and
    /// the room's styled text, and calls `Map::locate` (`0x1015b357c`), which
    /// returns a niche-encoded `Option<RoomId>`. The *order* in which `locate`
    /// consults those four things is inside `cena_map`, so this tries the number
    /// first (it is the only exact key), then the full `title, subtitle`, then
    /// the title alone when it is unambiguous.
    pub fn locate(&self, at: &Located) -> Option<RoomId> {
        if let Some(n) = at.number {
            if let Ok(id) = u32::try_from(n) {
                if let Some(r) = self.room(id) {
                    return Some(r.id);
                }
            }
        }
        let title = at.title.as_deref().unwrap_or("");
        let subtitle = at.subtitle.as_deref().unwrap_or("");
        if !title.is_empty() {
            if let Some(r) = self.room_named(title, subtitle) {
                return Some(r.id);
            }
        }
        if let Some(exits) = &at.exits {
            // The component names exits as `word: id` pairs; when the map knows
            // one of those ids and the room is drawn there, that is the room.
            for chunk in exits.split([',', ';']) {
                if let Some((_, id)) = chunk.split_once(':') {
                    if let Ok(n) = id.trim().parse::<u32>() {
                        if let Some(r) = self.room(n) {
                            return Some(r.id);
                        }
                    }
                }
            }
        }
        None
    }
}

/// `cena_map::binary::decode`.
///
/// **UNKNOWN — documented seam.** The map file itself is a binary blob owned by
/// `cena_map` and the crate is not in this workspace, so its record layout is
/// not recoverable from `hydra`'s symbols. Two facts about it *are* recoverable
/// from the call site:
///
/// * the default image is an embedded blob that starts with the seven ASCII
///   bytes `HYDRAMAP` (`hydra_map::load` falls back to `&[u8]` at
///   `0x10245c10f`, length `0x1524665`, and `__const` there really does hold
///   `HYDRAMAP` followed by room description text);
/// * a decode failure becomes `"the map cannot be used: {}"`.
///
/// This decoder accepts that magic and then the simple record layout written by
/// [`encode`], which is what the tests use. Anything else is refused with the
/// recovered error wording.
pub fn decode(bytes: &[u8]) -> Result<Map, String> {
    const MAGIC: &[u8] = b"HYDRAMAP";
    if !bytes.starts_with(MAGIC) {
        return Err("not a Hydra map (no HYDRAMAP magic)".to_string());
    }
    let mut r = Reader {
        b: &bytes[MAGIC.len()..],
        i: 0,
    };
    let count = r.u32()? as usize;
    let mut rooms = Vec::with_capacity(count.min(1 << 16));
    for _ in 0..count {
        let id = r.u32()?;
        let title = r.string()?;
        let area = r.string()?;
        let note = match r.u32()? {
            0 => None,
            _ => Some(r.string()?),
        };
        let n = r.u32()? as usize;
        let mut exits = Vec::with_capacity(n.min(1 << 10));
        for _ in 0..n {
            let word = r.string()?;
            let to = r.u32()?;
            let price = match r.u32()? {
                0 => None,
                p => Some(p),
            };
            exits.push(Exit {
                word,
                to,
                price,
                when: None,
            });
        }
        rooms.push(Room {
            id,
            title,
            paths: Vec::new(),
            areas: if area.is_empty() { Vec::new() } else { vec![area] },
            specs: Vec::new(),
            note,
            exits,
        });
    }
    Ok(Map::from_rooms(rooms))
}

/// The writer [`decode`] understands. Only this crate's tests use it.
pub fn encode(map: &Map) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"HYDRAMAP");
    put_u32(&mut out, map.rooms.len() as u32);
    for r in map.rooms() {
        put_u32(&mut out, r.id);
        put_str(&mut out, &r.title);
        put_str(&mut out, r.area().unwrap_or(""));
        match &r.note {
            Some(n) => {
                put_u32(&mut out, 1);
                put_str(&mut out, n);
            }
            None => put_u32(&mut out, 0),
        }
        put_u32(&mut out, r.exits.len() as u32);
        for e in &r.exits {
            put_str(&mut out, &e.word);
            put_u32(&mut out, e.to);
            put_u32(&mut out, e.price.unwrap_or(0));
        }
    }
    out
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_u32(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn u32(&mut self) -> Result<u32, String> {
        if self.i + 4 > self.b.len() {
            return Err("truncated".to_string());
        }
        let v = u32::from_le_bytes(self.b[self.i..self.i + 4].try_into().unwrap());
        self.i += 4;
        Ok(v)
    }

    fn string(&mut self) -> Result<String, String> {
        let n = self.u32()? as usize;
        if self.i + n > self.b.len() {
            return Err("truncated".to_string());
        }
        let s = std::str::from_utf8(&self.b[self.i..self.i + n])
            .map_err(|_| "bad utf-8".to_string())?;
        self.i += n;
        Ok(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> Map {
        Map::from_rooms(vec![
            Room {
                id: 7116001,
                title: "The Tenebrous Cauldron".into(),
                paths: vec![7116001],
                areas: vec!["North Blue".into()],
                specs: Vec::new(),
                note: Some("Pier One".into()),
                exits: vec![Exit {
                    word: "north".into(),
                    to: 7116224,
                    price: None,
                    when: None,
                }],
            },
            Room {
                id: 7116224,
                title: "Sleeping Drake Harbor".into(),
                paths: vec![7116001, 7116224],
                areas: vec!["North Blue".into()],
                specs: Vec::new(),
                note: Some("Pier One".into()),
                exits: Vec::new(),
            },
        ])
    }

    #[test]
    fn title_from_subtitle_splits_at_the_first_comma() {
        assert_eq!(
            title_from_subtitle("Sleeping Drake Harbor, Pier One"),
            (
                "Sleeping Drake Harbor".to_string(),
                "Pier One".to_string()
            )
        );
        assert_eq!(
            title_from_subtitle("Wehnimer's Landing"),
            ("Wehnimer's Landing".to_string(), String::new())
        );
    }

    #[test]
    fn locate_prefers_the_number_then_the_name() {
        let m = map();
        let by_number = Located {
            number: Some(7116224),
            ..Default::default()
        };
        assert_eq!(m.locate(&by_number), Some(7116224));
        let by_name = Located {
            title: Some("Sleeping Drake Harbor".into()),
            subtitle: Some("Pier One".into()),
            ..Default::default()
        };
        assert_eq!(m.locate(&by_name), Some(7116224));
        // A number the map does not know falls through to the name.
        let both = Located {
            number: Some(1),
            title: Some("The Tenebrous Cauldron".into()),
            subtitle: Some("Pier One".into()),
            ..Default::default()
        };
        assert_eq!(m.locate(&both), Some(7116001));
        assert_eq!(m.locate(&Located::default()), None);
    }

    #[test]
    fn binary_decode_round_trips_and_refuses_other_blobs() {
        let m = map();
        let blob = encode(&m);
        let back = decode(&blob).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back.room(7116224).unwrap().title, "Sleeping Drake Harbor");
        assert_eq!(back.room(7116001).unwrap().exit("north").unwrap().to, 7116224);
        assert_eq!(
            decode(b"nope").unwrap_err(),
            "not a Hydra map (no HYDRAMAP magic)"
        );
        assert_eq!(decode(b"HYDRAMAP\x02").unwrap_err(), "truncated");
    }

    #[test]
    fn from_rooms_keeps_the_first_of_a_repeated_title() {
        let first = map().rooms()[0].clone();
        let mut second = first.clone();
        second.id = 9;
        let m = Map::from_rooms(vec![first, second]);
        assert_eq!(
            m.room_named("The Tenebrous Cauldron", "Pier One").unwrap().id,
            7116001
        );
        assert_eq!(m.room(9).unwrap().id, 9);
    }
}
