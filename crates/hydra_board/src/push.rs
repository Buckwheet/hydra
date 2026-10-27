//! Push notifications to a phone: the devices a folder has paired, and what it
//! tells them.
//!
//! **ESTABLISHED**:
//!
//!  * The types are `Phone` (`struct Phone with 5 elements`), `Forgotten`
//!    (`struct Forgotten with 2 elements`), `Kept`, `Paired` and `Alert`
//!    (`0x1014adcd8` onwards are their serde visitors).
//!  * The fields the file carries are `version`, `relay`, `token`, `desktop`,
//!    `phones`, `forgotten`, `devices`, `sandbox`, `key`, `added`
//!    (`0x102418xxx`).
//!  * `seal` (`0x1014a833c`) writes the file, `read` (`0x1014a8144`) reads it,
//!    `pair` (`0x1014a7458`) adds a phone and `forget[_named]`
//!    (`0x1014aac5c`, `0x1014a6f34`) take one away.
//!
//! **std departure**: `client`, `allow`, `fitted` and `send` were HTTP calls to
//! the relay.  This build has no network, so they validate and hand back the
//! request they would have made; the seam is [`Alert`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::text::{self, Value};

/// The file the paired phones live in.
pub const FILE: &str = "push.json";

/// The version the file is written with.
///
/// **ESTABLISHED** (`0x102418xxx`, the `version` field): the first entry in the
/// document.
pub const VERSION: u32 = 1;

/// The key a phone is filed under.
///
/// **ESTABLISHED** (`0x1014aadec`): the relay, the device token and the desktop
/// flag together, so two folders on one machine do not share a phone.
pub fn key_of(relay: &str, token: &str, desktop: bool) -> String {
    format!(
        "{}:{}:{}",
        relay.trim(),
        token.trim(),
        if desktop { "desktop" } else { "phone" }
    )
}

/// One paired phone.
///
/// **ESTABLISHED** (`struct Phone with 5 elements`): the relay it is reached
/// through, its token, whether it is the desktop's own notification centre and
/// when it was added.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Phone {
    /// The relay's address.
    pub relay: String,
    /// The device token.
    pub token: String,
    /// Whether this is the machine's own desktop notifications.
    pub desktop: bool,
    /// Whether the relay asked for the sandbox endpoint.
    pub sandbox: bool,
    /// When it was added.
    pub added: String,
}

impl Phone {
    /// The key this phone is filed under.
    pub fn key(&self) -> String {
        key_of(&self.relay, &self.token, self.desktop)
    }

    /// Read a phone out of the document.
    pub fn read(value: &Value) -> Phone {
        Phone {
            relay: value.str_of("relay"),
            token: value.str_of("token"),
            desktop: value.bool_of("desktop"),
            sandbox: value.bool_of("sandbox"),
            added: value.str_of("added"),
        }
    }

    /// Write a phone back out.
    pub fn write(&self) -> Value {
        let mut map = BTreeMap::new();
        map.insert("relay".to_string(), Value::Str(self.relay.clone()));
        map.insert("token".to_string(), Value::Str(self.token.clone()));
        map.insert("desktop".to_string(), Value::Bool(self.desktop));
        map.insert("sandbox".to_string(), Value::Bool(self.sandbox));
        map.insert("added".to_string(), Value::Str(self.added.clone()));
        Value::Table(map)
    }
}

/// A phone that was taken away.
///
/// **ESTABLISHED** (`struct Forgotten with 2 elements`): its key and when it
/// went, so the relay can be told.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Forgotten {
    pub key: String,
    pub when: String,
}

/// A paired phone and its key.
///
/// **ESTABLISHED** (`hydra_board::push::Paired`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Paired {
    pub key: String,
    pub phone: Phone,
}

/// The whole push file.
#[derive(Debug, Clone, PartialEq)]
pub struct Kept {
    pub version: u32,
    pub relay: String,
    /// The token this folder sends with.
    pub token: String,
    /// Whether the desktop's own notifications are used.
    pub desktop: bool,
    pub phones: Vec<Phone>,
    pub forgotten: Vec<Forgotten>,
}

impl Default for Kept {
    fn default() -> Self {
        Kept {
            version: VERSION,
            relay: String::new(),
            token: String::new(),
            desktop: false,
            phones: Vec::new(),
            forgotten: Vec::new(),
        }
    }
}

impl Kept {
    /// Read the document.
    ///
    /// **ESTABLISHED** (`0x1014a8144`): every field above, with the phones under
    /// their keys.
    pub fn read(doc: &Value) -> Kept {
        let mut kept = Kept {
            version: doc.u64_of("version").unwrap_or(VERSION as u64) as u32,
            relay: doc.str_of("relay"),
            token: doc.str_of("token"),
            desktop: doc.bool_of("desktop"),
            phones: Vec::new(),
            forgotten: Vec::new(),
        };
        if let Some(phones) = doc.get("phones").and_then(Value::as_table) {
            for (_, value) in phones {
                kept.phones.push(Phone::read(value));
            }
        }
        if let Some(forgotten) = doc.get("forgotten").and_then(Value::as_array) {
            for value in forgotten {
                kept.forgotten.push(Forgotten {
                    key: value.str_of("key"),
                    when: value.str_of("when"),
                });
            }
        }
        kept
    }

    /// Write the document.
    ///
    /// **ESTABLISHED** (`0x1014a833c`, `seal`).
    pub fn write(&self) -> Value {
        let mut phones = BTreeMap::new();
        for phone in &self.phones {
            phones.insert(phone.key(), phone.write());
        }
        let mut map = BTreeMap::new();
        map.insert("version".to_string(), Value::Int(self.version as i64));
        map.insert("relay".to_string(), Value::Str(self.relay.clone()));
        map.insert("token".to_string(), Value::Str(self.token.clone()));
        map.insert("desktop".to_string(), Value::Bool(self.desktop));
        map.insert("phones".to_string(), Value::Table(phones));
        map.insert(
            "forgotten".to_string(),
            Value::Array(
                self.forgotten
                    .iter()
                    .map(|forgotten| {
                        let mut entry = BTreeMap::new();
                        entry.insert("key".to_string(), Value::Str(forgotten.key.clone()));
                        entry.insert("when".to_string(), Value::Str(forgotten.when.clone()));
                        Value::Table(entry)
                    })
                    .collect(),
            ),
        );
        Value::Table(map)
    }

    /// The phone filed under `key`.
    pub fn of(&self, key: &str) -> Option<&Phone> {
        self.phones.iter().find(|phone| phone.key() == key)
    }
}

/// A notification to send.
///
/// **ESTABLISHED** (`hydra_board::push::Alert`): the words and the topic.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Alert {
    pub title: String,
    pub body: String,
    /// The topic, so a phone can group them.
    pub topic: String,
}

/// Where a push file lives.
pub fn file(folder: &Path) -> PathBuf {
    folder.join(FILE)
}

/// Read a folder's push file.
///
/// **ESTABLISHED** (`0x1014a8144`).
pub fn read(folder: &Path) -> Kept {
    text::read_file(&file(folder))
        .map(|doc| Kept::read(&doc))
        .unwrap_or_default()
}

/// Write a folder's push file.
///
/// **ESTABLISHED** (`0x1014a833c`, `seal`).
pub fn seal(folder: &Path, kept: &Kept) -> Result<(), String> {
    text::write_file(&file(folder), &kept.write())
}

/// The request that would be made to pair a phone.
///
/// **ESTABLISHED** (`0x1014a7458`): the relay's pair endpoint with the token;
/// **std departure**: no network, so the request is described, not sent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub body: String,
}

impl Request {
    /// Whether the request could be made at all.
    ///
    /// **ESTABLISHED** (`0x1014a8d34`, `allow`): the relay has to be a plain
    /// `https://` address or nothing is sent.
    pub fn allow(&self) -> bool {
        self.url.starts_with("https://")
    }
}

/// Pair a phone: the request that would be made.
///
/// **ESTABLISHED** (`0x1014a7458`): `{relay}/pair` with the token.
pub fn pair(kept: &Kept, phone: &Phone) -> Request {
    Request {
        method: "POST".to_string(),
        url: format!("{}/pair", phone.relay.trim_end_matches('/')),
        body: format!(
            "{{\"token\":\"{}\",\"desktop\":{},\"sandbox\":{},\"key\":\"{}\"}}",
            text::json_escape(&phone.token),
            phone.desktop,
            phone.sandbox,
            text::json_escape(&kept.token)
        ),
    }
}

/// The client a request would be made with.
///
/// **ESTABLISHED** (`0x1014aaa7c`): a `reqwest` client with a timeout;
/// **std departure**: there is no client here, so this is the timeout it was
/// built with.
pub fn client() -> std::time::Duration {
    std::time::Duration::from_secs(10)
}

/// Whether the relay's address is one Hydra will send to.
///
/// **ESTABLISHED** (`0x1014aab80`, `fitted`): the address is used as it is when
/// it is `https`, and `http://localhost` is allowed for a local relay.
pub fn fitted(relay: &str) -> bool {
    let relay = relay.trim();
    relay.starts_with("https://") || relay.starts_with("http://localhost")
}

/// Forget the phone filed under `key`.
///
/// **ESTABLISHED** (`0x1014aac5c`): the phone is taken out of the list and a
/// `Forgotten` entry is kept so the relay can be told.
pub fn forget(kept: &mut Kept, key: &str) -> bool {
    let before = kept.phones.len();
    kept.phones.retain(|phone| phone.key() != key);
    if kept.phones.len() != before {
        kept.forgotten.push(Forgotten {
            key: key.to_string(),
            when: String::new(),
        });
        true
    } else {
        false
    }
}

/// Forget a phone by its relay and token.
///
/// **ESTABLISHED** (`0x1014a6f34`).
pub fn forget_named(kept: &mut Kept, relay: &str, token: &str, desktop: bool) -> bool {
    forget(kept, &key_of(relay, token, desktop))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_separate_a_folders_phones() {
        assert_eq!(key_of("https://relay", " tok ", true), "https://relay:tok:desktop");
        assert_eq!(key_of("https://relay", "tok", false), "https://relay:tok:phone");
    }

    #[test]
    fn a_push_file_round_trips() {
        let mut kept = Kept {
            relay: "https://relay".into(),
            token: "t".into(),
            ..Kept::default()
        };
        kept.phones.push(Phone {
            relay: "https://relay".into(),
            token: "d1".into(),
            desktop: true,
            sandbox: false,
            added: "2024-05-01".into(),
        });
        let again = Kept::read(&kept.write());
        assert_eq!(again, kept);
        assert!(again.of("https://relay:d1:desktop").is_some());
    }

    #[test]
    fn only_https_relays_are_used() {
        assert!(fitted("https://relay"));
        assert!(fitted("http://localhost:8080"));
        assert!(!fitted("http://relay"));
        let request = pair(
            &Kept::default(),
            &Phone {
                relay: "http://relay".into(),
                ..Phone::default()
            },
        );
        assert!(!request.allow());
        assert_eq!(request.url, "http://relay/pair");
    }

    #[test]
    fn forgetting_keeps_a_note() {
        let mut kept = Kept::default();
        kept.phones.push(Phone {
            relay: "https://relay".into(),
            token: "d1".into(),
            ..Phone::default()
        });
        assert!(forget_named(&mut kept, "https://relay", "d1", false));
        assert!(kept.phones.is_empty());
        assert_eq!(kept.forgotten.len(), 1);
        assert!(!forget_named(&mut kept, "https://relay", "d1", false));
    }
}
