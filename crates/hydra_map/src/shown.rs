//! The route and the course the map is currently showing.
//!
//! Two process-wide caches, both `LazyLock<Mutex<HashMap<String, Arc<…>>>>`:
//! `shown::board::BOARD` (the routes, `0x104586e18`, state word at `+0x40`) and
//! `shown::courses::COURSES` (the courses, `0x104586e60`). Both `MutexGuard`
//! types survive in the symbol table:
//! `MutexGuard<HashMap<String, Arc<hydra_map::shown::ShownRoute>>>`.
//!
//! `ShownRoute::from` (`0x1015af040`) and `ShownCourse::from` (`0x1015af3d4`)
//! answer "where does this route pass through this room": both walk their
//! `Vec` **backwards** and return a slice that starts at the last match.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

/// A route being shown: the rooms a walk goes through.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShownRoute {
    /// The rooms, in walking order.
    pub rooms: Vec<u32>,
}

/// A course being shown: the sea rooms a sail goes through.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShownCourse {
    /// The sea rooms, in sailing order.
    pub rooms: Vec<u64>,
}

impl ShownRoute {
    /// @0x1015af040 — the tail of the route from the last time it passed
    /// through room `id`.
    ///
    /// The pointer arithmetic is exact: `x10 = len * 4`, then the loop reads
    /// `[ptr + x10 - 4]` and walks `x10` down by four while counting `x1` up, and
    /// when it finds the id it returns `(ptr + x10, count)`. So the result is
    /// `&rooms[len - count..]`, and a route that never passes through `id` yields
    /// the whole route.
    pub fn from(&self, id: u32) -> &[u32] {
        match self.rooms.iter().rposition(|&r| r == id) {
            Some(i) => &self.rooms[i..],
            None => &self.rooms[..],
        }
    }

    /// The index of the last time the route passes through `id`.
    pub fn last(&self, id: u32) -> Option<usize> {
        self.rooms.iter().rposition(|&r| r == id)
    }
}

impl ShownCourse {
    /// @0x1015af3d4 — the same walk as [`ShownRoute::from`], over `Vec<u64>`
    /// (`lsl x10, x9, #3`; eight-byte elements).
    pub fn from(&self, id: u64) -> &[u64] {
        match self.rooms.iter().rposition(|&r| r == id) {
            Some(i) => &self.rooms[i..],
            None => &self.rooms[..],
        }
    }

    /// The index of the last time the course passes through `id`.
    pub fn last(&self, id: u64) -> Option<usize> {
        self.rooms.iter().rposition(|&r| r == id)
    }
}

static BOARD: LazyLock<Mutex<HashMap<String, Arc<ShownRoute>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static COURSES: LazyLock<Mutex<HashMap<String, Arc<ShownCourse>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn board() -> std::sync::MutexGuard<'static, HashMap<String, Arc<ShownRoute>>> {
    BOARD.lock().unwrap_or_else(|e| e.into_inner())
}

fn courses() -> std::sync::MutexGuard<'static, HashMap<String, Arc<ShownCourse>>> {
    COURSES.lock().unwrap_or_else(|e| e.into_inner())
}

/// @0x1015af084 — remember `route` under `key`, so the map can draw it.
pub fn show_route(key: &str, route: ShownRoute) {
    board().insert(key.to_string(), Arc::new(route));
}

/// @0x1015af774 — the route stored under `key`, cloned out of the cache.
pub fn shown_route(key: &str) -> Option<Arc<ShownRoute>> {
    board().get(key).cloned()
}

/// @0x1015af418 — remember `course` under `key`.
pub fn show_course(key: &str, course: ShownCourse) {
    courses().insert(key.to_string(), Arc::new(course));
}

/// @0x1015afa14 — the course stored under `key`, cloned out of the cache.
///
/// **Partly recovered.** At 4 520 bytes this is the largest of the four and the
/// only one that was not dumped line by line; it locks `COURSES`, looks the key
/// up and clones the `Arc`, which is the behaviour implemented here. The body may
/// additionally build a course from the ocean on a miss (it has the room for it),
/// but that could not be established.
pub fn shown_course(key: &str) -> Option<Arc<ShownCourse>> {
    courses().get(key).cloned()
}

/// Forget everything shown. Not in the stubs; provided so callers (and tests)
/// can reset the two caches.
pub fn forget(key: &str) {
    board().remove(key);
    courses().remove(key);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_reports_its_tail_from_the_last_passage() {
        let route = ShownRoute {
            rooms: vec![1, 2, 3, 2, 4],
        };
        assert_eq!(route.from(2), &[2, 4][..], "the *last* 2 wins");
        assert_eq!(route.last(2), Some(3));
        assert_eq!(route.from(4), &[4][..]);
        assert_eq!(route.from(9), &[1, 2, 3, 2, 4][..], "unknown id: whole route");
        assert_eq!(ShownRoute::default().from(1), &[][..]);
    }

    #[test]
    fn a_course_reports_its_tail_from_the_last_passage() {
        let course = ShownCourse {
            rooms: vec![7116001, 7116002, 7116001],
        };
        assert_eq!(course.from(7116001), &[7116001][..]);
        assert_eq!(course.from(7116002), &[7116002, 7116001][..]);
        assert_eq!(course.last(7116001), Some(2));
    }

    #[test]
    fn the_two_caches_are_separate_and_round_trip() {
        show_route("bank", ShownRoute { rooms: vec![7, 8] });
        assert_eq!(shown_route("bank").unwrap().rooms, vec![7, 8]);
        assert!(shown_route("nowhere").is_none());
        assert!(shown_course("bank").is_none(), "routes and courses differ");

        show_course("bank", ShownCourse { rooms: vec![7116001] });
        assert_eq!(shown_course("bank").unwrap().rooms, vec![7116001]);

        forget("bank");
        assert!(shown_route("bank").is_none());
        assert!(shown_course("bank").is_none());
    }
}
