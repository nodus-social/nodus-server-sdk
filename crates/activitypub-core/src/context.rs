//! Shared `@context` documents and the special public audience id.

use serde_json::{Value, json};

/// ActivityStreams 2.0 namespace.
pub const ACTIVITYSTREAMS: &str = "https://www.w3.org/ns/activitystreams";

/// The W3C security vocabulary, used by `publicKey`.
pub const SECURITY_V1: &str = "https://w3id.org/security/v1";

/// The public collection: every actor may read an object addressed to it.
pub const PUBLIC: &str = "https://www.w3.org/ns/activitystreams#Public";

/// The context an instance puts on actors, notes and activities.
///
/// The Mastodon terms are included even when a document does not use them: clients
/// such as Mastodon cache one context per host and fall back to plain ActivityStreams
/// when a term is missing, which silently drops `discoverable`, emoji and blurhash.
#[must_use]
pub fn activity_context() -> Value {
    json!([
        ACTIVITYSTREAMS,
        SECURITY_V1,
        {
            "manuallyApprovesFollowers": "as:manuallyApprovesFollowers",
            "toot": "http://joinmastodon.org/ns#",
            "discoverable": "toot:discoverable",
            "indexable": "toot:indexable",
            "featured": "toot:featured",
            "featuredTags": "toot:featuredTags",
            "alsoKnownAs": { "@id": "as:alsoKnownAs", "@type": "@id" },
            "movedTo": { "@id": "as:movedTo", "@type": "@id" },
            "schema": "http://schema.org#",
            "PropertyValue": "schema:PropertyValue",
            "value": "schema:value",
            "Hashtag": "as:Hashtag",
            "sensitive": "as:sensitive",
            "Emoji": "toot:Emoji",
            "blurhash": "toot:blurhash",
            "focalPoint": { "@container": "@list", "@id": "toot:focalPoint" },
            "votersCount": "toot:votersCount"
        }
    ])
}
