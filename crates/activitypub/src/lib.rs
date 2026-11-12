#![allow(clippy::doc_markdown)]

//! ActivityPub protocol facade.
//!
//! Protocol types are re-exported from `nodus-activitypub-core`. The read-only
//! transport is an opt-in `native` feature from `nodus-activitypub-client`.
//! Mastodon, Misskey and Nodus endpoint shapes live in `crates/extensions`.

pub use nodus_activitypub_core::*;

#[cfg(feature = "native")]
pub use nodus_activitypub_client::ActivityPub;
