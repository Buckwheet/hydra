//! Test-only mock servers for the Phase 6 login path.
//!
//! Two servers, both bound to `127.0.0.1:0` (loopback, ephemeral port, never a
//! real game server):
//!
//! * [`MockEaccess`] — the eaccess login service over **real TLS**, speaking the
//!   tab-separated stage script a real login walks: `K`, `A`, `M`, `F`, `G`,
//!   `C`, `L` (see `hydra_net::eaccess::wire` and SPEC_P6.md).  Its certificate
//!   is a self-signed Ed25519 certificate generated at test time ([`cert`]), so
//!   no key or certificate fixture is ever committed.
//! * [`MockGame`] — the game socket entry point: plain TCP, reads the two-line
//!   handshake (`KEY`, then [`CLIENT_IDENTIFICATION`]) and writes exactly the
//!   synthetic XML stream it was started with.
//!
//! Nothing here is part of the shipped binary; the crate exists so the login
//! client and the session wiring can be tested end to end with no network, no
//! Ruby, no Lich and no credentials.

pub mod cert;
pub mod eaccess;
pub mod game;

pub use cert::GeneratedCert;
pub use eaccess::{EaccessScript, LaunchInfo, MockEaccess};
pub use game::{MockGame, CLIENT_IDENTIFICATION};

/// Install rustls's `ring` crypto provider as the process default.
///
/// rustls 0.23 refuses to build a `ClientConfig`/`ServerConfig` without a
/// provider, and this workspace can only use `ring` (the default `aws-lc-rs`
/// provider is not in the offline registry).  Calling this twice is fine: an
/// already-installed provider is left alone.
pub fn install_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}
