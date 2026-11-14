//! An ActivityPub actor, reduced to the fields a reader can see.

use serde_json::{Map, Value, json};

use crate::Error;
use crate::context::{PUBLIC, activity_context};

/// An actor's avatar or header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// The image URL.
    pub url: String,
}

/// A custom emoji declared by an actor or a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emoji {
    /// `:shortcode:`.
    pub shortcode: String,
    /// Image URL.
    pub url: String,
}

impl Emoji {
    /// The ActivityStreams `Emoji` tag.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "type": "Emoji",
            "name": self.shortcode,
            "icon": { "type": "Image", "url": self.url },
        })
    }
}

/// The public profile behind an actor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    /// Canonical actor id.
    pub id: String,
    /// ActivityPub type, usually `Person`.
    pub kind: String,
    /// Local username.
    pub username: String,
    /// Display name.
    pub name: String,
    /// Profile HTML.
    pub summary: String,
    /// Profile page URL.
    pub url: String,
    /// Avatar.
    pub avatar: Option<String>,
    /// Header image.
    pub header: Option<String>,
    /// Followers collection URL.
    pub followers: Option<String>,
    /// Following collection URL.
    pub following: Option<String>,
    /// Outbox collection URL.
    pub outbox: Option<String>,
    /// Inbox collection URL. Servers publish this; readers usually ignore it.
    pub inbox: Option<String>,
    /// Shared inbox URL, when the actor advertises one.
    pub shared_inbox: Option<String>,
    /// Key id for the actor's RSA public key (`publicKey.id`).
    pub public_key_id: Option<String>,
    /// PEM-encoded RSA public key (`publicKey.publicKeyPem`).
    pub public_key_pem: Option<String>,
    /// RFC 3339 account creation time.
    pub published: Option<String>,
    /// Whether the account opts into discovery.
    pub discoverable: bool,
    /// Whether search engines may index the profile.
    pub indexable: bool,
    /// Whether new follows require approval.
    pub manually_approves_followers: bool,
    /// Properties this crate does not model (for example Misskey or Mastodon
    /// extensions), preserved for a lossless round trip.
    pub extra: Map<String, Value>,
}

const ACTOR_KEYS: &[&str] = &[
    "@context",
    "id",
    "type",
    "preferredUsername",
    "name",
    "summary",
    "url",
    "icon",
    "image",
    "followers",
    "following",
    "outbox",
    "inbox",
    "endpoints",
    "publicKey",
    "published",
    "discoverable",
    "indexable",
    "manuallyApprovesFollowers",
];

impl Actor {
    /// Read the fields this crate understands.
    ///
    /// # Errors
    /// [`Error::Object`] when the JSON is not an actor with an id.
    pub fn from_json(value: &Value) -> Result<Self, Error> {
        let id = string(value.get("id")).ok_or_else(|| Error::Object("actor has no id".into()))?;
        let username = string(value.get("preferredUsername"))
            .or_else(|| value.get("name").and_then(Value::as_str).map(str::to_owned))
            .unwrap_or_default();
        let url = link(value.get("url")).unwrap_or_else(|| id.clone());
        let avatar = value.get("icon").and_then(|value| link(Some(value)));
        let header = value.get("image").and_then(|value| link(Some(value)));
        Ok(Self {
            id,
            kind: string(value.get("type")).unwrap_or_else(|| "Person".to_owned()),
            username,
            name: string(value.get("name")).unwrap_or_default(),
            summary: string(value.get("summary")).unwrap_or_default(),
            url,
            avatar,
            header,
            followers: link(value.get("followers")),
            following: link(value.get("following")),
            outbox: link(value.get("outbox")),
            inbox: link(value.get("inbox")),
            shared_inbox: link(
                value
                    .get("endpoints")
                    .and_then(|endpoints| endpoints.get("sharedInbox")),
            ),
            public_key_id: value
                .get("publicKey")
                .and_then(|key| key.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            public_key_pem: value
                .get("publicKey")
                .and_then(|key| string(Some(key)))
                .or_else(|| {
                    value
                        .get("publicKey")
                        .and_then(|key| key.get("publicKeyPem"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                }),
            published: string(value.get("published")),
            discoverable: value
                .get("discoverable")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            indexable: value
                .get("indexable")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            manually_approves_followers: value
                .get("manuallyApprovesFollowers")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            extra: crate::extra_fields(value, ACTOR_KEYS),
        })
    }

    /// The actor document an instance serves at its actor URL.
    ///
    /// `None` fields are omitted rather than sent as `null`: an ActivityPub consumer
    /// treats a null collection as a collection with no members, which is not the same
    /// as one that was never advertised.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut value = json!({
            "@context": activity_context(),
            "id": self.id,
            "type": self.kind,
            "preferredUsername": self.username,
            "name": self.name,
            "summary": self.summary,
            "url": self.url,
            "discoverable": self.discoverable,
            "indexable": self.indexable,
            "manuallyApprovesFollowers": self.manually_approves_followers,
        });
        let map = value.as_object_mut().expect("actor document is an object");
        map.extend(
            self.extra
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        if let Some(avatar) = &self.avatar {
            map.insert("icon".to_owned(), json!({ "type": "Image", "url": avatar }));
        }
        if let Some(header) = &self.header {
            map.insert(
                "image".to_owned(),
                json!({ "type": "Image", "url": header }),
            );
        }
        if let Some(followers) = &self.followers {
            map.insert("followers".to_owned(), json!(followers));
        }
        if let Some(following) = &self.following {
            map.insert("following".to_owned(), json!(following));
        }
        if let Some(outbox) = &self.outbox {
            map.insert("outbox".to_owned(), json!(outbox));
        }
        if let Some(inbox) = &self.inbox {
            map.insert("inbox".to_owned(), json!(inbox));
        }
        if let Some(shared) = &self.shared_inbox {
            map.insert("endpoints".to_owned(), json!({ "sharedInbox": shared }));
        }
        if let Some(published) = &self.published {
            map.insert("published".to_owned(), json!(published));
        }
        if let Some(pem) = &self.public_key_pem {
            let key_id = self
                .public_key_id
                .clone()
                .unwrap_or_else(|| format!("{}#main-key", self.id));
            map.insert(
                "publicKey".to_owned(),
                json!({
                    "id": key_id,
                    "owner": self.id,
                    "publicKeyPem": pem,
                }),
            );
        }
        value
    }

    /// The actor's canonical public id, used as the default audience for a public post.
    #[must_use]
    pub fn public_id(&self) -> &'static str {
        PUBLIC
    }

    /// The `user@host` handle, from the actor URL.
    #[must_use]
    pub fn acct(&self, host: &str) -> String {
        format!("{}@{host}", self.username)
    }
}

pub(crate) fn string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.clone()),
        Value::Object(map) => map.get("href").and_then(Value::as_str).map(str::to_owned),
        _ => None,
    }
}

pub fn link(value: Option<&Value>) -> Option<String> {
    let value = value?;
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => items.first().and_then(|item| link(Some(item))),
        Value::Object(map) => map
            .get("url")
            .or_else(|| map.get("href"))
            .and_then(|inner| link(Some(inner)))
            .or_else(|| map.get("id").and_then(Value::as_str).map(str::to_owned)),
        _ => None,
    }
}

pub(crate) fn total_items(value: Option<&Value>) -> u64 {
    let Some(Value::Object(map)) = value else {
        return 0;
    };
    map.get("totalItems")
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

pub(crate) fn items(value: Option<&Value>) -> &[Value] {
    match value {
        Some(Value::Array(items)) => items.as_slice(),
        Some(item @ Value::Object(_)) => std::slice::from_ref(item),
        _ => &[],
    }
}

pub(crate) fn audience(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items.iter().filter_map(|item| link(Some(item))).collect(),
        Some(other) => link(Some(other)).into_iter().collect(),
        None => Vec::new(),
    }
}

pub(crate) fn parse_emoji(item: &Value) -> Option<Emoji> {
    if item.get("type").and_then(Value::as_str) != Some("Emoji") {
        return None;
    }
    let shortcode = item.get("name").and_then(Value::as_str)?.to_owned();
    let url = item.get("icon").and_then(|icon| link(Some(icon)))?;
    Some(Emoji { shortcode, url })
}

pub(crate) fn emojis(value: Option<&Value>) -> Vec<Emoji> {
    items(value).iter().filter_map(parse_emoji).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_public_key_id_is_preserved_on_round_trip() {
        let raw = json!({
            "id": "https://gts.example/users/alice",
            "type": "Person",
            "preferredUsername": "alice",
            "publicKey": {
                "id": "https://gts.example/users/alice/main-key",
                "owner": "https://gts.example/users/alice",
                "publicKeyPem": "-----BEGIN PUBLIC KEY-----\nTEST\n-----END PUBLIC KEY-----"
            }
        });
        let actor = Actor::from_json(&raw).expect("parse actor");
        assert_eq!(
            actor.public_key_id.as_deref(),
            Some("https://gts.example/users/alice/main-key")
        );
        let serialized = actor.to_json();
        assert_eq!(
            serialized["publicKey"]["id"],
            "https://gts.example/users/alice/main-key"
        );
        let round_tripped = Actor::from_json(&serialized).expect("round trip");
        assert_eq!(round_tripped, actor);
    }

    #[test]
    fn actor_defaults_public_key_id_when_absent() {
        let raw = json!({
            "id": "https://a.example/users/alice",
            "type": "Person",
            "preferredUsername": "alice",
            "publicKey": {
                "owner": "https://a.example/users/alice",
                "publicKeyPem": "-----BEGIN PUBLIC KEY-----\nTEST\n-----END PUBLIC KEY-----"
            }
        });
        let actor = Actor::from_json(&raw).expect("parse actor");
        assert_eq!(actor.public_key_id, None);
        assert_eq!(
            actor.to_json()["publicKey"]["id"],
            "https://a.example/users/alice#main-key"
        );
    }

    #[test]
    fn emojis_accepts_single_object_tag() {
        let single = json!({
            "type": "Emoji",
            "name": ":blobcat:",
            "icon": { "type": "Image", "url": "https://a.example/emoji/blobcat.png" }
        });
        let parsed = emojis(Some(&single));
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].shortcode, ":blobcat:");
        assert_eq!(parsed[0].url, "https://a.example/emoji/blobcat.png");
    }
}
