//! The JSON wire format spoken between Hydra and the web page.
//!
//! Everything here was read out of the binary's serde glue:
//!
//! * `Out` is an **internally tagged** enum (`#[serde(tag = "t")]`) — the tag
//!   field is literally `"t"` and every variant's struct fields are spliced in
//!   next to it.  The full variant list is recovered (see [`Out::tag`]).
//! * `In` is the mirror image: the same `"t"` tag, 37 variants (`In::Login`,
//!   `In::Send`, `In::Wheel`, …).
//! * Every `Out` payload struct's field names *and their order* are recovered
//!   from `SerializeMap::serialize_entry` call sites, so the JSON below matches
//!   the original field for field.
//!
//! The original crate used `serde`/`serde_json`; this module writes and reads
//! the same documents through [`crate::json`].

use crate::json::Value;

// ---------------------------------------------------------------------------
// Leaf types
// ---------------------------------------------------------------------------

/// One styled span of text (`wire::Run`).
///
/// Recovered field list and order from `Serialize for wire::Run`
/// (`0x100270cd4`): `text, token, color, named, background, bold, mono, link,
/// prompt`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Run {
    pub text: String,
    /// Theme token name (`hydra_ui::theme::Token::name`).
    pub token: Option<String>,
    /// Resolved colour, e.g. `#a0c0ff`.
    pub color: Option<String>,
    /// Named palette entry the colour came from.
    pub named: Option<String>,
    pub background: Option<String>,
    pub bold: bool,
    pub mono: bool,
    /// The command a click should send, if the run is a link.
    pub link: Option<String>,
    /// The run is the game prompt.
    pub prompt: bool,
}

impl Run {
    /// A plain run.
    pub fn text(text: impl Into<String>) -> Run {
        Run {
            text: text.into(),
            ..Run::default()
        }
    }

    /// The JSON fields, in the recovered order.
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("text", Value::str(self.text.clone())),
            ("token", opt(&self.token)),
            ("color", opt(&self.color)),
            ("named", opt(&self.named)),
            ("background", opt(&self.background)),
            ("bold", Value::Bool(self.bold)),
            ("mono", Value::Bool(self.mono)),
            ("link", opt(&self.link)),
            ("prompt", Value::Bool(self.prompt)),
        ]
    }
}

fn opt(v: &Option<String>) -> Value {
    match v {
        Some(s) => Value::str(s.clone()),
        None => Value::Null,
    }
}

fn strs(xs: &[String]) -> Value {
    Value::Arr(xs.iter().map(|s| Value::str(s.clone())).collect())
}

fn runs_value(runs: &[Run]) -> Value {
    arr(runs.iter().map(Run::fields).collect())
}

/// Build an array out of already-built `(key, value)` field lists.
fn arr(items: Vec<Vec<(&'static str, Value)>>) -> Value {
    Value::Arr(items.into_iter().map(object).collect())
}

/// Turn a field list into an object value (insertion order kept).
pub fn object(fields: Vec<(&'static str, Value)>) -> Value {
    Value::Obj(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn opt_object<T>(v: &Option<T>, f: impl Fn(&T) -> Vec<(&'static str, Value)>) -> Value {
    match v {
        Some(x) => object(f(x)),
        None => Value::Null,
    }
}

fn opt_int(v: Option<u32>) -> Value {
    match v {
        Some(n) => Value::int(n),
        None => Value::Null,
    }
}

/// A vitals gauge.  Recovered order (`0x1002714b0`):
/// `label, token, percent, current, max`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Gauge {
    pub label: String,
    pub token: Option<String>,
    pub percent: u32,
    pub current: i64,
    pub max: i64,
}

impl Gauge {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("label", Value::str(self.label.clone())),
            ("token", opt(&self.token)),
            ("percent", Value::int(self.percent)),
            ("current", Value::int(self.current)),
            ("max", Value::int(self.max)),
        ]
    }
}

/// An effect row (`0x1002723ec`): `text, left, percent`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct EffectOut {
    pub text: String,
    pub left: Option<String>,
    pub percent: Option<u32>,
}

impl EffectOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("text", Value::str(self.text.clone())),
            ("left", opt(&self.left)),
            ("percent", opt_int(self.percent)),
        ]
    }
}

/// A group of effects (`0x1002728ec`): `effects, list`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct EffectList {
    pub effects: Vec<EffectOut>,
    pub list: String,
}

impl EffectList {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("effects", arr(self.effects.iter().map(EffectOut::fields).collect())),
            ("list", Value::str(self.list.clone())),
        ]
    }
}

/// A spell row (`0x100272028`): `name, circle, number`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct SpellOut {
    pub name: String,
    pub circle: u32,
    pub number: u32,
}

impl SpellOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", Value::str(self.name.clone())),
            ("circle", Value::int(self.circle)),
            ("number", Value::int(self.number)),
        ]
    }
}

/// A target row (`0x100272640`): `name, bare, noun, statuses, hint, health,
/// current`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct TargetOut {
    pub name: String,
    pub bare: String,
    pub noun: String,
    pub statuses: Vec<String>,
    pub hint: String,
    pub health: Option<String>,
    pub current: bool,
}

impl TargetOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", Value::str(self.name.clone())),
            ("bare", Value::str(self.bare.clone())),
            ("noun", Value::str(self.noun.clone())),
            ("statuses", strs(&self.statuses)),
            ("hint", Value::str(self.hint.clone())),
            ("health", opt(&self.health)),
            ("current", Value::Bool(self.current)),
        ]
    }
}

/// The known-targets panel (`0x100272c5c`): `look, targets, arms, known`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct TargetsOut {
    pub look: String,
    pub targets: Vec<TargetOut>,
    pub arms: String,
    pub known: bool,
}

impl TargetsOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("look", Value::str(self.look.clone())),
            ("targets", arr(self.targets.iter().map(TargetOut::fields).collect())),
            ("arms", Value::str(self.arms.clone())),
            ("known", Value::Bool(self.known)),
        ]
    }
}

/// One chip/tag row (`0x10027183c`): `name, mark, words, rows, color, level,
/// shown`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ChipOut {
    pub name: String,
    pub mark: String,
    pub words: Vec<String>,
    pub rows: Vec<ChipRowOut>,
    pub color: String,
    pub level: u32,
    pub shown: bool,
}

impl ChipOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", Value::str(self.name.clone())),
            ("mark", Value::str(self.mark.clone())),
            ("words", strs(&self.words)),
            ("rows", arr(self.rows.iter().map(ChipRowOut::fields).collect())),
            ("color", Value::str(self.color.clone())),
            ("level", Value::int(self.level)),
            ("shown", Value::Bool(self.shown)),
        ]
    }
}

/// A sub-row of a chip (`0x1002727d0`): `name, list, left, up`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ChipRowOut {
    pub name: String,
    pub list: String,
    pub left: String,
    pub up: bool,
}

impl ChipRowOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", Value::str(self.name.clone())),
            ("list", Value::str(self.list.clone())),
            ("left", Value::str(self.left.clone())),
            ("up", Value::Bool(self.up)),
        ]
    }
}

/// A merchant row (`0x100272d78`): `name, room, status, left, announcement,
/// said, rules`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MerchantOut {
    pub name: String,
    pub room: Option<String>,
    pub status: String,
    pub left: String,
    pub announcement: Option<String>,
    pub said: Option<String>,
    pub rules: Vec<String>,
}

impl MerchantOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", Value::str(self.name.clone())),
            ("room", opt(&self.room)),
            ("status", Value::str(self.status.clone())),
            ("left", Value::str(self.left.clone())),
            ("announcement", opt(&self.announcement)),
            ("said", opt(&self.said)),
            ("rules", strs(&self.rules)),
        ]
    }
}

/// A room description (`0x100271db8`): `description, creatures, objects,
/// players, title, exits`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct RoomOut {
    pub description: Vec<Run>,
    pub creatures: Vec<Run>,
    pub objects: Vec<Run>,
    pub players: Vec<Run>,
    pub title: Vec<Run>,
    pub exits: Vec<Run>,
}

impl RoomOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("description", runs_value(&self.description)),
            ("creatures", runs_value(&self.creatures)),
            ("objects", runs_value(&self.objects)),
            ("players", runs_value(&self.players)),
            ("title", runs_value(&self.title)),
            ("exits", runs_value(&self.exits)),
        ]
    }
}

/// A team/party row (`0x100271f0c`): `name, members`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct TeamOut {
    pub name: String,
    pub members: Vec<Run>,
}

impl TeamOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", Value::str(self.name.clone())),
            ("members", runs_value(&self.members)),
        ]
    }
}

/// A scroll banner (`0x100272160`): `text, key, left_ms`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct BannerOut {
    pub text: String,
    pub key: String,
    pub left_ms: u32,
}

impl BannerOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("text", Value::str(self.text.clone())),
            ("key", Value::str(self.key.clone())),
            ("left_ms", Value::int(self.left_ms)),
        ]
    }
}

/// A hotbar button (`0x100272298`): `id, label, fg, bg, countdown, dim`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ButtonOut {
    pub id: String,
    pub label: String,
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub countdown: String,
    pub dim: bool,
}

impl ButtonOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("id", Value::str(self.id.clone())),
            ("label", Value::str(self.label.clone())),
            ("fg", opt(&self.fg)),
            ("bg", opt(&self.bg)),
            ("countdown", Value::str(self.countdown.clone())),
            ("dim", Value::Bool(self.dim)),
        ]
    }
}

/// A hotbar (`0x1002715e8`): `name, slots, title, columns`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct BarOut {
    pub name: String,
    pub slots: Vec<ButtonOut>,
    pub title: String,
    pub columns: u32,
}

impl BarOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", Value::str(self.name.clone())),
            ("slots", arr(self.slots.iter().map(ButtonOut::fields).collect())),
            ("title", Value::str(self.title.clone())),
            ("columns", Value::int(self.columns)),
        ]
    }
}

/// A stream tab (`0x100272524`): `id, name`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct StreamOut {
    pub id: String,
    pub name: String,
}

impl StreamOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("id", Value::str(self.id.clone())),
            ("name", Value::str(self.name.clone())),
        ]
    }
}

/// A help row for the command hints (`0x1002719ac`): `command, says`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct HelpOut {
    pub command: String,
    pub says: Vec<Run>,
}

impl HelpOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("command", Value::str(self.command.clone())),
            ("says", runs_value(&self.says)),
        ]
    }
}

/// An inline picture (`0x100272b40`): `src, alt, width, height`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PictureOut {
    pub src: String,
    pub alt: String,
    pub width: u32,
    pub height: u32,
}

impl PictureOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("src", Value::str(self.src.clone())),
            ("alt", Value::str(self.alt.clone())),
            ("width", Value::int(self.width)),
            ("height", Value::int(self.height)),
        ]
    }
}

/// A menu entry (`0x100272ee8`): `label, command`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MenuEntryOut {
    pub label: String,
    pub command: String,
}

impl MenuEntryOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("label", Value::str(self.label.clone())),
            ("command", Value::str(self.command.clone())),
        ]
    }
}

/// A menu group (`0x100273004`): `under, entries`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MenuGroupOut {
    pub under: String,
    pub entries: Vec<MenuEntryOut>,
}

impl MenuGroupOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("under", Value::str(self.under.clone())),
            ("entries", arr(self.entries.iter().map(MenuEntryOut::fields).collect())),
        ]
    }
}

/// The game-login offer (`0x100272a08`): `account, game, characters`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ListingOut {
    pub account: String,
    pub game: String,
    pub characters: Vec<String>,
}

impl ListingOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("account", Value::str(self.account.clone())),
            ("game", Value::str(self.game.clone())),
            ("characters", strs(&self.characters)),
        ]
    }
}

/// One character card on the hub (`0x100270f4c`): `name, game, vitals, room,
/// state, state_token, id, closed`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize)]
pub struct Card {
    pub name: String,
    pub game: String,
    pub vitals: Vec<Gauge>,
    pub room: Option<Vec<Run>>,
    /// Recovered as two 16-byte slots (`&'static str`) at `+0x60`/`+0x70`.
    pub state: &'static str,
    pub state_token: &'static str,
    pub id: u32,
    pub closed: bool,
}

impl Card {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", Value::str(self.name.clone())),
            ("game", Value::str(self.game.clone())),
            ("vitals", arr(self.vitals.iter().map(Gauge::fields).collect())),
            (
                "room",
                match &self.room {
                    Some(runs) => runs_value(runs),
                    None => Value::Null,
                },
            ),
            ("state", Value::str(self.state)),
            ("state_token", Value::str(self.state_token)),
            ("id", Value::int(self.id)),
            ("closed", Value::Bool(self.closed)),
        ]
    }
}

/// The hub panel (`0x100271704`): `cards, offered, teams, said, listing`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize)]
pub struct HubOut {
    pub cards: Vec<Card>,
    pub offered: Vec<Card>,
    pub teams: Vec<TeamOut>,
    pub said: Vec<Run>,
    pub listing: Option<ListingOut>,
}

impl HubOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("cards", arr(self.cards.iter().map(Card::fields).collect())),
            ("offered", arr(self.offered.iter().map(Card::fields).collect())),
            ("teams", arr(self.teams.iter().map(TeamOut::fields).collect())),
            ("said", runs_value(&self.said)),
            ("listing", opt_object(&self.listing, ListingOut::fields)),
        ]
    }
}

/// A line of story text (`0x1002710d8`): `runs, stream, kind, picture, help`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Line {
    pub runs: Vec<Run>,
    pub stream: String,
    /// Recovered at `+0x88`; the original is a small tag.
    pub kind: String,
    pub picture: Option<PictureOut>,
    pub help: Option<HelpOut>,
}

impl Line {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("runs", runs_value(&self.runs)),
            ("stream", Value::str(self.stream.clone())),
            ("kind", Value::str(self.kind.clone())),
            ("picture", opt_object(&self.picture, PictureOut::fields)),
            ("help", opt_object(&self.help, HelpOut::fields)),
        ]
    }
}

/// The whole game panel (`0x100271244`).
///
/// Field order recovered verbatim: `state, state_token, vitals, roundtime,
/// casttime, left, right, prepared, room, effects, conditions, injuries,
/// spells, merchants, targets, chips`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct View {
    pub state: String,
    pub state_token: String,
    pub vitals: Vec<Gauge>,
    pub roundtime: Option<u32>,
    pub casttime: Option<u32>,
    pub left: Option<String>,
    pub right: Option<String>,
    pub prepared: Option<String>,
    pub room: Option<RoomOut>,
    pub effects: Vec<EffectList>,
    pub conditions: Vec<String>,
    pub injuries: Option<crate::injuries::DollOut>,
    pub spells: Vec<SpellOut>,
    pub merchants: Vec<MerchantOut>,
    pub targets: TargetsOut,
    pub chips: Vec<ChipOut>,
}

impl View {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("state", Value::str(self.state.clone())),
            ("state_token", Value::str(self.state_token.clone())),
            ("vitals", arr(self.vitals.iter().map(Gauge::fields).collect())),
            ("roundtime", opt_int(self.roundtime)),
            ("casttime", opt_int(self.casttime)),
            ("left", opt(&self.left)),
            ("right", opt(&self.right)),
            ("prepared", opt(&self.prepared)),
            ("room", opt_object(&self.room, RoomOut::fields)),
            ("effects", arr(self.effects.iter().map(EffectList::fields).collect())),
            ("conditions", strs(&self.conditions)),
            ("injuries", opt_object(&self.injuries, crate::injuries::DollOut::fields)),
            ("spells", arr(self.spells.iter().map(SpellOut::fields).collect())),
            ("merchants", arr(self.merchants.iter().map(MerchantOut::fields).collect())),
            ("targets", object(self.targets.fields())),
            ("chips", arr(self.chips.iter().map(ChipOut::fields).collect())),
        ]
    }
}

// ---------------------------------------------------------------------------
// Out — server to page
// ---------------------------------------------------------------------------

/// One message from Hydra to the web page.
///
/// The original is `#[serde(tag = "t")]`; the variants and payload field names
/// below are recovered from `Serialize for wire::Out` (`0x10026f844`).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
// The recovered representation: an internally tagged enum whose tag key is
// the one-byte string `"t"` and whose payload fields are spliced next to it.
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Out {
    /// `{"t":"welcome","login":..}`
    Welcome { login: String },
    /// `{"t":"workspace","ask":..,"status":..}`
    Workspace { ask: u32, status: u32 },
    /// `{"t":"banners","banners":[..],"ground":..,"across":..,"down":..,"width":..,"opacity":..}`
    Banners {
        banners: Vec<BannerOut>,
        ground: String,
        across: String,
        down: String,
        width: f32,
        opacity: f32,
    },
    /// `{"t":"offer","text":..}` — the game-login offer line.
    Offer { text: String },
    /// `{"t":"theme","colors":..,"names":..,"prompt":..}`
    Theme {
        colors: Vec<String>,
        names: Vec<String>,
        prompt: String,
    },
    /// `{"t":"said","text":..}` — echoed chatter.
    Said { text: String },
    /// `{"t":"view", ..}` — the whole game panel.
    View(View),
    /// `{"t":"push_paired"}`
    PushPaired,
    /// `{"t":"phone","kind":..,"text":..}`
    Phone { kind: String, text: String },
    /// `{"t":"hotbars","bars":..,"problems":..}`
    Hotbars { bars: Vec<BarOut>, problems: Vec<String> },
    /// `{"t":"stream","id":..,"from":..}` — a new stream tab.
    Stream { id: String, from: String },
    /// `{"t":"themes","names":..}`
    Themes { names: Vec<String> },
    /// `{"t":"detached"}`
    Detached,
    /// `{"t":"refused","why":..}`
    Refused { why: String },
    /// `{"t":"highlights", ..}`
    Highlights {
        categories: Vec<crate::highlights::CategoryOut>,
        rules: Vec<crate::highlights::RuleOut>,
        sounds: Vec<String>,
    },
    /// `{"t":"tasks", ..}` — spliced `TasksOut` fields.
    Tasks(crate::tasks::TasksOut),
    /// `{"t":"hub", ..}` — spliced `HubOut` fields.
    Hub(HubOut),
    /// `{"t":"trainer", ..}` — spliced `TrainerOut` fields.
    Trainer(crate::trainer::TrainerOut),
    /// `{"t":"wheel","wheel":..}`
    Wheel { wheel: String },
    /// `{"t":"attached","name":..,"game":..,"streams":..,"story_lines":..,"stream_lines":..,"id":..}`
    Attached {
        name: String,
        game: String,
        streams: Vec<StreamOut>,
        story_lines: Vec<Line>,
        stream_lines: Vec<Vec<Line>>,
        id: u32,
    },
    /// `{"t":"injuries", ..}` — spliced `DollOut` fields.
    Injuries(crate::injuries::DollOut),
    /// `{"t":"menu","noun":..,"groups":..}`
    Menu { noun: String, groups: Vec<MenuGroupOut> },
    /// `{"t":"lines","lines":..,"from":..}`
    Lines { lines: Vec<Line>, from: String },
    /// `{"t":"note", ..}` — spliced `NoteOut` fields.
    Note(crate::notes::NoteOut),
    /// `{"t":"inventory", ..}` — spliced `InventoryOut` fields.
    Inventory(crate::inventory::InventoryOut),
    /// `{"t":"highlights_text","changed":..,"text":..}`
    HighlightsText { changed: Option<u64>, text: String },
    /// `{"t":"highlighted","text":..,"kept":..,"done":..}`
    Highlighted {
        text: String,
        kept: Vec<String>,
        done: bool,
    },
}

impl Out {
    /// The `"t"` tag.
    pub fn tag(&self) -> &'static str {
        match self {
            Out::Welcome { .. } => "welcome",
            Out::Workspace { .. } => "workspace",
            Out::Banners { .. } => "banners",
            Out::Offer { .. } => "offer",
            Out::Theme { .. } => "theme",
            Out::Said { .. } => "said",
            Out::View(_) => "view",
            Out::PushPaired => "push_paired",
            Out::Phone { .. } => "phone",
            Out::Hotbars { .. } => "hotbars",
            Out::Stream { .. } => "stream",
            Out::Themes { .. } => "themes",
            Out::Detached => "detached",
            Out::Refused { .. } => "refused",
            Out::Highlights { .. } => "highlights",
            Out::Tasks(_) => "tasks",
            Out::Hub(_) => "hub",
            Out::Trainer(_) => "trainer",
            Out::Wheel { .. } => "wheel",
            Out::Attached { .. } => "attached",
            Out::Injuries(_) => "injuries",
            Out::Menu { .. } => "menu",
            Out::Lines { .. } => "lines",
            Out::Note(_) => "note",
            Out::Inventory(_) => "inventory",
            Out::HighlightsText { .. } => "highlights_text",
            Out::Highlighted { .. } => "highlighted",
        }
    }

    /// The payload fields, in the recovered serialization order.
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        match self {
            Out::Welcome { login } => vec![("login", Value::str(login.clone()))],
            Out::Workspace { ask, status } => {
                vec![("ask", Value::int(*ask)), ("status", Value::int(*status))]
            }
            Out::Banners {
                banners,
                ground,
                across,
                down,
                width,
                opacity,
            } => vec![
                ("banners", arr(banners.iter().map(BannerOut::fields).collect())),
                ("ground", Value::str(ground.clone())),
                ("across", Value::str(across.clone())),
                ("down", Value::str(down.clone())),
                ("width", Value::float(*width as f64)),
                ("opacity", Value::float(*opacity as f64)),
            ],
            Out::Offer { text } => vec![("text", Value::str(text.clone()))],
            Out::Theme {
                colors,
                names,
                prompt,
            } => vec![
                ("colors", strs(colors)),
                ("names", strs(names)),
                ("prompt", Value::str(prompt.clone())),
            ],
            Out::Said { text } => vec![("text", Value::str(text.clone()))],
            Out::View(v) => v.fields(),
            Out::PushPaired => vec![],
            Out::Phone { kind, text } => vec![
                ("kind", Value::str(kind.clone())),
                ("text", Value::str(text.clone())),
            ],
            Out::Hotbars { bars, problems } => vec![
                ("bars", arr(bars.iter().map(BarOut::fields).collect())),
                ("problems", strs(problems)),
            ],
            Out::Stream { id, from } => vec![
                ("id", Value::str(id.clone())),
                ("from", Value::str(from.clone())),
            ],
            Out::Themes { names } => vec![("names", strs(names))],
            Out::Detached => vec![],
            Out::Refused { why } => vec![("why", Value::str(why.clone()))],
            Out::Highlights {
                categories,
                rules,
                sounds,
            } => vec![
                ("categories", arr(categories.iter().map(|c| c.fields()).collect())),
                ("rules", arr(rules.iter().map(|r| r.fields()).collect())),
                ("sounds", strs(sounds)),
            ],
            Out::Tasks(t) => t.fields(),
            Out::Hub(h) => h.fields(),
            Out::Trainer(t) => t.fields(),
            Out::Wheel { wheel } => vec![("wheel", Value::str(wheel.clone()))],
            Out::Attached {
                name,
                game,
                streams,
                story_lines,
                stream_lines,
                id,
            } => vec![
                ("name", Value::str(name.clone())),
                ("game", Value::str(game.clone())),
                ("streams", arr(streams.iter().map(StreamOut::fields).collect())),
                ("story_lines", arr(story_lines.iter().map(Line::fields).collect())),
                (
                    "stream_lines",
                    Value::Arr(
                        stream_lines
                            .iter()
                            .map(|ls| arr(ls.iter().map(Line::fields).collect()))
                            .collect(),
                    ),
                ),
                ("id", Value::int(*id)),
            ],
            Out::Injuries(d) => d.fields(),
            Out::Menu { noun, groups } => vec![
                ("noun", Value::str(noun.clone())),
                ("groups", arr(groups.iter().map(MenuGroupOut::fields).collect())),
            ],
            Out::Lines { lines, from } => vec![
                ("lines", arr(lines.iter().map(Line::fields).collect())),
                ("from", Value::str(from.clone())),
            ],
            Out::Note(n) => n.fields(),
            Out::Inventory(i) => i.fields(),
            Out::HighlightsText { changed, text } => vec![
                (
                    "changed",
                    match changed {
                        Some(v) => Value::int(*v),
                        None => Value::Null,
                    },
                ),
                ("text", Value::str(text.clone())),
            ],
            Out::Highlighted { text, kept, done } => vec![
                ("text", Value::str(text.clone())),
                ("kept", strs(kept)),
                ("done", Value::Bool(*done)),
            ],
        }
    }

    /// Serialize the way `client::message` did (`serde_json::to_string`).
    ///
    /// The document comes out of the derived `Serialize` impl, so the tag is
    /// written first and every payload field follows in declaration order —
    /// exactly the bytes the recovered `Serialize for wire::Out` produced.
    /// A non-finite `f32` (`Out::Banners`) is the one value `serde_json` refuses;
    /// it is written as `null` instead of panicking.
    pub fn to_json(&self) -> String {
        match serde_json::to_string(self) {
            Ok(text) => text,
            Err(_) => "null".to_string(),
        }
    }

    /// The websocket text frame body (`client::message` returned
    /// `bytes::Bytes`; `bytes` is not a workspace pin, so this is a `Vec<u8>`
    /// and [`crate::client::frame`] builds the `tungstenite` message).
    pub fn to_frame(&self) -> Vec<u8> {
        self.to_json().into_bytes()
    }
}

// ---------------------------------------------------------------------------
// In — page to server
// ---------------------------------------------------------------------------

/// One message from the web page to Hydra.
///
/// All 37 variant names are recovered from the serde `"struct variant In::…"`
/// expecting strings; the tag field is `"t"`.
///
/// `// UNKNOWN:` only `In::Login`'s fields are recovered from the binary
/// (`account`, `password`, `game`, `character`, `remember`).  The other
/// payloads carry the fields their handler needs; each is marked where guessed.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum In {
    Login {
        #[serde(default)]
        account: String,
        #[serde(default)]
        password: String,
        #[serde(default)]
        game: String,
        #[serde(default)]
        character: String,
        #[serde(default)]
        remember: bool,
    },
    Hello,
    Attach,
    Send {
        #[serde(default)]
        text: String,
    },
    Wheel {
        #[serde(default)]
        direction: String,
    },
    Button {
        #[serde(default)]
        id: String,
    },
    Link {
        #[serde(default)]
        noun: String,
    },
    Clip {
        #[serde(default)]
        text: String,
    },
    Characters,
    Launch {
        #[serde(default)]
        character: String,
    },
    LaunchTeam {
        #[serde(default)]
        character: String,
    },
    Choose {
        #[serde(default)]
        game: String,
    },
    Reconnect,
    Streams,
    Stream {
        #[serde(default)]
        id: String,
        #[serde(default)]
        text: String,
    },
    Tasks,
    Trainer,
    TrainerProfile {
        #[serde(default)]
        name: String,
    },
    TrainerStep {
        #[serde(default)]
        skill: String,
        #[serde(default)]
        step: i32,
    },
    Inventory,
    InventoryAct {
        #[serde(default)]
        act: String,
        #[serde(default)]
        item: String,
    },
    Notes,
    NoteNew {
        #[serde(default)]
        text: String,
    },
    NoteWrite {
        #[serde(default)]
        key: String,
        #[serde(default)]
        text: String,
    },
    NoteForget {
        #[serde(default)]
        key: String,
    },
    Highlights,
    HighlightsMerge {
        #[serde(default)]
        text: String,
    },
    HighlightSave {
        #[serde(flatten)]
        form: crate::highlights::Form,
    },
    HighlightRemove {
        #[serde(default)]
        name: String,
    },
    HighlightSwitch {
        #[serde(default)]
        name: String,
        #[serde(default)]
        on: bool,
    },
    Theme,
    Hotbar {
        #[serde(default)]
        bar: String,
    },
    PhoneGet,
    PhonePut {
        #[serde(default)]
        kind: String,
        #[serde(default)]
        text: String,
    },
    PushPair {
        #[serde(default)]
        token: String,
    },
    PushForget,
    Workspace,
    Remove,
}

impl In {
    /// The `"t"` tag, i.e. the serde variant name.
    pub fn tag(&self) -> &'static str {
        match self {
            In::Login { .. } => "login",
            In::Hello => "hello",
            In::Attach => "attach",
            In::Send { .. } => "send",
            In::Wheel { .. } => "wheel",
            In::Button { .. } => "button",
            In::Link { .. } => "link",
            In::Clip { .. } => "clip",
            In::Characters => "characters",
            In::Launch { .. } => "launch",
            In::LaunchTeam { .. } => "launch_team",
            In::Choose { .. } => "choose",
            In::Reconnect => "reconnect",
            In::Streams => "streams",
            In::Stream { .. } => "stream",
            In::Tasks => "tasks",
            In::Trainer => "trainer",
            In::TrainerProfile { .. } => "trainer_profile",
            In::TrainerStep { .. } => "trainer_step",
            In::Inventory => "inventory",
            In::InventoryAct { .. } => "inventory_act",
            In::Notes => "notes",
            In::NoteNew { .. } => "note_new",
            In::NoteWrite { .. } => "note_write",
            In::NoteForget { .. } => "note_forget",
            In::Highlights => "highlights",
            In::HighlightsMerge { .. } => "highlights_merge",
            In::HighlightSave { .. } => "highlight_save",
            In::HighlightRemove { .. } => "highlight_remove",
            In::HighlightSwitch { .. } => "highlight_switch",
            In::Theme => "theme",
            In::Hotbar { .. } => "hotbar",
            In::PhoneGet => "phone_get",
            In::PhonePut { .. } => "phone_put",
            In::PushPair { .. } => "push_pair",
            In::PushForget => "push_forget",
            In::Workspace => "workspace",
            In::Remove => "remove",
        }
    }

    /// Every tag the page may send, in the recovered `expecting` order
    /// (`0x1020710c7` and friends: `hub, link, characters, login, streams,
    /// wheel, theme, workspace, highlights, highlights_text, inventory, tasks,
    /// trainer, notes, …`).
    pub const TAGS: &'static [&'static str] = &[
        "hotbar",
        "note_new",
        "login",
        "phone_put",
        "highlights_merge",
        "trainer",
        "inventory",
        "streams",
        "push_pair",
        "highlights",
        "workspace",
        "reconnect",
        "note_write",
        "highlight_remove",
        "highlight_switch",
        "hello",
        "clip",
        "theme",
        "launch",
        "trainer_profile",
        "choose",
        "notes",
        "attach",
        "inventory_act",
        "launch_team",
        "link",
        "highlight_save",
        "remove",
        "trainer_step",
        "send",
        "tasks",
        "push_forget",
        "phone_get",
        "button",
        "characters",
        "wheel",
        "note_forget",
    ];

    /// Parse a page message.
    ///
    /// `serde_json` is the parser (the original's engine); the derived
    /// `Deserialize` impl is the one the recovered `Deserialize for wire::In`
    /// describes.  Unknown tags and malformed documents are errors; a missing
    /// payload field is not (every field carries `#[serde(default)]`, which is
    /// how the page's sparse messages parse).
    pub fn from_json(text: &str) -> Result<In, String> {
        serde_json::from_str(text).map_err(|e| e.to_string())
    }

    /// Parse an already-parsed value.
    ///
    /// Kept for the `crate::json::Value` callers; the value is written back out
    /// through `serde_json` and handed to [`In::from_json`], so there is only
    /// one parser.
    pub fn from_value(v: &Value) -> Result<In, String> {
        In::from_json(&v.to_json())
    }
}

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// The input shape [`run`] consumes.  The original took a `hydra_ui::run`
/// styled run; that crate is not recovered, so only the fields `wire::run`
/// actually reads are modelled here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextRun {
    pub text: String,
    /// `hydra_ui::theme::Token::name` — offset `+0x30` in the original.
    pub token: Option<String>,
    /// Resolved colour at `+0x48`.
    pub color: Option<String>,
    /// Bold flag read at `+0xa9`.
    pub bold: bool,
    pub mono: bool,
    /// Command for a click.
    pub link: Option<String>,
    pub prompt: bool,
}

/// Convert one styled run for the page (`0x100273120`).
///
/// Recovered behaviour: the theme token is looked up
/// (`hydra_ui::run::run_token` + `Token::name`) and a run whose text is a single
/// `_` is replaced by the token name.
pub fn run(r: &TextRun) -> Run {
    let mut out = Run {
        text: r.text.clone(),
        token: r.token.clone(),
        color: r.color.clone(),
        named: None,
        background: None,
        bold: r.bold,
        mono: r.mono,
        link: r.link.clone(),
        prompt: r.prompt,
    };
    if out.text == "_" {
        if let Some(token) = &out.token {
            out.text = token.clone();
        }
    }
    out
}

/// Convert a story line (`0x10026e804`).
///
/// `kind` is the story frame kind (7 variants in the original).
pub fn story_line(kind: &str, stream: &str, runs: &[TextRun]) -> Line {
    Line {
        runs: runs.iter().map(run).collect(),
        stream: stream.to_string(),
        kind: kind.to_string(),
        picture: None,
        help: None,
    }
}

/// Build the hub card for one character (`0x100273568`).
#[allow(clippy::too_many_arguments)]
pub fn card(
    id: u32,
    name: &str,
    game: &str,
    state: &'static str,
    state_token: &'static str,
    vitals: Vec<Gauge>,
    room: Option<Vec<TextRun>>,
    closed: bool,
) -> Card {
    Card {
        name: name.to_string(),
        game: game.to_string(),
        vitals,
        room: room.map(|runs| runs.iter().map(run).collect()),
        state,
        state_token,
        id,
        closed,
    }
}

/// Finish the game panel (`0x100273ae0`).
///
/// The original read the whole `hydra_model::state::GameState` (vitals, room,
/// effects, spells, merchants, targets, chips, …) and took the connection's
/// `state` / `state_token` from `hydra_board::sessions::Sessions::glance`.  The
/// model pieces arrive already extracted as a [`View`]; this fills the two
/// session-owned fields, which are the ones the state machine tracks
/// (`Connecting`, `Logging in`, `Connected`, `Reconnecting`, `Closed` — the
/// recovered state pool at `0x102071570`).
pub fn view(state: &str, state_token: &str, mut parts: View) -> View {
    parts.state = state.to_string();
    parts.state_token = state_token.to_string();
    parts
}

/// Append the scroll banners for the current room (`0x100274740`).
pub fn banners(into: &mut Vec<BannerOut>, text: &str, key: &str, left_ms: u32) {
    into.push(BannerOut {
        text: text.to_string(),
        key: key.to_string(),
        left_ms,
    });
}

/// The two hand slots (`0x100273888`): left, then right.
pub fn hand(left: Option<&str>, right: Option<&str>) -> (Option<String>, Option<String>) {
    (left.map(str::to_string), right.map(str::to_string))
}

/// An image reference (`wire::picture_out::{{closure}}`, `0x10026f7b4`).
pub fn picture_out(src: &str, alt: &str, width: u32, height: u32) -> PictureOut {
    PictureOut {
        src: src.to_string(),
        alt: alt.to_string(),
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_uses_the_t_tag_and_splices_fields() {
        let said = Out::Said {
            text: "hello".to_string(),
        };
        assert_eq!(said.to_json(), r#"{"t":"said","text":"hello"}"#);
        let refused = Out::Refused {
            why: "A character is logged in from the device Hydra runs on.".to_string(),
        };
        assert_eq!(
            refused.to_json(),
            r#"{"t":"refused","why":"A character is logged in from the device Hydra runs on."}"#
        );
    }

    #[test]
    fn view_field_order_is_the_recovered_order() {
        let json = Out::View(View::default()).to_json();
        let parsed = crate::json::parse(&json).unwrap();
        let crate::json::Value::Obj(pairs) = parsed else {
            panic!("not an object");
        };
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            &keys[..9],
            &[
                "t",
                "state",
                "state_token",
                "vitals",
                "roundtime",
                "casttime",
                "left",
                "right",
                "prepared"
            ]
        );
        assert_eq!(keys.len(), 17);
    }

    #[test]
    fn view_fills_the_session_state() {
        let v = view("Connected", "ok", View::default());
        assert_eq!(v.state, "Connected");
        assert_eq!(v.state_token, "ok");
        let json = Out::View(v).to_json();
        assert!(json.starts_with(r#"{"t":"view","state":"Connected","state_token":"ok","#));
    }

    #[test]
    fn unit_variants_have_no_payload() {
        assert_eq!(Out::Detached.to_json(), r#"{"t":"detached"}"#);
        assert_eq!(Out::PushPaired.to_json(), r#"{"t":"push_paired"}"#);
    }

    #[test]
    fn run_fields_and_underscore_substitution() {
        let r = run(&TextRun {
            text: "_".to_string(),
            token: Some("roomName".to_string()),
            bold: true,
            ..TextRun::default()
        });
        assert_eq!(r.text, "roomName");
        assert!(r.bold);
        assert_eq!(
            object(r.fields()).to_json(),
            r#"{"text":"roomName","token":"roomName","color":null,"named":null,"background":null,"bold":true,"mono":false,"link":null,"prompt":false}"#
        );
    }

    #[test]
    fn banner_and_hand_builders() {
        let mut v = Vec::new();
        banners(&mut v, "Dusk is falling.", "dusk", 4000);
        assert_eq!(
            object(v[0].fields()).to_json(),
            r#"{"text":"Dusk is falling.","key":"dusk","left_ms":4000}"#
        );
        assert_eq!(hand(Some("sword"), None), (Some("sword".to_string()), None));
    }

    // -- serde: the derived impls are the recovered documents ---------------

    /// The JSON `serde` writes for `value`, as an order-preserving `Value`.
    fn serde_value<T: serde::Serialize>(value: &T) -> crate::json::Value {
        let text = serde_json::to_string(value).expect("serializes");
        crate::json::parse(&text).expect("parses")
    }

    /// A leaf's serde document is exactly the recovered field list, and the
    /// document survives a serde round trip.
    fn same<T>(value: T, fields: Vec<(&'static str, crate::json::Value)>)
    where
        T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        assert_eq!(serde_value(&value), object(fields));
        let text = serde_json::to_string(&value).expect("serializes");
        let back: T = serde_json::from_str(&text).expect("parses");
        assert_eq!(back, value);
    }

    #[test]
    fn serde_matches_fields_for_every_wire_leaf() {
        let run = Run {
            text: "a kobold".into(),
            token: Some("creature".into()),
            color: Some("#ff0000".into()),
            named: None,
            background: Some("#000000".into()),
            bold: true,
            mono: false,
            link: Some(".look kobold".into()),
            prompt: false,
        };
        same(run.clone(), run.fields());

        let gauge = Gauge {
            label: "Health".into(),
            token: Some("health".into()),
            percent: 92,
            current: 46,
            max: 50,
        };
        same(gauge.clone(), gauge.fields());

        let effect = EffectOut {
            text: "Blessed".into(),
            left: None,
            percent: Some(40),
        };
        same(effect.clone(), effect.fields());
        let effects = EffectList {
            effects: vec![effect],
            list: "blessings".into(),
        };
        same(effects.clone(), effects.fields());

        let spell = SpellOut {
            name: "Minor Shock".into(),
            circle: 1,
            number: 901,
        };
        same(spell.clone(), spell.fields());

        let target = TargetOut {
            name: "a kobold".into(),
            bare: "kobold".into(),
            noun: "kobold".into(),
            statuses: vec!["stunned".into()],
            hint: "tired".into(),
            health: None,
            current: true,
        };
        same(target.clone(), target.fields());
        let targets = TargetsOut {
            look: "You see nothing.".into(),
            targets: vec![target],
            arms: "sword".into(),
            known: true,
        };
        same(targets.clone(), targets.fields());

        let row = ChipRowOut {
            name: "left".into(),
            list: "hands".into(),
            left: "sword".into(),
            up: true,
        };
        same(row.clone(), row.fields());
        let chip = ChipOut {
            name: "effects".into(),
            mark: "E".into(),
            words: vec!["blessed".into()],
            rows: vec![row],
            color: "#00ff00".into(),
            level: 2,
            shown: true,
        };
        same(chip.clone(), chip.fields());

        let merchant = MerchantOut {
            name: "the peddler".into(),
            room: Some("Town Square".into()),
            status: "open".into(),
            left: "5 minutes".into(),
            announcement: None,
            said: Some("silver only".into()),
            rules: vec!["no haggling".into()],
        };
        same(merchant.clone(), merchant.fields());

        let room = RoomOut {
            description: vec![run.clone()],
            creatures: vec![],
            objects: vec![],
            players: vec![],
            title: vec![run.clone()],
            exits: vec![run.clone()],
        };
        same(room.clone(), room.fields());
        let team = TeamOut {
            name: "the pack".into(),
            members: vec![run.clone()],
        };
        same(team.clone(), team.fields());

        let banner = BannerOut {
            text: "Dusk is falling.".into(),
            key: "dusk".into(),
            left_ms: 4000,
        };
        same(banner.clone(), banner.fields());
        let button = ButtonOut {
            id: "look".into(),
            label: "Look".into(),
            fg: Some("#ffffff".into()),
            bg: None,
            countdown: "3".into(),
            dim: false,
        };
        same(button.clone(), button.fields());
        let bar = BarOut {
            name: "main".into(),
            slots: vec![button],
            title: "Hotbar".into(),
            columns: 8,
        };
        same(bar.clone(), bar.fields());

        let stream = StreamOut {
            id: "main".into(),
            name: "Story".into(),
        };
        same(stream.clone(), stream.fields());
        let help = HelpOut {
            command: "look".into(),
            says: vec![run.clone()],
        };
        same(help.clone(), help.fields());
        let picture = PictureOut {
            src: "/pictures/kobold.png".into(),
            alt: "a kobold".into(),
            width: 64,
            height: 64,
        };
        same(picture.clone(), picture.fields());

        let entry = MenuEntryOut {
            label: "Look".into(),
            command: "look".into(),
        };
        same(entry.clone(), entry.fields());
        let group = MenuGroupOut {
            under: "actions".into(),
            entries: vec![entry],
        };
        same(group.clone(), group.fields());

        let listing = ListingOut {
            account: "IMAGINE".into(),
            game: "GS4".into(),
            characters: vec!["Ima".into()],
        };
        same(listing.clone(), listing.fields());

        let card = card(
            7,
            "Ima",
            "GS4",
            "Connected",
            "ok",
            vec![gauge.clone()],
            Some(vec![TextRun {
                text: "Town Square".into(),
                ..TextRun::default()
            }]),
            false,
        );
        // `Card` carries two `&'static str` state slots, so it is
        // serialize-only; compare the document instead of round-tripping it.
        assert_eq!(serde_value(&card), object(card.fields()));
        let hub = HubOut {
            cards: vec![card.clone()],
            offered: vec![],
            teams: vec![team],
            said: vec![run.clone()],
            listing: Some(listing),
        };
        assert_eq!(serde_value(&hub), object(hub.fields()));

        let line = Line {
            runs: vec![run.clone()],
            stream: "main".into(),
            kind: "story".into(),
            picture: Some(picture),
            help: Some(help),
        };
        same(line.clone(), line.fields());

        let view = View {
            state: "Connected".into(),
            state_token: "ok".into(),
            vitals: vec![gauge],
            roundtime: Some(3),
            casttime: None,
            left: Some("sword".into()),
            right: None,
            prepared: Some("Minor Shock".into()),
            room: Some(room),
            effects: vec![effects],
            conditions: vec!["stunned".into()],
            injuries: Some(crate::injuries::doll()),
            spells: vec![spell],
            merchants: vec![merchant],
            targets,
            chips: vec![chip],
        };
        same(view.clone(), view.fields());
    }

    #[test]
    fn serde_matches_fields_for_every_payload_leaf() {
        let objective = crate::tasks::ObjectiveOut {
            name: "kobolds".into(),
            kind: "kill".into(),
            location: "Wehnimer's Landing".into(),
            state: "2/10".into(),
            description: "kobolds slain".into(),
        };
        same(objective.clone(), objective.fields());
        let tasks = crate::tasks::TasksOut {
            brief: "Kill ten kobolds".into(),
            bounty: "kobold".into(),
            objectives: vec![objective],
        };
        same(tasks.clone(), tasks.fields());

        let skill = crate::trainer::SkillOut {
            section: "Weapon".into(),
            name: "Edged Weapons".into(),
            cost: "3".into(),
            has: "5".into(),
            goal: "10".into(),
            id: "edged-weapons".into(),
        };
        same(skill.clone(), skill.fields());
        let sheet = crate::trainer::SheetOut {
            title: "The skill manager".into(),
            physical: "2".into(),
            mental: "1".into(),
            skills: vec![skill],
            changed: "now".into(),
        };
        same(sheet.clone(), sheet.fields());
        let trainer = crate::trainer::TrainerOut {
            profiles: vec!["hunter".into()],
            said: "Applying…".into(),
            sheet: Some(sheet),
            busy: true,
        };
        same(trainer.clone(), trainer.fields());

        let note = crate::notes::NoteOut {
            who: "room:12345".into(),
            character: "Ima".into(),
            name: "Town Square".into(),
            changed: "2024-05-01 10:00".into(),
            text: "a\nb".into(),
            lines: vec!["a".into(), "b".into()],
        };
        same(note.clone(), note.fields());
        // `NotesOut` is the untagged enum: one shape per variant.
        same(
            crate::notes::NotesOut::Notes(vec![note.clone()]),
            crate::notes::NotesOut::Notes(vec![note.clone()]).fields(),
        );
        same(
            crate::notes::NotesOut::Problem("boom".into()),
            crate::notes::NotesOut::Problem("boom".into()).fields(),
        );

        let act = crate::inventory::ActOut {
            act: "wear".into(),
            label: "Wear".into(),
        };
        same(act.clone(), act.fields());
        let item = crate::inventory::ItemOut {
            within: "Worn".into(),
            id: "a rusty sword".into(),
            parent: "".into(),
            noun: "sword".into(),
            name: "a rusty sword".into(),
            pounds: "8".into(),
            acts: vec![act],
            count: 1,
            fill: "filling".into(),
            container: false,
            locked: true,
        };
        same(item.clone(), item.fields());
        let group = crate::inventory::GroupOut {
            ids: vec!["a rusty sword".into()],
            heading: "Worn".into(),
            tab: "items".into(),
        };
        same(group.clone(), group.fields());
        let detail = crate::inventory::DetailOut {
            id: "a rusty sword".into(),
            name: "a rusty sword".into(),
            sections: vec![group.clone()],
        };
        same(detail.clone(), detail.fields());
        let inventory = crate::inventory::InventoryOut {
            summary: "You are carrying 1 item.".into(),
            groups: vec![group],
            items: vec![item],
            item: Some(detail),
            known: true,
            room_left: false,
        };
        same(inventory.clone(), inventory.fields());

        for shape in [
            crate::injuries::ShapeOut::Circle { c: 50, r: 12 },
            crate::injuries::ShapeOut::Line { a: 1, b: 2, w: 3 },
            crate::injuries::ShapeOut::Block { min: 4, max: 5 },
            crate::injuries::ShapeOut::Letter {
                c: 6,
                letter: "H".into(),
            },
        ] {
            same(shape.clone(), shape.fields());
        }
        let part = crate::injuries::PartOut {
            shape: crate::injuries::ShapeOut::Circle { c: 5, r: 6 },
            id: 1,
            name: "head".into(),
            anchor: "head".into(),
        };
        same(part.clone(), part.fields());
        let doll = crate::injuries::DollOut {
            parts: vec![part, crate::injuries::table()[1].clone()],
            aspect: 0.75,
        };
        same(doll.clone(), doll.fields());
        let injury = crate::injuries::InjuryOut {
            id: "leftArm".into(),
            name: "left arm".into(),
            token: "wound".into(),
            rank: 2,
        };
        same(injury.clone(), injury.fields());

        let category = crate::highlights::CategoryOut {
            name: "Combat".into(),
            on: 1,
        };
        same(category.clone(), category.fields());
        let mark = crate::highlights::MarkForm {
            name: "watch".into(),
            seconds: 30,
            clear: false,
        };
        same(mark.clone(), mark.fields());
        let form = crate::highlights::Form {
            category: "Combat".into(),
            find: "kobold".into(),
            pattern: true,
            squelch: true,
            seconds: 12,
            volume: 7,
            ..crate::highlights::Form::default()
        };
        same(form.clone(), form.fields());
        let rule = crate::highlights::RuleOut {
            name: "kobold".into(),
            form,
            problem: None,
        };
        same(rule.clone(), rule.fields());
    }

    /// The recovered `"t"` tag is the serde tag, for every variant.
    #[test]
    fn out_documents_are_the_serde_documents() {
        let out = vec![
            Out::Welcome {
                login: "Ima".into(),
            },
            Out::Workspace {
                ask: 2,
                status: 200,
            },
            Out::Banners {
                banners: vec![BannerOut {
                    text: "Dusk is falling.".into(),
                    key: "dusk".into(),
                    left_ms: 4000,
                }],
                ground: "#000".into(),
                across: "1".into(),
                down: "2".into(),
                width: 1.5,
                opacity: 0.5,
            },
            Out::Offer {
                text: "GS4".into(),
            },
            Out::Theme {
                colors: vec!["#fff".into()],
                names: vec!["text".into()],
                prompt: ">".into(),
            },
            Out::Said {
                text: "hello".into(),
            },
            Out::View(View::default()),
            Out::PushPaired,
            Out::Phone {
                kind: "ui".into(),
                text: "{}".into(),
            },
            Out::Hotbars {
                bars: vec![],
                problems: vec!["bar".into()],
            },
            Out::Stream {
                id: "main".into(),
                from: "Game".into(),
            },
            Out::Themes {
                names: vec!["dark".into()],
            },
            Out::Detached,
            Out::Refused {
                why: "no".into(),
            },
            Out::Highlights {
                categories: vec![crate::highlights::CategoryOut {
                    name: "Combat".into(),
                    on: 1,
                }],
                rules: vec![],
                sounds: vec!["a.ogg".into()],
            },
            Out::Tasks(crate::tasks::TasksOut::default()),
            Out::Hub(HubOut::default()),
            Out::Trainer(crate::trainer::TrainerOut::default()),
            Out::Wheel { wheel: "n".into() },
            Out::Injuries(crate::injuries::doll()),
            Out::Menu {
                noun: "sword".into(),
                groups: vec![],
            },
            Out::Lines {
                lines: vec![],
                from: "story".into(),
            },
            Out::Note(crate::notes::NoteOut::default()),
            Out::Inventory(crate::inventory::InventoryOut::default()),
            Out::HighlightsText {
                changed: Some(17),
                text: "Combat|kobold\n".into(),
            },
            Out::Highlighted {
                text: "Combat|kobold\n".into(),
                kept: vec!["kobold".into()],
                done: true,
            },
        ];
        // Every variant is covered by the list above.
        assert_eq!(out.len(), 26);
        for out in &out {
            let mut pairs = vec![("t".to_string(), crate::json::Value::str(out.tag()))];
            pairs.extend(out.fields().into_iter().map(|(k, v)| (k.to_string(), v)));
            assert_eq!(
                crate::json::parse(&out.to_json()).unwrap(),
                crate::json::Value::Obj(pairs),
                "{}",
                out.tag()
            );
            // The tag the serde derive writes is the recovered one.
            assert!(out.to_json().starts_with(&format!(r#"{{"t":"{}""#, out.tag())));
        }
        // `Out::Attached` needs a scrollback, so it is checked separately.
        let attached = Out::Attached {
            name: "Ima".into(),
            game: "GS4".into(),
            streams: vec![],
            story_lines: vec![],
            stream_lines: vec![vec![]],
            id: 3,
        };
        assert_eq!(
            attached.to_json(),
            r#"{"t":"attached","name":"Ima","game":"GS4","streams":[],"story_lines":[],"stream_lines":[[]],"id":3}"#
        );
    }

    #[test]
    fn in_documents_are_the_serde_documents() {
        for tag in In::TAGS {
            let text = format!(r#"{{"t":"{tag}"}}"#);
            let msg = In::from_json(&text).unwrap_or_else(|e| panic!("{tag}: {e}"));
            assert_eq!(msg.tag(), *tag);
            let back = serde_json::to_string(&msg).unwrap();
            assert!(back.starts_with(&format!(r#"{{"t":"{tag}""#)), "{back}");
            // And the message round trips through serde.
            assert_eq!(In::from_json(&back).unwrap(), msg);
        }

        // A fully populated `login`: the one payload whose field names the
        // binary pins down.
        let login = In::from_json(
            r#"{"t":"login","account":"A","password":"P","game":"GS4","character":"Ima","remember":true,"extra":1}"#,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_string(&login).unwrap(),
            r#"{"t":"login","account":"A","password":"P","game":"GS4","character":"Ima","remember":true}"#
        );

        // `highlight_save` splices the 33 form fields next to the tag.
        let save = In::from_json(
            r#"{"t":"highlight_save","category":"Combat","find":"kobold","pattern":true,"squelch":true,"volume":7}"#,
        )
        .unwrap();
        let In::HighlightSave { form } = &save else {
            panic!("{save:?}");
        };
        assert_eq!(form.category, "Combat");
        assert_eq!(form.find, "kobold");
        assert!(form.pattern);
        assert!(form.squelch);
        assert_eq!(form.volume, 7);
        assert_eq!(form.color, "", "the other form fields default");
        // A message with no form fields at all still parses.
        assert!(matches!(
            In::from_json(r#"{"t":"highlight_save"}"#).unwrap(),
            In::HighlightSave { .. }
        ));
    }

    #[test]
    fn in_tags_are_all_unique_and_parseable() {
        let seen: std::collections::BTreeSet<&str> = In::TAGS.iter().copied().collect();
        assert_eq!(seen.len(), 37);
        for tag in In::TAGS {
            assert!(In::from_json(&format!(r#"{{"t":"{tag}"}}"#)).is_ok(), "{tag}");
        }
        let m = In::from_json(
            r#"{"t":"login","account":"A","password":"P","game":"GS4","character":"Ima","remember":true}"#,
        )
        .unwrap();
        assert_eq!(
            m,
            In::Login {
                account: "A".into(),
                password: "P".into(),
                game: "GS4".into(),
                character: "Ima".into(),
                remember: true,
            }
        );
    }

    #[test]
    fn in_rejects_unknown_and_missing_tags() {
        assert!(In::from_json(r#"{"t":"nope"}"#).is_err());
        assert!(In::from_json(r#"{"x":1}"#).is_err());
        assert!(In::from_json("not json").is_err());
    }

    #[test]
    fn card_and_line_shapes() {
        let c = card(
            7,
            "Ima",
            "GS4",
            "Connected",
            "ok",
            vec![Gauge {
                label: "Health".into(),
                token: Some("health".into()),
                percent: 92,
                current: 46,
                max: 50,
            }],
            None,
            false,
        );
        let json = object(c.fields()).to_json();
        assert!(json.starts_with(r#"{"name":"Ima","game":"GS4","vitals":[{"label":"Health""#));
        assert!(json.contains(r#""state":"Connected","state_token":"ok","id":7,"closed":false}"#));

        let line = story_line(
            "story",
            "main",
            &[TextRun {
                text: "You see a kobold.".to_string(),
                ..TextRun::default()
            }],
        );
        let json = object(line.fields()).to_json();
        assert!(json.starts_with(r#"{"runs":[{"text":"You see a kobold.","#));
        assert!(json.contains(r#""stream":"main""#));
        assert!(json.contains(r#""kind":"story""#));
    }
}
