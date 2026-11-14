//! Activities an instance sends to an inbox and receives from one.
//!
//! This is the protocol vocabulary, not a delivery mechanism. A server builds an
//! `Activity`, signs the request with `nodus-federation`, and posts it to a peer's
//! inbox; the peer parses the same document with [`Activity::from_json`].

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::Error;
use crate::actor::string;
use crate::context::activity_context;

/// The activity verbs this crate can represent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivityKind {
    /// A new object was published.
    Create,
    /// Follow an actor.
    Follow,
    /// Accept a follow or another activity.
    Accept,
    /// Reject a follow or another activity.
    Reject,
    /// Undo a previous activity.
    Undo,
    /// Delete an object.
    Delete,
    /// Replace an object.
    Update,
    /// Share another object.
    Announce,
    /// Like an object.
    Like,
    /// Block an actor.
    Block,
    /// Report an actor or object.
    Flag,
    /// Move an account to a new actor id.
    Move,
    /// Add an object to a collection.
    Add,
    /// Remove an object from a collection.
    Remove,
}

impl ActivityKind {
    /// The ActivityStreams `type`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "Create",
            Self::Follow => "Follow",
            Self::Accept => "Accept",
            Self::Reject => "Reject",
            Self::Undo => "Undo",
            Self::Delete => "Delete",
            Self::Update => "Update",
            Self::Announce => "Announce",
            Self::Like => "Like",
            Self::Block => "Block",
            Self::Flag => "Flag",
            Self::Move => "Move",
            Self::Add => "Add",
            Self::Remove => "Remove",
        }
    }

    /// Parses a verb, returning `None` for an activity this crate does not model.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "Create" => Self::Create,
            "Follow" => Self::Follow,
            "Accept" => Self::Accept,
            "Reject" => Self::Reject,
            "Undo" => Self::Undo,
            "Delete" => Self::Delete,
            "Update" => Self::Update,
            "Announce" => Self::Announce,
            "Like" => Self::Like,
            "Block" => Self::Block,
            "Flag" => Self::Flag,
            "Move" => Self::Move,
            "Add" => Self::Add,
            "Remove" => Self::Remove,
            _ => return None,
        })
    }
}

/// What an activity acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityObject {
    /// A bare id, as sent by Follow, Like, Announce and Delete.
    Id(String),
    /// An embedded object, as sent by Create and Update.
    Value(Value),
    /// An embedded activity, as sent by Undo and Accept.
    Activity(Box<Activity>),
}

impl ActivityObject {
    /// The JSON form: a string for an id, an object for an embedded document.
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Self::Id(id) => json!(id),
            Self::Value(value) => value.clone(),
            Self::Activity(activity) => activity.to_json(),
        }
    }

    /// Parses an activity object from JSON: either a string URI, an embedded activity,
    /// or an embedded object (such as a Note or Tombstone).
    ///
    /// # Errors
    /// [`Error::Object`] when the value is not a string or object.
    pub fn from_json(value: &Value) -> Result<Self, Error> {
        match value {
            Value::String(id) => Ok(Self::Id(id.clone())),
            Value::Object(_) => {
                if value
                    .get("type")
                    .and_then(Value::as_str)
                    .and_then(ActivityKind::parse)
                    .is_some()
                    && value.get("actor").is_some()
                {
                    if let Ok(activity) = Activity::from_json(value) {
                        return Ok(Self::Activity(Box::new(activity)));
                    }
                }
                Ok(Self::Value(value.clone()))
            }
            _ => Err(Error::Object(
                "activity object is not an id or object".into(),
            )),
        }
    }
}

impl From<String> for ActivityObject {
    fn from(value: String) -> Self {
        Self::Id(value)
    }
}

impl From<&str> for ActivityObject {
    fn from(value: &str) -> Self {
        Self::Id(value.to_owned())
    }
}

impl From<Value> for ActivityObject {
    fn from(value: Value) -> Self {
        Self::Value(value)
    }
}

impl From<Activity> for ActivityObject {
    fn from(value: Activity) -> Self {
        Self::Activity(Box::new(value))
    }
}

impl From<Box<Activity>> for ActivityObject {
    fn from(value: Box<Activity>) -> Self {
        Self::Activity(value)
    }
}

impl Serialize for ActivityObject {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_json().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ActivityObject {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Self::from_json(&value).map_err(serde::de::Error::custom)
    }
}

/// One ActivityPub activity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    /// Activity id. Most activities have one; a bare Follow from some servers does not.
    pub id: Option<String>,
    /// The verb.
    pub kind: ActivityKind,
    /// Actor performing the activity.
    pub actor: String,
    /// Object or embedded activity.
    pub object: ActivityObject,
    /// Primary audience.
    pub to: Vec<String>,
    /// Secondary audience.
    pub cc: Vec<String>,
    /// Properties this crate does not model, preserved for a lossless round trip.
    pub extra: Map<String, Value>,
}

const ACTIVITY_KEYS: &[&str] = &["@context", "id", "type", "actor", "object", "to", "cc"];

impl Activity {
    /// Builds an activity without an id; [`Self::with_id`] adds one.
    #[must_use]
    pub fn new(
        kind: ActivityKind,
        actor: impl Into<String>,
        object: impl Into<ActivityObject>,
    ) -> Self {
        Self {
            id: None,
            kind,
            actor: actor.into(),
            object: object.into(),
            to: Vec::new(),
            cc: Vec::new(),
            extra: Map::new(),
        }
    }

    /// Adds the activity id an outbox entry needs.
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Sets `to` and `cc`.
    #[must_use]
    pub fn addressed_to(mut self, to: Vec<String>, cc: Vec<String>) -> Self {
        self.to = to;
        self.cc = cc;
        self
    }

    /// A `Create` for a local note.
    #[must_use]
    pub fn create(actor: impl Into<String>, note: &crate::note::Note) -> Self {
        Self::new(ActivityKind::Create, actor, note.to_json())
            .addressed_to(note.to.clone(), note.cc.clone())
            .with_id(format!("{}/activity", note.id.trim_end_matches('/')))
    }

    /// Builds an `Accept` whose object is the original `Follow`.
    #[must_use]
    pub fn accept(actor: impl Into<String>, follow: &Self) -> Self {
        Self::new(
            ActivityKind::Accept,
            actor,
            ActivityObject::Activity(Box::new(follow.clone())),
        )
    }

    /// The actor that performed the activity.
    #[must_use]
    pub fn actor(&self) -> &str {
        &self.actor
    }

    /// Parses an inbox/outbox document.
    ///
    /// # Errors
    /// [`Error::Object`] when the type is missing or unknown, or the actor/object is
    /// absent. An unknown verb is rejected rather than silently dropped: a server that
    /// accepts it cannot later decide it was not allowed.
    pub fn from_json(value: &Value) -> Result<Self, Error> {
        let raw = string(value.get("type"))
            .ok_or_else(|| Error::Object("activity has no type".into()))?;
        let kind = ActivityKind::parse(&raw)
            .ok_or_else(|| Error::Object(format!("unsupported activity type: {raw}")))?;
        let actor = string(value.get("actor"))
            .ok_or_else(|| Error::Object("activity has no actor".into()))?;
        let object_value = value
            .get("object")
            .ok_or_else(|| Error::Object("activity has no object".into()))?;
        let object = ActivityObject::from_json(object_value)?;
        Ok(Self {
            id: string(value.get("id")),
            kind,
            actor,
            object,
            to: audience(value.get("to")),
            cc: audience(value.get("cc")),
            extra: crate::extra_fields(value, ACTIVITY_KEYS),
        })
    }

    /// The outbox document. `@context` is included because an outbox page is consumed
    /// outside the context of the page that contains it.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut value = json!({
            "@context": activity_context(),
            "type": self.kind.as_str(),
            "actor": self.actor,
            "object": self.object.to_json(),
        });
        let map = value.as_object_mut().expect("activity is an object");
        map.extend(
            self.extra
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        if let Some(id) = &self.id {
            map.insert("id".to_owned(), json!(id));
        }
        if !self.to.is_empty() {
            map.insert("to".to_owned(), json!(self.to));
        }
        if !self.cc.is_empty() {
            map.insert("cc".to_owned(), json!(self.cc));
        }
        value
    }
}

impl Serialize for Activity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_json().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Activity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Self::from_json(&value).map_err(serde::de::Error::custom)
    }
}

fn audience(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(id)) => vec![id.clone()],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::PUBLIC;

    #[test]
    fn follow_serializes_with_its_type_tag() {
        let value = Activity::new(
            ActivityKind::Follow,
            "https://a.example/users/a",
            "https://b.example/users/b",
        )
        .to_json();
        assert_eq!(value["type"], "Follow");
        assert_eq!(value["actor"], "https://a.example/users/a");
        assert_eq!(value["object"], "https://b.example/users/b");
        assert!(value["@context"].is_array());
    }

    #[test]
    fn a_create_round_trips_through_json() {
        let note = crate::note::Note {
            id: "https://a.example/statuses/1".into(),
            url: "https://a.example/@a/1".into(),
            content: "<p>hello</p>".into(),
            published: "2026-01-01T00:00:00Z".into(),
            sensitive: false,
            summary: String::new(),
            attachments: Vec::new(),
            emojis: Vec::new(),
            in_reply_to: None,
            replies: None,
            replies_count: 0,
            attributed_to: Some("https://a.example/users/a".into()),
            to: vec![PUBLIC.into()],
            cc: Vec::new(),
            mentions: Vec::new(),
            extra: Map::new(),
        };
        let activity = Activity::create("https://a.example/users/a", &note);
        let parsed = Activity::from_json(&activity.to_json()).expect("round trip");
        assert_eq!(parsed.kind, ActivityKind::Create);
        assert_eq!(parsed.actor, "https://a.example/users/a");
        assert_eq!(parsed.to, vec![PUBLIC]);
    }

    #[test]
    fn an_unknown_verb_is_rejected() {
        let value = json!({
            "type": "Question",
            "actor": "https://a.example/users/a",
            "object": "https://b.example/statuses/1"
        });
        assert!(Activity::from_json(&value).is_err());
    }

    #[test]
    fn an_embedded_follow_inside_accept_is_parsed() {
        let value = json!({
            "type": "Accept",
            "actor": "https://b.example/users/b",
            "object": {
                "type": "Follow",
                "actor": "https://a.example/users/a",
                "object": "https://b.example/users/b"
            }
        });
        let parsed = Activity::from_json(&value).expect("accept");
        assert!(matches!(parsed.object, ActivityObject::Activity(_)));
    }

    #[test]
    fn every_kind_round_trips_its_name() {
        for kind in [
            ActivityKind::Create,
            ActivityKind::Follow,
            ActivityKind::Accept,
            ActivityKind::Reject,
            ActivityKind::Undo,
            ActivityKind::Delete,
            ActivityKind::Update,
            ActivityKind::Announce,
            ActivityKind::Like,
            ActivityKind::Block,
            ActivityKind::Flag,
            ActivityKind::Move,
            ActivityKind::Add,
            ActivityKind::Remove,
        ] {
            assert_eq!(ActivityKind::parse(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn delete_with_embedded_tombstone_parses_and_round_trips() {
        let tombstone = json!({
            "id": "https://mastodon.social/users/alice/statuses/123",
            "type": "Tombstone",
            "atomUri": "https://mastodon.social/users/alice/statuses/123"
        });
        let raw = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": "https://mastodon.social/users/alice/statuses/123/activity",
            "type": "Delete",
            "actor": "https://mastodon.social/users/alice",
            "to": [PUBLIC],
            "object": tombstone
        });
        let activity = Activity::from_json(&raw).expect("parse delete");
        assert_eq!(activity.kind, ActivityKind::Delete);
        assert_eq!(activity.actor, "https://mastodon.social/users/alice");
        assert_eq!(
            activity.id.as_deref(),
            Some("https://mastodon.social/users/alice/statuses/123/activity")
        );
        assert_eq!(activity.to, vec![PUBLIC]);
        assert_eq!(activity.object, ActivityObject::Value(tombstone));

        let serialized: Value = serde_json::to_value(&activity).expect("serialize");
        let deserialized: Activity = serde_json::from_value(serialized).expect("deserialize");
        assert_eq!(deserialized, activity);
    }

    #[test]
    fn delete_with_bare_string_id_parses_and_round_trips() {
        let raw = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "type": "Delete",
            "actor": "https://mastodon.social/users/alice",
            "object": "https://mastodon.social/users/alice/statuses/123"
        });
        let activity: Activity = serde_json::from_value(raw).expect("parse delete");
        assert_eq!(activity.kind, ActivityKind::Delete);
        assert_eq!(
            activity.object,
            ActivityObject::Id("https://mastodon.social/users/alice/statuses/123".into())
        );
        let serialized = serde_json::to_value(&activity).expect("serialize");
        let deserialized: Activity = serde_json::from_value(serialized).expect("deserialize");
        assert_eq!(deserialized, activity);
    }

    #[test]
    fn an_embedded_like_inside_undo_is_parsed() {
        let value = json!({
            "type": "Undo",
            "actor": "https://a.example/users/a",
            "object": {
                "type": "Like",
                "actor": "https://a.example/users/a",
                "object": "https://b.example/notes/1"
            }
        });
        let parsed = Activity::from_json(&value).expect("undo");
        assert!(matches!(parsed.object, ActivityObject::Activity(_)));
    }
}
