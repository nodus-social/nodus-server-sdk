//! WebFinger responses.
//!
//! A server answers `/.well-known/webfinger?resource=acct:alice@example.test` with an
//! `acct:` subject and an `activity+json` self link. Clients use that link to find the
//! actor document; without it a handle is not resolvable, no matter how good the actor
//! document is.

use serde_json::{Value, json};

/// One WebFinger link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebfingerLink {
    /// Registered relation, e.g. `self`.
    pub rel: String,
    /// MIME type for a `self` link.
    pub kind: Option<String>,
    /// Target URL.
    pub href: Option<String>,
    /// URI template, used by host-meta-style links.
    pub template: Option<String>,
}

impl WebfingerLink {
    /// The link every ActivityPub client follows.
    #[must_use]
    pub fn activitypub(actor_url: &str) -> Self {
        Self {
            rel: "self".to_owned(),
            kind: Some("application/activity+json".to_owned()),
            href: Some(actor_url.to_owned()),
            template: None,
        }
    }

    /// The profile page a browser should open.
    #[must_use]
    pub fn profile(url: &str) -> Self {
        Self {
            rel: "http://webfinger.net/rel/profile-page".to_owned(),
            kind: Some("text/html".to_owned()),
            href: Some(url.to_owned()),
            template: None,
        }
    }

    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut value = json!({ "rel": self.rel });
        let map = value.as_object_mut().expect("link is an object");
        if let Some(kind) = &self.kind {
            map.insert("type".to_owned(), json!(kind));
        }
        if let Some(href) = &self.href {
            map.insert("href".to_owned(), json!(href));
        }
        if let Some(template) = &self.template {
            map.insert("template".to_owned(), json!(template));
        }
        value
    }
}

/// A WebFinger document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Webfinger {
    /// `acct:alice@example.test`.
    pub subject: String,
    /// Alternate identifiers, usually the actor URL.
    pub aliases: Vec<String>,
    /// Links the client can follow.
    pub links: Vec<WebfingerLink>,
}

impl Webfinger {
    /// The minimal document for a local actor.
    #[must_use]
    pub fn for_actor(username: &str, domain: &str, actor_url: &str, profile_url: &str) -> Self {
        Self {
            subject: format!("acct:{username}@{domain}"),
            aliases: vec![actor_url.to_owned()],
            links: vec![
                WebfingerLink::activitypub(actor_url),
                WebfingerLink::profile(profile_url),
            ],
        }
    }

    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "subject": self.subject,
            "aliases": self.aliases,
            "links": self.links.iter().map(WebfingerLink::to_json).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_actor_has_an_activitypub_self_link() {
        let value = Webfinger::for_actor(
            "alice",
            "social.test",
            "https://social.test/users/alice",
            "https://social.test/@alice",
        )
        .to_json();
        assert_eq!(value["subject"], "acct:alice@social.test");
        assert_eq!(value["links"][0]["rel"], "self");
        assert_eq!(value["links"][0]["type"], "application/activity+json");
        assert_eq!(value["links"][0]["href"], "https://social.test/users/alice");
    }
}
