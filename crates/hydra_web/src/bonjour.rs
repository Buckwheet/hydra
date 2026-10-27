//! mDNS announcement so the phone can find Hydra on the LAN.
//!
//! Recovered from `bonjour::Announced::start` (`0x1002c9d90`, 2616 bytes) and
//! `<Announced as Drop>::drop` (`0x1002c5fc4`).
//!
//! The original used the `mdns_sd` crate: `start` built a `ServiceInfo` and
//! registered it on a `ServiceDaemon`; `Drop` called `unregister` and
//! `shutdown`.  Nothing here talks to the network: the daemon is a trait so the
//! caller can supply the real mDNS backend, and the default one only records the
//! announcement.

/// The registered service type (`0x1020725fc`, 19 bytes).
pub const SERVICE: &str = "v1_hydra._tcp.local.";
/// TXT record key carrying the pairing token (9 bytes).
pub const TXT_TOKEN: &str = "web.token";
/// TXT record key carrying the storage schema (19 bytes).
pub const TXT_VERSION: &str = "web.storage_version";
/// TXT record key carrying the chosen layout (6 bytes).
pub const TXT_LAYOUT: &str = "layout";
/// Environment variable read for the instance name (12 bytes, `0x1002c9dd0`).
pub const HOSTNAME_ENV: &str = "COMPUTERNAME";
/// Fallback instance name (`"this computer"`, 13 bytes).
pub const DEFAULT_NAME: &str = "this computer";

/// The seam for the real mDNS daemon.
pub trait Daemon {
    /// Register `instance` under [`SERVICE`] on `port` with these TXT records.
    fn register(&self, instance: &str, port: u16, txt: &[(&str, String)]);
    /// Withdraw a previous registration.
    fn unregister(&self, instance: &str);
    /// Shut the daemon down.
    fn shutdown(&self);
}

/// A daemon that remembers what it was told to do (the default, no I/O).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recording {
    pub registered: Vec<(String, u16, Vec<(String, String)>)>,
    pub unregistered: Vec<String>,
    pub shutdowns: usize,
}

impl Daemon for std::cell::RefCell<Recording> {
    fn register(&self, instance: &str, port: u16, txt: &[(&str, String)]) {
        self.borrow_mut().registered.push((
            instance.to_string(),
            port,
            txt.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect(),
        ));
    }

    fn unregister(&self, instance: &str) {
        self.borrow_mut().unregistered.push(instance.to_string());
    }

    fn shutdown(&self) {
        self.borrow_mut().shutdowns += 1;
    }
}

/// A live announcement.
pub struct Announced {
    /// The instance name that was registered.
    pub instance: String,
    /// The port the web server listens on.
    pub port: u16,
    /// The TXT records, in the recovered key order.
    pub txt: Vec<(String, String)>,
    daemon: Option<Box<dyn Daemon>>,
}

impl std::fmt::Debug for Announced {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Announced")
            .field("instance", &self.instance)
            .field("port", &self.port)
            .field("txt", &self.txt)
            .field("daemon", &self.daemon.is_some())
            .finish()
    }
}

impl Announced {
    /// Announce the web page.
    ///
    /// Recovered (`0x1002c9d90`): the instance name comes from `COMPUTERNAME`
    /// (falling back to `"this computer"`), the service type is
    /// [`SERVICE`] and the TXT records are [`TXT_TOKEN`],
    /// [`TXT_VERSION`] and [`TXT_LAYOUT`].
    ///
    /// `// UNKNOWN:` the TXT values' exact encoding.  The token is passed through
    /// verbatim, the storage version as decimal text and the layout as-is.
    pub fn start(token: &str, port: u16, storage_version: u32, layout: &str) -> Announced {
        Announced::start_on(Box::new(std::cell::RefCell::new(Recording::default())), token, port, storage_version, layout)
    }

    /// Announce with an explicit daemon.
    pub fn start_on(
        daemon: Box<dyn Daemon>,
        token: &str,
        port: u16,
        storage_version: u32,
        layout: &str,
    ) -> Announced {
        let instance = std::env::var(HOSTNAME_ENV)
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                std::env::var("HOSTNAME")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            })
            .unwrap_or_else(|| DEFAULT_NAME.to_string());
        let txt = vec![
            (TXT_TOKEN.to_string(), token.to_string()),
            (
                TXT_VERSION.to_string(),
                storage_version.to_string(),
            ),
            (TXT_LAYOUT.to_string(), layout.to_string()),
        ];
        daemon.register(&instance, port, &[(
            TXT_TOKEN,
            token.to_string(),
        ), (
            TXT_VERSION,
            storage_version.to_string(),
        ), (
            TXT_LAYOUT,
            layout.to_string(),
        )]);
        Announced {
            instance,
            port,
            txt,
            daemon: Some(daemon),
        }
    }

    /// The full service name, e.g. `hydra v1_hydra._tcp.local.`.
    pub fn service_name(&self) -> String {
        format!("{}.{}", self.instance, SERVICE)
    }
}

impl Drop for Announced {
    /// Recovered (`0x1002c5fc4`): `unregister` then `shutdown`, then drop the
    /// daemon handle.
    fn drop(&mut self) {
        if let Some(daemon) = &self.daemon {
            daemon.unregister(&self.instance);
            daemon.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn service_and_txt_keys_are_the_recovered_ones() {
        assert_eq!(SERVICE, "v1_hydra._tcp.local.");
        assert_eq!(TXT_TOKEN, "web.token");
        assert_eq!(TXT_VERSION, "web.storage_version");
        assert_eq!(TXT_LAYOUT, "layout");
        assert_eq!(HOSTNAME_ENV, "COMPUTERNAME");
    }

    #[test]
    fn start_registers_and_drop_unregisters() {
        let recorder = Rc::new(std::cell::RefCell::new(Recording::default()));
        struct Shared(Rc<std::cell::RefCell<Recording>>);
        impl Daemon for Shared {
            fn register(&self, instance: &str, port: u16, txt: &[(&str, String)]) {
                self.0.borrow_mut().registered.push((
                    instance.to_string(),
                    port,
                    txt.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect(),
                ));
            }
            fn unregister(&self, instance: &str) {
                self.0.borrow_mut().unregistered.push(instance.to_string());
            }
            fn shutdown(&self) {
                self.0.borrow_mut().shutdowns += 1;
            }
        }

        let ann = Announced::start_on(
            Box::new(Shared(recorder.clone())),
            "deadbeef",
            7788,
            3,
            "layout.json",
        );
        assert_eq!(ann.port, 7788);
        assert_eq!(ann.txt[0], (TXT_TOKEN.to_string(), "deadbeef".to_string()));
        assert_eq!(ann.txt[1], (TXT_VERSION.to_string(), "3".to_string()));
        assert_eq!(ann.txt[2], (TXT_LAYOUT.to_string(), "layout.json".to_string()));
        assert!(ann.service_name().ends_with("v1_hydra._tcp.local."));

        {
            let rec = recorder.borrow();
            assert_eq!(rec.registered.len(), 1);
            assert_eq!(rec.registered[0].1, 7788);
            assert_eq!(rec.registered[0].2[0].0, "web.token");
        }

        let instance = ann.instance.clone();
        drop(ann);
        let rec = recorder.borrow();
        assert_eq!(rec.unregistered, vec![instance]);
        assert_eq!(rec.shutdowns, 1);
    }
}
