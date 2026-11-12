//! Misskey external identifiers.
//!
//! Misskey clients sort and paginate with `aid` strings, not the server's
//! internal ids. The host keeps the mapping; this type only encodes and decodes
//! the wire form so every Misskey endpoint agrees on ordering.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// An invalid Misskey `aid`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AidError {
    /// The value was not exactly ten base-36 digits.
    #[error("not a ten-digit base-36 aid: {0}")]
    Shape(String),
    /// The value did not fit in the supported integer range.
    #[error("aid overflows: {0}")]
    Overflow(String),
}

/// A Misskey `aid`, a ten-digit base-36 sortable identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Aid(u64);

impl Aid {
    /// Builds an id from a millisecond timestamp and a small random component.
    ///
    /// Misskey's generator uses `(unix_ms * 1000) + random`, which is why ids
    /// sort by creation time as strings.
    #[must_use]
    pub fn from_parts(timestamp_ms: u64, random: u16) -> Self {
        Self(
            timestamp_ms
                .saturating_mul(1000)
                .saturating_add(u64::from(random % 1000)),
        )
    }

    /// The integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The millisecond timestamp encoded in the id.
    #[must_use]
    pub const fn timestamp_ms(self) -> u64 {
        self.0 / 1000
    }

    /// The ten-character wire form.
    #[must_use]
    pub fn as_str(self) -> String {
        base36(self.0)
    }
}

impl FromStr for Aid {
    type Err = AidError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 10 || !value.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            return Err(AidError::Shape(value.to_owned()));
        }
        let parsed = u64::from_str_radix(&value.to_ascii_lowercase(), 36)
            .map_err(|_| AidError::Overflow(value.to_owned()))?;
        if parsed >= 36_u64.pow(10) {
            return Err(AidError::Overflow(value.to_owned()));
        }
        Ok(Self(parsed))
    }
}

impl fmt::Display for Aid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.as_str())
    }
}

fn base36(mut value: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut output = [b'0'; 10];
    for index in (0..10).rev() {
        output[index] = DIGITS[(value % 36) as usize];
        value /= 36;
    }
    String::from_utf8_lossy(&output).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_ten_digits_and_sort_as_strings() {
        let earlier = Aid::from_parts(1_700_000_000_000, 1);
        let later = Aid::from_parts(1_700_000_000_001, 2);
        assert_eq!(earlier.as_str().len(), 10);
        assert!(earlier.as_str() < later.as_str());
    }

    #[test]
    fn round_trip_preserves_the_timestamp() {
        let id = Aid::from_parts(1_760_000_000_123, 42);
        let parsed: Aid = id.as_str().parse().expect("aid");
        assert_eq!(parsed, id);
        assert_eq!(parsed.timestamp_ms(), 1_760_000_000_123);
    }

    #[test]
    fn malformed_ids_are_rejected() {
        assert!("short".parse::<Aid>().is_err());
        assert!("!!!!!!!!!!".parse::<Aid>().is_err());
    }
}
