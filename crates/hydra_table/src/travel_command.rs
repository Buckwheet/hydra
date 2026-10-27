//! The `.go2` travel command's message layer and its walk claims.
//!
//! Recovered from `hydra_table::travel_command` (`0x10121e604` … `0x101224140`).
//! The route finding itself lives in `hydra_travel` (`hydra_travel::file::load`,
//! `forget_target`, `walk::preflight::gigas_way`, `settings::TravelSettings`) and
//! `cena_map` (`map::Map::room`, `route::Routes::path_to`), neither of which is
//! part of this reconstruction.  What *is* recovered here is the wording — every
//! message below was read out of `__TEXT` at the address in its doc comment,
//! together with the argument count and order the original's `format_inner`
//! calls pass — plus the shape of the global walk registry, whose cancellation
//! is the original's real `tokio_util::sync::CancellationToken`.
//!
//! So this module is the command's **answer builder**: the callers hand it the
//! resolved route data and get back the line Hydra types into the game.

pub mod sea;

use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};

use tokio_util::sync::CancellationToken;

use crate::pages;

/// `.go2 save <name>` line shown when the room has no saved names.
///
/// **ESTABLISHED**: 53 bytes at `0x102408985` (`travel_command::listed`,
/// `0x101222d18`), copied into a 53-byte allocation and returned as the single
/// element of the list.
pub const NO_NAMES: &str = "No names are saved: .go2 save <name> names this room.";

/// The room is not known to the map.
///
/// **ESTABLISHED**: 33 bytes at `0x10240877a` (`travel_command::rooms_meant`,
/// `0x10121e704`).
pub const NO_ROOM: &str = "I cannot tell which room this is.";

/// `.go2` stopped because the room could not be read.
///
/// **ESTABLISHED**: `0x1024088c4` (`I do not know what room this is, so I have
/// stopped.`).
pub const UNKNOWN_ROOM_STOPPED: &str = "I do not know what room this is, so I have stopped.";

/// `.go2` stopped because the walk kept drifting.
///
/// **ESTABLISHED**: `0x102408900` (`I keep being carried off the route, so I
/// have stopped.`).
pub const OFF_ROUTE_STOPPED: &str = "I keep being carried off the route, so I have stopped.";

/// `.go2` stopped because the connection went away.
///
/// **ESTABLISHED**: `0x102408948` (`stopped, because the connection went
/// away.`).
pub const DISCONNECTED_STOPPED: &str = "stopped, because the connection went away.";

/// The character cannot walk there at all.
///
/// **ESTABLISHED**: `0x1024088e8` (`there is no way there that Hydra can walk
/// this character.`).
pub const CANNOT_WALK: &str = "there is no way there that Hydra can walk this character.";

/// `set_out` was cancelled before the first step.
///
/// **ESTABLISHED**: 28 bytes at `0x10240887f` (`travel_command::set_out`,
/// `0x101222e1c`).
pub const STOPPED_BEFORE_SETTING_OUT: &str = "Stopped before setting out.";

/// The travel setting that says whether `call wind` may be used.
///
/// **ESTABLISHED**: the 9-byte literal at `0x1024088a1`, passed to
/// `hydra_travel::settings::TravelSettings::on` in [`helm`] (`0x101221128`).
pub const CALL_WIND: &str = "call_wind";

/// The travel setting for typing ahead.
///
/// **ESTABLISHED**: the 9-byte literal at `0x1024088b0`, read in
/// `travel_command::ended` (`0x101221b08`).
pub const TYPEAHEAD: &str = "typeahead";

/// The travel setting for docking.
///
/// **ESTABLISHED**: the 4-byte literal at `0x10240889d`, used by
/// `travel_command::sea::dock_npc` (`0x101220edc`).
pub const DOCK: &str = "dock";

/// Whether `.go2` may `call wind` on the way.
///
/// **ESTABLISHED** (`travel_command::helm`, `0x101221100`):
/// `pages::travel_settings().on("call_wind")`.
///
/// **Departure**: `hydra_travel::settings::TravelSettings` is not reconstructed,
/// so the settings map read here is the one [`crate::pages::travel_settings`]
/// returns.
pub fn helm(root: &std::path::Path, who: &str) -> bool {
    pages::travel_settings(root, who)
        .get(CALL_WIND)
        .copied()
        .unwrap_or(false)
}

/// The saved names of a room, or the "no names are saved" hint.
///
/// **ESTABLISHED** (`travel_command::listed`, `0x101222cb4`): the names are a
/// `Vec<String>` at `+0x20` of the argument (length `+0x30`); when the list is
/// empty the answer is a one-element list holding [`NO_NAMES`].
///
/// **INFERRED**: the argument is taken as the slice of names itself rather than
/// the whole travel state.
pub fn listed(names: &[String]) -> Vec<String> {
    if names.is_empty() {
        vec![NO_NAMES.to_string()]
    } else {
        names.to_vec()
    }
}

/// "You are with … already."
///
/// **ESTABLISHED** (`travel_command::already`, `0x101222dac`): the format string
/// at `0x103e62431` is `You are with {} already ({}).`; the two arguments are the
/// `Option<&str>` (a 16-byte pair at `sp+0x10`) and the 32-bit room id
/// (`sp+0xc`).  When the name is `None` only the id is shown.
///
/// **UNKNOWN**: the wording of the `None` arm — its format string is the one at
/// `0x103e62441` in the same blob but its pieces are not separated by a
/// terminator, so only the shape (id only) is certain.
pub fn already(room: u32, name: Option<&str>) -> String {
    match name {
        Some(name) => format!("You are with {name} already ({room})."),
        None => format!("You are with a room already ({room})."),
    }
}

/// A named way: "The way to …".
///
/// **ESTABLISHED**: the format string at `0x1024089cc` —
/// `The way to {} ({}): {} step{}, about {} seconds.` — with the arguments
/// (name, room id, step count, the `{}` of `step{}`, the seconds).  The plural
/// marker is a separate one-byte literal (`s` at `0x1024089e2`), which is why
/// the original writes `step{}`.
pub fn way_named(name: &str, room: u32, steps: u32, seconds: u32) -> String {
    let plural = if steps == 1 { "" } else { "s" };
    format!("The way to {name} ({room}): {steps} step{plural}, about {seconds} seconds.")
}

/// The step count of the longest way out of a room.
///
/// **ESTABLISHED** (`travel_command::ways_listed`, `0x10121e7a0`): a max
/// reduction over a `0x38`-byte-stride slice starting from `[arg, #0x10]`,
/// taking the 64-bit field at `+0x00` of every element.
///
/// **INFERRED**: that field is the number of steps.
pub fn ways_listed(ways: &[Way]) -> u32 {
    ways.iter().map(|way| way.steps).max().unwrap_or(0)
}

/// One way out of the current room, as much as the message layer needs.
///
/// **INFERRED**: the original's element type is `hydra_travel`'s route step
/// (`0x38` bytes, step count at `+0x00`); only the step count is used by
/// [`ways_listed`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Way {
    pub steps: u32,
    pub seconds: u32,
}

/// The rooms a `.go2` query means.
///
/// **ESTABLISHED** (`travel_command::rooms_meant`, `0x10121e604`): with a
/// non-empty room list the answer comes from the list; with an empty list and a
/// known room (`w3` bit 0 set) the answer is the single room in `w4`
/// (`mov w0, #4` allocates exactly one `u32`); otherwise `Err(`[`NO_ROOM`]`)`.
///
/// **INFERRED**: the argument pair is modelled as `Option<u32>` for the room the
/// party is standing in.
pub fn rooms_meant(rooms: &[u32], here: Option<u32>) -> Result<Vec<u32>, String> {
    if !rooms.is_empty() {
        return Ok(rooms.to_vec());
    }
    match here {
        Some(room) => Ok(vec![room]),
        None => Err(NO_ROOM.to_string()),
    }
}

/// "I do not know which side you came into … from".
///
/// **ESTABLISHED**: the format string at `0x103e61e8a`
/// (`travel_command::side_unknown`, `0x10121e8b8`).
pub fn side_unknown(room: &str, setting: &str) -> String {
    format!(
        "I do not know which side you came into {room} from, so the way back is not priced. \
         Set {setting} on the Travel page, or ride in again."
    )
}

/// "No way by land to …: .go2 swims there."
///
/// **ESTABLISHED**: the piece at `0x103e61f9c` (`No way by land to {}: .go2
/// swims there.`), used by the sea module.
pub fn no_way_by_land_then_swim(destination: &str) -> String {
    format!("No way by land to {destination}: .go2 swims there.")
}

/// "No way by land to …: .go2 takes your ship."
///
/// **ESTABLISHED**: the format string at `0x103e62048`.
pub fn no_way_by_land_then_ship(destination: &str) -> String {
    format!("No way by land to {destination}: .go2 takes your ship.")
}

/// "No way by land to …. Your own ship could take you, from … to …".
///
/// **ESTABLISHED**: the format string at `0x103e61fd8`.
pub fn no_way_by_land_own_ship(destination: &str, from: &str, to: &str) -> String {
    format!(
        "No way by land to {destination}. Your own ship could take you, from {from} to {to}: \
         turn on \"Use your own ship\" on the Travel page."
    )
}

/// "Your own ship could take you, from … to …".
///
/// **ESTABLISHED**: the format string at `0x103e61f78`.
pub fn own_ship_could_take_you(from: &str, to: &str) -> String {
    format!(
        "Your own ship could take you, from {from} to {to}: turn on \"Use your own ship\" \
         on the Travel page."
    )
}

/// "… is a room of the open sea, … by ship from …".
///
/// **ESTABLISHED**: the format string at `0x103e61f10`
/// (`{} is a room of the open sea, {} by ship from {}. To sail there and drop
/// anchor: .go2 {} --no-confirm`).
pub fn open_sea(room: &str, rooms: u32, from: &str, go: &str) -> String {
    let sea = if rooms == 1 { "room" } else { "rooms" };
    format!(
        "{room} is a room of the open sea, {rooms} {sea} by ship from {from}. \
         To sail there and drop anchor: .go2 {go} --no-confirm"
    )
}

/// "{} rooms".
///
/// **ESTABLISHED**: the format string at `0x103e61f07`.
pub fn rooms(count: u32) -> String {
    format!("{count} rooms")
}

/// ", about {} seconds".
///
/// **ESTABLISHED**: the piece at `0x10240885f` of the same format-blob family as
/// [`way_named`]; used by the sea walker.
pub fn about_seconds(seconds: u32) -> String {
    format!(", about {seconds} seconds")
}

/// A room on the walk, ready to be shown.
///
/// **ESTABLISHED** (`travel_command::in_a_line`, `0x1012239b0`): the room name
/// comes from `cena_map::map::Map::room(id)` (`0x101662e80`) — the `String` at
/// `+0x38` of the room — and the walk is a slice of `0x78`-byte steps whose last
/// step's 16-byte command is read at `[end - 0x10]`.
///
/// **UNKNOWN**: the step type (`hydra_travel`'s, 120 bytes) and the `cena_map`
/// room lookup are not reconstructed, so the caller passes the name and the next
/// command.
pub fn in_a_line(room_name: &str, next: Option<&str>) -> String {
    match next {
        Some(command) => format!("{room_name}: {command}"),
        None => room_name.to_string(),
    }
}

/// What `.go2 forget` says.
///
/// **ESTABLISHED** (`travel_command::forgotten`, `0x1012237a0`): the name is
/// removed with `hydra_travel::file::forget_target` (`0x1015055c8`); the answer
/// is `everyone's` (the 10-byte literal at `0x1024089ba`, `mov w9, #0xa`) or the
/// plain name, followed by the forgotten text.
pub fn forgotten(name: &str, shared: bool) -> String {
    if shared {
        format!("everyone's {name} is forgotten.")
    } else {
        format!("{name} is forgotten.")
    }
}

/// What `.go2 notes` answers.
///
/// **ESTABLISHED** (`travel_command::notes`, `0x101222940`):
/// `hydra_travel::file::load` (`0x10150858c`) answers a three-way enum (`cmp w8,
/// #0x2` = the "loaded" arm); a refusal is rendered with
/// `<hydra_travel::file::Refused as Display>::fmt` (`0x101510ea0`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notes {
    /// The travel file's notes, one string per line.
    Notes(Vec<String>),
    /// The travel file refused the query; the `Display` text of the refusal.
    Refused(String),
    /// The travel file could not be read at all.
    Unreadable(String),
}

/// The notes for the current travel file.
///
/// **INFERRED**: `hydra_travel::file` is not reconstructed, so the caller
/// supplies what the load produced.
pub fn notes(notes: Vec<String>, refused: Option<String>, unreadable: Option<String>) -> Notes {
    if let Some(error) = unreadable {
        return Notes::Unreadable(error);
    }
    if let Some(refusal) = refused {
        return Notes::Refused(refusal);
    }
    Notes::Notes(notes)
}

/// How the first leg of a several-stop journey is reached.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Dock {
    /// No way to the dock handler is known.
    #[default]
    Unknown,
    /// The way to the dock handler, in words.
    Reached(String),
}

/// The sea leg of a several-stop journey.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sail {
    pub from: String,
    pub to: String,
    pub rooms: u32,
}

/// The pier leg of a several-stop journey.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pier {
    pub from: String,
    pub to: String,
    pub rooms: Option<String>,
}

/// A several-stop journey.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub start: String,
    pub dock: Dock,
    pub sail: Option<Sail>,
    pub pier: Option<Pier>,
}

/// The dock handler line, `1.` of a several-stop answer.
///
/// **ESTABLISHED**: `1. To {}'s dock handler: no way there is known from here.`
/// (`0x103e62073`) and `1. To {}'s dock handler: {}.` (`0x103e620b2`).
pub fn dock_line(start: &str, how: Option<&str>) -> String {
    match how {
        None => format!("1. To {start}'s dock handler: no way there is known from here."),
        Some(how) => format!("1. To {start}'s dock handler: {how}."),
    }
}

/// The `2. Sail from … to …` line.
///
/// **ESTABLISHED**: `2. Sail from {} to {}.` (`0x103e620d0`) and
/// `2. Sail from {} to {}: {} rooms of sea.` (`0x103e620e8`).
pub fn sail_line(from: &str, to: &str, rooms: u32) -> String {
    if rooms == 0 {
        format!("2. Sail from {from} to {to}.")
    } else {
        format!("2. Sail from {from} to {to}: {rooms} rooms of sea.")
    }
}

/// The `3. From …'s pier to …` line.
///
/// **ESTABLISHED**: `3. From {}'s pier to {}.` and `3. From {}'s pier to {}:
/// {}.` (both in the `0x103e620e8` blob).
pub fn pier_line(from: &str, to: &str, rooms: Option<&str>) -> String {
    match rooms {
        None => format!("3. From {from}'s pier to {to}."),
        Some(rooms) => format!("3. From {from}'s pier to {to}: {rooms}."),
    }
}

/// A several-stop answer, numbered.
///
/// **ESTABLISHED** (`travel_command::several`, `0x101223184`): a `Vec<String>`
/// built from the numbered pieces recovered in [`dock_line`], [`sail_line`] and
/// [`pier_line`].
pub fn several(plan: &Plan) -> Vec<String> {
    let mut lines = Vec::new();
    match &plan.dock {
        Dock::Unknown => lines.push(dock_line(&plan.start, None)),
        Dock::Reached(how) => lines.push(dock_line(&plan.start, Some(how))),
    }
    if let Some(sail) = &plan.sail {
        lines.push(sail_line(&sail.from, &sail.to, sail.rooms));
    }
    if let Some(pier) = &plan.pier {
        lines.push(pier_line(&pier.from, &pier.to, pier.rooms.as_deref()));
    }
    lines
}

/// Whether the current room can reach `destination` on foot.
///
/// **ESTABLISHED** (`travel_command::reachable`, `0x101223b78`):
/// `cena_map::route::<impl Map>::routes` (`0x10128c8a0`) then
/// `cena_map::route::Routes::path_to` (`0x1016765cc`) — the answer is whether a
/// path came back.
///
/// **UNKNOWN**: `cena_map` is not reconstructed, so the caller supplies the
/// path finding result.
pub fn reachable(path_found: bool) -> bool {
    path_found
}

/// "Who is this way for?" — the first name the iterator yields.
///
/// **ESTABLISHED** (`travel_command::to_whom`, `0x101223368`): three inlined
/// `Iterator::try_fold` calls over the candidate list, i.e. a short-circuiting
/// search for the first match.
///
/// **INFERRED**: the search is modelled as the first candidate that is not
/// empty.
pub fn to_whom(candidates: &[String]) -> Option<&str> {
    candidates
        .iter()
        .map(|candidate| candidate.trim())
        .find(|candidate| !candidate.is_empty())
}

/// A running walk, held in one global registry.
///
/// **ESTABLISHED**: `Claim::take` (`0x101221164`) lower-cases the name, locks a
/// `Mutex` that sits next to a `CancellationToken`, takes the entry out of a
/// `HashMap` (`hashbrown::map::HashMap::rustc_entry` at `0x101249d5c`) and
/// clones the string; `stop_walk` (`0x101223c80`) and `Claim::set_out`
/// (`0x1012214b4`) use the same three statics at `0x104586b90`, `0x104586b98`
/// and `0x104586bd8`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Claim {
    /// The character the walk belongs to, lower-cased.
    pub who: String,
    /// Whether the walk has been cancelled (`CancellationToken`).
    pub cancelled: bool,
    /// The cancellation token id the original cloned (`0x101d378b4`).
    pub token: u64,
}

impl Claim {
    /// Take the claim for `who` out of the registry.
    ///
    /// **ESTABLISHED** (`Claim::take`, `0x101221164`): `who.trim().to_lowercase()`
    /// (`<str>::to_lowercase` at `0x1012211b0`), lock, take the map entry.
    pub fn take(who: &str) -> Option<Claim> {
        claims().lock().unwrap().remove(&claim_key(who))
    }

    /// Put the claim for `who` into the registry.
    ///
    /// **ESTABLISHED** (`Claim::set_out`, `0x1012214b4`): the same key and map as
    /// [`Claim::take`].
    pub fn set_out(claim: Claim) -> Option<Claim> {
        claims()
            .lock()
            .unwrap()
            .insert(claim_key(&claim.who), claim)
    }

    /// Cancel the walk for `who`, if there is one.
    ///
    /// **ESTABLISHED** (`travel_command::stop_walk`, `0x101223c80`): the same
    /// registry statics as [`Claim::take`], and the answer text comes from
    /// `travel_command::ended`.
    ///
    /// The cancellation itself is the real thing the original did: the walk's
    /// [`CancellationToken`] (see [`claim_token`]) is cancelled first, so a walk
    /// that is awaiting `token.cancelled()` stops, and the claim is then marked
    /// `cancelled` for the recovered answer text.
    pub fn stop_walk(who: &str) -> Option<Claim> {
        let key = claim_key(who);
        if let Some(token) = tokens().lock().unwrap().get(&key) {
            token.cancel();
        }
        let mut claims = claims().lock().unwrap();
        match claims.get_mut(&key) {
            None => None,
            Some(claim) => {
                claim.cancelled = true;
                Some(claim.clone())
            }
        }
    }
}

/// The registry key: `trim().to_lowercase()`.
///
/// **ESTABLISHED**: `Claim::take` and `stop_walk` both lower-case the name
/// before the map lookup.
pub fn claim_key(who: &str) -> String {
    who.trim().to_lowercase()
}

fn claims() -> &'static Mutex<BTreeMap<String, Claim>> {
    static CLAIMS: LazyLock<Mutex<BTreeMap<String, Claim>>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));
    &CLAIMS
}

/// The live cancellation token of `who`'s walk, creating it on first use.
///
/// **ESTABLISHED**: the claim registry is a `Mutex<HashMap>` sitting next to a
/// `CancellationToken` (`0x104586b90`, `0x104586b98`, `0x104586bd8`), and
/// `set_sail` (`0x101223430`) clones that token (`0x101d378b4`) when it replaces
/// the claim.  [`Claim`] keeps the recovered *value* shape (`cancelled`,
/// `token`); the token a walk actually awaits is this
/// `tokio_util::sync::CancellationToken`, which [`Claim::stop_walk`] cancels.
///
/// A token that has already been cancelled is replaced by a fresh one, so the
/// next walk for the same character starts live.
///
/// A walk loop waits on it with
/// `claim_token(who).cancelled().await` inside its `select!`.
pub fn claim_token(who: &str) -> CancellationToken {
    let mut tokens = tokens().lock().unwrap();
    let token = tokens.entry(claim_key(who)).or_default();
    if token.is_cancelled() {
        *token = CancellationToken::new();
    }
    token.clone()
}

/// Whether `who`'s walk has been stopped.
///
/// True once [`Claim::stop_walk`] has cancelled the walk's real token (or once
/// the claim itself is marked `cancelled`).
pub fn stopped(who: &str) -> bool {
    let key = claim_key(who);
    if let Some(token) = tokens().lock().unwrap().get(&key) {
        if token.is_cancelled() {
            return true;
        }
    }
    claims()
        .lock()
        .unwrap()
        .get(&key)
        .map(|claim| claim.cancelled)
        .unwrap_or(false)
}

fn tokens() -> &'static Mutex<BTreeMap<String, CancellationToken>> {
    static TOKENS: LazyLock<Mutex<BTreeMap<String, CancellationToken>>> =
        LazyLock::new(|| Mutex::new(BTreeMap::new()));
    &TOKENS
}

/// What the game said about the gigas way.
///
/// **ESTABLISHED** (`travel_command::Gigas::said`, `0x10122180c`):
/// `hydra_travel::walk::preflight::gigas_way` (`0x101528c70`) decides, and the
/// `Vec` is built from the recovered lines.
///
/// **UNKNOWN**: `gigas_way` is not reconstructed; the caller supplies whether the
/// way is a gigas way and which lines it wants.
pub fn gigas_said(lines: Vec<String>) -> Vec<String> {
    lines
}

/// Why a walk ended, as `.go2` reports it.
///
/// **ESTABLISHED** (`travel_command::ended`, `0x101221b08`): the three end
/// messages ([`UNKNOWN_ROOM_STOPPED`], [`OFF_ROUTE_STOPPED`],
/// [`DISCONNECTED_STOPPED`]) and [`CANNOT_WALK`] are chosen here, and the
/// `typeahead` travel setting is read next to them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkEnd {
    /// The room could not be read.
    UnknownRoom,
    /// The walk kept drifting off the route.
    OffRoute,
    /// The connection went away.
    Disconnected,
    /// There is no walkable way there.
    NoWay,
}

/// The line `.go2` types when a walk ends.
///
/// **ESTABLISHED** (`travel_command::ended`, `0x101221b08`): the four texts in
/// the `0x1024088b0` … `0x102408948` blob, with the destination name prefixed by
/// the original (`{} stopped, …` family).
pub fn ended(destination: &str, why: WalkEnd) -> String {
    match why {
        WalkEnd::UnknownRoom => format!("{destination}: {UNKNOWN_ROOM_STOPPED}"),
        WalkEnd::OffRoute => format!("{destination}: {OFF_ROUTE_STOPPED}"),
        WalkEnd::Disconnected => format!("{destination}: {DISCONNECTED_STOPPED}"),
        WalkEnd::NoWay => format!("{destination}: {CANNOT_WALK}"),
    }
}

/// Start walking, or say why not.
///
/// **ESTABLISHED** (`travel_command::set_out`, `0x101222e1c`): the answer is
/// either [`STOPPED_BEFORE_SETTING_OUT`] — when the cancellation token is
/// already set before the first step — or the first command to type.
///
/// **INFERRED**: the first command is supplied by the caller (the route comes
/// from `hydra_travel`, which is not reconstructed).
pub fn set_out(cancelled: bool, first_step: Option<&str>) -> String {
    if cancelled {
        return STOPPED_BEFORE_SETTING_OUT.to_string();
    }
    match first_step {
        Some(step) => step.to_string(),
        None => STOPPED_BEFORE_SETTING_OUT.to_string(),
    }
}

/// Begin sailing: take the claim, mark it as setting out, return its token.
///
/// **ESTABLISHED** (`travel_command::set_sail`, `0x101223430`):
/// `Claim::take` (`0x101221164`) then `Claim::set_out` (`0x1012214b4`) and a
/// `CancellationToken::clone` (`0x101d378b4`), i.e. the claim is replaced by a
/// fresh one carrying the same token.
pub fn set_sail(who: &str) -> Option<Claim> {
    let old = Claim::take(who);
    let token = old.as_ref().map(|claim| claim.token).unwrap_or_default();
    let claim = Claim {
        who: who.trim().to_string(),
        cancelled: false,
        token,
    };
    Claim::set_out(claim.clone());
    Some(claim)
}

/// Stop the walk for `who`.
///
/// **ESTABLISHED** (`travel_command::stop_walk`, `0x101223c80`): the same
/// registry statics as [`Claim::take`]; the claim is marked cancelled and kept
/// for the answer.
pub fn stop_walk(who: &str) -> Option<Claim> {
    Claim::stop_walk(who)
}

/// The gigas way check.
///
/// **ESTABLISHED**: `travel_command::Gigas::said` (`0x10122180c`) is the only
/// user of `hydra_travel::walk::preflight::gigas_way` (`0x101528c70`).
///
/// **UNKNOWN**: `gigas_way` is not reconstructed, so [`Gigas::said`] passes the
/// lines the caller built.
pub struct Gigas;

impl Gigas {
    /// The lines the game gets for a gigas way.
    pub fn said(lines: Vec<String>) -> Vec<String> {
        gigas_said(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recovered_messages_are_exact() {
        assert_eq!(
            NO_NAMES,
            "No names are saved: .go2 save <name> names this room."
        );
        assert_eq!(NO_NAMES.len(), 53, "the recovered piece is 53 bytes");
        assert_eq!(NO_ROOM, "I cannot tell which room this is.");
        assert_eq!(NO_ROOM.len(), 33);
        assert_eq!(STOPPED_BEFORE_SETTING_OUT, "Stopped before setting out.");
        assert_eq!(STOPPED_BEFORE_SETTING_OUT.len(), 27);
        assert_eq!(CALL_WIND, "call_wind");
        assert_eq!(TYPEAHEAD, "typeahead");
        assert_eq!(DOCK, "dock");
        assert_eq!(
            CANNOT_WALK,
            "there is no way there that Hydra can walk this character."
        );
    }

    #[test]
    fn a_room_without_names_says_how_to_save_one() {
        assert_eq!(listed(&[]), vec![NO_NAMES.to_string()]);
        assert_eq!(listed(&["bank".to_string()]), vec!["bank".to_string()]);
    }

    #[test]
    fn already_names_the_room_and_the_id() {
        assert_eq!(
            already(1234, Some("the bank")),
            "You are with the bank already (1234)."
        );
    }

    #[test]
    fn a_named_way_counts_its_steps() {
        assert_eq!(
            way_named("the bank", 77, 1, 3),
            "The way to the bank (77): 1 step, about 3 seconds."
        );
        assert_eq!(
            way_named("the bank", 77, 12, 40),
            "The way to the bank (77): 12 steps, about 40 seconds."
        );
    }

    #[test]
    fn the_longest_way_wins() {
        let ways = [
            Way {
                steps: 3,
                seconds: 5,
            },
            Way {
                steps: 11,
                seconds: 20,
            },
        ];
        assert_eq!(ways_listed(&ways), 11);
        assert_eq!(ways_listed(&[]), 0);
    }

    #[test]
    fn rooms_meant_falls_back_to_the_room_you_are_in() {
        assert_eq!(rooms_meant(&[1, 2], Some(9)), Ok(vec![1, 2]));
        assert_eq!(rooms_meant(&[], Some(9)), Ok(vec![9]));
        assert_eq!(rooms_meant(&[], None), Err(NO_ROOM.to_string()));
    }

    #[test]
    fn the_sea_answers_are_the_recovered_wording() {
        assert_eq!(rooms(4), "4 rooms");
        assert_eq!(
            own_ship_could_take_you("the landing", "the pier"),
            "Your own ship could take you, from the landing to the pier: turn on \
             \"Use your own ship\" on the Travel page."
        );
        assert_eq!(
            no_way_by_land_then_ship("the pier"),
            "No way by land to the pier: .go2 takes your ship."
        );
        assert_eq!(about_seconds(12), ", about 12 seconds");
    }

    #[test]
    fn a_several_stop_plan_is_numbered() {
        let plan = Plan {
            start: "the landing".to_string(),
            dock: Dock::Reached("3 steps".to_string()),
            sail: Some(Sail {
                from: "a pier".to_string(),
                to: "the far pier".to_string(),
                rooms: 12,
            }),
            pier: Some(Pier {
                from: "the far pier".to_string(),
                to: "the bank".to_string(),
                rooms: None,
            }),
        };
        assert_eq!(
            several(&plan),
            vec![
                "1. To the landing's dock handler: 3 steps.".to_string(),
                "2. Sail from a pier to the far pier: 12 rooms of sea.".to_string(),
                "3. From the far pier's pier to the bank.".to_string(),
            ]
        );
        assert_eq!(
            dock_line("the landing", None),
            "1. To the landing's dock handler: no way there is known from here."
        );
        assert_eq!(sail_line("a", "b", 0), "2. Sail from a to b.");
        assert_eq!(
            pier_line("a", "b", Some("9 rooms")),
            "3. From a's pier to b: 9 rooms."
        );
    }

    #[test]
    fn a_claim_is_taken_and_cancelled_by_lower_cased_name() {
        Claim::set_out(Claim {
            who: "Aelith".to_string(),
            cancelled: false,
            token: 7,
        })
        ;
        let stopped = Claim::stop_walk(" AELITH ").expect("claim");
        assert!(stopped.cancelled);
        assert_eq!(stopped.token, 7);
        let taken = Claim::take("aelith").expect("still there");
        assert!(taken.cancelled);
        assert_eq!(Claim::take("aelith"), None);
    }

    #[tokio::test]
    async fn stopping_a_walk_cancels_the_real_cancellation_token() {
        let who = "Tokenwalker";
        let _ = Claim::take(who);
        Claim::set_out(Claim {
            who: who.to_string(),
            cancelled: false,
            token: 0,
        });
        assert!(!stopped(who));
        let token = claim_token(who);
        assert!(!token.is_cancelled(), "a fresh walk starts live");
        let waiting = token.clone();
        let walking = tokio::spawn(async move {
            waiting.cancelled().await;
        });
        assert!(!walking.is_finished(), "nothing has cancelled it yet");

        let stopped_claim = stop_walk(who).expect("claim");
        assert!(stopped_claim.cancelled);
        // The walk that was awaiting the token finishes.
        tokio::time::timeout(std::time::Duration::from_secs(5), walking)
            .await
            .expect("the token is cancelled")
            .expect("the walking task finished");
        assert!(token.is_cancelled());
        assert!(stopped(who));

        // The next walk for the same character gets a live token again.
        let _ = Claim::take(who);
        assert!(!claim_token(who).is_cancelled());
        assert!(!stopped(who));
    }

    #[test]
    fn to_whom_skips_blank_candidates() {
        let candidates = vec!["  ".to_string(), " Aelith ".to_string()];
        assert_eq!(to_whom(&candidates), Some("Aelith"));
        assert_eq!(to_whom(&[]), None);
    }
}
