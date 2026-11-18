#![allow(clippy::doc_markdown)]

//! Read-only ActivityPub client.
//!
//! Transport is deliberately outside `nodus-activitypub-core` so protocol types
//! never pull HTTP into a server or a WASM build.

pub mod client;
pub mod signing;

pub use client::ActivityPub;
pub use nodus_activitypub_core as core;
pub use nodus_federation::SignatureError;
pub use signing::{HttpSigner, SignedHeaders};
