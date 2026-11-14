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
    /// The reaction was not a valid Unicode emoji.
    #[error("reaction is not a valid Unicode emoji")]
    Emoji,
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
    /// or HTML-significant characters, is not a valid Unicode emoji, or has a
    /// malformed custom shortcode.
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
        } else if !is_unicode_emoji(value) {
            return Err(ReactionError::Emoji);
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

fn is_unicode_emoji(value: &str) -> bool {
    is_keycap_emoji(value)
        || is_flag_emoji(value)
        || is_tag_flag_emoji(value)
        || value.split('\u{200d}').all(is_emoji_component)
}

fn is_keycap_emoji(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(base) = chars.next() else {
        return false;
    };
    if !matches!(base, '0'..='9' | '#' | '*') {
        return false;
    }
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some('\u{20e3}'), None, None) | (Some('\u{fe0f}'), Some('\u{20e3}'), None)
    )
}

fn is_flag_emoji(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(first), Some(second), None)
            if is_regional_indicator(first) && is_regional_indicator(second)
    )
}

fn is_tag_flag_emoji(value: &str) -> bool {
    let mut chars = value.chars();
    if chars.next() != Some('\u{1f3f4}') {
        return false;
    }
    let mut tag_count = 0_usize;
    while let Some(ch) = chars.next() {
        if ch == '\u{e007f}' {
            return tag_count > 0 && chars.next().is_none();
        }
        if !('\u{e0020}'..='\u{e007e}').contains(&ch) {
            return false;
        }
        tag_count += 1;
    }
    false
}

fn is_emoji_component(segment: &str) -> bool {
    let mut chars = segment.chars();
    let Some(base) = chars.next() else {
        return false;
    };
    if !is_emoji_base(base) {
        return false;
    }
    let mut seen_vs16 = false;
    let mut seen_skin_tone = false;
    for ch in chars {
        if ch == '\u{fe0f}' && !seen_vs16 {
            seen_vs16 = true;
        } else if is_skin_tone_modifier(ch) && !seen_skin_tone {
            seen_skin_tone = true;
        } else {
            return false;
        }
    }
    true
}

fn is_regional_indicator(ch: char) -> bool {
    ('\u{1f1e6}'..='\u{1f1ff}').contains(&ch)
}

fn is_skin_tone_modifier(ch: char) -> bool {
    ('\u{1f3fb}'..='\u{1f3ff}').contains(&ch)
}

fn is_emoji_base(ch: char) -> bool {
    matches!(
        ch as u32,
        0x00A9
            | 0x00AE
            | 0x203C
            | 0x2049
            | 0x2122
            | 0x2139
            | 0x2194..=0x2199
            | 0x21A9..=0x21AA
            | 0x231A..=0x231B
            | 0x2328
            | 0x23CF
            | 0x23E9..=0x23F3
            | 0x23F8..=0x23FA
            | 0x24C2
            | 0x25AA..=0x25AB
            | 0x25B6
            | 0x25C0
            | 0x25FB..=0x25FE
            | 0x2600..=0x27BF
            | 0x2934..=0x2935
            | 0x2B05..=0x2B07
            | 0x2B1B..=0x2B1C
            | 0x2B50
            | 0x2B55
            | 0x3030
            | 0x303D
            | 0x3297
            | 0x3299
            | 0x1F000..=0x1FAFF
    ) && !is_regional_indicator(ch)
        && !is_skin_tone_modifier(ch)
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

    #[test]
    fn arbitrary_ascii_and_non_emoji_text_are_rejected() {
        for invalid in [
            "hello_world",
            "javascript:alert(1)",
            "a",
            "1",
            "#",
            "*",
            "foo:",
            "👍hello",
            "👍👍",
            "日本語",
            "\u{fe0f}",
            "\u{200d}",
            "👍\u{200d}",
            "\u{200d}👍",
        ] {
            assert!(
                Reaction::parse(invalid).is_err(),
                "expected {invalid:?} to be rejected"
            );
        }
    }

    #[test]
    fn complex_unicode_emoji_sequences_are_accepted() {
        for valid in [
            "👍",
            "👍🏻",
            "❤️",
            "❤",
            "👨‍👩‍👧‍👦",
            "🏳️‍🌈",
            "🇺🇸",
            "🏴󠁧󠁢󠁳󠁣󠁴󠁿",
            "1️⃣",
            "#️⃣",
            "*️⃣",
            "©️",
            "🐦‍🔥",
        ] {
            assert_eq!(
                Reaction::parse(valid)
                    .unwrap_or_else(|err| panic!("expected {valid:?} to be accepted: {err}"))
                    .as_str(),
                valid
            );
        }
    }
}
