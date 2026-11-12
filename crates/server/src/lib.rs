//! The Nodus server SDK.
//!
//! Server processes share one protocol surface: canonical `ActivityPub` objects,
//! federation activities, HTTP signatures and delivery. This facade re-exports
//! those crates without pulling in a client runtime or a storage layer.
//!
//! ```toml
//! nodus-server-sdk = { path = "../server-sdk/crates/server", features = ["native", "federation", "crypto"] }
//! ```

#[cfg(feature = "activitypub")]
pub use nodus_activitypub_sdk as activitypub;

#[cfg(feature = "federation")]
pub use nodus_federation as federation;

#[cfg(feature = "mastodon")]
pub use nodus_mastodon_sdk as mastodon;

#[cfg(feature = "nodus")]
pub use nodus_performance_sdk as nodus;

#[cfg(feature = "service")]
pub use nodus_service_sdk as service;
