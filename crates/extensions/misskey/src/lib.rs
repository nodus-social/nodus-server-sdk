#![allow(clippy::doc_markdown)]
#![allow(clippy::struct_excessive_bools)]

//! Misskey-compatible client API and ActivityPub dialect types.
//!
//! This crate describes the two wire surfaces a Misskey client uses. It does not
//! own storage, authorisation or routing: the host maps these types onto its
//! domain services, exactly as it does for the Mastodon extension.
//!
//! Misskey federates over ActivityPub. Its protocol extensions live in
//! [`activitypub`]; its client API lives in [`api`].

pub mod activitypub;
pub mod api;
pub mod id;
pub mod reaction;
pub mod streaming;

pub use activitypub::MisskeyExtensions;
pub use id::{Aid, AidError};
pub use reaction::{Reaction, ReactionError};
pub use streaming::{Connect, StreamingChannel};
