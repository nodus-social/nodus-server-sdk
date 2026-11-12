//! Misskey client API wire shapes.
//!
//! Misskey clients call JSON-RPC-style endpoints under `/api/*`, usually with a
//! token in the `i` body field. These types model the payloads, not the routing
//! or the token store. They are deliberately independent of the Mastodon API:
//! field names, visibility names, IDs and pagination differ.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Misskey's note visibility vocabulary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteVisibility {
    /// Public; appears on local and global timelines.
    #[default]
    Public,
    /// Home; appears to the author's followers.
    Home,
    /// Followers only.
    Followers,
    /// Only the users named in the note.
    Specified,
}

/// Instance metadata returned by `POST /api/meta`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    /// Instance display name.
    pub name: String,
    /// Misskey-compatible software version.
    pub version: String,
    /// Canonical instance URL.
    pub uri: String,
    /// Short description.
    #[serde(default)]
    pub description: Option<String>,
    /// Whether registration is closed.
    #[serde(default)]
    pub disable_registration: bool,
    /// Maximum note body length.
    #[serde(default = "default_max_note_text_length")]
    pub max_note_text_length: u32,
}

const fn default_max_note_text_length() -> u32 {
    3000
}

/// A Misskey user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    /// Misskey user id (`aid`).
    pub id: String,
    /// Local username.
    pub username: String,
    /// Display name.
    #[serde(default)]
    pub name: Option<String>,
    /// Remote host; absent for local users.
    #[serde(default)]
    pub host: Option<String>,
    /// Profile description.
    #[serde(default)]
    pub description: Option<String>,
    /// Avatar URL.
    #[serde(default)]
    pub avatar_url: Option<String>,
    /// Banner URL.
    #[serde(default)]
    pub banner_url: Option<String>,
    /// Whether the account is marked as a bot.
    #[serde(default)]
    pub is_bot: bool,
    /// Whether the account is marked as a cat.
    #[serde(default)]
    pub is_cat: bool,
    /// Whether the account is an administrator.
    #[serde(default)]
    pub is_admin: bool,
    /// Whether the account is a moderator.
    #[serde(default)]
    pub is_moderator: bool,
    /// Whether new follows require approval.
    #[serde(default)]
    pub is_locked: bool,
    /// Whether the account is suspended.
    #[serde(default)]
    pub is_suspended: bool,
    /// Followers count.
    #[serde(default)]
    pub followers_count: u64,
    /// Following count.
    #[serde(default)]
    pub following_count: u64,
    /// Note count.
    #[serde(default)]
    pub notes_count: u64,
    /// Creation time.
    #[serde(default)]
    pub created_at: Option<String>,
    /// Canonical ActivityPub actor URL.
    #[serde(default)]
    pub uri: Option<String>,
    /// Destination after an account move.
    #[serde(default)]
    pub moved_to_uri: Option<String>,
    /// Aliases declared for an account move.
    #[serde(default)]
    pub also_known_as: Vec<String>,
}

/// A file attached to a note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    /// Misskey file id.
    pub id: String,
    /// Original filename.
    #[serde(default)]
    pub name: Option<String>,
    /// Object type, such as `image/png`.
    #[serde(default)]
    pub kind: Option<String>,
    /// MIME type.
    #[serde(default)]
    pub mime_type: Option<String>,
    /// Download URL.
    pub url: String,
    /// Thumbnail URL.
    #[serde(default)]
    pub thumbnail_url: Option<String>,
    /// Size in bytes.
    #[serde(default)]
    pub size: u64,
    /// Whether the media needs a click.
    #[serde(default)]
    pub is_sensitive: bool,
    /// BlurHash preview.
    #[serde(default)]
    pub blurhash: Option<String>,
}

/// One choice in a Misskey poll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PollChoice {
    /// Choice text.
    pub text: String,
    /// Vote count.
    #[serde(default)]
    pub votes: u64,
    /// Whether the authenticated viewer voted for this choice.
    #[serde(default)]
    pub is_voted: bool,
}

/// A Misskey poll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Poll {
    /// Choices, in display order.
    pub choices: Vec<PollChoice>,
    /// Whether multiple choices are allowed.
    #[serde(default)]
    pub multiple: bool,
    /// Expiry time.
    #[serde(default)]
    pub expires_at: Option<String>,
    /// Whether voting has closed.
    #[serde(default)]
    pub expired: bool,
}

/// A Misskey note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    /// Misskey note id (`aid`).
    pub id: String,
    /// Creation time.
    pub created_at: String,
    /// Author id.
    pub user_id: String,
    /// Embedded author, when the endpoint expands it.
    #[serde(default)]
    pub user: Option<User>,
    /// Plain-text body.
    #[serde(default)]
    pub text: Option<String>,
    /// Content warning.
    #[serde(default)]
    pub cw: Option<String>,
    /// Visibility.
    #[serde(default)]
    pub visibility: NoteVisibility,
    /// Whether the note stays on this instance.
    #[serde(default)]
    pub local_only: bool,
    /// Renote count.
    #[serde(default)]
    pub renote_count: u64,
    /// Reply count.
    #[serde(default)]
    pub replies_count: u64,
    /// Reaction counts keyed by reaction, including custom emoji.
    #[serde(default)]
    pub reactions: BTreeMap<String, u64>,
    /// Total reaction count.
    #[serde(default)]
    pub reaction_count: u64,
    /// Renoted note id.
    #[serde(default)]
    pub renote_id: Option<String>,
    /// Embedded renote, when the endpoint expands it.
    #[serde(default)]
    pub renote: Option<Box<Note>>,
    /// Parent note id.
    #[serde(default)]
    pub reply_id: Option<String>,
    /// Embedded parent note, when the endpoint expands it.
    #[serde(default)]
    pub reply: Option<Box<Note>>,
    /// Canonical ActivityPub note URL.
    #[serde(default)]
    pub uri: Option<String>,
    /// Human page URL.
    #[serde(default)]
    pub url: Option<String>,
    /// Attached file ids.
    #[serde(default)]
    pub file_ids: Vec<String>,
    /// Expanded attachments.
    #[serde(default)]
    pub files: Vec<File>,
    /// Hashtags, without `#`.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Poll, when present.
    #[serde(default)]
    pub poll: Option<Poll>,
    /// Custom emoji used in the body.
    #[serde(default)]
    pub emojis: BTreeMap<String, String>,
    /// Channel id.
    #[serde(default)]
    pub channel_id: Option<String>,
    /// Whether the note is hidden from public timelines.
    #[serde(default)]
    pub is_hidden: bool,
}

/// A timeline request body.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineRequest {
    /// Maximum number of notes.
    #[serde(default)]
    pub limit: Option<u32>,
    /// Return notes newer than this id.
    #[serde(default)]
    pub since_id: Option<String>,
    /// Return notes older than this id.
    #[serde(default)]
    pub until_id: Option<String>,
    /// Allow a partial page instead of an error.
    #[serde(default)]
    pub allow_partial: bool,
}

/// Body for `POST /api/notes/create`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateNoteRequest {
    /// Visibility; Misskey defaults to public.
    #[serde(default)]
    pub visibility: Option<NoteVisibility>,
    /// Plain-text body.
    #[serde(default)]
    pub text: Option<String>,
    /// Content warning.
    #[serde(default)]
    pub cw: Option<String>,
    /// Renote target.
    #[serde(default)]
    pub renote_id: Option<String>,
    /// Reply target.
    #[serde(default)]
    pub reply_id: Option<String>,
    /// Attached file ids.
    #[serde(default)]
    pub file_ids: Vec<String>,
    /// Poll to create with the note.
    #[serde(default)]
    pub poll: Option<Poll>,
    /// Keep the note local.
    #[serde(default)]
    pub local_only: bool,
    /// Channel id.
    #[serde(default)]
    pub channel_id: Option<String>,
}

/// Request body for `POST /api/miauth/{session}/check`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MiAuthCheckRequest {
    /// Session id created by the client.
    #[serde(default)]
    pub session: Option<String>,
}

/// Response from MiAuth session completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MiAuthCheckResponse {
    /// Whether the user approved the session.
    pub ok: bool,
    /// Issued access token; present when `ok` is true.
    #[serde(default)]
    pub token: Option<String>,
}

/// Generic Misskey error envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    /// Stable error code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Optional error identity.
    #[serde(default)]
    pub id: Option<String>,
    /// Optional extra data.
    #[serde(default)]
    pub info: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_uses_misskey_names() {
        assert_eq!(
            serde_json::to_string(&NoteVisibility::Home).expect("serialize"),
            "\"home\""
        );
        assert_eq!(
            serde_json::from_str::<NoteVisibility>("\"followers\"").expect("deserialize"),
            NoteVisibility::Followers
        );
    }

    #[test]
    fn note_defaults_match_a_minimal_response() {
        let note: Note = serde_json::from_value(serde_json::json!({
            "id": "9am0f01abc",
            "createdAt": "2026-01-01T00:00:00Z",
            "userId": "9am0f00xyz"
        }))
        .expect("minimal note");
        assert_eq!(note.visibility, NoteVisibility::Public);
        assert_eq!(note.files, Vec::<File>::new());
        assert!(note.reactions.is_empty());
    }

    #[test]
    fn create_request_accepts_a_poll() {
        let request: CreateNoteRequest = serde_json::from_value(serde_json::json!({
            "text": "which?",
            "poll": { "choices": [{ "text": "a" }, { "text": "b" }] }
        }))
        .expect("create request");
        assert_eq!(request.poll.expect("poll").choices.len(), 2);
    }
}
