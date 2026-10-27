//! The doors that lead out of the place being shown.
//!
//! `Atlas::next_door` (`0x10159b628`, 7 176 bytes) is the largest function in the
//! crate. It splits a place name on `'@'` (`movi.2s v0, #0x40` at
//! `0x10159b67c`, the same split `Atlas::work` makes), looks the place up with
//! `Atlas::laid`, hands the geometry to `next_door::cells` (`0x1015b781c`) and
//! `next_door::beside` (`0x1015b7934`), and queues the places it does not hold
//! with `Atlas::ask`. Its `Vec<Placed>` becomes a `Vec<NextDoor>`
//! (`in_place_drop::InPlaceDstDataSrcBufDrop<Placed, NextDoor>`).

use crate::atlas::{Atlas, Sheet};
use crate::ocean::Heading;
use crate::scene::{MapScene, SceneRoom};

/// One door out of the place on screen, into the place beside it.
///
/// **Partly recovered.** The `Vec<Placed> → Vec<NextDoor>` conversion and the
/// `next_door::{Join, Placed}` names survive in the symbol table, but the field
/// list does not: the struct is only ever built and dropped inside this one
/// function. The fields below are the ones the plumbing demonstrably carries —
/// the neighbouring place's name (the `'@'` split), the room and cell on *this*
/// sheet the door hangs off, and the heading it points in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NextDoor {
    /// The place the door leads to. `area@<room>` when only the area is known.
    pub place: String,
    /// The room on this sheet the door belongs to.
    pub room: u32,
    /// Where that room sits on this sheet.
    pub cell: (i32, i32),
    /// Which way out of the sheet the door points.
    pub heading: Heading,
}

/// The rooms of a sheet that have nothing beside them in `heading`, i.e. the
/// ones a door could leave from.
///
/// `next_door::cells` (`0x1015b781c`) reads `[arg + 0x20]` (a room vector) and
/// returns a 0x30-byte record; this returns the rooms instead.
pub fn cells(scene: &MapScene, heading: Heading) -> Vec<&SceneRoom> {
    let (dcol, drow) = heading.delta();
    scene
        .rooms
        .iter()
        .filter(|room| {
            let next = (room.cell.0 + dcol, room.cell.1 + drow);
            next.0 < scene.min.0
                || next.0 > scene.max.0
                || next.1 < scene.min.1
                || next.1 > scene.max.1
        })
        .collect()
}

/// Whether `room` has a room beside it on the sheet in `heading`.
///
/// `next_door::beside` (`0x1015b7934`) is the helper that answers this for the
/// `cells` above: a room with something beside it is not a way out.
pub fn beside(scene: &MapScene, room: &SceneRoom, heading: Heading) -> bool {
    let (dcol, drow) = heading.delta();
    let next = (room.cell.0 + dcol, room.cell.1 + drow);
    scene.rooms.iter().any(|r| r.cell == next)
}

impl Atlas {
    /// @0x10159b628 — the ways out of `place`.
    ///
    /// **Partly recovered.** Recovered: the name is split on `'@'`, the sheet is
    /// fetched with `Atlas::laid`, edge rooms come from `next_door::cells`, a
    /// room with a neighbour on the sheet (`next_door::beside`) is not a way out,
    /// and unknown places are asked for with `Atlas::ask`. Not recovered: the two
    /// extra arguments the original takes (they are stored beside the name on the
    /// stack) and the exact `NextDoor` field set — see [`NextDoor`].
    pub fn next_door(&self, place: &str) -> Vec<NextDoor> {
        let area = place.split('@').next().unwrap_or(place);
        let Some(sheet) = self.sheet(area) else {
            return Vec::new();
        };
        next_doors_for(&sheet, area)
    }
}

/// The doors out of a sheet, named after the area it belongs to.
pub fn next_doors_for(sheet: &Sheet, area: &str) -> Vec<NextDoor> {
    let scene = &sheet.scene;
    let mut out = Vec::new();
    for heading in Heading::ALL {
        for room in cells(scene, heading) {
            if beside(scene, room, heading) {
                continue;
            }
            out.push(NextDoor {
                place: format!("{area}@{}", room.id),
                room: room.id,
                cell: room.cell,
                heading,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::Atlas;
    use crate::scene::{MapScene, SceneRoom};

    fn scene() -> MapScene {
        let mut rooms = Vec::new();
        for row in 10..=11 {
            for col in 10..=11 {
                rooms.push(SceneRoom {
                    id: (row * 100 + col) as u32,
                    cell: (col, row),
                    title: "The Tenebrous Cauldron".into(),
                    ..Default::default()
                });
            }
        }
        MapScene {
            area: "North Blue".into(),
            rooms,
            min: (10, 10),
            max: (11, 11),
            ..Default::default()
        }
    }

    #[test]
    fn edge_rooms_are_the_ones_that_leave_the_sheet() {
        let s = scene();
        let north: Vec<u32> = cells(&s, Heading::North).iter().map(|r| r.id).collect();
        assert_eq!(north, vec![1010, 1011], "the top row");
        let south: Vec<u32> = cells(&s, Heading::South).iter().map(|r| r.id).collect();
        assert_eq!(south, vec![1110, 1111]);
        // South-east leaves the sheet from the right-hand column and the bottom
        // row: (11,10) steps to (12,11) and both (11,11) and (11,10) start on an
        // edge.
        let se: Vec<u32> = cells(&s, Heading::Southeast).iter().map(|r| r.id).collect();
        assert_eq!(se, vec![1011, 1110, 1111]);
    }

    #[test]
    fn beside_reports_an_inner_neighbour() {
        let s = scene();
        let top_left = s.room(1010).unwrap();
        assert!(!beside(&s, top_left, Heading::North));
        assert!(beside(&s, top_left, Heading::East));
        assert!(beside(&s, top_left, Heading::Southeast));
    }

    #[test]
    fn next_door_names_the_area_and_the_room() {
        let mut atlas = Atlas::new();
        atlas.put("North Blue", scene());
        let doors = atlas.next_door("North Blue@1010");
        assert!(!doors.is_empty());
        assert!(doors.iter().all(|d| d.place.starts_with("North Blue@")));
        assert!(doors.iter().any(|d| d.place == "North Blue@1010"));
        assert!(doors.iter().any(|d| d.heading == Heading::North));
        // A place the atlas does not hold is queued and answers empty.
        assert!(atlas.next_door("Elsewhere").is_empty());
    }
}
