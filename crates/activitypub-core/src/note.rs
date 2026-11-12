//! A public note, reduced to what a card draws.

use serde_json::{Map, Value, json};

use crate::Error;
use crate::actor::{Emoji, emojis, link, string, total_items};
use crate::context::activity_context;

/// A note attachment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    /// Object type, e.g. `Image`, `Video`, `Document`.
    pub kind: String,
    /// Full media URL.
    pub url: String,
    /// Preview image, when the origin gives one.
    pub preview_url: Option<String>,
    /// Alt text.
    pub description: Option<String>,
    /// Pixel width, when known.
    pub width: Option<u32>,
    /// Pixel height, when known.
    pub height: Option<u32>,
}

impl Attachment {
    /// The ActivityStreams document for this attachment.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut value = json!({
            "type": self.kind,
            "url": self.url,
        });
        let map = value.as_object_mut().expect("attachment is an object");
        if let Some(preview) = &self.preview_url {
            map.insert(
                "icon".to_owned(),
                json!({ "type": "Image", "url": preview }),
            );
        }
        if let Some(description) = &self.description {
            map.insert("name".to_owned(), json!(description));
        }
        if let Some(width) = self.width {
            map.insert("width".to_owned(), json!(width));
        }
        if let Some(height) = self.height {
            map.insert("height".to_owned(), json!(height));
        }
        value
    }
}

/// A public note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// Canonical object id.
    pub id: String,
    /// Human page URL.
    pub url: String,
    /// HTML body.
    pub content: String,
    /// RFC 3339 publication time.
    pub published: String,
    /// Whether the body or media needs a click.
    pub sensitive: bool,
    /// Content warning / subject.
    pub summary: String,
    /// Media, in display order.
    pub attachments: Vec<Attachment>,
    /// Custom emoji in the body.
    pub emojis: Vec<Emoji>,
    /// Parent note id, when this is a reply.
    pub in_reply_to: Option<String>,
    /// Replies collection URL.
    pub replies: Option<String>,
    /// Reply count when the origin publishes it.
    pub replies_count: u64,
    /// Author actor id.
    pub attributed_to: Option<String>,
    /// Primary audience.
    pub to: Vec<String>,
    /// Secondary audience (usually the author's followers collection).
    pub cc: Vec<String>,
    /// Mentioned actor ids, rendered as `Mention` tags.
    pub mentions: Vec<String>,
    /// Properties this crate does not model, preserved for a lossless round trip.
    pub extra: Map<String, Value>,
}

const NOTE_KEYS: &[&str] = &[
    "@context",
    "id",
    "type",
    "url",
    "content",
    "published",
    "sensitive",
    "summary",
    "attachment",
    "tag",
    "inReplyTo",
    "replies",
    "attributedTo",
    "to",
    "cc",
];

impl Note {
    /// Parse a `Note`, or the `object` of a `Create`.
    ///
    /// # Errors
    /// [`Error::Object`] when there is no id.
    pub fn from_json(value: &Value) -> Result<Self, Error> {
        let object = if value.get("type").and_then(Value::as_str) == Some("Create") {
            value
                .get("object")
                .ok_or_else(|| Error::Object("Create has no object".into()))?
        } else {
            value
        };
        let id = string(object.get("id")).ok_or_else(|| Error::Object("note has no id".into()))?;
        let url = link(object.get("url")).unwrap_or_else(|| id.clone());
        Ok(Self {
            id,
            url,
            content: string(object.get("content")).unwrap_or_default(),
            published: string(object.get("published")).unwrap_or_default(),
            sensitive: object
                .get("sensitive")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            summary: string(object.get("summary")).unwrap_or_default(),
            attachments: attachments(object.get("attachment")),
            emojis: emojis(object.get("tag")),
            in_reply_to: link(object.get("inReplyTo")),
            replies: link(object.get("replies")),
            replies_count: total_items(object.get("replies")),
            attributed_to: link(object.get("attributedTo")),
            to: audience(object.get("to")),
            cc: audience(object.get("cc")),
            mentions: mentions(object.get("tag")),
            extra: crate::extra_fields(object, NOTE_KEYS),
        })
    }

    /// The `Note` object an instance serves at its canonical URL.
    ///
    /// Empty optional fields are omitted, not sent as `""`: Mastodon renders an empty
    /// `summary` as a content warning with no text and hides the body behind it.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut value = json!({
            "@context": activity_context(),
            "id": self.id,
            "type": "Note",
            "url": self.url,
            "content": self.content,
            "published": self.published,
            "sensitive": self.sensitive,
            "to": self.to,
            "cc": self.cc,
        });
        let map = value.as_object_mut().expect("note is an object");
        map.extend(
            self.extra
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        if let Some(author) = &self.attributed_to {
            map.insert("attributedTo".to_owned(), json!(author));
        }
        if !self.summary.is_empty() {
            map.insert("summary".to_owned(), json!(self.summary));
        }
        if let Some(parent) = &self.in_reply_to {
            map.insert("inReplyTo".to_owned(), json!(parent));
        }
        if let Some(replies) = &self.replies {
            map.insert("replies".to_owned(), json!(replies));
        }
        if !self.attachments.is_empty() {
            map.insert(
                "attachment".to_owned(),
                Value::Array(self.attachments.iter().map(Attachment::to_json).collect()),
            );
        }
        let mut tags = Vec::with_capacity(self.emojis.len() + self.mentions.len());
        for emoji in &self.emojis {
            tags.push(emoji.to_json());
        }
        for mention in &self.mentions {
            tags.push(json!({ "type": "Mention", "href": mention }));
        }
        if !tags.is_empty() {
            map.insert("tag".to_owned(), Value::Array(tags));
        }
        value
    }

    /// The content warning, if any.
    #[must_use]
    pub fn warning(&self) -> Option<&str> {
        (!self.summary.is_empty()).then_some(self.summary.as_str())
    }
}

fn attachments(value: Option<&Value>) -> Vec<Attachment> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let url = link(item.get("url"))?;
            Some(Attachment {
                kind: string(item.get("type")).unwrap_or_else(|| "Document".to_owned()),
                url,
                preview_url: item.get("icon").and_then(|icon| link(Some(icon))),
                description: string(item.get("name")),
                width: item
                    .get("width")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok()),
                height: item
                    .get("height")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok()),
            })
        })
        .collect()
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

fn mentions(value: Option<&Value>) -> Vec<String> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("Mention"))
        .filter_map(|item| link(item.get("href")))
        .collect()
}
