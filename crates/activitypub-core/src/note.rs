//! A public note, reduced to what a card draws.

use serde_json::{Map, Value, json};

use crate::Error;
use crate::actor::{Emoji, audience, emojis, items, link, parse_emoji, string, total_items};
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
        let tag = object.get("tag");
        let mut extra = crate::extra_fields(object, NOTE_KEYS);
        let extra_tags = unmodeled_tags(tag);
        if !extra_tags.is_empty() {
            extra.insert("tag".to_owned(), Value::Array(extra_tags));
        }
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
            emojis: emojis(tag),
            in_reply_to: link(object.get("inReplyTo")),
            replies: link(object.get("replies")),
            replies_count: total_items(object.get("replies")),
            attributed_to: link(object.get("attributedTo")),
            to: audience(object.get("to")),
            cc: audience(object.get("cc")),
            mentions: mentions(tag),
            extra,
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
                .filter(|(key, _)| key.as_str() != "tag")
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
        match (&self.replies, self.replies_count) {
            (Some(replies), 0) => {
                map.insert("replies".to_owned(), json!(replies));
            }
            (Some(replies), count) => {
                map.insert(
                    "replies".to_owned(),
                    json!({
                        "id": replies,
                        "type": "Collection",
                        "totalItems": count,
                    }),
                );
            }
            (None, 0) => {}
            (None, count) => {
                map.insert(
                    "replies".to_owned(),
                    json!({
                        "type": "Collection",
                        "totalItems": count,
                    }),
                );
            }
        }
        if !self.attachments.is_empty() {
            map.insert(
                "attachment".to_owned(),
                Value::Array(self.attachments.iter().map(Attachment::to_json).collect()),
            );
        }
        let extra_tags = items(self.extra.get("tag"));
        let mut tags =
            Vec::with_capacity(self.emojis.len() + self.mentions.len() + extra_tags.len());
        for emoji in &self.emojis {
            tags.push(emoji.to_json());
        }
        for mention in &self.mentions {
            tags.push(json!({ "type": "Mention", "href": mention }));
        }
        tags.extend(extra_tags.iter().cloned());
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
    items(value)
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

fn parse_mention(item: &Value) -> Option<String> {
    if item.get("type").and_then(Value::as_str) != Some("Mention") {
        return None;
    }
    link(item.get("href"))
}

fn mentions(value: Option<&Value>) -> Vec<String> {
    items(value).iter().filter_map(parse_mention).collect()
}

fn unmodeled_tags(value: Option<&Value>) -> Vec<Value> {
    items(value)
        .iter()
        .filter(|item| parse_emoji(item).is_none() && parse_mention(item).is_none())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::PUBLIC;

    #[test]
    fn replies_count_survives_json_round_trip() {
        let raw = json!({
            "id": "https://a.example/notes/1",
            "type": "Note",
            "content": "<p>hello</p>",
            "replies": {
                "id": "https://a.example/notes/1/replies",
                "type": "Collection",
                "totalItems": 7
            }
        });
        let note = Note::from_json(&raw).expect("parse note");
        assert_eq!(
            note.replies.as_deref(),
            Some("https://a.example/notes/1/replies")
        );
        assert_eq!(note.replies_count, 7);

        let round_tripped = Note::from_json(&note.to_json()).expect("round trip");
        assert_eq!(round_tripped.replies, note.replies);
        assert_eq!(round_tripped.replies_count, 7);

        let count_only = json!({
            "id": "https://a.example/notes/1b",
            "type": "Note",
            "content": "<p>hello</p>",
            "replies": {
                "type": "Collection",
                "totalItems": 3
            }
        });
        let count_only_note = Note::from_json(&count_only).expect("parse count-only replies");
        assert_eq!(count_only_note.replies, None);
        assert_eq!(count_only_note.replies_count, 3);
        let round_tripped_count_only =
            Note::from_json(&count_only_note.to_json()).expect("round trip count-only");
        assert_eq!(round_tripped_count_only, count_only_note);

        let zero_replies = json!({
            "id": "https://a.example/notes/1c",
            "type": "Note",
            "content": "<p>hello</p>",
            "replies": "https://a.example/notes/1c/replies"
        });
        let zero_note = Note::from_json(&zero_replies).expect("parse string replies");
        assert_eq!(
            zero_note.to_json()["replies"],
            "https://a.example/notes/1c/replies"
        );
    }

    #[test]
    fn hashtag_tags_survive_round_trip_alongside_emojis_and_mentions() {
        let raw = json!({
            "id": "https://a.example/notes/2",
            "type": "Note",
            "content": "<p>hello #rust</p>",
            "tag": [
                {
                    "type": "Emoji",
                    "name": ":ferris:",
                    "icon": { "type": "Image", "url": "https://a.example/emoji/ferris.png" }
                },
                {
                    "type": "Mention",
                    "href": "https://b.example/users/bob"
                },
                {
                    "type": "Hashtag",
                    "href": "https://a.example/tags/rust",
                    "name": "#rust"
                }
            ]
        });
        let note = Note::from_json(&raw).expect("parse note");
        assert_eq!(note.emojis.len(), 1);
        assert_eq!(note.mentions, vec!["https://b.example/users/bob"]);

        let serialized = note.to_json();
        let round_tripped = Note::from_json(&serialized).expect("round trip");
        assert_eq!(round_tripped, note);

        let tags = serialized["tag"].as_array().expect("tag array");
        assert!(tags.iter().any(|t| {
            t.get("type").and_then(Value::as_str) == Some("Hashtag")
                && t.get("name").and_then(Value::as_str) == Some("#rust")
        }));
    }

    #[test]
    fn note_accepts_single_object_attachment_tags_and_link_audience() {
        let raw = json!({
            "id": "https://a.example/notes/3",
            "type": "Note",
            "content": "<p>media</p>",
            "attachment": {
                "type": "Image",
                "url": "https://a.example/media/1.png",
                "name": "alt"
            },
            "tag": {
                "type": "Mention",
                "href": "https://b.example/users/bob"
            },
            "to": [
                { "type": "Link", "href": PUBLIC }
            ],
            "cc": { "id": "https://a.example/users/alice/followers" }
        });
        let note = Note::from_json(&raw).expect("parse note");
        assert_eq!(note.attachments.len(), 1);
        assert_eq!(note.attachments[0].url, "https://a.example/media/1.png");
        assert_eq!(note.mentions, vec!["https://b.example/users/bob"]);
        assert_eq!(note.to, vec![PUBLIC]);
        assert_eq!(note.cc, vec!["https://a.example/users/alice/followers"]);

        let emoji_single = json!({
            "id": "https://a.example/notes/4",
            "type": "Note",
            "content": "<p>:ferris:</p>",
            "tag": {
                "type": "Emoji",
                "name": ":ferris:",
                "icon": { "type": "Image", "url": "https://a.example/emoji/ferris.png" }
            }
        });
        let note_emoji = Note::from_json(&emoji_single).expect("parse emoji note");
        assert_eq!(note_emoji.emojis.len(), 1);
        assert_eq!(note_emoji.emojis[0].shortcode, ":ferris:");

        let hashtag_single = json!({
            "id": "https://a.example/notes/5",
            "type": "Note",
            "content": "<p>#rust</p>",
            "tag": {
                "type": "Hashtag",
                "href": "https://a.example/tags/rust",
                "name": "#rust"
            }
        });
        let note_hashtag = Note::from_json(&hashtag_single).expect("parse hashtag note");
        let serialized_hashtag = note_hashtag.to_json();
        assert_eq!(serialized_hashtag["tag"][0]["type"], "Hashtag");
        assert_eq!(
            Note::from_json(&serialized_hashtag).expect("round trip hashtag"),
            note_hashtag
        );
    }
}
