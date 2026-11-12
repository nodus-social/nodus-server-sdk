//! Account shapes shared by profiles, statuses and timelines.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A profile as Mastodon clients expect it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub username: String,
    pub acct: String,
    pub display_name: String,
    pub locked: bool,
    pub bot: bool,
    pub discoverable: bool,
    pub group: bool,
    pub created_at: DateTime<Utc>,
    pub note: String,
    pub url: String,
    pub avatar: String,
    pub avatar_static: String,
    pub header: String,
    pub header_static: String,
    pub followers_count: i64,
    pub following_count: i64,
    pub statuses_count: i64,
    pub last_status_at: Option<String>,
    pub noindex: bool,
    pub emojis: Vec<Emoji>,
    pub fields: Vec<Field>,
    pub moved: Option<serde_json::Value>,
    pub suspended: bool,
    pub limited: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Emoji {
    pub shortcode: String,
    pub url: String,
    pub static_url: String,
    pub visible_in_picker: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub value: String,
    pub verified_at: Option<DateTime<Utc>>,
}
