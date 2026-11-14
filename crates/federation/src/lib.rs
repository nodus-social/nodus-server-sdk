//! `ActivityPub` federation as a library.
//!
//! This crate is the protocol half of an instance, not an instance. It defines the
//! activities, HTTP signature envelope, delivery and inbox ports. A host supplies the
//! listener, storage, key custody and policy.

pub mod activity;
pub mod delivery;
pub mod signature;

pub use activity::{Activity, ActivityKind, ActivityObject};
pub use delivery::{
    DEFAULT_MAX_CLOCK_SKEW, Delivery, DeliveryError, Inbox, REQUIRED_SIGNED_HEADERS, SignedRequest,
};
pub use signature::{HttpSignature, SignatureError, Signer, Verifier};
