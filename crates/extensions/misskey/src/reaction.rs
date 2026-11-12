//! Misskey reaction values.
//!
//! A Misskey reaction is either one Unicode emoji or a `:shortcode:` custom
//! emoji. It is not a Mastodon favourite: one account may react once per note
//! with any allowed value. Validation happens at the API edge so stored values
//! cannot be used as markup.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// An invalid reaction value.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReactionError {
    /// The reaction was empty or too long.
    #[error("reaction must be 1-64 bytes")]
    Length,
    /// The reaction contained whitespace or markup characters.
    #[error("reaction contains an unsafe character")]
    Unsafe,
    /// A custom shortcode was malformed.
    #[error("invalid custom emoji shortcode: {0}")]
    Shortcode(String),
}

/// A validated Misskey reaction.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Reaction(String);

impl Reaction {
    /// Validates a reaction from a client or a federated activity.
    ///
    /// # Errors
    /// [`ReactionError`] when the value is empty, too long, contains whitespace
    /// or HTML-significant characters, or has a malformed custom shortcode.
    pub fn parse(value: &str) -> Result<Self, ReactionError> {
        if value.is_empty() || value.len() > 64 {
            return Err(ReactionError::Length);
        }
        if value.chars().any(|character| {
            character.is_whitespace()
                || character.is_control()
                || matches!(character, '<' | '>' | '"' | '\'' | '&')
        }) {
            return Err(ReactionError::Unsafe);
        }
        if value.starts_with(':') {
            let shortcode = value
                .strip_prefix(':')
                .and_then(|rest| rest.strip_suffix(':'))
                .ok_or_else(|| ReactionError::Shortcode(value.to_owned()))?;
            validate_shortcode(shortcode)?;
        }
        Ok(Self(value.to_owned()))
    }

    /// The wire value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is a `:shortcode:` custom emoji.
    #[must_use]
    pub fn is_custom(&self) -> bool {
        self.0.starts_with(':')
    }

    /// The shortcode without colons, for a custom emoji.
    #[must_use]
    pub fn shortcode(&self) -> Option<&str> {
        self.is_custom()
            .then(|| {
                self.0
                    .strip_prefix(':')
                    .and_then(|rest| rest.strip_suffix(':'))
            })
            .flatten()
    }
}

impl fmt::Display for Reaction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for Reaction {
    type Err = ReactionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

fn validate_shortcode(shortcode: &str) -> Result<(), ReactionError> {
    let (name, host) = shortcode
        .split_once('@')
        .map_or((shortcode, None), |(name, host)| (name, Some(host)));
    let valid_name = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+'));
    let valid_host = host.is_none_or(|host| {
        !host.is_empty()
            && host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':'))
    });
    if valid_name && valid_host {
        Ok(())
    } else {
        Err(ReactionError::Shortcode(shortcode.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_and_custom_reactions_are_accepted() {
        assert_eq!(Reaction::parse("👍").expect("emoji").as_str(), "👍");
        let custom = Reaction::parse(":blobcat@remote.example:").expect("custom");
        assert!(custom.is_custom());
        assert_eq!(custom.shortcode(), Some("blobcat@remote.example"));
    }

    #[test]
    fn markup_and_whitespace_are_rejected() {
        assert!(Reaction::parse("<script>").is_err());
        assert!(Reaction::parse("two words").is_err());
        assert!(Reaction::parse(":").is_err());
    }
}
