//! Misskey's ActivityPub dialect.
//!
//! Misskey federation is ActivityPub, but notes carry properties the standard
//! model does not define. The core crate preserves them in [`Note::extra`]; this
//! module gives those fields names and keeps them out of the protocol core.

use std::collections::BTreeMap;

use nodus_activitypub_core::Note;
use serde_json::Value;

/// Misskey-specific properties on an ActivityPub note.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MisskeyExtensions {
    /// `_misskey_content`: plain-text source alongside the HTML `content`.
    pub content: Option<String>,
    /// `question`: Misskey's poll object.
    pub question: Option<Value>,
    /// `renote`: the note this one renotes.
    pub renote: Option<Value>,
    /// `reactions`: reaction counts keyed by shortcode or Unicode emoji.
    pub reactions: BTreeMap<String, u64>,
}

impl MisskeyExtensions {
    /// Reads the Misskey fields preserved on a parsed note.
    #[must_use]
    pub fn from_note(note: &Note) -> Self {
        Self {
            content: note
                .extra
                .get("_misskey_content")
                .and_then(Value::as_str)
                .map(str::to_owned),
            question: note.extra.get("question").cloned(),
            renote: note.extra.get("renote").cloned(),
            reactions: reactions(note.extra.get("reactions")),
        }
    }

    /// Whether the note is a pure renote rather than a quote.
    #[must_use]
    pub fn is_renote(&self, note: &Note) -> bool {
        self.renote.is_some() && note.content.trim().is_empty()
    }

    /// Writes the fields back onto an outbound note document.
    pub fn apply(&self, note: &mut Value) {
        let Some(map) = note.as_object_mut() else {
            return;
        };
        if let Some(content) = &self.content {
            map.insert(
                "_misskey_content".to_owned(),
                Value::String(content.clone()),
            );
        }
        if let Some(question) = &self.question {
            map.insert("question".to_owned(), question.clone());
        }
        if let Some(renote) = &self.renote {
            map.insert("renote".to_owned(), renote.clone());
        }
        if !self.reactions.is_empty() {
            map.insert("reactions".to_owned(), serde_json::json!(self.reactions));
        }
    }
}

fn reactions(value: Option<&Value>) -> BTreeMap<String, u64> {
    let Some(map) = value.and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    map.iter()
        .filter_map(|(key, value)| Some((key.clone(), value.as_u64()?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn unknown_note_fields_survive_in_the_core_model() {
        let note = Note::from_json(&json!({
            "id": "https://misskey.example/notes/1",
            "type": "Note",
            "content": "<p>hello</p>",
            "_misskey_content": "hello",
            "question": { "choices": ["a", "b"] },
            "renote": "https://misskey.example/notes/0",
            "reactions": { "👍": 3, ":blobcat:": 1, "bad": "ignored" }
        }))
        .expect("note");
        let extensions = MisskeyExtensions::from_note(&note);
        assert_eq!(extensions.content.as_deref(), Some("hello"));
        assert_eq!(extensions.reactions.get("👍"), Some(&3));
        assert_eq!(extensions.reactions.len(), 2);
        assert!(extensions.renote.is_some());
    }

    #[test]
    fn applying_extensions_does_not_replace_standard_fields() {
        let mut value = json!({ "id": "https://misskey.example/notes/1", "content": "safe" });
        MisskeyExtensions {
            content: Some("source".into()),
            ..MisskeyExtensions::default()
        }
        .apply(&mut value);
        assert_eq!(value["content"], "safe");
        assert_eq!(value["_misskey_content"], "source");
    }
}
