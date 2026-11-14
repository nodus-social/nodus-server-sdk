//! The HTTP `Signature` envelope.
//!
//! Signing and verification are ports: an instance chooses its key custody and crypto
//! provider. The crate owns the header grammar and the canonical signing string.

use std::fmt;

/// Why a signature header or signing string was invalid.
#[derive(Debug, thiserror::Error)]
pub enum SignatureError {
    /// A required parameter is missing.
    #[error("signature is missing {0}")]
    Missing(&'static str),
    /// The header is not `key="value"` pairs.
    #[error("malformed signature header")]
    Malformed,
    /// The algorithm is not supported by the caller.
    #[error("unsupported signature algorithm: {0}")]
    Algorithm(String),
    /// The signing string has no headers.
    #[error("signature has no headers")]
    Empty,
    /// Key material or a cryptographic operation failed.
    #[error("signature crypto failed: {0}")]
    Crypto(String),
}

/// A parsed `Signature` / `Authorization: Signature` header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpSignature {
    /// Key id, normally an actor's `#main-key`.
    pub key_id: String,
    /// `rsa-sha256`, `ed25519`, etc.
    pub algorithm: String,
    /// Header names in signing order.
    pub headers: Vec<String>,
    /// Base64 signature.
    pub signature: String,
}

impl HttpSignature {
    /// Parse the parameters of an HTTP signature header.
    ///
    /// # Errors
    /// [`SignatureError::Malformed`] when a parameter is not quoted `key="value"`.
    pub fn parse(header: &str) -> Result<Self, SignatureError> {
        let header = strip_signature_scheme(header);
        let mut key_id = None;
        let mut algorithm = None;
        let mut headers = None;
        let mut signature = None;
        for part in header.split(',') {
            let (name, value) = part.split_once('=').ok_or(SignatureError::Malformed)?;
            let value = value.trim().trim_matches('"').to_owned();
            match name.trim() {
                "keyId" => key_id = Some(value),
                "algorithm" => algorithm = Some(value),
                "headers" => {
                    let parsed: Vec<String> = value
                        .split_whitespace()
                        .map(str::to_ascii_lowercase)
                        .collect();
                    if parsed.is_empty() {
                        return Err(SignatureError::Empty);
                    }
                    headers = Some(parsed);
                }
                "signature" => signature = Some(value),
                _ => {}
            }
        }
        Ok(Self {
            key_id: key_id
                .filter(|id| !id.is_empty())
                .ok_or(SignatureError::Missing("keyId"))?,
            algorithm: algorithm.unwrap_or_else(|| "rsa-sha256".to_owned()),
            headers: headers.unwrap_or_else(|| vec!["date".to_owned()]),
            signature: signature
                .filter(|sig| !sig.is_empty())
                .ok_or(SignatureError::Missing("signature"))?,
        })
    }

    /// The canonical string a signer signs.
    ///
    /// `request` is called with each lower-case header name in the declared order.
    ///
    /// # Errors
    /// [`SignatureError::Empty`] when no headers were declared.
    pub fn signing_string<F>(&self, mut request: F) -> Result<String, SignatureError>
    where
        F: FnMut(&str) -> Option<String>,
    {
        if self.headers.is_empty() {
            return Err(SignatureError::Empty);
        }
        let mut lines = Vec::with_capacity(self.headers.len());
        for header in &self.headers {
            let lower;
            let name = if header.bytes().any(|b| b.is_ascii_uppercase()) {
                lower = header.to_ascii_lowercase();
                lower.as_str()
            } else {
                header.as_str()
            };
            let value = request(name).ok_or(SignatureError::Malformed)?;
            lines.push(format!("{name}: {value}"));
        }
        Ok(lines.join("\n"))
    }
}

fn strip_signature_scheme(header: &str) -> &str {
    let trimmed = header.trim();
    trimmed
        .split_once(char::is_whitespace)
        .filter(|(prefix, _)| prefix.eq_ignore_ascii_case("Signature"))
        .map_or(trimmed, |(_, rest)| rest.trim_start())
}

impl fmt::Display for HttpSignature {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "keyId=\"{}\",algorithm=\"{}\",headers=\"{}\",signature=\"{}\"",
            self.key_id,
            self.algorithm,
            self.headers.join(" "),
            self.signature
        )
    }
}

/// A signer supplied by the host.
pub trait Signer: Send + Sync {
    /// Sign a canonical string.
    fn sign(&self, key_id: &str, algorithm: &str, message: &[u8])
    -> Result<String, SignatureError>;
}

/// A verifier supplied by the host.
pub trait Verifier: Send + Sync {
    /// Verify a canonical string.
    fn verify(
        &self,
        key_id: &str,
        algorithm: &str,
        message: &[u8],
        signature: &str,
    ) -> Result<(), SignatureError>;
}

/// RSA-SHA256 signing and verification.
///
/// Key custody stays with the host: this type only wraps a parsed key, it does
/// not fetch or persist one.
#[cfg(feature = "crypto")]
pub mod crypto {
    use base64::Engine as _;
    use rsa::pkcs1::DecodeRsaPublicKey;
    use rsa::pkcs1v15::{Signature, SigningKey, VerifyingKey};
    use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
    use rsa::signature::{SignatureEncoding, Signer as _, Verifier as _};
    use sha2::{Digest, Sha256};

    use super::{SignatureError, Signer, Verifier};

    /// An RSA private key that signs `rsa-sha256`.
    #[derive(Debug)]
    pub struct RsaSigner {
        key: SigningKey<Sha256>,
    }

    impl RsaSigner {
        /// Parse a PKCS#8 PEM private key.
        ///
        /// # Errors
        /// [`SignatureError::Crypto`] when the PEM or key is invalid.
        pub fn from_pkcs8_pem(pem: &str) -> Result<Self, SignatureError> {
            let key = rsa::RsaPrivateKey::from_pkcs8_pem(pem)
                .map_err(|error| SignatureError::Crypto(error.to_string()))?;
            Ok(Self {
                key: SigningKey::new(key),
            })
        }
    }

    impl Signer for RsaSigner {
        fn sign(
            &self,
            _key_id: &str,
            algorithm: &str,
            message: &[u8],
        ) -> Result<String, SignatureError> {
            if algorithm != "rsa-sha256" {
                return Err(SignatureError::Algorithm(algorithm.to_owned()));
            }
            let signature = self.key.sign(message);
            Ok(base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()))
        }
    }

    /// An RSA public key that verifies `rsa-sha256`.
    #[derive(Debug)]
    pub struct RsaVerifier {
        key: VerifyingKey<Sha256>,
    }

    impl RsaVerifier {
        /// Parse an SPKI (`BEGIN PUBLIC KEY`) or PKCS#1 (`BEGIN RSA PUBLIC KEY`) PEM public key.
        ///
        /// # Errors
        /// [`SignatureError::Crypto`] when the PEM or key is invalid.
        pub fn from_public_key_pem(pem: &str) -> Result<Self, SignatureError> {
            let key = rsa::RsaPublicKey::from_public_key_pem(pem)
                .or_else(|_| rsa::RsaPublicKey::from_pkcs1_pem(pem))
                .map_err(|error| SignatureError::Crypto(error.to_string()))?;
            Ok(Self {
                key: VerifyingKey::new(key),
            })
        }
    }

    impl Verifier for RsaVerifier {
        fn verify(
            &self,
            _key_id: &str,
            algorithm: &str,
            message: &[u8],
            signature: &str,
        ) -> Result<(), SignatureError> {
            if algorithm != "rsa-sha256" {
                return Err(SignatureError::Algorithm(algorithm.to_owned()));
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(signature)
                .map_err(|error| SignatureError::Crypto(error.to_string()))?;
            let signature = Signature::try_from(bytes.as_slice())
                .map_err(|error| SignatureError::Crypto(error.to_string()))?;
            self.key
                .verify(message, &signature)
                .map_err(|error| SignatureError::Crypto(error.to_string()))
        }
    }

    /// The `Digest` header value for a request body.
    #[must_use]
    pub fn body_digest(body: &[u8]) -> String {
        let digest = Sha256::digest(body);
        format!(
            "SHA-256={}",
            base64::engine::general_purpose::STANDARD.encode(digest)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_mastodon_signature_header() {
        let signature = HttpSignature::parse(
            r#"keyId="https://example.social/users/a#main-key",algorithm="rsa-sha256",headers="(request-target) host date digest",signature="abc""#,
        )
        .expect("signature");
        assert_eq!(signature.key_id, "https://example.social/users/a#main-key");
        assert_eq!(signature.algorithm, "rsa-sha256");
        assert_eq!(
            signature.headers,
            ["(request-target)", "host", "date", "digest"]
        );
        assert_eq!(signature.signature, "abc");
    }

    #[test]
    fn canonical_signing_string_keeps_header_order() {
        let signature =
            HttpSignature::parse(r#"keyId="k",headers="(request-target) host date",signature="s""#)
                .expect("signature");
        let message = signature
            .signing_string(|name| {
                Some(match name {
                    "(request-target)" => "post /inbox".to_owned(),
                    "host" => "example.social".to_owned(),
                    "date" => "Tue, 12 Nov 2026 03:00:00 GMT".to_owned(),
                    _ => return None,
                })
            })
            .expect("message");
        assert_eq!(
            message,
            "(request-target): post /inbox\nhost: example.social\ndate: Tue, 12 Nov 2026 03:00:00 GMT"
        );
    }

    #[test]
    fn display_round_trips_through_parse() {
        let header = r#"keyId="k",algorithm="rsa-sha256",headers="host date",signature="s""#;
        let signature = HttpSignature::parse(header).expect("signature");
        let parsed = HttpSignature::parse(&signature.to_string()).expect("round trip");
        assert_eq!(signature, parsed);
    }

    #[test]
    fn parses_authorization_signature_prefix() {
        let signature = HttpSignature::parse(
            r#"Signature keyId="https://example.social/users/a#main-key",algorithm="rsa-sha256",headers="(request-target) host date",signature="abc""#,
        )
        .expect("Signature-prefixed header");
        assert_eq!(signature.key_id, "https://example.social/users/a#main-key");
        assert_eq!(signature.headers, ["(request-target)", "host", "date"]);
        assert_eq!(signature.signature, "abc");

        let lowercase_prefix =
            HttpSignature::parse(r#"  signature   keyId="k",headers="date",signature="s""#)
                .expect("lowercase signature prefix with whitespace");
        assert_eq!(lowercase_prefix.key_id, "k");
        assert_eq!(lowercase_prefix.headers, ["date"]);
    }

    #[test]
    fn lowercases_header_names_on_parse_and_signing_string() {
        let signature = HttpSignature::parse(
            r#"keyId="k",headers="(Request-Target) Host Date Digest",signature="s""#,
        )
        .expect("signature");
        assert_eq!(
            signature.headers,
            ["(request-target)", "host", "date", "digest"]
        );

        let manual = HttpSignature {
            key_id: "k".to_owned(),
            algorithm: "rsa-sha256".to_owned(),
            headers: vec!["Host".to_owned(), "Date".to_owned()],
            signature: "s".to_owned(),
        };
        let mut seen = Vec::new();
        let message = manual
            .signing_string(|name| {
                seen.push(name.to_owned());
                Some(match name {
                    "host" => "example.social".to_owned(),
                    "date" => "Tue, 12 Nov 2026 03:00:00 GMT".to_owned(),
                    _ => return None,
                })
            })
            .expect("signing string");
        assert_eq!(seen, ["host", "date"]);
        assert_eq!(
            message,
            "host: example.social\ndate: Tue, 12 Nov 2026 03:00:00 GMT"
        );
    }

    #[test]
    fn rejects_empty_headers_parameter() {
        let err = HttpSignature::parse(r#"keyId="k",headers="",signature="s""#).unwrap_err();
        assert!(matches!(err, SignatureError::Empty));

        let err = HttpSignature::parse(r#"keyId="k",headers="   ",signature="s""#).unwrap_err();
        assert!(matches!(err, SignatureError::Empty));
    }

    #[test]
    fn defaults_omitted_headers_to_date() {
        let signature = HttpSignature::parse(r#"keyId="k",signature="s""#).expect("signature");
        assert_eq!(signature.headers, ["date"]);
    }

    #[test]
    fn rejects_empty_key_id_or_signature() {
        let err = HttpSignature::parse(r#"keyId="",signature="s""#).unwrap_err();
        assert!(matches!(err, SignatureError::Missing("keyId")));

        let err = HttpSignature::parse(r#"keyId="k",signature="""#).unwrap_err();
        assert!(matches!(err, SignatureError::Missing("signature")));
    }

    #[cfg(feature = "crypto")]
    #[test]
    fn verifies_spki_and_pkcs1_public_keys() {
        use super::crypto::{RsaSigner, RsaVerifier};

        const TEST_PRIVATE_KEY_PKCS8: &str = "\
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

        const TEST_PUBLIC_KEY_SPKI: &str = "\
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

        const TEST_PUBLIC_KEY_PKCS1: &str = "\
-----BEGIN RSA PUBLIC KEY-----
MIIBCgKCAQEAr3hs6l/6aW86iWNI4rqotRitk6EI9WMdR3q7b6ECxQTf03FdRfLD
3m5oOoDB27mGUCYWfoCyfIMPSGT9dJ/sCkvFIFiucpZUMf16F+nUC1XnZ/6n06sd
XoInxnBGuqHajjzlYhqnKocuEZH451fgw0DGv+PNISIl1q9HB5qjPImnh2HuopVq
+xbjH0iSZcS31nnQF1GnMQY7DgYF+sLQTaWLUMsgqmhCvSoDwro/cFMeWyEUE0tK
vmtBLn0ng8ckPN+CsZnOUxq+3qKMivqOMAqH5PHGGzifV6NyWWkn/s4fXFUmSayM
r64BoKdpR8YlbcOnIBBVrvwrPLlm/+DMTwIDAQAB
-----END RSA PUBLIC KEY-----
";

        let signer = RsaSigner::from_pkcs8_pem(TEST_PRIVATE_KEY_PKCS8).expect("signer");
        let message = b"(request-target): post /inbox\nhost: example.social\ndate: Tue, 12 Nov 2026 03:00:00 GMT";
        let signature = signer
            .sign(
                "https://example.social/users/a#main-key",
                "rsa-sha256",
                message,
            )
            .expect("sign");

        let spki_verifier =
            RsaVerifier::from_public_key_pem(TEST_PUBLIC_KEY_SPKI).expect("spki verifier");
        spki_verifier
            .verify(
                "https://example.social/users/a#main-key",
                "rsa-sha256",
                message,
                &signature,
            )
            .expect("verify with spki");

        let pkcs1_verifier =
            RsaVerifier::from_public_key_pem(TEST_PUBLIC_KEY_PKCS1).expect("pkcs1 verifier");
        pkcs1_verifier
            .verify(
                "https://example.social/users/a#main-key",
                "rsa-sha256",
                message,
                &signature,
            )
            .expect("verify with pkcs1");

        let err = RsaVerifier::from_public_key_pem("not a pem").unwrap_err();
        assert!(matches!(err, SignatureError::Crypto(_)));
    }
}
