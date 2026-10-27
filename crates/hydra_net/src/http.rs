//! The shared `reqwest` seam.
//!
//! The original spoke HTTP through `reqwest` from inside its `tokio` runtime,
//! with one client per feature (the store, the trainer, the web login).  This is
//! the piece they had in common: one lazily built [`Client`], a couple of thin
//! helpers, and a bridge for callers that are still synchronous.
//!
//! The client carries no cookie jar: the `cookies` feature is not part of the
//! workspace pins, and the recovered code passed its single `Cookie:` header
//! explicitly anyway (see [`crate::trainer::http::Trainer::cookie_header`]).
//! Redirects are followed, which is what the store's sign-in flow needs; a
//! redirect *loop* surfaces as [`reqwest::Error::is_redirect`] and lands in
//! [`crate::store::http::Family::Redirect`].

use std::future::Future;

use reqwest::Client;

/// The shared client, built once.
///
/// `Client` is reference counted and cheap to clone; building it resolves the
/// TLS backend, so it is done once and kept.
pub fn client() -> Client {
    static CLIENT: std::sync::OnceLock<Client> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| Client::builder().build().unwrap_or_default())
        .clone()
}

/// GET `url` and hand back its body as text.
///
/// Redirects are followed.  The error is `reqwest`'s own so the caller can
/// classify it (`is_connect`, `is_timeout`, `is_redirect`); see
/// [`crate::store::http::Family::of_reqwest`].
pub async fn get_text(url: &str) -> Result<String, reqwest::Error> {
    client().get(url).send().await?.text().await
}

/// Run `future` to completion on a private current-thread runtime.
///
/// The recovered sibling crates in this workspace are still synchronous, and
/// `reqwest`'s own blocking client is not buildable here (its `blocking` feature
/// needs `futures-io`, which is not in the offline registry), so this is the
/// bridge: a current-thread runtime of our own, dropped again afterwards.
///
/// **Do not call this from inside a `tokio` runtime** — blocking on a runtime
/// from within one is what panics (or deadlocks, with a single thread).  The
/// attempt is refused with an error instead of taking the process down.
pub fn block_on<F: Future>(future: F) -> Result<F::Output, String> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(
            "cannot block on a request from inside a tokio runtime: await the async helper instead"
                .to_string(),
        );
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("no runtime for the request: {}", error))?;
    Ok(runtime.block_on(future))
}

/// A one-shot HTTP/1.1 server on the loopback interface, for the tests.
///
/// The tests here must not touch the network, so every `reqwest` round trip is
/// answered by a thread of our own on `127.0.0.1:0`.  The thread reads one
/// request (head and, when `Content-Length` says so, body), writes one reply and
/// hands the raw request text back through its `JoinHandle`.
#[cfg(test)]
pub(crate) mod loopback {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread::JoinHandle;

    /// Serve `status` (e.g. `200 OK`), `headers` and `body` exactly once.
    ///
    /// Returns the base URL to call and the thread, which yields the raw request
    /// it saw.
    pub(crate) fn serve_once(
        status: &str,
        headers: &str,
        body: &str,
    ) -> (String, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("a bound address").port();
        let status = status.to_string();
        let headers = headers.to_string();
        let body = body.to_string();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("one connection");
            let mut seen = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let read = match stream.read(&mut chunk) {
                    Ok(read) => read,
                    Err(error) => panic!("reading the request: {}", error),
                };
                if read == 0 {
                    break;
                }
                seen.extend_from_slice(&chunk[..read]);
                if request_is_complete(&seen) {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&seen).to_string();
            let reply = format!(
                "HTTP/1.1 {}\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                status,
                headers,
                body.len(),
                body
            );
            stream
                .write_all(reply.as_bytes())
                .and_then(|()| stream.flush())
                .expect("writing the reply");
            request
        });
        (format!("http://127.0.0.1:{}", port), handle)
    }

    /// True once the request head — and the body `Content-Length` promises — has
    /// arrived.
    fn request_is_complete(seen: &[u8]) -> bool {
        let text = String::from_utf8_lossy(seen);
        let Some(head_end) = text.find("\r\n\r\n") else {
            return false;
        };
        let length = text[..head_end]
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, value)| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        seen.len() >= head_end + 4 + length
    }
}
