//! mDNS announcement so the phone can find Hydra on the LAN.
//!
//! Recovered from `bonjour::Announced::start` (`0x1002c9d90`, 2616 bytes) and
//! `<Announced as Drop>::drop` (`0x1002c5fc4`).
//!
//! The original used the `mdns_sd` crate: `start` built a `ServiceInfo` and
//! registered it on a `ServiceDaemon`; `Drop` called `unregister` and
//! `shutdown`.  Both halves are here: [`Mdns`] is that crate behind the
//! [`Daemon`] trait (and [`service_info`] builds the record it registers), while
//! the default [`Announced::start`] keeps the recording daemon so nothing in
//! this crate opens a socket unless it is asked to
//! ([`Announced::start_mdns`]).

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

// ---------------------------------------------------------------------------
// The real daemon (mdns-sd)
// ---------------------------------------------------------------------------

/// The service record an announcement advertises.
///
/// Recovered: the instance name, the [`SERVICE`] type and the TXT records
/// [`TXT_TOKEN`], [`TXT_VERSION`], [`TXT_LAYOUT`].  The host name is the
/// instance's own (`<instance>.local.`) and the address is the machine's LAN
/// address ([`crate::lan_address`]).
///
/// `// UNKNOWN:` the TXT value encodings.  The token is passed through verbatim,
/// the storage version is decimal text and the layout is as given.
pub fn service_info(
    instance: &str,
    port: u16,
    ip: std::net::IpAddr,
    txt: &[(&str, String)],
) -> Result<mdns_sd::ServiceInfo, String> {
    let host = format!("{instance}.local.");
    mdns_sd::ServiceInfo::new(SERVICE, instance, &host, ip, port, txt)
        .map_err(|e| format!("{instance} cannot be announced: {e}"))
}

/// The mDNS responder the original used (`mdns_sd::ServiceDaemon`), behind the
/// [`Daemon`] seam.
///
/// [`Mdns::new`] starts the real responder (it opens the multicast sockets);
/// `register`, `unregister` and `shutdown` are the three calls the binary makes.
/// No test constructs one: [`Announced::start`] keeps the recording daemon, and
/// [`Announced::start_mdns`] is the only way to reach the network.
pub struct Mdns {
    daemon: mdns_sd::ServiceDaemon,
    /// `(instance, fullname)` of every registration, so `unregister` can name
    /// the record it withdraws.
    registered: std::sync::Mutex<Vec<(String, String)>>,
}

impl std::fmt::Debug for Mdns {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let registered = self.registered.lock().map(|known| known.len()).unwrap_or(0);
        f.debug_struct("Mdns")
            .field("registered", &registered)
            .finish()
    }
}

impl Mdns {
    /// Start the daemon (`ServiceDaemon::new`).
    pub fn new() -> Result<Mdns, String> {
        Ok(Mdns {
            daemon: mdns_sd::ServiceDaemon::new().map_err(|e| e.to_string())?,
            registered: std::sync::Mutex::new(Vec::new()),
        })
    }
}

impl Daemon for Mdns {
    fn register(&self, instance: &str, port: u16, txt: &[(&str, String)]) {
        let ip = crate::lan_address()
            .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
        let Ok(info) = service_info(instance, port, ip, txt) else {
            return;
        };
        let fullname = info.get_fullname().to_string();
        if let Ok(mut known) = self.registered.lock() {
            // Recorded before the registration so `Drop`'s unconditional
            // `unregister` is always safe.
            known.push((instance.to_string(), fullname));
        }
        let _ = self.daemon.register(info);
    }

    fn unregister(&self, instance: &str) {
        let names: Vec<String> = self
            .registered
            .lock()
            .map(|known| {
                known
                    .iter()
                    .filter(|(who, _)| who == instance)
                    .map(|(_, fullname)| fullname.clone())
                    .collect()
            })
            .unwrap_or_default();
        for fullname in names {
            let _ = self.daemon.unregister(&fullname);
        }
    }

    fn shutdown(&self) {
        let _ = self.daemon.shutdown();
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

    /// Announce the web page on the real mDNS responder (`mdns_sd`).
    ///
    /// This is the recovered path: `ServiceDaemon::new` (in [`Mdns::new`]) then
    /// `register` with the [`service_info`] record.  It opens multicast sockets,
    /// which is why it is not what [`Announced::start`] does by default.
    pub fn start_mdns(
        token: &str,
        port: u16,
        storage_version: u32,
        layout: &str,
    ) -> Result<Announced, String> {
        let daemon = Mdns::new()?;
        Ok(Announced::start_on(
            Box::new(daemon),
            token,
            port,
            storage_version,
            layout,
        ))
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

    /// The record `Announced::start_mdns` registers — built without a daemon, so
    /// no socket is opened.
    #[test]
    fn the_service_record_is_the_recovered_announcement() {
        let txt = [
            (TXT_TOKEN, "deadbeef".to_string()),
            (TXT_VERSION, "3".to_string()),
            (TXT_LAYOUT, "layout.json".to_string()),
        ];
        let ip: std::net::IpAddr = "192.168.1.10".parse().unwrap();
        let info = service_info("hydra", 7788, ip, &txt).unwrap();
        assert_eq!(info.get_fullname(), "hydra.v1_hydra._tcp.local.");
        assert_eq!(info.get_type(), SERVICE);
        assert_eq!(info.get_port(), 7788);
        assert_eq!(info.get_hostname(), "hydra.local.");
        assert_eq!(info.get_property_val_str(TXT_TOKEN), Some("deadbeef"));
        assert_eq!(info.get_property_val_str(TXT_VERSION), Some("3"));
        assert_eq!(info.get_property_val_str(TXT_LAYOUT), Some("layout.json"));
        assert!(info
            .get_addresses_v4()
            .iter()
            .any(|a| a.to_string() == "192.168.1.10"));
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
