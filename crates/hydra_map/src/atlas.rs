//! The atlas: the map's room list plus the background laid-out places.
//!
//! The `Atlas` value is large (0x2b8 bytes) and almost every field is visible in
//! the instruction stream:
//!
//! ```text
//! +0x000  HashMap<u32, String>            room id → area name       (area_of)
//! +0x030  HashSet<String>                 the map's area names      (area_names)
//! +0x060  RwLock<HashMap<String, Arc<Sheet>>>   the laid-out places (laid/sheet)
//! +0x170  RwLock<HashMap<u32, String>>    room id → area name cache (scene_of)
//! +0x1e0  Mutex<Queue>                    the layout queue          (ask/work)
//! +0x290  bool                            closed                    (close/ask)
//! +0x298  Condvar                         work finished             (close/work)
//! +0x2a8  Condvar                         a name was queued         (ask/close)
//! ```
//!
//! and the map's `HashMap`/`HashSet` layout is
//! `{bucket_mask@0, ctrl@8, growth_left@0x10, items@0x18, RandomState@0x20}`,
//! with values before keys inside a bucket (`HashMap<u32, String>` closes with
//! `ldp x0, x1, [bucket - 0x10]`, i.e. the value's `(ptr, len)`).

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};

use crate::cena::Map;
use crate::scene::MapScene;

/// A laid-out place, as the worker leaves it in [`Atlas`]'s map.
///
/// The `Arc` payload: `scene_of` does `MapScene::room(sheet + 0x10, id)`
/// (`0x10159b388`), so the scene sits sixteen bytes in.
#[derive(Clone, Debug, Default)]
pub struct Sheet {
    /// **UNKNOWN** — the sixteen bytes at `+0x00`. No recovered method reads
    /// them; they are most likely the sheet's own bounds (`min`/`max` are
    /// already inside the scene).
    pub reserved: [u64; 2],
    pub scene: MapScene,
}

impl Sheet {
    pub fn new(scene: MapScene) -> Sheet {
        Sheet {
            reserved: [0; 2],
            scene,
        }
    }
}

/// How often a name is already known to the queue.
///
/// `waiting_for` returns `2` when the name is already in the "done" set
/// (`mov w0, #2` at `0x101597410`) and `1` after queueing it (`mov w0, #1` at
/// `0x101597428`), so the two variants carry those discriminants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Waiting {
    /// The name was not known; it has just been queued.
    Queued = 1,
    /// The name was already known.
    Already = 2,
}

/// The layout queue.
///
/// Field offsets are `ask`'s: two `VecDeque<String>` at `+0x00` and `+0x20`
/// (element size 0x18) and two `HashSet<String>` at `+0x40` and `+0x70`.
#[derive(Debug, Default)]
pub struct Queue {
    /// Names waiting for the worker.
    pub queue: VecDeque<String>,
    /// Names the worker is on (re-queued to the back when asked again).
    pub marks: VecDeque<String>,
    /// Names the worker has taken.
    pub asked: HashSet<String>,
    /// Names that are done.
    pub done: HashSet<String>,
}

impl Queue {
    fn known(&self, name: &str) -> bool {
        self.done.contains(name)
    }

    /// Ask for `name` to be laid out.
    ///
    /// `Atlas::ask` (`0x101597460`), in this order:
    ///
    /// 1. a name in the `done` set is a no-op (`0x10159753c`);
    /// 2. a closed atlas is a no-op (`0x1015975bc`);
    /// 3. a name in `marks` is removed from it (`wrap_copy` + `len -= 1` — the
    ///    `VecDeque::remove` dance at `0x1015977fc`..`0x1015978d4`);
    /// 4. a name in `asked` is a no-op (`0x1015976d4`);
    /// 5. a name already in `queue` is a no-op (`0x101597790`);
    /// 6. otherwise the name is pushed onto `queue` and the condvar at `+0x2a8`
    ///    is told.
    ///
    /// Returns whether anything changed.
    fn ask(&mut self, name: &str) -> bool {
        if self.known(name) {
            return false;
        }
        if let Some(i) = self.marks.iter().position(|m| m == name) {
            self.marks.remove(i);
        }
        if self.asked.contains(name) || self.queue.iter().any(|q| q == name) {
            return false;
        }
        self.queue.push_back(name.to_string());
        true
    }
}

/// How many layout workers are alive.
///
/// `Atlas::held` (`0x101597a2c`, twelve bytes) is a bare load of
/// `hydra_map::atlas::LIVE` (`0x10458d760`).
pub static LIVE: AtomicUsize = AtomicUsize::new(0);

/// The atlas.
#[derive(Debug, Default)]
pub struct Atlas {
    areas: HashMap<u32, String>,
    area_names: HashSet<String>,
    places: RwLock<HashMap<String, Arc<Sheet>>>,
    cache: RwLock<HashMap<u32, String>>,
    queue: Mutex<Queue>,
    closed: Mutex<bool>,
    finished: Condvar,
    queued: Condvar,
}

impl Atlas {
    pub fn new() -> Atlas {
        Atlas::default()
    }

    /// Build the atlas from the map: one `(room, area)` entry per room and the
    /// set of distinct area names.
    pub fn from_map(map: &Map) -> Atlas {
        let mut atlas = Atlas::new();
        for room in map.rooms() {
            if let Some(area) = room.area() {
                atlas.set_area(room.id, area);
            }
        }
        atlas
    }

    /// Insert one room's area. (The original populated the two collections from
    /// the map while decoding it.)
    pub fn set_area(&mut self, room: u32, area: &str) {
        self.areas.insert(room, area.to_string());
        self.area_names.insert(area.to_string());
    }

    /// @0x1015971d4 — the map's area names, sorted.
    ///
    /// The body collects the `HashSet<String>` at `+0x30` into a
    /// `Vec<String>` (`SpecFromIterNested<Vec<String>>`, `0x101597220`), sorts it
    /// with `slice::sort` (`insertion_sort_shift_left` under 21 elements,
    /// `driftsort_main` above) and returns it.
    pub fn area_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.area_names.iter().cloned().collect();
        names.sort();
        names
    }

    /// @0x10159b034 — the area the room belongs to, or `None` when the map does
    /// not know the room (the empty-`HashMap` early return at `0x10159b04c`).
    pub fn area_of(&self, room: u32) -> Option<&str> {
        self.areas.get(&room).map(|s| s.as_str())
    }

    /// @0x101597a2c — how many layout workers are alive.
    pub fn held() -> usize {
        LIVE.load(Ordering::Relaxed)
    }

    /// @0x10159b10c — `(places the atlas holds that are also one of the map's
    /// areas, how many areas there are)`.
    ///
    /// `progress` read-locks the places map at `+0x60`, walks its keys
    /// (`sub x23, x23, #0x100` per eight control bytes, i.e. a 32-byte stride)
    /// and calls `HashSet::contains` on the area-name set for each
    /// (`0x10159b1d0`); the second return value is `[self + 0x48]`, that set's
    /// length.
    pub fn progress(&self) -> (usize, usize) {
        let places = self.places.read().unwrap_or_else(|e| e.into_inner());
        let held = places.keys().filter(|k| self.area_names.contains(*k)).count();
        (held, self.area_names.len())
    }

    /// @0x101597a38 — the laid-out sheet for `place`, cloned out of the places
    /// map (`ldadd x8, x8, [x0]` is the `Arc` strong-count bump at
    /// `0x101597b64`).
    pub fn laid(&self, place: &str) -> Option<Arc<Sheet>> {
        self.places
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(place)
            .cloned()
    }

    /// @0x101599c78 — the sheet for `place`, asking the worker for it when it is
    /// not there yet.
    pub fn sheet(&self, place: &str) -> Option<Arc<Sheet>> {
        if let Some(sheet) = self.laid(place) {
            return Some(sheet);
        }
        self.ask(place);
        None
    }

    /// Queue `place` for the layout worker.
    ///
    /// `Atlas::ask` (`0x101597460`); see [`Queue::ask`] for the exact order of
    /// the checks.
    pub fn ask(&self, place: &str) -> Waiting {
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        if queue.known(place) {
            return Waiting::Already;
        }
        if *self.closed.lock().unwrap_or_else(|e| e.into_inner()) {
            return Waiting::Already;
        }
        if queue.ask(place) {
            drop(queue);
            self.queued.notify_one();
        }
        Waiting::Queued
    }

    /// @0x10159728c — `Already` when the name is known, otherwise queue it.
    ///
    /// The body looks the name up in the "done" set first (`0x101597310`), then
    /// falls through to `ask` (`0x101597424`).
    pub fn waiting_for(&self, place: &str) -> Waiting {
        if self
            .queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .known(place)
        {
            return Waiting::Already;
        }
        self.ask(place)
    }

    /// @0x10159b290 — the sheet holding `room`, asking for its area when the
    /// atlas does not have it.
    ///
    /// Three steps, all visible: `area_of(room)`, then `laid(area)`, then
    /// `MapScene::room(place + 0x10, room)` (`0x10159b388`) — a miss in the
    /// scene is cached in the `RwLock<HashMap<u32, String>>` at `+0x170` before
    /// the asks. The `Option<Arc<Sheet>>` is what the map-drawing code needs to
    /// keep the scene alive while it reads a room out of it.
    pub fn scene_of(&self, room: u32) -> Option<Arc<Sheet>> {
        if let Some(name) = self
            .cache
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&room)
            .cloned()
        {
            if let Some(sheet) = self.laid(&name) {
                if sheet.scene.room(room).is_some() {
                    return Some(sheet);
                }
            }
        }
        let area = self.area_of(room)?.to_string();
        match self.laid(&area) {
            Some(sheet) if sheet.scene.room(room).is_some() => {
                self.cache
                    .write()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(room, area);
                Some(sheet)
            }
            Some(_) => None,
            None => {
                self.waiting_for(&area);
                None
            }
        }
    }

    /// Record a finished sheet. Used by [`Atlas::start`]'s worker and by tests.
    pub fn put(&self, place: &str, scene: MapScene) -> Arc<Sheet> {
        let sheet = Arc::new(Sheet::new(scene));
        self.places
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(place.to_string(), Arc::clone(&sheet));
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        queue.done.insert(place.to_string());
        queue.asked.remove(place);
        queue.queue.retain(|q| q != place);
        drop(queue);
        self.finished.notify_all();
        sheet
    }

    /// @0x101599ba8 — stop the worker.
    ///
    /// It locks the queue, sets the closed flag at `+0x290`, unlocks and then
    /// `notify_all`s both condvars (`0x101599c08`, `0x101599c1c`).
    pub fn close(&self) {
        {
            let _queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
            *self.closed.lock().unwrap_or_else(|e| e.into_inner()) = true;
        }
        self.finished.notify_all();
        self.queued.notify_all();
    }

    pub fn is_closed(&self) -> bool {
        *self.closed.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// @0x101599cc4 — start the background layout worker.
    ///
    /// The original spawns a thread (`spawn_unchecked<Atlas::start::{{closure}},
    /// ()>` is in the symbol table) that runs the `work` loop, and turns a spawn
    /// failure into `"[map] cannot start a layout worker: {}"`
    /// (`0x103e6c98a`, a 36-byte piece).
    ///
    /// **Departure.** Where the sheets come from is the seam: the original
    /// called `cena_map`/`layout` to lay a place out from the pack files. The
    /// caller supplies a `load` closure instead; a name it cannot resolve stays
    /// queued and is retried on the next wake-up.
    pub fn start<F>(self: &Arc<Self>, load: F) -> Result<(), String>
    where
        F: Fn(&str) -> Option<MapScene> + Send + 'static,
    {
        let atlas = Arc::clone(self);
        LIVE.fetch_add(1, Ordering::Relaxed);
        let spawned = std::thread::Builder::new()
            .name("hydra-map-layout".to_string())
            .spawn(move || {
                atlas.work(load);
                LIVE.fetch_sub(1, Ordering::Relaxed);
            });
        match spawned {
            Ok(_) => Ok(()),
            Err(e) => {
                LIVE.fetch_sub(1, Ordering::Relaxed);
                Err(format!("[map] cannot start a layout worker: {e}"))
            }
        }
    }

    /// The worker loop.
    ///
    /// `Atlas::work` (`0x101597c14`): take the queue lock, stop when the closed
    /// flag is set, pop a name from the queue (`VecDeque::pop_front` on the
    /// deque at `+0x1f0`, then the one at `+0x210`), clone it into the "asked"
    /// set (`0x101597d98`), release the lock, resolve the name, insert the sheet
    /// and wake the waiters. When the queue is empty it waits on the condvar at
    /// `+0x2a8` (`0x101597d50`).
    pub fn work<F>(&self, load: F)
    where
        F: Fn(&str) -> Option<MapScene>,
    {
        loop {
            let name = {
                let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
                loop {
                    if *self.closed.lock().unwrap_or_else(|e| e.into_inner()) {
                        return;
                    }
                    if let Some(name) = queue.queue.pop_front().or_else(|| queue.marks.pop_front())
                    {
                        queue.asked.insert(name.clone());
                        break name;
                    }
                    queue = self
                        .queued
                        .wait(queue)
                        .unwrap_or_else(|e| e.into_inner());
                }
            };
            if let Some(scene) = load(&name) {
                self.put(&name, scene);
            } else {
                let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
                queue.asked.remove(&name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cena::Room;
    use crate::scene::{MapScene, SceneRoom};

    fn map() -> Map {
        Map::from_rooms(vec![
            Room {
                id: 1,
                title: "The Tenebrous Cauldron".into(),
                areas: vec!["[North Blue]".into()],
                ..Default::default()
            },
            Room {
                id: 2,
                title: "Brisker's Cove".into(),
                areas: vec!["[Brisker's Cove]".into()],
                ..Default::default()
            },
        ])
    }

    fn scene(id: u32) -> MapScene {
        MapScene {
            area: "North Blue".into(),
            rooms: vec![SceneRoom {
                id,
                cell: (10, 53),
                title: "The Tenebrous Cauldron".into(),
                ..Default::default()
            }],
            min: (10, 53),
            max: (10, 53),
            ..Default::default()
        }
    }

    #[test]
    fn areas_are_indexed_per_room_and_sorted() {
        let atlas = Atlas::from_map(&map());
        assert_eq!(atlas.area_of(1), Some("[North Blue]"));
        assert_eq!(atlas.area_of(2), Some("[Brisker's Cove]"));
        assert_eq!(atlas.area_of(3), None);
        assert_eq!(
            atlas.area_names(),
            vec!["[Brisker's Cove]".to_string(), "[North Blue]".to_string()]
        );
        assert_eq!(Atlas::new().area_names(), Vec::<String>::new());
    }

    #[test]
    fn sheet_asks_for_a_place_it_does_not_hold() {
        let atlas = Atlas::new();
        assert!(atlas.sheet("North Blue").is_none());
        // … and the second look does not queue it twice.
        assert!(atlas.sheet("North Blue").is_none());
        {
            let queue = atlas.queue.lock().unwrap();
            assert_eq!(queue.queue.len(), 1);
            assert_eq!(queue.queue[0], "North Blue");
        }
    }

    #[test]
    fn a_held_sheet_is_found_by_room_and_counted() {
        let mut atlas = Atlas::new();
        atlas.set_area(1, "North Blue");
        atlas.put("North Blue", scene(1));
        assert_eq!(atlas.progress(), (1, 1));
        assert_eq!(atlas.laid("North Blue").unwrap().scene.area, "North Blue");
        let sheet = atlas.sheet("North Blue").unwrap();
        assert_eq!(sheet.scene.room(1).unwrap().cell, (10, 53));
        assert!(atlas.scene_of(1).is_some());
        // A room the sheet does not carry is not reported.
        assert!(atlas.scene_of(2).is_none());
        // Unknown rooms are not either.
        assert!(atlas.scene_of(99).is_none());
    }

    #[test]
    fn waiting_for_reports_queued_then_already() {
        let atlas = Atlas::new();
        assert_eq!(atlas.waiting_for("North Blue"), Waiting::Queued);
        assert_eq!(atlas.ask("North Blue"), Waiting::Queued);
        atlas.put("North Blue", scene(1));
        assert_eq!(atlas.waiting_for("North Blue"), Waiting::Already);
        assert_eq!(atlas.ask("North Blue"), Waiting::Already);
    }

    #[test]
    fn a_closed_atlas_takes_no_more_work() {
        let atlas = Atlas::new();
        atlas.close();
        assert!(atlas.is_closed());
        assert_eq!(atlas.ask("North Blue"), Waiting::Already);
        assert!(atlas.queue.lock().unwrap().queue.is_empty());
    }

    #[test]
    fn the_worker_lays_out_a_place_and_stops_when_closed() {
        let atlas = Arc::new(Atlas::new());
        atlas.ask("North Blue");
        // Resolve exactly one name, then close, so the loop terminates.
        let a = Arc::clone(&atlas);
        let handle = std::thread::spawn(move || {
            a.work(|name| {
                (name == "North Blue").then(|| scene(1))
            });
        });
        while atlas.laid("North Blue").is_none() {
            std::thread::yield_now();
        }
        atlas.close();
        handle.join().unwrap();
        assert!(atlas.laid("North Blue").is_some());
        assert_eq!(atlas.progress(), (0, 0), "no area names were registered");
    }

    #[test]
    fn held_counts_live_workers() {
        let before = Atlas::held();
        let atlas = Arc::new(Atlas::new());
        atlas.start(|_| None).unwrap();
        while Atlas::held() == before {
            std::thread::yield_now();
        }
        atlas.close();
        assert_eq!(Atlas::held(), before + 1);
        atlas.queued.notify_all();
        while Atlas::held() != before {
            std::thread::yield_now();
        }
        assert_eq!(Atlas::held(), before);
    }
}
