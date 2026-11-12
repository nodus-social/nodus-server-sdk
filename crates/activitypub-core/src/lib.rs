#![allow(clippy::doc_markdown)]

//! ActivityPub protocol types and documents.
//!
//! This crate is protocol-only: no HTTP client, no server, no storage and no
//! Mastodon/Misskey/Nodus endpoint shapes. The reader lives in
//! `nodus-activitypub-client`; federation in `nodus-federation`.

pub mod activity;
pub mod actor;
pub mod collection;
pub mod context;
pub mod note;
pub mod webfinger;

pub use activity::{Activity, ActivityKind, ActivityObject};
pub use actor::{Actor, Emoji, Image};
pub use collection::{Collection, CollectionPage};
pub use note::{Attachment, Note};
pub use webfinger::{Webfinger, WebfingerLink};

use thiserror::Error;

/// Why an ActivityPub operation failed.
#[derive(Debug, Error)]
pub enum Error {
    /// The host or handle was not a valid `user@host`.
    #[error("not a federated handle: {0}")]
    Handle(String),
    /// A URL was not absolute or not HTTPS.
    #[error("not an https URL: {0}")]
    Url(String),
    /// WebFinger did not name an ActivityPub actor.
    #[error("webfinger has no activity+json actor for {0}")]
    NoActor(String),
    /// The object is not the ActivityPub type the caller needed.
    #[error("unexpected activitypub object: {0}")]
    Object(String),
    /// The remote host answered with an error.
    #[error("remote host returned {status} for {url}")]
    Status {
        /// HTTP status.
        status: u16,
        /// Requested URL.
        url: String,
    },
    /// Transport or JSON failure.
    #[error("activitypub read failed: {0}")]
    Transport(String),
}

/// Fields this crate does not model, kept so a round trip does not erase another
/// implementation's properties.
pub(crate) fn extra_fields(
    value: &serde_json::Value,
    known: &[&str],
) -> serde_json::Map<String, serde_json::Value> {
    value
        .as_object()
        .map(|object| {
            object
                .iter()
                .filter(|(key, _)| !known.contains(&key.as_str()))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default()
}
