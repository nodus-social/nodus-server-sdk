//! Delivery and inbox ports. The host supplies HTTP, retries, storage and policy.

use std::collections::HashSet;
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};

use crate::{Activity, HttpSignature, Verifier};

/// Default maximum allowed clock skew for incoming `Date` headers (5 minutes).
pub const DEFAULT_MAX_CLOCK_SKEW: Duration = Duration::from_secs(300);

/// Header names that every incoming inbox HTTP signature must cover.
pub const REQUIRED_SIGNED_HEADERS: [&str; 4] = ["(request-target)", "host", "date", "digest"];

/// Why federation delivery or verification failed.
#[derive(Debug, thiserror::Error)]
pub enum DeliveryError {
    /// The target is not an HTTPS inbox.
    #[error("not an https inbox: {0}")]
    Inbox(String),
    /// Signature verification failed.
    #[error("signature: {0}")]
    Signature(String),
    /// The incoming activity payload could not be parsed.
    #[error("invalid activity payload: {0}")]
    Payload(String),
    /// The peer rejected the activity.
    #[error("remote rejected the activity: {0}")]
    Remote(String),
}

/// An incoming HTTP request envelope delivered to an `ActivityPub` inbox.
///
/// Captures the HTTP request line and headers needed to validate an HTTP
/// signature (`draft-cavage-http-signatures`), prevent replay and body
/// tampering, and bind the signing key to `activity.actor()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignedRequest<'a> {
    /// HTTP method (for example `"POST"`).
    pub method: &'a str,
    /// Request target path and optional query string (for example `"/inbox"`).
    pub path: &'a str,
    /// `Host` header value.
    pub host: &'a str,
    /// `Date` header value (RFC 2822 / IMF-fixdate).
    pub date: &'a str,
    /// `Digest` header value (for example `"SHA-256=..."`).
    pub digest: &'a str,
    /// Raw `Signature` or `Authorization: Signature ...` header value.
    pub signature: &'a str,
    /// Raw request body bytes.
    pub body: &'a [u8],
}

impl SignedRequest<'_> {
    /// Resolve a lower-case header name from this request for [`HttpSignature::signing_string`].
    #[must_use]
    pub fn header_value(&self, name: &str) -> Option<String> {
        match name {
            "(request-target)" => Some(format!(
                "{} {}",
                self.method.to_ascii_lowercase(),
                self.path
            )),
            "host" => Some(self.host.to_owned()),
            "date" => Some(self.date.to_owned()),
            "digest" => Some(self.digest.to_owned()),
            _ => None,
        }
    }

    /// Validate envelope fields, parse the HTTP `Signature` header, enforce that
    /// `(request-target)`, `host`, `date`, and `digest` are signed, and check
    /// that `Date` falls within `max_clock_skew` of `now`.
    pub fn validate_metadata_at(
        &self,
        now: DateTime<Utc>,
        max_clock_skew: Duration,
    ) -> Result<HttpSignature, DeliveryError> {
        self.validate_envelope()?;

        let parsed = HttpSignature::parse(self.signature)
            .map_err(|error| DeliveryError::Signature(error.to_string()))?;

        validate_required_headers(&parsed)?;
        validate_date(self.date, now, max_clock_skew)?;

        Ok(parsed)
    }

    /// Validate that `self.digest` matches the `SHA-256` digest of `self.body`.
    #[cfg(feature = "crypto")]
    pub fn validate_digest(&self) -> Result<(), DeliveryError> {
        let expected = crate::signature::crypto::body_digest(self.body);
        if !digest_matches(self.digest, &expected) {
            return Err(DeliveryError::Signature(
                "Digest header does not match request body".to_owned(),
            ));
        }
        Ok(())
    }

    /// Validate request metadata, required signed headers, `Date` clock skew,
    /// and `Digest` against the current UTC clock before fetching remote keys.
    #[cfg(feature = "crypto")]
    pub fn validate_request(
        &self,
        max_clock_skew: Duration,
    ) -> Result<HttpSignature, DeliveryError> {
        self.validate_request_at(Utc::now(), max_clock_skew)
    }

    /// Deterministic variant of [`Self::validate_request`] evaluated at `now`.
    #[cfg(feature = "crypto")]
    pub fn validate_request_at(
        &self,
        now: DateTime<Utc>,
        max_clock_skew: Duration,
    ) -> Result<HttpSignature, DeliveryError> {
        let parsed = self.validate_metadata_at(now, max_clock_skew)?;
        self.validate_digest()?;
        Ok(parsed)
    }

    /// Verify the incoming request's headers, `Digest`, `Date` clock skew, HTTP
    /// signature, activity payload, and actor-key ownership, returning the
    /// verified [`Activity`].
    ///
    /// `key_owner` resolves `signature.key_id` to the canonical actor URI that
    /// owns that key (or `None` if the key is unknown or untrusted).
    #[cfg(feature = "crypto")]
    pub fn verify<V, F>(
        &self,
        verifier: &V,
        key_owner: F,
        max_clock_skew: Duration,
    ) -> Result<Activity, DeliveryError>
    where
        V: Verifier + ?Sized,
        F: FnOnce(&str) -> Option<String>,
    {
        self.verify_at(verifier, key_owner, Utc::now(), max_clock_skew)
    }

    /// Deterministic variant of [`Self::verify`] that checks the `Date` header against `now`.
    #[cfg(feature = "crypto")]
    pub fn verify_at<V, F>(
        &self,
        verifier: &V,
        key_owner: F,
        now: DateTime<Utc>,
        max_clock_skew: Duration,
    ) -> Result<Activity, DeliveryError>
    where
        V: Verifier + ?Sized,
        F: FnOnce(&str) -> Option<String>,
    {
        self.verify_with_digest_at(
            verifier,
            |digest, body| digest_matches(digest, &crate::signature::crypto::body_digest(body)),
            key_owner,
            now,
            max_clock_skew,
        )
    }

    /// Verify the incoming request using a caller-supplied `Digest` predicate.
    ///
    /// Prefer [`Self::verify`] or [`Self::verify_at`] when the `crypto` feature is enabled.
    pub fn verify_with_digest_at<V, D, F>(
        &self,
        verifier: &V,
        digest_matches_fn: D,
        key_owner: F,
        now: DateTime<Utc>,
        max_clock_skew: Duration,
    ) -> Result<Activity, DeliveryError>
    where
        V: Verifier + ?Sized,
        D: FnOnce(&str, &[u8]) -> bool,
        F: FnOnce(&str) -> Option<String>,
    {
        let parsed = self.validate_metadata_at(now, max_clock_skew)?;

        if !digest_matches_fn(self.digest, self.body) {
            return Err(DeliveryError::Signature(
                "Digest header does not match request body".to_owned(),
            ));
        }

        let signing_string = parsed
            .signing_string(|name| self.header_value(name))
            .map_err(|error| DeliveryError::Signature(error.to_string()))?;

        verifier
            .verify(
                &parsed.key_id,
                &parsed.algorithm,
                signing_string.as_bytes(),
                &parsed.signature,
            )
            .map_err(|error| DeliveryError::Signature(error.to_string()))?;

        let activity: Activity = serde_json::from_slice(self.body)
            .map_err(|error| DeliveryError::Payload(error.to_string()))?;

        let owner = key_owner(&parsed.key_id).ok_or_else(|| {
            DeliveryError::Signature(format!("unknown signing key owner for {}", parsed.key_id))
        })?;
        if owner.is_empty() || owner != activity.actor() {
            return Err(DeliveryError::Signature(format!(
                "signing key {} belongs to {owner}, not activity actor {}",
                parsed.key_id,
                activity.actor()
            )));
        }

        Ok(activity)
    }

    fn validate_envelope(&self) -> Result<(), DeliveryError> {
        if self.method.is_empty()
            || self
                .method
                .bytes()
                .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
        {
            return Err(DeliveryError::Signature(
                "invalid HTTP method in signed request".to_owned(),
            ));
        }
        if !self.path.starts_with('/')
            || self
                .path
                .bytes()
                .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
        {
            return Err(DeliveryError::Signature(
                "invalid request path in signed request".to_owned(),
            ));
        }
        for (name, value) in [
            ("host", self.host),
            ("date", self.date),
            ("digest", self.digest),
            ("signature", self.signature),
        ] {
            if value.trim().is_empty() || value.bytes().any(|byte| byte.is_ascii_control()) {
                return Err(DeliveryError::Signature(format!(
                    "invalid {name} header in signed request"
                )));
            }
        }
        Ok(())
    }
}

fn validate_required_headers(signature: &HttpSignature) -> Result<(), DeliveryError> {
    let mut seen = HashSet::with_capacity(signature.headers.len());
    for header in &signature.headers {
        let lower = header.to_ascii_lowercase();
        if !seen.insert(lower) {
            return Err(DeliveryError::Signature(format!(
                "duplicate header in signature: {header}"
            )));
        }
    }
    for required in REQUIRED_SIGNED_HEADERS {
        if !seen.contains(required) {
            return Err(DeliveryError::Signature(format!(
                "signature is missing required header: {required}"
            )));
        }
    }
    Ok(())
}

fn validate_date(
    raw_date: &str,
    now: DateTime<Utc>,
    max_clock_skew: Duration,
) -> Result<(), DeliveryError> {
    let parsed = DateTime::parse_from_rfc2822(raw_date.trim())
        .map_err(|error| DeliveryError::Signature(format!("invalid Date header: {error}")))?
        .with_timezone(&Utc);
    let skew = TimeDelta::from_std(max_clock_skew)
        .map_err(|error| DeliveryError::Signature(format!("invalid clock skew window: {error}")))?;
    let earliest = now
        .checked_sub_signed(skew)
        .unwrap_or(DateTime::<Utc>::MIN_UTC);
    let latest = now
        .checked_add_signed(skew)
        .unwrap_or(DateTime::<Utc>::MAX_UTC);
    if parsed < earliest || parsed > latest {
        return Err(DeliveryError::Signature(
            "Date header is outside the allowed clock-skew window".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(feature = "crypto")]
fn digest_matches(header: &str, expected: &str) -> bool {
    let Some((actual_alg, actual_val)) = header.trim().split_once('=') else {
        return false;
    };
    let Some((expected_alg, expected_val)) = expected.split_once('=') else {
        return false;
    };
    actual_alg.eq_ignore_ascii_case(expected_alg)
        && constant_time_eq(actual_val.as_bytes(), expected_val.as_bytes())
}

#[cfg(feature = "crypto")]
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut diff = u8::from(left.len() != right.len());
    for (i, &a) in left.iter().enumerate() {
        let b = right.get(i).copied().unwrap_or(0);
        diff = std::hint::black_box(diff | (a ^ b));
    }
    std::hint::black_box(diff) == 0
}

/// Deliver one activity to one inbox.
pub trait Delivery: Send + Sync {
    /// Perform the delivery.
    fn deliver(
        &self,
        inbox: &str,
        activity: &Activity,
    ) -> impl Future<Output = Result<(), DeliveryError>> + Send;
}

/// Accept one incoming activity after signature verification.
pub trait Inbox: Send + Sync {
    /// Verify and parse an incoming request.
    fn accept(
        &self,
        request: &SignedRequest<'_>,
    ) -> impl Future<Output = Result<Activity, DeliveryError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityKind, SignatureError};

    struct StubVerifier;

    impl Verifier for StubVerifier {
        fn verify(
            &self,
            _key_id: &str,
            _algorithm: &str,
            _message: &[u8],
            signature: &str,
        ) -> Result<(), SignatureError> {
            if signature == "valid-sig" {
                Ok(())
            } else {
                Err(SignatureError::Crypto("bad signature".to_owned()))
            }
        }
    }

    fn fixed_now() -> DateTime<Utc> {
        DateTime::parse_from_rfc2822("Thu, 12 Nov 2026 03:00:00 GMT")
            .expect("valid rfc2822")
            .with_timezone(&Utc)
    }

    fn sample_activity_bytes(actor: &str) -> Vec<u8> {
        let activity = Activity::new(
            ActivityKind::Create,
            actor,
            "https://example.social/notes/1",
        );
        serde_json::to_vec(&activity).expect("serialize activity")
    }

    #[test]
    fn verify_with_digest_at_accepts_valid_request_and_checks_rfc2822_gmt() {
        let body = sample_activity_bytes("https://example.social/users/alice");
        let sig = r#"keyId="https://example.social/users/alice#main-key",algorithm="rsa-sha256",headers="(request-target) host date digest",signature="valid-sig""#;
        let request = SignedRequest {
            method: "POST",
            path: "/inbox",
            host: "target.example",
            date: "Thu, 12 Nov 2026 03:00:00 GMT",
            digest: "SHA-256=stub",
            signature: sig,
            body: &body,
        };

        let activity = request
            .verify_with_digest_at(
                &StubVerifier,
                |digest, _| digest == "SHA-256=stub",
                |key_id| {
                    (key_id == "https://example.social/users/alice#main-key")
                        .then(|| "https://example.social/users/alice".to_owned())
                },
                fixed_now(),
                DEFAULT_MAX_CLOCK_SKEW,
            )
            .expect("verified activity");

        assert_eq!(activity.actor(), "https://example.social/users/alice");
        assert_eq!(activity.kind, ActivityKind::Create);
    }

    #[test]
    fn rejects_missing_required_headers_in_signature() {
        let body = sample_activity_bytes("https://example.social/users/alice");
        for headers in [
            "date",
            "host date digest",
            "(request-target) date digest",
            "(request-target) host digest",
            "(request-target) host date",
        ] {
            let sig = format!(
                r#"keyId="https://example.social/users/alice#main-key",headers="{headers}",signature="valid-sig""#
            );
            let request = SignedRequest {
                method: "POST",
                path: "/inbox",
                host: "target.example",
                date: "Thu, 12 Nov 2026 03:00:00 GMT",
                digest: "SHA-256=stub",
                signature: &sig,
                body: &body,
            };
            let err = request
                .verify_with_digest_at(
                    &StubVerifier,
                    |_, _| true,
                    |_| Some("https://example.social/users/alice".to_owned()),
                    fixed_now(),
                    DEFAULT_MAX_CLOCK_SKEW,
                )
                .unwrap_err();
            assert!(
                matches!(err, DeliveryError::Signature(_)),
                "headers={headers} should be rejected, got {err:?}"
            );
        }

        // Omitted headers parameter defaults to ["date"] and must also be rejected.
        let sig_no_headers =
            r#"keyId="https://example.social/users/alice#main-key",signature="valid-sig""#;
        let request = SignedRequest {
            method: "POST",
            path: "/inbox",
            host: "target.example",
            date: "Thu, 12 Nov 2026 03:00:00 GMT",
            digest: "SHA-256=stub",
            signature: sig_no_headers,
            body: &body,
        };
        let err = request
            .verify_with_digest_at(
                &StubVerifier,
                |_, _| true,
                |_| Some("https://example.social/users/alice".to_owned()),
                fixed_now(),
                DEFAULT_MAX_CLOCK_SKEW,
            )
            .unwrap_err();
        assert!(matches!(err, DeliveryError::Signature(_)));
    }

    #[test]
    fn rejects_date_outside_clock_skew_window() {
        let body = sample_activity_bytes("https://example.social/users/alice");
        let sig = r#"keyId="https://example.social/users/alice#main-key",headers="(request-target) host date digest",signature="valid-sig""#;

        for bad_date in [
            "Thu, 12 Nov 2026 02:54:59 GMT", // 301s in the past
            "Thu, 12 Nov 2026 03:05:01 GMT", // 301s in the future
            "not-a-valid-http-date",
            "Tue, 12 Nov 9999 03:00:00 GMT",
        ] {
            let request = SignedRequest {
                method: "POST",
                path: "/inbox",
                host: "target.example",
                date: bad_date,
                digest: "SHA-256=stub",
                signature: sig,
                body: &body,
            };
            let err = request
                .verify_with_digest_at(
                    &StubVerifier,
                    |_, _| true,
                    |_| Some("https://example.social/users/alice".to_owned()),
                    fixed_now(),
                    DEFAULT_MAX_CLOCK_SKEW,
                )
                .unwrap_err();
            assert!(
                matches!(err, DeliveryError::Signature(_)),
                "date={bad_date} should be rejected"
            );
        }

        // Exact boundaries (now +/- 300s) should succeed.
        for edge_date in [
            "Thu, 12 Nov 2026 02:55:00 GMT",
            "Thu, 12 Nov 2026 03:05:00 GMT",
        ] {
            let request = SignedRequest {
                method: "POST",
                path: "/inbox",
                host: "target.example",
                date: edge_date,
                digest: "SHA-256=stub",
                signature: sig,
                body: &body,
            };
            request
                .verify_with_digest_at(
                    &StubVerifier,
                    |_, _| true,
                    |_| Some("https://example.social/users/alice".to_owned()),
                    fixed_now(),
                    DEFAULT_MAX_CLOCK_SKEW,
                )
                .expect("boundary date should be accepted");
        }
    }

    #[test]
    fn rejects_actor_spoofing_when_key_owner_differs_from_activity_actor() {
        let body = sample_activity_bytes("https://victim.example/users/alice");
        let sig = r#"keyId="https://attacker.example/users/mallory#main-key",headers="(request-target) host date digest",signature="valid-sig""#;
        let request = SignedRequest {
            method: "POST",
            path: "/inbox",
            host: "target.example",
            date: "Thu, 12 Nov 2026 03:00:00 GMT",
            digest: "SHA-256=stub",
            signature: sig,
            body: &body,
        };

        let err = request
            .verify_with_digest_at(
                &StubVerifier,
                |_, _| true,
                |_| Some("https://attacker.example/users/mallory".to_owned()),
                fixed_now(),
                DEFAULT_MAX_CLOCK_SKEW,
            )
            .unwrap_err();
        assert!(matches!(err, DeliveryError::Signature(_)));

        // Prefix confusion must also fail.
        let err = request
            .verify_with_digest_at(
                &StubVerifier,
                |_, _| true,
                |_| Some("https://victim.example/users/alice.evil".to_owned()),
                fixed_now(),
                DEFAULT_MAX_CLOCK_SKEW,
            )
            .unwrap_err();
        assert!(matches!(err, DeliveryError::Signature(_)));

        // Unknown key owner must fail.
        let err = request
            .verify_with_digest_at(
                &StubVerifier,
                |_, _| true,
                |_| None,
                fixed_now(),
                DEFAULT_MAX_CLOCK_SKEW,
            )
            .unwrap_err();
        assert!(matches!(err, DeliveryError::Signature(_)));
    }

    #[test]
    fn rejects_duplicate_unknown_headers_and_crlf_injection() {
        let body = sample_activity_bytes("https://example.social/users/alice");
        let dup_sig =
            r#"keyId="k",headers="(request-target) host date digest date",signature="valid-sig""#;
        let request = SignedRequest {
            method: "POST",
            path: "/inbox",
            host: "target.example",
            date: "Thu, 12 Nov 2026 03:00:00 GMT",
            digest: "SHA-256=stub",
            signature: dup_sig,
            body: &body,
        };
        assert!(matches!(
            request.verify_with_digest_at(
                &StubVerifier,
                |_, _| true,
                |_| Some("https://example.social/users/alice".to_owned()),
                fixed_now(),
                DEFAULT_MAX_CLOCK_SKEW,
            ),
            Err(DeliveryError::Signature(_))
        ));

        let extra_header_sig = r#"keyId="k",headers="(request-target) host date digest x-extra",signature="valid-sig""#;
        let request_extra = SignedRequest {
            signature: extra_header_sig,
            ..request
        };
        assert!(matches!(
            request_extra.verify_with_digest_at(
                &StubVerifier,
                |_, _| true,
                |_| Some("https://example.social/users/alice".to_owned()),
                fixed_now(),
                DEFAULT_MAX_CLOCK_SKEW,
            ),
            Err(DeliveryError::Signature(_))
        ));

        let injected = SignedRequest {
            host: "target.example\nfake: header",
            signature: r#"keyId="k",headers="(request-target) host date digest",signature="valid-sig""#,
            ..request
        };
        assert!(matches!(
            injected.verify_with_digest_at(
                &StubVerifier,
                |_, _| true,
                |_| Some("https://example.social/users/alice".to_owned()),
                fixed_now(),
                DEFAULT_MAX_CLOCK_SKEW,
            ),
            Err(DeliveryError::Signature(_))
        ));
    }

    #[cfg(feature = "crypto")]
    mod crypto_tests {
        use std::task::{Context, Poll, Waker};

        use super::*;
        use crate::Signer;
        use crate::signature::crypto::{RsaSigner, RsaVerifier, body_digest};

        const ALICE_PRIVATE_KEY_PKCS8: &str = "\
-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCveGzqX/ppbzqJ
Y0jiuqi1GK2ToQj1Yx1HertvoQLFBN/TcV1F8sPebmg6gMHbuYZQJhZ+gLJ8gw9I
ZP10n+wKS8UgWK5yllQx/XoX6dQLVedn/qfTqx1egifGcEa6odqOPOViGqcqhy4R
kfjnV+DDQMa/480hIiXWr0cHmqM8iaeHYe6ilWr7FuMfSJJlxLfWedAXUacxBjsO
BgX6wtBNpYtQyyCqaEK9KgPCuj9wUx5bIRQTS0q+a0EufSeDxyQ834Kxmc5TGr7e
ooyK+o4wCofk8cYbOJ9Xo3JZaSf+zh9cVSZJrIyvrgGgp2lHxiVtw6cgEFWu/Cs8
uWb/4MxPAgMBAAECggEAHuBF6gfxfFQCLOK3lI3b2eH5yRxxAxzprgY+rSnb3uc4
q3ArRX+uT5mF5aV6dztyEvBgzOnAfoBhVA/ROZBabIVgroxsReWF8zXzIfoXQddf
3MDGr0xQ4qxsa1KeX4DTgGvewSCHgcg/pv/UmICyIClpz6I6GjZ0Cg7VpNTlHGWh
xPNldBR+OIIPf7ivAjMHyM9YEwiPDZznkQ6ZdyiIlaghE4EK+EIY/joo52MtcrAR
gtLrT8EvQ3Bwc60/0EnCmSnVMzggcZU/X/ZfH4REAUvOc/WT8ot9lOB9Zc/k9fAD
64IMpYJGO0Welcqa1YfT0oX7nHYuzOf/01jLbnbbYQKBgQDhmA8Q09ReSJb+ubMC
KseX1Agi4NihvM+RyHnexi4nPs//KnyCt/03i9kZSJg0goI+kElO/+rmus4Q6wpS
hS3ox8jd/qWFOlebDPZVPP/rUYORGvg494jQ7TOQMzzgI73qATh4vqmDUrPxC6oZ
hOyZ53fI3IQIdKG/oORP0Fxr5QKBgQDHHuSHJ2sh5bqWIKyXAPWfQiFiki9zmiFp
pmBSDhZlyls20w7OZqEBEAlGyn3trG2YbLWJKuWErWY52dT2nT4m4WkVLkNSYeSY
SwTELrx9RhFnNdybE+S55UkHZHq3E7GZWen4w2vpLz/+vzrsIOo1bb+cgBE/Cm4H
Mcu0jJ0cIwKBgGTn4QVD2t/5VCDH6zvy6qgM3T7Ml9lwXN/2bhypRIelXt/hnyPR
LGAmG4tlnz8NiTh2zXOaoPVnZTjICSv0Ch7vDffOKTc4a2sdFyn1SPt20ISdLZ49
imL5VnEPmhEfq5j5xsqrbaMh4P7WmtUjUbldgYTErgIiOR8/1h+lMVHJAoGAIQnV
LT+zHOq7LNG7EUsNdiFI3MwYp3u5c4gxjyvbN4R94fjfCgwpRr3cbCyI/HiRrqrK
7Dl736nNvmeQWGL6vwTDpMoKTY8EmOhJoCuJBtgrZw6ezfRdDSMoHNvlKJ9R5nnu
wmHac9RtL92W7zr+4WKWWmc9dVJLs1Hl1M1H/DUCgYEAzRnSpKy5zndx30QFrHSx
mnHCOp9IbIXA+V2r4a3YAJRcq7ifrjVQDfgJMPRfUiFvfhWP+yfp9WlTvzsE8LU8
oVmlDOVdsJ72O3ici71/2PSymBdtxU1yBpJO2V+RobnrNM/DjpvOGwrQd2sB2XOd
8gFepkGkt+GeKTydmqi4sic=
-----END PRIVATE KEY-----
";

        const ALICE_PUBLIC_KEY_SPKI: &str = "\
-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAr3hs6l/6aW86iWNI4rqo
tRitk6EI9WMdR3q7b6ECxQTf03FdRfLD3m5oOoDB27mGUCYWfoCyfIMPSGT9dJ/s
CkvFIFiucpZUMf16F+nUC1XnZ/6n06sdXoInxnBGuqHajjzlYhqnKocuEZH451fg
w0DGv+PNISIl1q9HB5qjPImnh2HuopVq+xbjH0iSZcS31nnQF1GnMQY7DgYF+sLQ
TaWLUMsgqmhCvSoDwro/cFMeWyEUE0tKvmtBLn0ng8ckPN+CsZnOUxq+3qKMivqO
MAqH5PHGGzifV6NyWWkn/s4fXFUmSayMr64BoKdpR8YlbcOnIBBVrvwrPLlm/+DM
TwIDAQAB
-----END PUBLIC KEY-----
";

        fn block_on<F: Future>(future: F) -> F::Output {
            let waker = Waker::noop();
            let mut cx = Context::from_waker(waker);
            let mut pinned = std::pin::pin!(future);
            match pinned.as_mut().poll(&mut cx) {
                Poll::Ready(out) => out,
                Poll::Pending => panic!("test future unexpectedly yielded Pending"),
            }
        }

        fn sign_request(
            signer: &RsaSigner,
            key_id: &str,
            draft: &SignedRequest<'_>,
            headers: &str,
        ) -> String {
            let signing_sig = HttpSignature {
                key_id: key_id.to_owned(),
                algorithm: "rsa-sha256".to_owned(),
                headers: headers
                    .split_whitespace()
                    .map(str::to_ascii_lowercase)
                    .collect(),
                signature: "placeholder".to_owned(),
            };
            let message = signing_sig
                .signing_string(|name| draft.header_value(name))
                .expect("signing string");
            let sig_b64 = signer
                .sign(key_id, "rsa-sha256", message.as_bytes())
                .expect("sign");
            format!(
                r#"Signature keyId="{key_id}",algorithm="rsa-sha256",headers="{headers}",signature="{sig_b64}""#
            )
        }

        #[test]
        fn verify_at_accepts_valid_rsa_signed_request() {
            let signer = RsaSigner::from_pkcs8_pem(ALICE_PRIVATE_KEY_PKCS8).expect("signer");
            let verifier =
                RsaVerifier::from_public_key_pem(ALICE_PUBLIC_KEY_SPKI).expect("verifier");
            let actor = "https://remote.example/users/alice";
            let key_id = "https://remote.example/users/alice#main-key";
            let body = sample_activity_bytes(actor);
            let digest = body_digest(&body);
            let date = "Thu, 12 Nov 2026 03:00:00 GMT";
            let mut request = SignedRequest {
                method: "POST",
                path: "/users/bob/inbox?ref=1",
                host: "local.example",
                date,
                digest: &digest,
                signature: "",
                body: &body,
            };
            let signature = sign_request(
                &signer,
                key_id,
                &request,
                "(request-target) host date digest",
            );
            request.signature = &signature;

            let activity = request
                .verify_at(
                    &verifier,
                    |kid| (kid == key_id).then(|| actor.to_owned()),
                    fixed_now(),
                    DEFAULT_MAX_CLOCK_SKEW,
                )
                .expect("valid signed request");
            assert_eq!(activity.actor(), actor);

            // Different header order and lowercase sha-256 algorithm prefix should also succeed.
            let lower_digest = digest.replacen("SHA-256=", "sha-256=", 1);
            let mut reordered = SignedRequest {
                digest: &lower_digest,
                signature: "",
                ..request
            };
            let reordered_sig = sign_request(
                &signer,
                key_id,
                &reordered,
                "digest date host (request-target)",
            );
            reordered.signature = &reordered_sig;
            reordered
                .verify_at(
                    &verifier,
                    |kid| (kid == key_id).then(|| actor.to_owned()),
                    fixed_now(),
                    DEFAULT_MAX_CLOCK_SKEW,
                )
                .expect("reordered headers with lowercase sha-256 digest");
        }

        #[test]
        fn verify_accepts_current_utc_timestamp() {
            let signer = RsaSigner::from_pkcs8_pem(ALICE_PRIVATE_KEY_PKCS8).expect("signer");
            let verifier =
                RsaVerifier::from_public_key_pem(ALICE_PUBLIC_KEY_SPKI).expect("verifier");
            let actor = "https://remote.example/users/alice";
            let key_id = "https://remote.example/users/alice#main-key";
            let body = sample_activity_bytes(actor);
            let digest = body_digest(&body);
            let date = Utc::now().to_rfc2822();
            let mut request = SignedRequest {
                method: "POST",
                path: "/inbox",
                host: "local.example",
                date: &date,
                digest: &digest,
                signature: "",
                body: &body,
            };
            let signature = sign_request(
                &signer,
                key_id,
                &request,
                "(request-target) host date digest",
            );
            request.signature = &signature;

            let activity = request
                .verify(
                    &verifier,
                    |kid| (kid == key_id).then(|| actor.to_owned()),
                    DEFAULT_MAX_CLOCK_SKEW,
                )
                .expect("verify against current Utc::now()");
            assert_eq!(activity.actor(), actor);
        }

        #[test]
        fn verify_at_rejects_body_tampering_and_digest_mismatch() {
            let signer = RsaSigner::from_pkcs8_pem(ALICE_PRIVATE_KEY_PKCS8).expect("signer");
            let verifier =
                RsaVerifier::from_public_key_pem(ALICE_PUBLIC_KEY_SPKI).expect("verifier");
            let actor = "https://remote.example/users/alice";
            let key_id = "https://remote.example/users/alice#main-key";
            let original_body = sample_activity_bytes(actor);
            let digest = body_digest(&original_body);
            let date = "Thu, 12 Nov 2026 03:00:00 GMT";
            let mut request = SignedRequest {
                method: "POST",
                path: "/inbox",
                host: "local.example",
                date,
                digest: &digest,
                signature: "",
                body: &original_body,
            };
            let signature = sign_request(
                &signer,
                key_id,
                &request,
                "(request-target) host date digest",
            );
            request.signature = &signature;

            // Tampered body with original signed Digest must fail Digest validation.
            let tampered_body = sample_activity_bytes("https://remote.example/users/evil");
            let tampered = SignedRequest {
                body: &tampered_body,
                ..request
            };
            let err = tampered
                .verify_at(
                    &verifier,
                    |_| Some(actor.to_owned()),
                    fixed_now(),
                    DEFAULT_MAX_CLOCK_SKEW,
                )
                .unwrap_err();
            assert!(matches!(err, DeliveryError::Signature(_)));
            assert!(matches!(
                tampered.validate_request_at(fixed_now(), DEFAULT_MAX_CLOCK_SKEW),
                Err(DeliveryError::Signature(_))
            ));

            // Weak / mismatched Digest algorithm signed by attacker must also fail.
            let weak_digest = "SHA-1=2jmj7l5rSw0yVb/vlWAYkK/YBwk=";
            let mut weak_req = SignedRequest {
                digest: weak_digest,
                signature: "",
                ..request
            };
            let weak_sig = sign_request(
                &signer,
                key_id,
                &weak_req,
                "(request-target) host date digest",
            );
            weak_req.signature = &weak_sig;
            let err = weak_req
                .verify_at(
                    &verifier,
                    |_| Some(actor.to_owned()),
                    fixed_now(),
                    DEFAULT_MAX_CLOCK_SKEW,
                )
                .unwrap_err();
            assert!(matches!(err, DeliveryError::Signature(_)));

            // Multi-digest header must be rejected.
            let multi_digest = format!("{digest},SHA-512=unused");
            let mut multi_req = SignedRequest {
                digest: &multi_digest,
                signature: "",
                ..request
            };
            let multi_sig = sign_request(
                &signer,
                key_id,
                &multi_req,
                "(request-target) host date digest",
            );
            multi_req.signature = &multi_sig;
            assert!(matches!(
                multi_req.verify_at(
                    &verifier,
                    |_| Some(actor.to_owned()),
                    fixed_now(),
                    DEFAULT_MAX_CLOCK_SKEW,
                ),
                Err(DeliveryError::Signature(_))
            ));
        }

        #[test]
        fn verify_at_rejects_replayed_target_or_host() {
            let signer = RsaSigner::from_pkcs8_pem(ALICE_PRIVATE_KEY_PKCS8).expect("signer");
            let verifier =
                RsaVerifier::from_public_key_pem(ALICE_PUBLIC_KEY_SPKI).expect("verifier");
            let actor = "https://remote.example/users/alice";
            let key_id = "https://remote.example/users/alice#main-key";
            let body = sample_activity_bytes(actor);
            let digest = body_digest(&body);
            let date = "Thu, 12 Nov 2026 03:00:00 GMT";
            let mut request = SignedRequest {
                method: "POST",
                path: "/users/bob/inbox",
                host: "local.example",
                date,
                digest: &digest,
                signature: "",
                body: &body,
            };
            let signature = sign_request(
                &signer,
                key_id,
                &request,
                "(request-target) host date digest",
            );
            request.signature = &signature;

            // Replayed to another path:
            let wrong_path = SignedRequest {
                path: "/users/carol/inbox",
                ..request
            };
            assert!(matches!(
                wrong_path.verify_at(
                    &verifier,
                    |_| Some(actor.to_owned()),
                    fixed_now(),
                    DEFAULT_MAX_CLOCK_SKEW,
                ),
                Err(DeliveryError::Signature(_))
            ));

            // Replayed to another host:
            let wrong_host = SignedRequest {
                host: "other.example",
                ..request
            };
            assert!(matches!(
                wrong_host.verify_at(
                    &verifier,
                    |_| Some(actor.to_owned()),
                    fixed_now(),
                    DEFAULT_MAX_CLOCK_SKEW,
                ),
                Err(DeliveryError::Signature(_))
            ));

            // Replayed with another HTTP method:
            let wrong_method = SignedRequest {
                method: "PUT",
                ..request
            };
            assert!(matches!(
                wrong_method.verify_at(
                    &verifier,
                    |_| Some(actor.to_owned()),
                    fixed_now(),
                    DEFAULT_MAX_CLOCK_SKEW,
                ),
                Err(DeliveryError::Signature(_))
            ));
        }

        #[test]
        fn verify_at_rejects_spoofed_actor_with_valid_rsa_signature() {
            let signer = RsaSigner::from_pkcs8_pem(ALICE_PRIVATE_KEY_PKCS8).expect("signer");
            let verifier =
                RsaVerifier::from_public_key_pem(ALICE_PUBLIC_KEY_SPKI).expect("verifier");
            let signer_actor = "https://attacker.example/users/mallory";
            let signer_key_id = "https://attacker.example/users/mallory#main-key";
            let spoofed_body = sample_activity_bytes("https://victim.example/users/alice");
            let digest = body_digest(&spoofed_body);
            let date = "Thu, 12 Nov 2026 03:00:00 GMT";
            let mut request = SignedRequest {
                method: "POST",
                path: "/inbox",
                host: "local.example",
                date,
                digest: &digest,
                signature: "",
                body: &spoofed_body,
            };
            let signature = sign_request(
                &signer,
                signer_key_id,
                &request,
                "(request-target) host date digest",
            );
            request.signature = &signature;

            let err = request
                .verify_at(
                    &verifier,
                    |kid| (kid == signer_key_id).then(|| signer_actor.to_owned()),
                    fixed_now(),
                    DEFAULT_MAX_CLOCK_SKEW,
                )
                .unwrap_err();
            assert!(matches!(err, DeliveryError::Signature(_)));
        }

        #[test]
        fn verify_at_rejects_invalid_activity_payload_with_valid_signature() {
            let signer = RsaSigner::from_pkcs8_pem(ALICE_PRIVATE_KEY_PKCS8).expect("signer");
            let verifier =
                RsaVerifier::from_public_key_pem(ALICE_PUBLIC_KEY_SPKI).expect("verifier");
            let key_id = "https://remote.example/users/alice#main-key";
            let body = b"{\"not\":\"an activity\"}";
            let digest = body_digest(body);
            let date = "Thu, 12 Nov 2026 03:00:00 GMT";
            let mut request = SignedRequest {
                method: "POST",
                path: "/inbox",
                host: "local.example",
                date,
                digest: &digest,
                signature: "",
                body,
            };
            let signature = sign_request(
                &signer,
                key_id,
                &request,
                "(request-target) host date digest",
            );
            request.signature = &signature;
            let err = request
                .verify_at(
                    &verifier,
                    |_| Some("https://remote.example/users/alice".to_owned()),
                    fixed_now(),
                    DEFAULT_MAX_CLOCK_SKEW,
                )
                .unwrap_err();
            assert!(matches!(err, DeliveryError::Payload(_)));
        }

        #[test]
        fn inbox_trait_accepts_signed_request_envelope() {
            struct TestInbox {
                verifier: RsaVerifier,
                actor: String,
                key_id: String,
                now: DateTime<Utc>,
            }

            impl Inbox for TestInbox {
                fn accept(
                    &self,
                    request: &SignedRequest<'_>,
                ) -> impl Future<Output = Result<Activity, DeliveryError>> + Send {
                    std::future::ready(request.verify_at(
                        &self.verifier,
                        |kid| (kid == self.key_id).then(|| self.actor.clone()),
                        self.now,
                        DEFAULT_MAX_CLOCK_SKEW,
                    ))
                }
            }

            let signer = RsaSigner::from_pkcs8_pem(ALICE_PRIVATE_KEY_PKCS8).expect("signer");
            let verifier =
                RsaVerifier::from_public_key_pem(ALICE_PUBLIC_KEY_SPKI).expect("verifier");
            let actor = "https://remote.example/users/alice";
            let key_id = "https://remote.example/users/alice#main-key";
            let inbox = TestInbox {
                verifier,
                actor: actor.to_owned(),
                key_id: key_id.to_owned(),
                now: fixed_now(),
            };

            let body = sample_activity_bytes(actor);
            let digest = body_digest(&body);
            let date = "Thu, 12 Nov 2026 03:00:00 GMT";
            let mut request = SignedRequest {
                method: "POST",
                path: "/inbox",
                host: "local.example",
                date,
                digest: &digest,
                signature: "",
                body: &body,
            };
            let signature = sign_request(
                &signer,
                key_id,
                &request,
                "(request-target) host date digest",
            );
            request.signature = &signature;

            let accepted = block_on(inbox.accept(&request)).expect("inbox accept");
            assert_eq!(accepted.actor(), actor);
        }
    }
}
