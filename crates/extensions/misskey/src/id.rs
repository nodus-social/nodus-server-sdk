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

/// Misskey `aid` epoch: 2000-01-01T00:00:00.000Z in Unix milliseconds.
const TIME2000_MS: u64 = 946_684_800_000;
/// Two base-36 noise digits (`36^2 = 1296`).
const NOISE_FACTOR: u64 = 36_u64.pow(2);
/// Upper exclusive bound for 10 base-36 digits (`36^10`).
const AID_MODULUS: u64 = 36_u64.pow(10);
/// Maximum representable 10-digit base-36 `aid` (`36^10 - 1`).
const MAX_AID: u64 = AID_MODULUS - 1;

/// A Misskey `aid`, a ten-digit base-36 sortable identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Aid(u64);

impl Aid {
    /// Builds an id from a millisecond timestamp and a small random component.
    ///
    /// Misskey's generator uses `((unix_ms - 946_684_800_000) * 1296) + noise`
    /// (saturated to `[0, 36^10 - 1]`), which is why ids sort by creation time
    /// as strings.
    #[must_use]
    pub fn from_parts(timestamp_ms: u64, random: u16) -> Self {
        let relative_ms = timestamp_ms.saturating_sub(TIME2000_MS);
        let noise = u64::from(random) % NOISE_FACTOR;
        Self(
            relative_ms
                .saturating_mul(NOISE_FACTOR)
                .saturating_add(noise)
                .min(MAX_AID),
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
        TIME2000_MS + (self.0 / NOISE_FACTOR)
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
        if parsed >= AID_MODULUS {
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

fn base36(value: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut value = value.min(MAX_AID);
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

    #[test]
    fn misskey_epoch_and_noise_factor_match_real_aids() {
        // Real Misskey aid from api.rs fixture: 2023-01-30T06:44:15.982Z
        let aid: Aid = "9am0f01abc".parse().expect("valid aid");
        assert_eq!(aid.timestamp_ms(), 1_675_061_055_982);

        // 2000-01-01T00:00:00.000Z is the Misskey TIME2000 epoch -> "0000000000"
        let epoch = Aid::from_parts(946_684_800_000, 0);
        assert_eq!(epoch.as_str(), "0000000000");
        assert_eq!(epoch.timestamp_ms(), 946_684_800_000);

        // Noise factor is 36^2 = 1296 (values up to 1295 stay distinct within same ms)
        let a = Aid::from_parts(1_760_000_000_123, 1000);
        let b = Aid::from_parts(1_760_000_000_123, 1295);
        assert_ne!(a, b);
        assert_eq!(a.timestamp_ms(), 1_760_000_000_123);
        assert_eq!(b.timestamp_ms(), 1_760_000_000_123);
    }

    #[test]
    fn from_parts_bounds_values_within_ten_base36_digits() {
        let max_supported_ts = 946_684_800_000 + 36_u64.pow(8) - 1;
        let at_max = Aid::from_parts(max_supported_ts, 1295);
        assert_eq!(at_max.as_str(), "zzzzzzzzzz");
        assert_eq!(at_max.get(), 36_u64.pow(10) - 1);

        let overflowing = Aid::from_parts(u64::MAX, u16::MAX);
        assert!(overflowing.get() < 36_u64.pow(10));
        assert_eq!(overflowing.as_str().len(), 10);
        assert_eq!(
            overflowing.as_str().parse::<Aid>().expect("aid"),
            overflowing
        );
    }
}
