//! Status and attachment shapes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::account::Account;

pub type StatusId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Unlisted,
    Private,
    Direct,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub id: StatusId,
    pub created_at: DateTime<Utc>,
    pub in_reply_to_id: Option<StatusId>,
    pub in_reply_to_account_id: Option<String>,
    pub sensitive: bool,
    pub spoiler_text: String,
    pub visibility: Visibility,
    pub language: Option<String>,
    pub uri: String,
    pub url: Option<String>,
    pub replies_count: i64,
    pub reblogs_count: i64,
    pub favourites_count: i64,
    pub edited_at: Option<DateTime<Utc>>,
    pub content: String,
    pub reblog: Option<Box<Self>>,
    pub account: Account,
    pub media_attachments: Vec<serde_json::Value>,
    pub mentions: Vec<Mention>,
    pub tags: Vec<Tag>,
    pub card: Option<serde_json::Value>,
    pub poll: Option<serde_json::Value>,
    pub application: Option<Application>,
    pub favourited: bool,
    pub reblogged: bool,
    pub muted: bool,
    pub bookmarked: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mention {
    pub id: String,
    pub username: String,
    pub url: String,
    pub acct: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Application {
    pub name: String,
    pub website: Option<String>,
}
