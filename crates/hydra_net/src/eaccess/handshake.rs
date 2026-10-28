//! The eaccess login handshake: the eight stages from TLS to a launch key.
//!
//! Ground truth: `SPEC_P6.md` §1-6 (a real login, structure only), the
//! reference `https://github.com/elanthia-online/lich-5/blob/cec0f60e/docs/eaccess-protocol-analysis.md`
//! and the framing in [`crate::eaccess::wire`].  The original tree names this
//! module (`crates/hydra-net/src/eaccess/handshake.rs`, see `NOTES.md` §5.2)
//! but it is not in the symbol table, so the sequence below is reconstructed
//! from the recovered literals and the `[login] [stage: …]` progress lines the
//! original printed:
//!
//! ```text
//! stage          client sends                     server answers
//! cert_pin       —                                the TLS certificate, pinned by TOFU
//! k_response     K                                <32-byte key> — bare: no `K\t`, no newline
//! a_response     A\t<account>\t<keyed password>   A\t<account>\tKEY\t<session key>\t<holder>
//! m_response     M                                M\t<code>\t<name>\t<code>\t<name>…
//! f_response     F\t<game>                        F\t<NORMAL|PREMIUM|FREE|TRIAL|INTERNAL>
//! g_response     G\t<game>                        G\t<game metadata: ROOT=… MKTG=…>
//! c_response     C                                C\t<n>\t<n>\t0\t1\t<code>\t<name>…
//! l_response     L\t<char code>\tSTORM            L\tOK\t…GAMEHOST=…\tGAMEPORT=…\tKEY=…
//! ```
//!
//! # What this module will not do
//!
//! * It never sends the password before the certificate has been pinned
//!   ([`crate::eaccess::pin`], trust on first use): no pin file means the
//!   presented certificate is recorded, a *different* certificate is a refusal.
//! * It never prints key material.  The server's key, the keyed password, the
//!   session key and the launch key stay in memory; [`Login`]'s `Debug` masks
//!   every one of them, and no refusal message echoes a response line that
//!   could carry one.
//!
//! # Transport
//!
//! Everything works over [`Transport`], whose real implementation is
//! [`TlsTransport`]: **synchronous** `native-tls` over a `std::net::TcpStream`
//! (eaccess offers only TLS 1.2 with static-RSA key exchange, which `rustls`
//! never does), with SNI switched off so the ClientHello names no host.  The
//! certificate *chain* and the hostname are accepted unchecked — pinning is the
//! trust decision, and it happens one layer up, before anything is sent.

use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::Path;

use native_tls::{Protocol, TlsConnector, TlsStream};

use crate::eaccess::wire::{self, Refusal};

/// The real eaccess host (never dialled by a test: every test uses a loopback
/// mock through `HYDRA_EACCESS_HOST`/`HYDRA_EACCESS_PORT`).
pub const EACCESS_HOST: &str = "eaccess.play.net";

/// The eaccess port.
pub const EACCESS_PORT: u16 = 7910;

/// The front-end identification line, exactly as both the login socket and the
/// game socket send it (`docs/LICH_WIRE.md` §2, `SPEC_P6.md` §7).
pub const CLIENT_IDENTIFICATION: &str = "/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML";

/// The stage the login is at, named exactly as the original's progress lines
/// (`[login] [stage: cert_pin]`, …).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// The certificate has been presented and is about to be pinned/checked.
    CertPin,
    /// `K` — the key the password is hashed with.
    KResponse,
    /// `A` — the account and the keyed password.
    AResponse,
    /// `M` — the games this account may play.
    MResponse,
    /// `F` — the entitlement tier of the chosen game.
    FResponse,
    /// `G` — the chosen game's launch metadata.
    GResponse,
    /// `C` — the characters on the account.
    CResponse,
    /// `L` — the character is picked and the game socket is named.
    LResponse,
}

impl Stage {
    /// The recovered name of the stage, without the `[stage: …]` brackets.
    pub fn name(self) -> &'static str {
        match self {
            Stage::CertPin => "cert_pin",
            Stage::KResponse => "k_response",
            Stage::AResponse => "a_response",
            Stage::MResponse => "m_response",
            Stage::FResponse => "f_response",
            Stage::GResponse => "g_response",
            Stage::CResponse => "c_response",
            Stage::LResponse => "l_response",
        }
    }
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Everything one login needs.
///
/// `password` is the account password in the clear; it is hashed with the
/// server's key ([`wire::hash_password`]) and goes nowhere else.
pub struct LoginConfig<'a> {
    /// The account name (`main`, `Aelith`, …): sent on the `A` line.
    pub account: &'a str,
    /// The character to play, matched case-insensitively against the `C` list.
    pub character: &'a str,
    /// The game code asked for; `None` prefers `GS3`, else the first offered.
    pub game: Option<&'a str>,
    /// The account password.
    pub password: &'a [u8],
    /// Where the pinned certificate lives (`<data dir>/simu.pem`).
    pub pin_path: &'a Path,
}

/// One character the account can play: the launch code and the name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Character {
    /// The server's character code (`W_ACCOUNT_1234`), sent on the `L` line.
    pub code: String,
    /// The character's name, as a player sees it.
    pub name: String,
}

/// The launch details, owned.
///
/// `key` is key material: [`std::fmt::Debug`] masks it, and the only thing a
/// caller may do with it is send it as the game socket's first line
/// ([`crate::eaccess::game::connect`]).
#[derive(Clone, PartialEq, Eq)]
pub struct LaunchOwned {
    /// The session key the game socket wants as its first line.
    pub key: String,
    /// `GAMEHOST`: the game server.
    pub host: String,
    /// `GAMEPORT`: the game socket's port.
    pub port: u16,
    /// `GAMECODE`: the game the launch belongs to.
    pub code: String,
}

impl LaunchOwned {
    /// Own the borrowed [`wire::Launch`].
    pub fn from_wire(launch: &wire::Launch<'_>) -> LaunchOwned {
        LaunchOwned {
            key: launch.key.to_string(),
            host: launch.host.to_string(),
            port: launch.port,
            code: launch.code.to_string(),
        }
    }
}

impl std::fmt::Debug for LaunchOwned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaunchOwned")
            .field("key", &"<redacted>")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("code", &self.code)
            .finish()
    }
}

/// A completed login: the account, the chosen game and character, and the
/// launch that opens the game socket.
///
/// Nothing here is printed by this crate.  `Debug` is written by hand and masks
/// the holder name, the session key and the launch key: a login value in a
/// panic message must not carry key material.
pub struct Login {
    account: String,
    holder: String,
    session_key: String,
    game_code: String,
    game_name: String,
    tier: String,
    metadata: String,
    characters: Vec<Character>,
    character: Character,
    launch: LaunchOwned,
}

impl Login {
    /// The account the server named back.
    pub fn account(&self) -> &str {
        &self.account
    }

    /// The account holder's name.  Personal data: never printed.
    pub fn holder(&self) -> &str {
        &self.holder
    }

    /// The game code that was chosen (`GS3`, `DR`, …).
    pub fn game_code(&self) -> &str {
        &self.game_code
    }

    /// The game's name as the `M` response spelled it.
    pub fn game_name(&self) -> &str {
        &self.game_name
    }

    /// The entitlement tier from the `F` response.
    pub fn tier(&self) -> &str {
        &self.tier
    }

    /// The raw `G` payload (`ROOT=…`, `MKTG=…`).  Metadata only; not printed.
    pub fn metadata(&self) -> &str {
        &self.metadata
    }

    /// Every character the `C` response listed, in order.
    pub fn characters(&self) -> &[Character] {
        &self.characters
    }

    /// The character that was chosen.
    pub fn character(&self) -> &Character {
        &self.character
    }

    /// Where the game socket is and the key that opens it.
    pub fn launch(&self) -> &LaunchOwned {
        &self.launch
    }

    /// The session key from the `A` response.  Key material: never printed, and
    /// not needed to play (the launch `KEY` is what the game socket wants).
    pub fn session_key(&self) -> &str {
        &self.session_key
    }

    /// A `Debug` rendering with every secret masked, so a login value can be
    /// logged (or put in a test failure) without leaking anything.
    pub fn make_debug_redacted(&self) -> String {
        format!(
            "Login {{ account: {:?}, holder: \"<redacted>\", session_key: \"<redacted>\", \
             game_code: {:?}, game_name: {:?}, tier: {:?}, metadata: <{} bytes>, \
             characters: {:?}, character: {:?}, launch: {:?} }}",
            self.account,
            self.game_code,
            self.game_name,
            self.tier,
            self.metadata.len(),
            self.characters
                .iter()
                .map(|character| character.name.as_str())
                .collect::<Vec<_>>(),
            self.character.name,
            self.launch,
        )
    }
}

impl std::fmt::Debug for Login {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.make_debug_redacted())
    }
}

/// The line channel a login runs over.
///
/// Implemented by [`TlsTransport`] for real logins; a test can hand
/// [`login_with`] any other transport and drive the protocol at the framing
/// level without a socket.
pub trait Transport {
    /// Read one newline-terminated line, without its `\n`/`\r`.
    fn read_line(&mut self) -> io::Result<String>;
    /// Write one line — the bytes, then the terminator — and flush (the server
    /// answers line by line).
    ///
    /// The `A` line carries the *keyed* password, whose bytes are not
    /// necessarily UTF-8 (`hash_password` keeps them in `0x20..=0xFF`), so this
    /// is the byte-exact primitive and [`Transport::write_line`] is the text
    /// convenience over it: an implementation must not re-encode it.
    fn write_bytes(&mut self, line: &[u8]) -> io::Result<()>;
    /// Write one line of text and flush.
    fn write_line(&mut self, line: &str) -> io::Result<()> {
        self.write_bytes(line.as_bytes())
    }
    /// One reply that may not end in a newline: the real server answers `K`
    /// with the bare 32-byte key and nothing after it.
    fn read_unterminated(&mut self) -> io::Result<String> {
        self.read_line()
    }
    /// The end-entity certificate the TLS layer was handed, in DER.
    fn certificate_der(&self) -> Option<Vec<u8>>;
    /// Force the TLS handshake to finish before anything is read or written.
    fn complete_handshake(&mut self) -> io::Result<()>;
}

/// A real eaccess connection: synchronous `native-tls` over a `TcpStream`.
pub struct TlsTransport {
    stream: BufReader<TlsStream<TcpStream>>,
}

impl TlsTransport {
    /// Dial `host:port`, complete the handshake and return the open channel.
    ///
    /// SNI is switched off (`use_sni(false)`), so the ClientHello names no host
    /// — which is what a real login does and what `hydra_mocks::MockEaccess::sni_seen`
    /// checks.  The chain and the hostname are not checked here: the pin is the
    /// trust decision.  Every failure here is the `connect` stage.
    pub fn dial(host: &str, port: u16) -> Result<TlsTransport, Refusal> {
        let stream = TcpStream::connect((host, port)).map_err(|error| {
            Refusal::new(
                "connect",
                format!("could not reach the eaccess server at {host}:{port}: {error}"),
            )
        })?;
        // eaccess speaks only TLS 1.2 with static-RSA key exchange, which rustls
        // never offers, so the platform TLS stack is used.  No SNI, no chain
        // check: trust is the certificate pin, checked before anything is sent.
        let connector = TlsConnector::builder()
            .min_protocol_version(Some(Protocol::Tlsv12))
            .use_sni(false)
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true)
            .build()
            .map_err(|error| {
                Refusal::new(
                    "connect",
                    format!("could not build the TLS client for {host}:{port}: {error}"),
                )
            })?;
        let stream = connector.connect(host, stream).map_err(|error| {
            Refusal::new(
                "connect",
                format!("the TLS handshake with {host}:{port} failed: {error}"),
            )
        })?;
        Ok(TlsTransport {
            stream: BufReader::new(stream),
        })
    }
}

impl Transport for TlsTransport {
    // eaccess replies are single unterminated records (Lich sysreads them), so
    // every reply is one read, cut at a newline if the record has one.
    fn read_line(&mut self) -> io::Result<String> {
        self.read_unterminated()
    }

    fn read_unterminated(&mut self) -> io::Result<String> {
        let buf = self.stream.fill_buf()?;
        if buf.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the eaccess server closed the connection",
            ));
        }
        let end = buf.iter().position(|&b| b == 0x0A).map_or(buf.len(), |i| i + 1);
        let text = String::from_utf8_lossy(&buf[..end]).trim_end().to_string();
        self.stream.consume(end);
        Ok(text)
    }

    fn write_bytes(&mut self, line: &[u8]) -> io::Result<()> {
        let stream = self.stream.get_mut();
        // One record: the server parses per read, so the newline must travel with the line.
        let mut framed = line.to_vec();
        framed.push(0x0A);
        stream.write_all(&framed)?;
        stream.flush()
    }

    fn certificate_der(&self) -> Option<Vec<u8>> {
        self.stream
            .get_ref()
            .peer_certificate()
            .ok()
            .flatten()
            .and_then(|certificate| certificate.to_der().ok())
    }

    fn complete_handshake(&mut self) -> io::Result<()> {
        Ok(()) // connect() has already finished the handshake
    }
}

/// Run the whole login over `transport`, calling `on_stage` once per stage.
///
/// The certificate is checked (pinned, or recorded on first use) **before** the
/// `K` line is written, i.e. before the server is told anything.  Every refusal
/// carries the stage it happened at and a message with no key material in it.
pub fn login_with(
    transport: &mut dyn Transport,
    config: &LoginConfig<'_>,
    on_stage: &mut dyn FnMut(Stage),
) -> Result<Login, Refusal> {
    // 1. cert_pin: trust on first use, checked before anything is sent.
    on_stage(Stage::CertPin);
    transport
        .complete_handshake()
        .map_err(|error| Refusal::new("cert_pin", format!("the TLS handshake failed: {error}")))?;
    let certificate = transport.certificate_der().ok_or_else(|| {
        Refusal::new(
            "cert_pin",
            "the eaccess server presented no certificate, so there is nothing to pin. \
             Refusing to log in unpinned.",
        )
    })?;
    crate::eaccess::pin::verify(config.pin_path, &certificate)
        .map_err(|message| Refusal::new("cert_pin", message))?;

    // 2. k_response: the key the password is hashed with.
    on_stage(Stage::KResponse);
    transport.write_line("K").map_err(|error| {
        Refusal::new("k_response", format!("the server could not be written to: {error}"))
    })?;
    let k_line = transport.read_unterminated().map_err(|error| {
        Refusal::new("k_response", format!("the server did not answer: {error}"))
    })?;
    let key = sge_key(&k_line)?;

    // The password is hashed here, before the account is sent, and the hash is
    // never echoed or logged.  Its bytes need not be UTF-8, so the `A` line is
    // built and written as bytes.
    let hashed = wire::hash_password(config.password, &key)?;

    // 3. a_response: the account and the keyed password.
    on_stage(Stage::AResponse);
    let mut a_line_bytes = Vec::with_capacity(config.account.len() + hashed.len() + 3);
    a_line_bytes.extend_from_slice(b"A\t");
    a_line_bytes.extend_from_slice(config.account.as_bytes());
    a_line_bytes.push(b'\t');
    a_line_bytes.extend_from_slice(&hashed);
    let a_line = exchange_bytes(transport, &a_line_bytes, "a_response")?;
    let (holder, session_key) = parse_a_response(&a_line)?;

    // 4. m_response: the games offered, as code/name pairs.
    on_stage(Stage::MResponse);
    let games = pairs(&exchange(transport, "M", "m_response")?, 1);
    let (game_code, game_name) = choose_game(&games, config.game)?;

    // 5. f_response: the entitlement tier.
    on_stage(Stage::FResponse);
    let tier = parse_f_response(
        &exchange(transport, &format!("F\t{game_code}"), "f_response")?,
        &game_code,
    )?;

    // 6. g_response: the chosen game's launch metadata (kept, not printed).
    on_stage(Stage::GResponse);
    let g_line = exchange(transport, &format!("G\t{game_code}"), "g_response")?;
    let metadata = g_line
        .trim()
        .strip_prefix("G\t")
        .ok_or_else(|| {
            Refusal::new(
                "g_response",
                "the server's answer to G did not start with G, so the session is out of step",
            )
        })?
        .to_string();

    // 7. c_response: the characters, as code/name pairs.
    on_stage(Stage::CResponse);
    let offered = pairs(&exchange(transport, "C", "c_response")?, 5);
    let character = choose_character(&offered, config.character)?;
    let characters: Vec<Character> = offered
        .into_iter()
        .map(|(code, name)| Character { code, name })
        .collect();

    // 8. l_response: the character picked, the game socket named.
    on_stage(Stage::LResponse);
    let l_line = exchange(
        transport,
        &format!("L\t{}\tSTORM", character.code),
        "l_response",
    )?;
    let launch = parse_l_response(&l_line)?;

    Ok(Login {
        account: config.account.to_string(),
        holder,
        session_key,
        game_code,
        game_name,
        tier,
        metadata,
        characters,
        character,
        launch,
    })
}

/// Dial `host:port` over TLS and run [`login_with`] on it.
///
/// `host`/`port` default to [`EACCESS_HOST`]/[`EACCESS_PORT`]; the binary lets
/// `HYDRA_EACCESS_HOST`/`HYDRA_EACCESS_PORT` point them at a mock.
pub fn login_tls(
    host: &str,
    port: u16,
    config: &LoginConfig<'_>,
    on_stage: &mut dyn FnMut(Stage),
) -> Result<Login, Refusal> {
    let mut transport = TlsTransport::dial(host, port)?;
    login_with(&mut transport, config, on_stage)
}

/// Send one text command line and read the answer to it.
fn exchange(
    transport: &mut dyn Transport,
    line: &str,
    stage: &'static str,
) -> Result<String, Refusal> {
    exchange_bytes(transport, line.as_bytes(), stage)
}

/// Send one line's bytes (the `A` line is not UTF-8 necessarily) and read the
/// answer to it.
fn exchange_bytes(
    transport: &mut dyn Transport,
    line: &[u8],
    stage: &'static str,
) -> Result<String, Refusal> {
    transport.write_bytes(line).map_err(|error| {
        Refusal::new(stage, format!("the server could not be written to: {error}"))
    })?;
    transport
        .read_line()
        .map_err(|error| Refusal::new(stage, format!("the server did not answer: {error}")))
}

/// The key from the `K` response, used whole (normally 32 bytes).
///
/// The real server answers `K` with the **bare** key — no `K\t` prefix, no field
/// before it and nothing after it — so every non-empty byte of the answer is key
/// material.  (A leading `K\t` is still stripped if a server ever sends one;
/// nothing requires it.)  Nothing is refused for being merely short: a short key
/// is returned as it is, and refusing to truncate it is [`wire::hash_password`]'s
/// job, at stage `hash`, where the password's length is known.
fn sge_key(line: &str) -> Result<Vec<u8>, Refusal> {
    let trimmed = line.trim_end_matches(['\n', '\r']);
    // The real server answers K with the bare key line; a "K\t" prefix is tolerated.
    let key = trimmed.strip_prefix("K\t").unwrap_or(trimmed);
    // An empty answer, or one that is only a terminator, carries no key at all.
    if key.is_empty() {
        return Err(Refusal::new(
            "k_response",
            "the server's K response carried an empty key, so the password cannot be keyed",
        ));
    }
    Ok(key.as_bytes().to_vec())
}

/// The holder name and session key from the `A` response.
fn parse_a_response(line: &str) -> Result<(String, String), Refusal> {
    let fields: Vec<&str> = line.trim().split('\t').collect();
    if fields.len() >= 5 && fields[0] == "A" && fields[2] == "KEY" && !fields[3].is_empty() {
        return Ok((fields[4].to_string(), fields[3].to_string()));
    }
    // A refusal (`PASSWORD`, `REJECT`, …) or anything else.  Only the first
    // field is ever shown: the rest of a line at this stage could be anything.
    let answer = fields
        .first()
        .map(|field| wire::redact(field))
        .unwrap_or_default();
    Err(Refusal::new(
        "a_response",
        format!(
            "the account or the password was not accepted: the server answered {answer:?}. \
             Check the account name and the password; nothing was retried."
        ),
    ))
}

/// Split `first` onwards into `(code, name)` pairs, tolerating a trailing odd
/// field (which reads as an empty name).
fn pairs(line: &str, first: usize) -> Vec<(String, String)> {
    let fields: Vec<&str> = line.trim().split('\t').collect();
    let mut out = Vec::new();
    let mut index = first;
    while index < fields.len() {
        let code = fields[index].trim();
        let name = fields.get(index + 1).map(|name| name.trim()).unwrap_or("");
        // A response's own command letter (`C`, `M`) lands in `fields[0]`, but a
        // stray tab can put an empty field at the front of a pair: skip it
        // rather than producing a character called "".
        if !code.is_empty() {
            out.push((code.to_string(), name.to_string()));
        }
        index += 2;
    }
    out
}

/// The game to play: the one asked for, else `GS3`, else the first offered.
fn choose_game(
    games: &[(String, String)],
    wanted: Option<&str>,
) -> Result<(String, String), Refusal> {
    if games.is_empty() {
        return Err(Refusal::new(
            "m_response",
            "the account is offered no games at all (the M response was empty)",
        ));
    }
    if let Some(wanted) = wanted {
        return games
            .iter()
            .find(|(code, _)| code.eq_ignore_ascii_case(wanted))
            .cloned()
            .ok_or_else(|| {
                Refusal::new(
                    "m_response",
                    format!(
                        "the account may not play {wanted}: the server offered {}",
                        codes(games)
                    ),
                )
            });
    }
    games
        .iter()
        .find(|(code, _)| code == "GS3")
        .or_else(|| games.first())
        .cloned()
        .ok_or_else(|| {
            Refusal::new(
                "m_response",
                format!("the server offered no game code to play; it offered {}", codes(games)),
            )
        })
}

/// The game codes of an `M` response, for a refusal message (codes only: a game
/// name is not key material, but a refusal only needs the codes).
fn codes(games: &[(String, String)]) -> String {
    games
        .iter()
        .map(|(code, _)| code.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The tier from the `F` response.
///
/// `NEW_TO_GAME` and `UNKNOWN` mean the account may not play this game at all
/// and are refused with a sentence saying so.  The five tiers a real account
/// gets (`NORMAL`, `PREMIUM`, `FREE`, `TRIAL`, `INTERNAL`) are accepted, and so
/// is any future word: only the two known refusals are refusals.
fn parse_f_response(line: &str, game_code: &str) -> Result<String, Refusal> {
    let trimmed = line.trim();
    let tier = trimmed.strip_prefix("F\t").ok_or_else(|| {
        Refusal::new(
            "f_response",
            "the server's answer to F did not start with F, so the session is out of step",
        )
    })?;
    let tier = tier.trim();
    match tier {
        "NEW_TO_GAME" => Err(Refusal::new(
            "f_response",
            format!(
                "the account has never played {game_code} (the server answered NEW_TO_GAME): \
                 start a character there through the website once, then log in again. \
                 DO NOT RETRY as-is; this is account state."
            ),
        )),
        "UNKNOWN" => Err(Refusal::new(
            "f_response",
            format!(
                "the server does not know the game {game_code} for this account (it answered \
                 UNKNOWN); check the game code against what M offered."
            ),
        )),
        other => Ok(other.to_string()),
    }
}

/// The character to play, by name, case insensitively.
fn choose_character(
    characters: &[(String, String)],
    wanted: &str,
) -> Result<Character, Refusal> {
    if characters.is_empty() {
        return Err(Refusal::new(
            "c_response",
            "this account has no characters to play on that game",
        ));
    }
    characters
        .iter()
        .find(|(_, name)| name.eq_ignore_ascii_case(wanted))
        .map(|(code, name)| Character {
            code: code.clone(),
            name: name.clone(),
        })
        .ok_or_else(|| {
            let names = characters
                .iter()
                .map(|(_, name)| name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Refusal::new(
                "c_response",
                format!("this account has no character called {wanted:?}; it has {names}"),
            )
        })
}

/// The launch details from the `L` response.
fn parse_l_response(line: &str) -> Result<LaunchOwned, Refusal> {
    let trimmed = line.trim();
    if trimmed.starts_with("L\tPROBLEM") {
        return Err(Refusal::new(
            "l_response",
            crate::eaccess::refusal::describe_launch_refusal(trimmed),
        ));
    }
    if !wire::is_launch_ok(trimmed) {
        // Never echo the line: an L payload is where the launch key lives.
        return Err(Refusal::new(
            "l_response",
            "the server did not answer the launch with L OK, so there is no launch to make",
        ));
    }
    let payload = trimmed
        .strip_prefix("L\tOK\t")
        .or_else(|| trimmed.strip_prefix("L\tOK"))
        .unwrap_or_default();
    let launch = wire::parse_launch(payload).map_err(|refusal| {
        Refusal::new("l_response", format!("the launch payload is unusable: {}", refusal.detail))
    })?;
    Ok(LaunchOwned::from_wire(&launch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stage_names_are_the_recovered_ones() {
        let names: Vec<&str> = [
            Stage::CertPin,
            Stage::KResponse,
            Stage::AResponse,
            Stage::MResponse,
            Stage::FResponse,
            Stage::GResponse,
            Stage::CResponse,
            Stage::LResponse,
        ]
        .iter()
        .map(|stage| stage.name())
        .collect();
        assert_eq!(
            names,
            vec![
                "cert_pin",
                "k_response",
                "a_response",
                "m_response",
                "f_response",
                "g_response",
                "c_response",
                "l_response"
            ]
        );
        assert_eq!(Stage::CertPin.to_string(), "cert_pin");
    }

    #[test]
    fn constants_are_the_ones_a_login_uses() {
        assert_eq!(EACCESS_HOST, "eaccess.play.net");
        assert_eq!(EACCESS_PORT, 7910);
        assert_eq!(
            CLIENT_IDENTIFICATION,
            "/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML"
        );
    }

    #[test]
    fn the_bare_key_answer_is_used_whole() {
        // The real server answers K with the bare key and nothing else: no
        // "K\t" prefix, no field before it.  The whole answer is key material.
        assert_eq!(sge_key("abcdef").unwrap(), b"abcdef".to_vec());
        assert_eq!(
            sge_key("0123456789abcdef0123456789abcdef").unwrap(),
            b"0123456789abcdef0123456789abcdef".to_vec()
        );
        // A terminator that rode along is not part of the key.
        assert_eq!(sge_key("abcdef\r\n").unwrap(), b"abcdef".to_vec());
        // "K\t" is still tolerated, but it is not required and not key material.
        assert_eq!(sge_key("K\tabcdef").unwrap(), b"abcdef".to_vec());

        // An empty answer, and one that is only a terminator, are not keys.
        assert_eq!(sge_key("").unwrap_err().stage, "k_response");
        assert_eq!(sge_key("\r\n").unwrap_err().stage, "k_response");

        // A short key is handed back as it is: the refusal belongs to the
        // password hash, which will not truncate a key shorter than the password.
        let short = sge_key("abcd").unwrap();
        assert_eq!(short, b"abcd".to_vec());
        let password = b"eighteen-byte-pass";
        assert_eq!(password.len(), 18);
        let refusal = wire::hash_password(password, &short).unwrap_err();
        assert_eq!(refusal.stage, "hash");
        assert!(
            refusal
                .detail
                .starts_with("key (4 bytes) shorter than password (18 bytes)"),
            "{}",
            refusal.detail
        );
    }

    #[test]
    fn an_a_response_gives_the_holder_and_the_session_key() {
        let (holder, session) = parse_a_response("A\tmain\tKEY\tabc123\tTest Holder").unwrap();
        assert_eq!(holder, "Test Holder");
        assert_eq!(session, "abc123");
    }

    #[test]
    fn a_password_refusal_never_echoes_more_than_the_first_field() {
        let refusal = parse_a_response("PASSWORD").unwrap_err();
        assert_eq!(refusal.stage, "a_response");
        assert!(refusal.detail.contains("PASSWORD"), "{}", refusal.detail);

        // Even a line that carries something key-shaped after the first field
        // is not repeated.
        let refusal = parse_a_response("REJECT\tsecret-lookalike").unwrap_err();
        assert!(!refusal.detail.contains("secret-lookalike"), "{}", refusal.detail);

        // An "A" line that is missing its key is a refusal too.
        assert_eq!(
            parse_a_response("A\tmain\tNOKEY").unwrap_err().stage,
            "a_response"
        );
    }

    #[test]
    fn pairs_read_code_name_and_tolerate_an_odd_tail() {
        let games = pairs("M\tGS3\tGemStone IV\tDR\tDragonRealms", 1);
        assert_eq!(
            games,
            vec![
                ("GS3".to_string(), "GemStone IV".to_string()),
                ("DR".to_string(), "DragonRealms".to_string()),
            ]
        );
        // A trailing odd field is a code with no name.
        assert_eq!(
            pairs("M\tGS3\tGemStone IV\tDR", 1),
            vec![
                ("GS3".to_string(), "GemStone IV".to_string()),
                ("DR".to_string(), String::new()),
            ]
        );
        // The character list starts at the fifth field (four counts first).
        let characters = pairs("C\t1\t1\t0\t1\tW_TEST_001\tAelith", 5);
        assert_eq!(characters, vec![("W_TEST_001".to_string(), "Aelith".to_string())]);
    }

    #[test]
    fn the_game_is_the_one_asked_for_else_gs3_else_the_first() {
        let games = vec![
            ("DR".to_string(), "DragonRealms".to_string()),
            ("GS3".to_string(), "GemStone IV".to_string()),
        ];
        assert_eq!(choose_game(&games, Some("dr")).unwrap().0, "DR");
        assert_eq!(choose_game(&games, None).unwrap().0, "GS3");

        let only_dr = vec![("DR".to_string(), "DragonRealms".to_string())];
        assert_eq!(choose_game(&only_dr, None).unwrap().0, "DR");

        let refusal = choose_game(&games, Some("GSF")).unwrap_err();
        assert_eq!(refusal.stage, "m_response");
        assert!(refusal.detail.contains("DR, GS3"), "{}", refusal.detail);

        let refusal = choose_game(&[], None).unwrap_err();
        assert_eq!(refusal.stage, "m_response");
    }

    #[test]
    fn the_two_refusing_tiers_are_refused_and_the_real_ones_accepted() {
        for tier in ["NORMAL", "PREMIUM", "FREE", "TRIAL", "INTERNAL"] {
            assert_eq!(parse_f_response(&format!("F\t{tier}"), "GS3").unwrap(), tier);
        }
        let refusal = parse_f_response("F\tNEW_TO_GAME", "GS3").unwrap_err();
        assert_eq!(refusal.stage, "f_response");
        assert!(refusal.detail.contains("NEW_TO_GAME"), "{}", refusal.detail);
        let refusal = parse_f_response("F\tUNKNOWN", "GS3").unwrap_err();
        assert!(refusal.detail.contains("UNKNOWN"), "{}", refusal.detail);
        assert_eq!(
            parse_f_response("E\tNORMAL", "GS3").unwrap_err().stage,
            "f_response"
        );
    }

    #[test]
    fn the_character_is_matched_case_insensitively_and_refused_by_name() {
        let characters = vec![
            ("W_TEST_001".to_string(), "Aelith".to_string()),
            ("W_TEST_002".to_string(), "Bryn".to_string()),
        ];
        assert_eq!(choose_character(&characters, "aelith").unwrap().code, "W_TEST_001");
        assert_eq!(choose_character(&characters, "BRYN").unwrap().code, "W_TEST_002");

        let refusal = choose_character(&characters, "Nobody").unwrap_err();
        assert_eq!(refusal.stage, "c_response");
        // The refusal lists names only: a code can carry the account name.
        assert!(!refusal.detail.contains("W_TEST"), "{}", refusal.detail);
        assert!(refusal.detail.contains("Aelith"), "{}", refusal.detail);
        assert!(refusal.detail.contains("Bryn"), "{}", refusal.detail);

        assert_eq!(choose_character(&[], "Aelith").unwrap_err().stage, "c_response");
    }

    #[test]
    fn a_launch_line_gives_the_four_fields() {
        let launch = parse_l_response(
            "L\tOK\tUPPORT=5535\tGAME=STORM\tGAMECODE=GS3\tGAMEHOST=127.0.0.1\tGAMEPORT=10121\tKEY=abc",
        )
        .unwrap();
        assert_eq!(launch.key, "abc");
        assert_eq!(launch.host, "127.0.0.1");
        assert_eq!(launch.port, 10121);
        assert_eq!(launch.code, "GS3");
    }

    #[test]
    fn a_launch_refusal_is_described_and_never_echoes_a_key() {
        let refusal = parse_l_response("L\tPROBLEM\t1").unwrap_err();
        assert_eq!(refusal.stage, "l_response");
        assert!(refusal.detail.starts_with("launch refused ("), "{}", refusal.detail);

        // A non-OK answer that still carries a KEY= field is not repeated.
        let refusal = parse_l_response("L\tNO\tKEY=do-not-repeat-this").unwrap_err();
        assert!(!refusal.detail.contains("do-not-repeat-this"), "{}", refusal.detail);
    }

    #[test]
    fn the_launch_debug_masks_the_key() {
        let launch = LaunchOwned {
            key: "do-not-print-me".to_string(),
            host: "127.0.0.1".to_string(),
            port: 10121,
            code: "GS3".to_string(),
        };
        let shown = format!("{launch:?}");
        assert!(!shown.contains("do-not-print-me"), "{shown}");
        assert!(shown.contains("<redacted>"), "{shown}");
        assert!(shown.contains("10121"), "{shown}");
    }

    #[test]
    fn the_login_debug_masks_every_secret() {
        let login = Login {
            account: "main".to_string(),
            holder: "Real Person".to_string(),
            session_key: "session-key-material".to_string(),
            game_code: "GS3".to_string(),
            game_name: "GemStone IV".to_string(),
            tier: "NORMAL".to_string(),
            metadata: "STORM\tNORMAL\t0\t\tROOT=sgc/gs".to_string(),
            characters: vec![Character {
                code: "W_TEST_001".to_string(),
                name: "Aelith".to_string(),
            }],
            character: Character {
                code: "W_TEST_001".to_string(),
                name: "Aelith".to_string(),
            },
            launch: LaunchOwned {
                key: "launch-key-material".to_string(),
                host: "127.0.0.1".to_string(),
                port: 10121,
                code: "GS3".to_string(),
            },
        };
        let shown = login.make_debug_redacted();
        assert_eq!(shown, format!("{login:?}"));
        for secret in [
            "Real Person",
            "session-key-material",
            "launch-key-material",
            "ROOT=sgc/gs",
        ] {
            assert!(!shown.contains(secret), "{secret} leaked in {shown}");
        }
        // What is safe to show is shown.
        assert!(shown.contains("main"), "{shown}");
        assert!(shown.contains("Aelith"), "{shown}");
        assert!(shown.contains("GS3"), "{shown}");
        assert_eq!(login.holder(), "Real Person");
        assert_eq!(login.character().code, "W_TEST_001");
        assert_eq!(login.session_key(), "session-key-material");
    }
}
