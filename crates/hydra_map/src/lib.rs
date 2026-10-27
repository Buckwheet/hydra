//! Hydra's map and atlas: the decoded map file, the laid-out scenes, the ocean
//! the ships sail and the doors between places.
//!
//! Reconstructed from the arm64 binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
//! (no DWARF). See `NOTES.md` for the legend, the per-function table and every
//! `std`-only departure. The two that shape the whole crate:
//!
//! * the original used `serde`/`serde_json` for `scene` and `overrides` — this
//!   build carries a small JSON reader/writer in [`json`] and hand-written
//!   `to_json`/`from_json` pairs with the *recovered* field names and types;
//! * the original talked to the external `cena_map` crate (the room model and the
//!   binary map format) — this build carries the slice of it `hydra_map` uses in
//!   [`cena`], with the map-file decoder documented as a seam.
#![allow(dead_code, unused)]

pub mod atlas;
pub mod cena;
pub mod course;
pub mod follow;
pub mod json;
pub mod next_door;
pub mod ocean;
pub mod overrides;
pub mod room;
pub mod sailing;
pub mod scene;
pub mod shown;

use std::sync::{Arc, OnceLock};

pub use cena::{Map, RoomId};

/// What [`load`] and [`decode`] hand back.
///
/// `decode` returns a `(String, Arc<Map>)` pair — a fingerprint of the bytes it
/// decoded plus the map — and `shared`'s `OnceLock` holds
/// `Result<(String, Arc<Map>), String>`: a 32-byte payload plus the lock's state
/// word, which is exactly the `0x28` the state word at `SHARED + 0x20` implies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loaded {
    /// The map file's fingerprint: FNV-1a 64 over the bytes, in hex.
    ///
    /// **Partly recovered.** `decode` walks the input with the FNV-1a 64
    /// constants — `mov x8, #0x2325` / `#0x8422` / `#0x9ce4` / `#0xcbf2` spells
    /// `0xcbf29ce484222325`, and the multiplier `0x100000001b3` is at
    /// `0x1015bd98c` — and formats the result with one argument. The formatting
    /// spec lives in the `0x10245c048` piece blob, which does not decode with the
    /// crate's usual `<len><text>` / `0xC0` scheme, so the exact spelling (padded
    /// or not, upper or lower case) is not certain. This uses 16 lower-case hex
    /// digits.
    pub fingerprint: String,
    pub map: Arc<Map>,
}

/// The env var `load` reads first: `HYDRA_MAP` (`0x10245bffa`, nine bytes).
pub const MAP_ENV: &str = "HYDRA_MAP";

/// The message `load` gives when the file cannot be read:
/// `format!("{} cannot be read: {}", path, error)` (`0x103e25e7b`).
pub const CANNOT_READ: &str = "{} cannot be read: {}";

/// The message `decode` gives when the bytes are not a map:
/// `format!("the map cannot be used: {}", error)` (`0x103e6cdc1`).
pub const CANNOT_USE: &str = "the map cannot be used: {}";

/// @0x1015bd850 — decode the map image.
///
/// The original called `cena_map::binary::decode::decode` (`0x1015bd87c`) and
/// turned a failure into [`CANNOT_USE`]. On success it wrapped the map in an
/// `Arc` (one 160-byte `ArcInner`, `0x1015bd938`..) and computed the FNV-1a 64
/// fingerprint of the raw bytes, which is what [`Loaded::fingerprint`] is.
pub fn decode(bytes: &[u8]) -> Result<Loaded, String> {
    let map = cena::decode(bytes).map_err(|e| CANNOT_USE.replacen("{}", &e, 1))?;
    Ok(Loaded {
        fingerprint: fnv1a64_hex(bytes),
        map: Arc::new(map),
    })
}

/// @0x1015bd6d0 — read and decode the map image.
///
/// `std::env::var_os("HYDRA_MAP")` first (`0x1015bd6fc`); when that is unset it
/// falls back to the embedded image the binary carries at `0x10245c10f` (which
/// begins with the seven bytes `HYDRAMAP` and is `0x1524665` bytes long). The
/// file is read with `std::fs::read` (`0x1015bd738`) and a read error becomes
/// [`CANNOT_READ`].
///
/// **Departure.** The embedded image is not reproducible here, so the fallback is
/// [`DEFAULT_MAP`]; [`decode`] then answers with its "not a Hydra map" text.
pub fn load() -> Result<Loaded, String> {
    match std::env::var_os(MAP_ENV) {
        Some(path) => match std::fs::read(&path) {
            Ok(bytes) => decode(&bytes),
            Err(e) => Err(CANNOT_READ
                .replacen("{}", &std::path::Path::new(&path).display().to_string(), 1)
                .replacen("{}", &e.to_string(), 1)),
        },
        None => decode(DEFAULT_MAP),
    }
}

/// The image [`load`] decodes when `HYDRA_MAP` is unset.
pub const DEFAULT_MAP: &[u8] = b"";

static SHARED: OnceLock<Result<Loaded, String>> = OnceLock::new();

/// @0x1015bda48 — the process-wide map, loaded once.
///
/// `static SHARED: OnceLock<Result<(String, Arc<Map>), String>>`
/// (`hydra_map::shared::SHARED` at `0x104586ea8`, state word at `+0x20`), and the
/// accessor is `SHARED.get_or_init(load)`: the initialiser is the
/// `Once::call_once_force` closure at `0x1015ac58c`, which calls
/// `hydra_map::load` and copies the 32-byte answer into the lock.
pub fn shared() -> &'static Result<Loaded, String> {
    SHARED.get_or_init(load)
}

/// Install a specific map as the shared one. Only valid before [`shared`] is
/// first called; afterwards the installed value wins and `Err` carries a
/// reference to it.
pub fn share(loaded: Result<Loaded, String>) -> Result<(), &'static Result<Loaded, String>> {
    SHARED.set(loaded).map_err(|_| shared())
}

/// FNV-1a 64, as `decode` computes it (`0xcbf29ce484222325`, `× 0x100000001b3`).
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The fingerprint [`decode`] stores: [`fnv1a64`] in 16 lower-case hex digits.
pub fn fnv1a64_hex(bytes: &[u8]) -> String {
    format!("{:016x}", fnv1a64(bytes))
}

/* resolved symbols (trait impls, closures, generics) — each is covered here:
 *
 * `hydra_map::scene::_::<impl Deserialize for MapScene>::deserialize::__Visitor::expecting`
 *     -> `scene::MapScene::from_json`
 * `… for SceneDoor / SceneEdge / SceneRoom / SceneLabel …::expecting`
 *     -> `scene::{SceneDoor, SceneEdge, SceneRoom, SceneLabel}::from_value`
 * `hydra_map::overrides::_::<impl Deserialize for Overrides>::deserialize::__Visitor::expecting`
 *     -> `overrides::Overrides::from_json`
 * `hydra_map::overrides::_::<impl Serialize for Exported>::serialize`
 *     -> `overrides::Exported::write`
 * `hydra_map::atlas::Atlas::{ask, laid, waiting_for, work}` (private)
 *     -> `Atlas::{ask, laid, waiting_for, work}`
 * `hydra_map::next_door::{cells, beside}`, `hydra_map::room::found::{{closure}}`,
 * `hydra_map::overrides::{written, filed_in}`
 *     -> `next_door::{cells, beside}`, `room::found`, `overrides::{written, filed_in}`
 * `hydra_map::lay::{open_place, marks, places, convert, lay_out}` and
 * `cena_map_layout::*`
 *     -> `cena_map_layout`, outside this crate; the drawing seam (see `NOTES.md`).
 */

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cena::Room;

    #[test]
    fn fnv1a64_matches_the_reference_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
        assert_eq!(fnv1a64_hex(b"a"), "af63dc4c8601ec8c");
    }

    #[test]
    fn decode_fingerprints_and_wraps_the_map() {
        let map = Map::from_rooms(vec![Room {
            id: 7116001,
            title: "The Tenebrous Cauldron".into(),
            areas: vec!["North Blue".into()],
            ..Default::default()
        }]);
        let bytes = cena::encode(&map);
        let loaded = decode(&bytes).unwrap();
        assert_eq!(loaded.fingerprint, fnv1a64_hex(&bytes));
        assert_eq!(
            loaded.map.room(7116001).unwrap().title,
            "The Tenebrous Cauldron"
        );
        assert_eq!(decode(&bytes).unwrap().fingerprint, loaded.fingerprint);
        let mut other = bytes.clone();
        other[8] ^= 1;
        assert_ne!(decode(&other).unwrap().fingerprint, loaded.fingerprint);
    }

    #[test]
    fn decode_refuses_bytes_that_are_not_a_map() {
        assert_eq!(
            decode(b"nope"),
            Err("the map cannot be used: not a Hydra map (no HYDRAMAP magic)".to_string())
        );
    }

    #[test]
    fn load_reads_the_file_the_env_var_names() {
        let dir = std::env::temp_dir().join(format!("hydra-map-load-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("map.bin");
        std::fs::write(
            &path,
            cena::encode(&Map::from_rooms(vec![Room {
                id: 7,
                title: "Seven".into(),
                ..Default::default()
            }])),
        )
        .unwrap();
        // `HYDRA_MAP` is process-wide; this is the only test that sets it.
        std::env::set_var(MAP_ENV, &path);
        let loaded = load().unwrap();
        std::env::remove_var(MAP_ENV);
        assert_eq!(loaded.map.room(7).unwrap().title, "Seven");

        std::env::set_var(MAP_ENV, dir.join("missing.bin"));
        let err = load().unwrap_err();
        std::env::remove_var(MAP_ENV);
        assert!(err.contains(" cannot be read: "), "{err}");
        assert!(err.contains("missing.bin"), "{err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn shared_answers_the_same_way_every_time() {
        let first = shared().clone();
        assert_eq!(first, *shared());
        // It is already initialised, so a later `share` is refused.
        assert!(share(Err("other".into())).is_err());
    }
}
