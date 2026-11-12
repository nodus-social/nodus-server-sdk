#![allow(clippy::struct_excessive_bools)]

//! Mastodon-compatible client API types.
//!
//! These are wire shapes only. Storage, visibility and authorisation belong to the
//! server; this crate describes what a Mastodon client receives and sends.

mod account;
mod instance;
mod page;
mod status;

pub use account::{Account, Emoji, Field};
pub use instance::{
    Configuration, InstanceV1, InstanceV2, MediaConfiguration, Registrations, StatusConfiguration,
};
pub use page::{LinkHeader, Page, PageQuery};
pub use status::{Application, Mention, Status, StatusId, Tag, Visibility};
