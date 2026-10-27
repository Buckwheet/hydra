//! The mock eaccess login server: **real TLS**, scripted, loopback only.
//!
//! A real login walks a fixed sequence of newline-terminated, tab-separated
//! exchanges with `eaccess.play.net:7910` (SPEC_P6.md §1-6, and
//! `hydra_net::eaccess::wire` for the framing the client side uses):
//!
//! | stage | client sends | server answers |
//! | --- | --- | --- |
//! | `k_response` | `K` | `K\t<script key>` |
//! | `a_response` | `A\t<account>\t<keyed password>` | `A\t<account>\tKEY\t<session key>\t<holder>`, or `PASSWORD` |
//! | `m_response` | `M` | `M\t<code>\t<name>` per game |
//! | `f_response` | `F\t<game>` | `F\t<tier>` |
//! | `g_response` | `G\t<game>` | `G\t<g_response>` |
//! | `c_response` | `C` | `C\t<n>\t<n>\t0\t1` then `\t<code>\t<name>` per character |
//! | `l_response` | `L\t<code>\tSTORM` | `L\tOK\t...GAMEHOST=...\tGAMEPORT=...\tKEY=...`, or `L\tPROBLEM\t1` |
//!
//! The TLS certificate is generated per server (see [`crate::cert`]) so a test
//! can point a trust-on-first-use pin at it, and a *second* server presents a
//! different certificate — which is what the "the pinned certificate changed"
//! test needs.  The ClientHello is checked for SNI, because a real login sends
//! none: [`MockEaccess::sni_seen`] records what actually arrived.
//!
//! The server never panics: a malformed or unexpected line ends the session, is
//! recorded in [`MockEaccess::errors`], and the listener waits for the next
//! connection.  A client EOF ends the session cleanly.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

use crate::cert::{self, GeneratedCert};

/// How long a single read may take before the session is abandoned.
///
/// Without it a client that connects and then goes quiet would park the server
/// thread forever, and [`MockEaccess::join`] with it.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// What the `L` response tells the client about the game socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchInfo {
    /// `GAMECODE`: the game the launch is for.
    pub code: String,
    /// `GAMEHOST`: where the game socket is (always loopback in tests).
    pub host: String,
    /// `GAMEPORT`: the game socket's port.
    pub port: u16,
    /// `KEY`: the launch key the client sends as the first line to the game.
    pub key: String,
}

/// Everything the mock server says, in one place.
///
/// Every value is synthetic: no real account, password, character or key ever
/// appears here (this is a public repository).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EaccessScript {
    /// The script key (`K` response).  The client keys the password with it.
    pub key: String,
    /// The account name the `A` line must carry.
    pub account: String,
    /// The password the keyed hash on the `A` line must decode to.
    pub password: String,
    /// The holder name in the `A` response.
    pub holder: String,
    /// The session key in the `A` response.
    pub session_key: String,
    /// The game list: `(code, name)` pairs, answered in this order.
    pub games: Vec<(String, String)>,
    /// The subscription tier (`F` response).
    pub tier: String,
    /// The `G` response payload, i.e. what follows `G\t`.
    pub g_response: String,
    /// The character list: `(code, name)` pairs.
    pub characters: Vec<(String, String)>,
    /// The launch details the `L` response hands over.
    pub launch: LaunchInfo,
}

impl EaccessScript {
    /// A script of entirely synthetic values.
    ///
    /// `launch.port` is `0`: a test that walks as far as the `L` response gives
    /// a [`crate::MockGame`]'s address to `launch` first, so the client is sent
    /// to the port the mock game server actually listens on.
    pub fn synthetic() -> EaccessScript {
        EaccessScript {
            // Exactly 32 ASCII bytes, which is what a real server sends.
            key: "0123456789abcdef0123456789abcdef".to_string(),
            account: "main".to_string(),
            password: "synthetic-password".to_string(),
            holder: "Test Holder".to_string(),
            session_key: "5f3e2b1a09c8d7e6f5a4b3c2d1e0f9a8".to_string(),
            games: vec![("GS3".to_string(), "GemStone IV".to_string())],
            tier: "NORMAL".to_string(),
            g_response: "STORM\tNORMAL\t0\t\tROOT=sgc/gs\tMKTG=x".to_string(),
            characters: vec![("W_TEST_001".to_string(), "Aelith".to_string())],
            launch: LaunchInfo {
                code: "GS3".to_string(),
                host: "127.0.0.1".to_string(),
                port: 0,
                key: "9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c4d".to_string(),
            },
        }
    }
}

/// What the server saw and what went wrong, shared with the server thread.
#[derive(Debug, Default)]
struct Record {
    sni_seen: bool,
    sessions: usize,
    requests: Vec<String>,
    errors: Vec<String>,
}

fn lock(record: &Arc<Mutex<Record>>) -> MutexGuard<'_, Record> {
    // The server thread never panics (every failure is an `io::Error`), but a
    // poisoned lock must not turn a test into a second panic.
    record
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A scripted eaccess server on `127.0.0.1`, over real TLS.
///
/// It keeps serving connections (one at a time) until [`join`](Self::join) or
/// drop, so a trust-on-first-use test can log in twice against the same
/// certificate.
pub struct MockEaccess {
    addr: SocketAddr,
    cert_der: Vec<u8>,
    record: Arc<Mutex<Record>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<io::Result<()>>>,
}

impl MockEaccess {
    /// Bind `127.0.0.1:0`, generate a certificate, and start serving.
    ///
    /// Returns as soon as the listener is bound: nothing waits for a client.
    pub fn start(script: EaccessScript) -> io::Result<MockEaccess> {
        crate::install_provider();
        let generated = cert::generate();
        let config = Arc::new(server_config(&generated)?);
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let record = Arc::new(Mutex::new(Record::default()));
        let stop = Arc::new(AtomicBool::new(false));

        let handle = {
            let record = Arc::clone(&record);
            let stop = Arc::clone(&stop);
            thread::Builder::new()
                .name("mock-eaccess".to_string())
                .spawn(move || serve(listener, config, script, record, stop))?
        };

        Ok(MockEaccess {
            addr,
            cert_der: generated.cert_der,
            record,
            stop,
            handle: Some(handle),
        })
    }

    /// The loopback address the server is listening on.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The DER certificate this server presents.
    ///
    /// Two servers never share one: the bytes come from a freshly generated key
    /// each time, which is how a changed-certificate test is written.
    pub fn certificate_der(&self) -> Vec<u8> {
        self.cert_der.clone()
    }

    /// Whether the ClientHello carried a server name (SNI).
    ///
    /// A real login dials by address and sends none, so this should be `false`;
    /// it is recorded from the TLS connection, not assumed.
    pub fn sni_seen(&self) -> bool {
        lock(&self.record).sni_seen
    }

    /// The lines the client sent, in order.
    ///
    /// The keyed password on the `A` line is masked: the recorded line is
    /// `A\t<account>\t<key>`, whatever the client actually sent.
    pub fn requests(&self) -> Vec<String> {
        lock(&self.record).requests.clone()
    }

    /// Everything the mock noticed, in order.
    ///
    /// Malformed or unexpected lines, TLS failures, and the refusals it served
    /// (a wrong password, an unknown character).  Empty after a clean script.
    pub fn errors(&self) -> Vec<String> {
        lock(&self.record).errors.clone()
    }

    /// Stop serving and wait for the server thread to end.
    ///
    /// The accept loop is woken by a throwaway connection to the listener, so
    /// this returns even if no client ever connected.
    pub fn join(mut self) -> io::Result<()> {
        self.shutdown()
    }

    fn shutdown(&mut self) -> io::Result<()> {
        let Some(handle) = self.handle.take() else {
            return Ok(());
        };
        self.stop.store(true, Ordering::SeqCst);
        // Wake the accept loop: it sees the flag and returns.
        let _ = TcpStream::connect(self.addr);
        match handle.join() {
            Ok(result) => result,
            Err(_) => Err(io::Error::new(
                io::ErrorKind::Other,
                "the mock eaccess server thread panicked",
            )),
        }
    }
}

impl Drop for MockEaccess {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn server_config(generated: &GeneratedCert) -> io::Result<ServerConfig> {
    ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(generated.cert_der.clone())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(generated.key_der.clone())),
        )
        .map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("the generated certificate is not usable by rustls: {error}"),
            )
        })
}

/// Accept and serve connections until the stop flag is set.
fn serve(
    listener: TcpListener,
    config: Arc<ServerConfig>,
    script: EaccessScript,
    record: Arc<Mutex<Record>>,
    stop: Arc<AtomicBool>,
) -> io::Result<()> {
    loop {
        let (tcp, peer) = match listener.accept() {
            Ok(accepted) => accepted,
            Err(error) => {
                lock(&record).errors.push(format!("accept failed: {error}"));
                return Ok(());
            }
        };
        if stop.load(Ordering::SeqCst) {
            return Ok(());
        }

        let mut tcp = tcp;
        let _ = tcp.set_read_timeout(Some(READ_TIMEOUT));
        let mut conn = match ServerConnection::new(Arc::clone(&config)) {
            Ok(conn) => conn,
            Err(error) => {
                lock(&record).errors.push(format!("tls setup failed: {error}"));
                return Ok(());
            }
        };
        // Handshake first, so the SNI is known even if the client then says
        // nothing at all.
        if let Err(error) = handshake(&mut conn, &mut tcp) {
            lock(&record)
                .errors
                .push(format!("tls handshake from {peer} failed: {error}"));
            continue;
        }
        {
            let mut record = lock(&record);
            record.sessions += 1;
            record.sni_seen |= conn.server_name().is_some();
        }

        let mut wire = Wire {
            stream: BufReader::new(StreamOwned::new(conn, tcp)),
            record: Arc::clone(&record),
        };
        let outcome = converse(&mut wire, &script);
        wire.close();
        if let Err(error) = outcome {
            lock(&record).errors.push(format!("{error}"));
        }
    }
}

fn handshake(conn: &mut ServerConnection, tcp: &mut TcpStream) -> io::Result<()> {
    while conn.is_handshaking() {
        conn.complete_io(tcp)?;
    }
    Ok(())
}

/// The TLS stream plus the record of what went over it.
struct Wire {
    stream: BufReader<StreamOwned<ServerConnection, TcpStream>>,
    record: Arc<Mutex<Record>>,
}

impl Wire {
    /// Read one client line, recording it (with any key material masked).
    fn recv(&mut self, stage: &'static str) -> io::Result<Vec<u8>> {
        let mut line = Vec::new();
        let read = self.stream.read_until(b'\n', &mut line)?;
        if read == 0 {
            return Err(stage_error(
                stage,
                "the client closed the connection instead of sending a line",
            ));
        }
        while line.last() == Some(&b'\n') || line.last() == Some(&b'\r') {
            line.pop();
        }
        lock(&self.record)
            .requests
            .push(redact_request(&lossy(&line)));
        Ok(line)
    }

    /// Write one response line and flush it: the client is waiting on it.
    fn send(&mut self, line: &str) -> io::Result<()> {
        let stream = self.stream.get_mut();
        stream.write_all(line.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()
    }

    fn note(&self, note: String) {
        lock(&self.record).errors.push(note);
    }

    fn close(&mut self) {
        let stream = self.stream.get_mut();
        stream.conn.send_close_notify();
        let _ = stream.flush();
    }
}

/// Walk the eight stages of a login against the script.
fn converse(wire: &mut Wire, script: &EaccessScript) -> io::Result<()> {
    // 2. k_response: the client asks for the script key to key its password.
    let line = wire.recv("k_response")?;
    expect_command(&line, b'K', "k_response")?;
    wire.send(&format!("K\t{}", script.key))?;

    // 3. a_response: the account and the keyed password.
    let line = wire.recv("a_response")?;
    expect_command(&line, b'A', "a_response")?;
    let fields: Vec<&[u8]> = line.split(|byte| *byte == b'\t').collect();
    if fields.len() < 3 {
        return Err(stage_error(
            "a_response",
            format!(
                "expected A, the account and the keyed password, got {} field(s)",
                fields.len()
            ),
        ));
    }
    let account = lossy(fields[1]);
    if !account.eq_ignore_ascii_case(&script.account) {
        wire.send("PASSWORD")?;
        wire.note(format!(
            "the client asked for account {account:?} but the script is for {:?}; answered PASSWORD",
            script.account
        ));
        return Ok(());
    }
    let expected = keyed_password(&script.password, &script.key).ok_or_else(|| {
        stage_error(
            "a_response",
            "the script's key is shorter than the script's password, \
             so no keyed password can be checked",
        )
    })?;
    if fields[2] != expected.as_slice() {
        wire.send("PASSWORD")?;
        wire.note(
            "the client's keyed password does not match the script; answered PASSWORD".to_string(),
        );
        return Ok(());
    }
    wire.send(&format!(
        "A\t{}\tKEY\t{}\t{}",
        script.account, script.session_key, script.holder
    ))?;

    // 4. m_response: the game list.
    let line = wire.recv("m_response")?;
    expect_command(&line, b'M', "m_response")?;
    for (code, name) in &script.games {
        wire.send(&format!("M\t{code}\t{name}"))?;
    }

    // 5. f_response: the tier for the chosen game.
    let line = wire.recv("f_response")?;
    expect_command(&line, b'F', "f_response")?;
    wire.send(&format!("F\t{}", script.tier))?;

    // 6. g_response: the game's launch details, and the game it was for.
    let line = wire.recv("g_response")?;
    expect_command(&line, b'G', "g_response")?;
    let chosen = field(&line, 1).unwrap_or_default();
    wire.send(&format!("G\t{}", script.g_response))?;

    // 7. c_response: the character list, as one line.
    let line = wire.recv("c_response")?;
    expect_command(&line, b'C', "c_response")?;
    let count = script.characters.len();
    let mut response = format!("C\t{count}\t{count}\t0\t1");
    for (code, name) in &script.characters {
        response.push_str(&format!("\t{code}\t{name}"));
    }
    wire.send(&response)?;

    // 8. l_response: the character is picked, the game socket is named.  The
    // server closes the connection after this line, as a real one does.
    let line = wire.recv("l_response")?;
    expect_command(&line, b'L', "l_response")?;
    let code = field(&line, 1).unwrap_or_default();
    if !script.characters.iter().any(|(known, _)| *known == code) {
        wire.send("L\tPROBLEM\t1")?;
        wire.note(format!(
            "no script character is {code:?}; answered L PROBLEM 1"
        ));
        return Ok(());
    }
    // GAMECODE follows the game the client asked about in the G stage; the
    // script's launch code is the fallback for a client that sent nothing.
    let game = if chosen.is_empty() {
        script.launch.code.clone()
    } else {
        chosen
    };
    wire.send(&format!(
        "L\tOK\tUPPORT=5535\tGAME=STORM\tGAMECODE={}\tFULLGAMENAME=Wrayth\tGAMEFILE=WRAYTH.EXE\t\
         GAMEHOST={}\tGAMEPORT={}\tKEY={}",
        game, script.launch.host, script.launch.port, script.launch.key
    ))?;
    Ok(())
}

/// Check that a line is the expected command letter (alone or with fields).
fn expect_command(line: &[u8], command: u8, stage: &'static str) -> io::Result<()> {
    if line.first() == Some(&command) && (line.len() == 1 || line[1] == b'\t') {
        return Ok(());
    }
    Err(stage_error(
        stage,
        format!(
            "expected a {} line, got {:?}",
            command as char,
            truncate(&lossy(line))
        ),
    ))
}

/// The field at `index`, as a string, of a tab-separated line.
fn field(line: &[u8], index: usize) -> Option<String> {
    line.split(|byte| *byte == b'\t')
        .nth(index)
        .map(lossy)
        .filter(|field| !field.is_empty())
}

/// The server's keying of the password, from `hydra_net::eaccess::wire::hash_password`:
/// `out[i] = ((password[i] - 0x20) ^ key[i]) + 0x20`.
///
/// `None` when the key is shorter than the password: a real server keys the
/// whole password or refuses, it never truncates.
fn keyed_password(password: &str, key: &str) -> Option<Vec<u8>> {
    let password = password.as_bytes();
    let key = key.as_bytes();
    if key.len() < password.len() {
        return None;
    }
    Some(
        password
            .iter()
            .zip(key.iter())
            .map(|(byte, key)| byte.wrapping_sub(0x20) ^ key)
            .map(|byte| byte.wrapping_add(0x20))
            .collect(),
    )
}

/// Mask key material in a client line before it is recorded.
///
/// The `A` line carries the keyed password, so everything from its third field
/// on becomes `<key>` — the same shape `hydra_net`'s own redaction uses, and the
/// reason [`MockEaccess::requests`] is safe to print in a test failure.
fn redact_request(line: &str) -> String {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.first() == Some(&"A") && fields.len() >= 3 {
        return format!("{}\t{}\t<key>", fields[0], fields[1]);
    }
    line.to_string()
}

fn stage_error(stage: &'static str, detail: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("[{stage}] {detail}"))
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn truncate(text: &str) -> String {
    text.chars().take(120).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::crypto::CryptoProvider;
    use rustls::pki_types::{ServerName, UnixTime};
    use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme};
    use std::io::Read;

    /// A verifier that accepts any certificate: the mock's is self-signed, and
    /// pinning it is the login client's job, not the test client's.
    #[derive(Debug)]
    struct AcceptAnyCertificate(Arc<CryptoProvider>);

    impl ServerCertVerifier for AcceptAnyCertificate {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            self.0.signature_verification_algorithms.supported_schemes()
        }
    }

    /// A real TLS client speaking the login wire protocol.
    struct Client {
        stream: BufReader<StreamOwned<ClientConnection, TcpStream>>,
    }

    impl Client {
        /// Dial the mock by IP, which is exactly what a real login does: no
        /// server name, therefore no SNI.
        fn connect(addr: SocketAddr) -> Client {
            crate::install_provider();
            let provider = CryptoProvider::get_default()
                .cloned()
                .expect("install_provider installs one");
            let config = ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(AcceptAnyCertificate(provider)))
                .with_no_client_auth();
            let name = ServerName::IpAddress(addr.ip().into());
            let connection = ClientConnection::new(Arc::new(config), name).unwrap();
            let tcp = TcpStream::connect(addr).unwrap();
            Client {
                stream: BufReader::new(StreamOwned::new(connection, tcp)),
            }
        }

        fn send_bytes(&mut self, line: &[u8]) {
            let stream = self.stream.get_mut();
            stream.write_all(line).unwrap();
            stream.write_all(b"\n").unwrap();
            stream.flush().unwrap();
        }

        fn send(&mut self, line: &str) {
            self.send_bytes(line.as_bytes());
        }

        fn recv(&mut self) -> String {
            let mut line = String::new();
            self.stream.read_line(&mut line).unwrap();
            line.trim_end_matches(['\r', '\n']).to_string()
        }

        /// Everything left after the server closes: always empty.
        fn remainder(&mut self) -> String {
            let mut rest = String::new();
            assert_eq!(
                self.stream.read_to_string(&mut rest).unwrap(),
                0,
                "the server must close the connection"
            );
            rest
        }
    }

    /// The `A` line the client sends: account and raw keyed-password bytes.
    fn a_line(account: &str, keyed: &[u8]) -> Vec<u8> {
        let mut line = Vec::new();
        line.extend_from_slice(b"A\t");
        line.extend_from_slice(account.as_bytes());
        line.push(b'\t');
        line.extend_from_slice(keyed);
        line
    }

    /// Walk the stages up to the character list, which the caller then uses.
    fn reach_character_list(client: &mut Client, script: &EaccessScript) {
        client.send("K");
        assert!(client.recv().starts_with("K\t"));
        let keyed = keyed_password(&script.password, &script.key).unwrap();
        client.send_bytes(&a_line(&script.account, &keyed));
        assert!(client.recv().starts_with("A\t"));
        client.send("M");
        for _ in &script.games {
            assert!(client.recv().starts_with("M\t"));
        }
        client.send("F\tGS3");
        assert!(client.recv().starts_with("F\t"));
        client.send("G\tGS3");
        assert!(client.recv().starts_with("G\t"));
        client.send("C");
        assert!(client.recv().starts_with("C\t"));
    }

    /// The value of a `NAME=value` field of a tab-separated line.
    fn named<'a>(line: &'a str, name: &str) -> Option<&'a str> {
        let prefix = format!("{name}=");
        line.split('\t')
            .find_map(|field| field.strip_prefix(prefix.as_str()))
    }

    #[test]
    fn the_whole_eight_step_script_runs_over_real_tls() {
        let script = EaccessScript::synthetic();
        let server = MockEaccess::start(script.clone()).unwrap();
        let mut client = Client::connect(server.addr());

        // 2. k_response.
        client.send("K");
        assert_eq!(client.recv(), format!("K\t{}", script.key));

        // 3. a_response: the keyed password, then the key and the holder.
        let keyed = keyed_password(&script.password, &script.key).unwrap();
        client.send_bytes(&a_line(&script.account, &keyed));
        let a_response = client.recv();
        assert_eq!(
            a_response,
            format!(
                "A\t{}\tKEY\t{}\t{}",
                script.account, script.session_key, script.holder
            )
        );
        let fields: Vec<&str> = a_response.split('\t').collect();
        assert_eq!(fields[0], "A");
        assert_eq!(fields[2], "KEY");
        assert_eq!(fields[3], script.session_key);
        assert_eq!(fields[4], script.holder);
        assert_eq!(script.session_key.len(), 32, "a session key is 32 hex chars");

        // 4. m_response: the game list, one line per game.
        client.send("M");
        for (code, name) in &script.games {
            let response = client.recv();
            let fields: Vec<&str> = response.split('\t').collect();
            assert_eq!(fields[0], "M");
            assert_eq!(fields[1], code);
            assert_eq!(fields[2], name);
        }

        // 5. f_response.
        client.send("F\tGS3");
        assert_eq!(client.recv(), format!("F\t{}", script.tier));

        // 6. g_response.
        client.send("G\tGS3");
        let g_response = client.recv();
        assert_eq!(g_response, format!("G\t{}", script.g_response));
        assert_eq!(g_response.split('\t').nth(1), Some("STORM"));
        assert_eq!(named(&g_response, "ROOT"), Some("sgc/gs"));

        // 7. c_response: the count first, then code/name pairs.
        client.send("C");
        let c_response = client.recv();
        let fields: Vec<&str> = c_response.split('\t').collect();
        assert_eq!(&fields[..5], ["C", "1", "1", "0", "1"]);
        assert_eq!(fields[5], script.characters[0].0);
        assert_eq!(fields[6], script.characters[0].1);

        // 8. l_response: where the game socket is and the key to open it.
        client.send(&format!("L\t{}\tSTORM", script.characters[0].0));
        let l_response = client.recv();
        assert_eq!(named(&l_response, "UPPORT"), Some("5535"));
        assert_eq!(named(&l_response, "GAME"), Some("STORM"));
        assert_eq!(named(&l_response, "GAMECODE"), Some("GS3"));
        assert_eq!(named(&l_response, "FULLGAMENAME"), Some("Wrayth"));
        assert_eq!(named(&l_response, "GAMEFILE"), Some("WRAYTH.EXE"));
        assert_eq!(named(&l_response, "GAMEHOST"), Some(script.launch.host.as_str()));
        assert_eq!(
            named(&l_response, "GAMEPORT"),
            Some(script.launch.port.to_string().as_str())
        );
        assert_eq!(named(&l_response, "KEY"), Some(script.launch.key.as_str()));

        // The server closes after the launch line.
        assert_eq!(client.remainder(), "");

        // Nothing in what the server saw leaks the keyed password, and the
        // ClientHello carried no SNI.
        assert!(!server.sni_seen(), "an IP server name must not send SNI");
        assert_eq!(
            server.requests(),
            vec![
                "K".to_string(),
                format!("A\t{}\t<key>", script.account),
                "M".to_string(),
                "F\tGS3".to_string(),
                "G\tGS3".to_string(),
                "C".to_string(),
                format!("L\t{}\tSTORM", script.characters[0].0),
            ]
        );
        assert_eq!(server.errors(), Vec::<String>::new());
        server.join().unwrap();
    }

    #[test]
    fn a_second_game_and_a_second_character_come_out_in_order() {
        let mut script = EaccessScript::synthetic();
        script.games.push(("DR".to_string(), "DragonRealms".to_string()));
        script
            .characters
            .push(("W_TEST_002".to_string(), "Bryn".to_string()));
        let server = MockEaccess::start(script.clone()).unwrap();
        let mut client = Client::connect(server.addr());

        client.send("K");
        assert!(client.recv().starts_with("K\t"));
        let keyed = keyed_password(&script.password, &script.key).unwrap();
        client.send_bytes(&a_line(&script.account, &keyed));
        assert!(client.recv().starts_with("A\t"));

        // One `M` line per game, in script order.
        client.send("M");
        assert_eq!(client.recv(), "M\tGS3\tGemStone IV");
        assert_eq!(client.recv(), "M\tDR\tDragonRealms");

        client.send("F\tDR");
        assert_eq!(client.recv(), "F\tNORMAL");
        client.send("G\tDR");
        assert!(client.recv().starts_with("G\t"));

        // One `C` line, every character after the count.
        client.send("C");
        let c_response = client.recv();
        let fields: Vec<&str> = c_response.split('\t').collect();
        assert_eq!(&fields[..5], ["C", "2", "2", "0", "1"]);
        assert_eq!(&fields[5..], ["W_TEST_001", "Aelith", "W_TEST_002", "Bryn"]);

        // GAMECODE follows the game the client asked about, and the launch port
        // is the script's (here the untouched placeholder).
        client.send("L\tW_TEST_002\tSTORM");
        let l_response = client.recv();
        assert_eq!(named(&l_response, "GAMECODE"), Some("DR"));
        assert_eq!(named(&l_response, "GAMEPORT"), Some("0"));
        assert_eq!(client.remainder(), "");
        server.join().unwrap();
    }

    #[test]
    fn a_wrong_password_gets_password_and_ends_the_connection() {
        let script = EaccessScript::synthetic();
        let server = MockEaccess::start(script.clone()).unwrap();
        let mut client = Client::connect(server.addr());

        client.send("K");
        assert!(client.recv().starts_with("K\t"));

        let keyed = keyed_password("wrong-password", &script.key).unwrap();
        client.send_bytes(&a_line(&script.account, &keyed));
        assert_eq!(client.recv(), "PASSWORD");
        assert_eq!(client.remainder(), "");

        let errors = server.errors();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("does not match the script"), "{errors:?}");
        assert_eq!(server.requests()[1], format!("A\t{}\t<key>", script.account));
        server.join().unwrap();
    }

    #[test]
    fn a_wrong_account_gets_password_too() {
        let script = EaccessScript::synthetic();
        let server = MockEaccess::start(script.clone()).unwrap();
        let mut client = Client::connect(server.addr());

        client.send("K");
        assert!(client.recv().starts_with("K\t"));
        let keyed = keyed_password(&script.password, &script.key).unwrap();
        client.send_bytes(&a_line("someone-else", &keyed));
        assert_eq!(client.recv(), "PASSWORD");

        let errors = server.errors();
        assert!(errors[0].contains("someone-else"), "{errors:?}");
        server.join().unwrap();
    }

    #[test]
    fn a_malformed_line_is_recorded_and_the_session_dropped() {
        let server = MockEaccess::start(EaccessScript::synthetic()).unwrap();
        let mut client = Client::connect(server.addr());

        client.send("nonsense");
        assert_eq!(client.remainder(), "");

        assert_eq!(server.requests(), vec!["nonsense".to_string()]);
        let errors = server.errors();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].starts_with("[k_response]"), "{errors:?}");
        // The server survives a malformed session: it is still listening.
        let mut second = Client::connect(server.addr());
        second.send("K");
        assert!(second.recv().starts_with("K\t"));
        drop(second);
        server.join().unwrap();
    }

    #[test]
    fn an_unknown_character_gets_l_problem() {
        let script = EaccessScript::synthetic();
        let server = MockEaccess::start(script.clone()).unwrap();
        let mut client = Client::connect(server.addr());

        reach_character_list(&mut client, &script);
        client.send("L\tNOBODY\tSTORM");
        assert_eq!(client.recv(), "L\tPROBLEM\t1");
        assert_eq!(client.remainder(), "");

        let errors = server.errors();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("NOBODY"), "{errors:?}");
        server.join().unwrap();
    }

    #[test]
    fn the_same_server_serves_a_second_login_with_the_same_certificate() {
        let server = MockEaccess::start(EaccessScript::synthetic()).unwrap();
        let certificate = server.certificate_der();

        for _ in 0..2 {
            let mut client = Client::connect(server.addr());
            client.send("K");
            assert!(client.recv().starts_with("K\t"));
            // Walk away mid-script: the server must shrug and take the next
            // connection, still with the same certificate.
            drop(client);
            assert_eq!(server.certificate_der(), certificate);
        }
        assert_eq!(server.requests(), vec!["K".to_string(), "K".to_string()]);
        assert!(!server.sni_seen());
        server.join().unwrap();
    }

    #[test]
    fn two_servers_present_different_certificates() {
        let first = MockEaccess::start(EaccessScript::synthetic()).unwrap();
        let second = MockEaccess::start(EaccessScript::synthetic()).unwrap();
        assert_ne!(first.certificate_der(), second.certificate_der());
        assert_ne!(first.addr(), second.addr());
        first.join().unwrap();
        second.join().unwrap();
    }

    #[test]
    fn a_server_with_no_client_still_joins() {
        let server = MockEaccess::start(EaccessScript::synthetic()).unwrap();
        assert_eq!(server.addr().ip().to_string(), "127.0.0.1");
        assert_eq!(server.requests(), Vec::<String>::new());
        server.join().unwrap();
    }

    #[test]
    fn the_password_is_keyed_the_way_the_client_keys_it() {
        // The recovered formula: ((password - 0x20) ^ key) + 0x20, byte by byte.
        let keyed = keyed_password("ab", "\x01\x02").unwrap();
        assert_eq!(
            keyed,
            vec![(b'a' - 0x20 ^ 0x01) + 0x20, (b'b' - 0x20 ^ 0x02) + 0x20]
        );
        // A key longer than the password is used whole (the extra is ignored).
        assert_eq!(
            keyed_password("ab", "\x01\x02\x03\x04").unwrap(),
            keyed_password("ab", "\x01\x02").unwrap()
        );
        // A key shorter than the password cannot produce the hash at all.
        assert!(keyed_password("password", "short").is_none());
    }

    #[test]
    fn recorded_lines_mask_the_keyed_password() {
        assert_eq!(redact_request("K"), "K");
        assert_eq!(redact_request("A\tmain\tsecret"), "A\tmain\t<key>");
        assert_eq!(redact_request("A\tmain\tmore\tsecret"), "A\tmain\t<key>");
        assert_eq!(redact_request("M\tGS3"), "M\tGS3");
    }

    #[test]
    fn the_command_letter_must_be_alone_or_followed_by_a_tab() {
        assert!(expect_command(b"K", b'K', "k_response").is_ok());
        assert!(expect_command(b"K\tabc", b'K', "k_response").is_ok());
        assert!(expect_command(b"KEY", b'K', "k_response").is_err());
        assert!(expect_command(b"", b'K', "k_response").is_err());
        assert!(expect_command(b"A\tx\ty", b'K', "k_response").is_err());
    }
}
