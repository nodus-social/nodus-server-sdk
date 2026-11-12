//! Cursor pagination shared by timeline endpoints.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Query parameters a Mastodon timeline accepts.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PageQuery {
    pub max_id: Option<String>,
    pub since_id: Option<String>,
    pub min_id: Option<String>,
    pub limit: Option<u32>,
    pub local: Option<bool>,
}

/// A page plus the cursor a client uses for the next request.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_max_id: Option<String>,
}

/// The RFC 8288 `Link` value Mastodon clients follow for pagination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkHeader(String);

impl LinkHeader {
    #[must_use]
    pub fn next(path: &str, max_id: &str, limit: u32) -> Self {
        Self(format!(
            "<{path}?max_id={max_id}&limit={limit}>; rel=\"next\""
        ))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<T: Serialize> Page<T> {
    #[must_use]
    pub fn into_json(self) -> Value {
        serde_json::to_value(self.items).unwrap_or(Value::Array(Vec::new()))
    }
}
