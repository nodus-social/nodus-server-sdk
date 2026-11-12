#![allow(clippy::doc_markdown)]

//! The Nodus performance protocol.
//!
//! These are the server-side wire types for the extensions a Nodus client negotiates on
//! top of the Mastodon compatibility floor: ordered batch calls, semantic event streams
//! and hash-chain sync. No storage or transport is defined here.

use serde::{Deserialize, Serialize};

/// The header carrying the contract version.
pub const CONTRACT_HEADER: &str = "X-Contract-Version";
/// The current protocol contract.
pub const CONTRACT_VERSION: &str = "nodus-1";

/// A capability document returned by `/api/v1/nodus/capabilities`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub contract: String,
    pub features: Vec<Feature>,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            contract: CONTRACT_VERSION.to_owned(),
            features: vec![Feature::Batch, Feature::Events, Feature::StateSync],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    Batch,
    Events,
    StateSync,
}

/// One call in a batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchCall {
    pub id: String,
    #[serde(flatten)]
    pub call: Verb,
    #[serde(default)]
    pub after: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Verb {
    Account {
        id: String,
    },
    Status {
        id: String,
    },
    Context {
        id: String,
    },
    Timeline {
        name: String,
        max_id: Option<String>,
        limit: Option<u32>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchResponse {
    pub results: Vec<BatchResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchResult {
    pub id: String,
    pub value: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub kind: EventKind,
    pub at: String,
    pub actor: Option<String>,
    pub object: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Post,
    Edit,
    Delete,
    Like,
    Follow,
    Mention,
    Announce,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    pub kinds: Vec<EventKind>,
    pub timeline: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncRoot {
    pub collection: String,
    pub hash: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncEvent {
    pub id: String,
    #[serde(rename = "kind")]
    pub kind: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncDiff {
    pub collection: String,
    pub from: String,
    pub to: String,
    pub events: Vec<SyncEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncStatus {
    UpToDate,
    Diff,
    Diverged,
    Reset,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_batch_call_keeps_its_dependency_list() {
        let call = BatchCall {
            id: "a".to_owned(),
            call: Verb::Context { id: "1".to_owned() },
            after: vec!["b".to_owned()],
        };
        let json = serde_json::to_value(&call).expect("serialise");
        assert_eq!(json["method"], "context");
        assert_eq!(json["after"][0], "b");
    }

    #[test]
    fn default_capabilities_advertise_all_three_extensions() {
        assert_eq!(Capabilities::default().features.len(), 3);
    }
}
