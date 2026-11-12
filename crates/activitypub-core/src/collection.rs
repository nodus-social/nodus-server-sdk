//! Ordered and unordered ActivityPub collections.
//!
//! The reader and the server share these types: an instance builds the same
//! `Collection` and `CollectionPage` documents a peer parses, so a round trip through
//! JSON cannot drift into two subtly different shapes.

use serde_json::{Value, json};

use crate::Error;
use crate::actor::{link, string, total_items};
use crate::context::activity_context;

/// A collection's header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collection {
    /// Canonical collection id.
    pub id: String,
    /// Number of members when the origin publishes it.
    pub total_items: u64,
    /// First page URL.
    pub first: Option<String>,
}

impl Collection {
    /// Parse a collection header.
    ///
    /// # Errors
    /// [`Error::Object`] when there is no id.
    pub fn from_json(value: &Value) -> Result<Self, Error> {
        let id =
            string(value.get("id")).ok_or_else(|| Error::Object("collection has no id".into()))?;
        Ok(Self {
            id,
            total_items: total_items(Some(value)),
            first: link(value.get("first")),
        })
    }

    /// An `OrderedCollection` document. Mastodon and Misskey both accept this shape
    /// for followers, following and outbox.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut value = json!({
            "@context": activity_context(),
            "id": self.id,
            "type": "OrderedCollection",
            "totalItems": self.total_items,
        });
        if let Some(first) = &self.first {
            value
                .as_object_mut()
                .expect("collection is an object")
                .insert("first".to_owned(), json!(first));
        }
        value
    }
}

/// One page of a collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionPage {
    /// Raw members. Callers know whether they are actors, notes or URLs.
    pub items: Vec<Value>,
    /// Next page URL, when the origin publishes one.
    pub next: Option<String>,
    /// Total across the whole collection, when present.
    pub total_items: u64,
}

impl CollectionPage {
    /// Parse a collection page, or a collection that carries items inline.
    ///
    /// # Errors
    /// [`Error::Object`] when neither `orderedItems` nor `items` is present.
    pub fn from_json(value: &Value) -> Result<Self, Error> {
        let items = value
            .get("orderedItems")
            .or_else(|| value.get("items"))
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Object("collection page has no items".into()))?
            .clone();
        Ok(Self {
            items,
            next: link(value.get("next")),
            total_items: total_items(Some(value)),
        })
    }

    /// The page document an instance serves when a client follows `first`.
    ///
    /// `id` is the page's own URL and `part_of` the collection header, which is what
    /// tells a consumer that this page is not the whole collection.
    #[must_use]
    pub fn to_json(&self, id: &str, part_of: &str) -> Value {
        let mut value = json!({
            "@context": activity_context(),
            "id": id,
            "type": "OrderedCollectionPage",
            "partOf": part_of,
            "totalItems": self.total_items,
            "orderedItems": self.items,
        });
        if let Some(next) = &self.next {
            value
                .as_object_mut()
                .expect("collection page is an object")
                .insert("next".to_owned(), json!(next));
        }
        value
    }
}
