//! Misskey streaming channel types.
//!
//! Misskey clients open `/streaming` and send `connect` messages. The channel
//! vocabulary is not the Mastodon streaming API, so it stays in this extension.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A Misskey streaming channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamingChannel {
    /// Notifications and the home timeline.
    Main,
    /// Notes from followed users.
    HomeTimeline,
    /// Local public notes.
    LocalTimeline,
    /// Federated public notes.
    HybridTimeline,
    /// Federated public notes, including remote-only traffic.
    GlobalTimeline,
    /// A user list.
    UserList,
    /// An antenna.
    Antenna,
    /// A channel.
    Channel,
    /// A hashtag.
    Hashtag,
}

/// Body of a Misskey streaming `connect` message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectBody {
    /// Channel to subscribe to.
    pub channel: StreamingChannel,
    /// Client-chosen connection id.
    pub id: String,
    /// Channel parameters, such as `listId` or `channelId`.
    #[serde(default)]
    pub params: Value,
}

/// A Misskey streaming `connect` message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connect {
    /// Always `connect`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Subscription body.
    pub body: ConnectBody,
}

impl Connect {
    /// Builds a `connect` message.
    #[must_use]
    pub fn new(channel: StreamingChannel, id: impl Into<String>, params: Value) -> Self {
        Self {
            kind: "connect".to_owned(),
            body: ConnectBody {
                channel,
                id: id.into(),
                params,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn connect_uses_the_misskey_envelope() {
        let value = serde_json::to_value(Connect::new(
            StreamingChannel::LocalTimeline,
            "c1",
            json!({}),
        ))
        .expect("serialize");
        assert_eq!(value["type"], "connect");
        assert_eq!(value["body"]["channel"], "localTimeline");
    }
}
