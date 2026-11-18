//! HTTP signatures for outbound requests.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use nodus_federation::signature::crypto::{RsaSigner, body_digest};
use nodus_federation::{HttpSignature, SignatureError, Signer};
use url::Url;

const ALGORITHM: &str = "rsa-sha256";
const REQUEST_TARGET: &str = "(request-target)";
const BODY_HEADERS: [&str; 4] = [REQUEST_TARGET, "host", "date", "digest"];
const EMPTY_HEADERS: [&str; 3] = [REQUEST_TARGET, "host", "date"];

/// Signs outbound requests with an RSA private key.
#[derive(Clone)]
pub struct HttpSigner {
    key_id: String,
    key: Arc<RsaSigner>,
}

impl std::fmt::Debug for HttpSigner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The wrapped key's `Debug` prints its private components.
        formatter
            .debug_struct("HttpSigner")
            .field("key_id", &self.key_id)
            .finish_non_exhaustive()
    }
}

impl HttpSigner {
    /// Build a signer from a PKCS#8 PEM private key and the key id it signs as.
    ///
    /// # Errors
    /// [`SignatureError::Crypto`] when the PEM is not a valid RSA private key.
    pub fn from_pkcs8_pem(key_id: impl Into<String>, pem: &str) -> Result<Self, SignatureError> {
        Ok(Self {
            key_id: key_id.into(),
            key: Arc::new(RsaSigner::from_pkcs8_pem(pem)?),
        })
    }

    /// The `keyId` this signer names in the `Signature` header.
    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Sign one request.
    ///
    /// A body digest is covered only when a body is passed, because a `Digest`
    /// header exists only on requests that carry one.
    ///
    /// # Errors
    /// [`SignatureError::Missing`] when the URL has no host, or any crypto failure.
    pub fn sign(
        &self,
        method: &str,
        url: &Url,
        body: Option<&[u8]>,
        date: DateTime<Utc>,
    ) -> Result<SignedHeaders, SignatureError> {
        let host = host_header(url).ok_or(SignatureError::Missing("host"))?;
        let target = request_target(method, url);
        let date = date.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let digest = body.map(body_digest);
        let headers: &[&str] = if digest.is_some() {
            &BODY_HEADERS
        } else {
            &EMPTY_HEADERS
        };
        let mut signature = HttpSignature {
            key_id: self.key_id.clone(),
            algorithm: ALGORITHM.to_owned(),
            headers: headers.iter().map(|name| (*name).to_owned()).collect(),
            signature: String::new(),
        };
        let message = signature.signing_string(|name| match name {
            REQUEST_TARGET => Some(target.clone()),
            "host" => Some(host.clone()),
            "date" => Some(date.clone()),
            "digest" => digest.clone(),
            _ => None,
        })?;
        signature.signature = self.key.sign(&self.key_id, ALGORITHM, message.as_bytes())?;
        Ok(SignedHeaders {
            host,
            date,
            digest,
            signature: signature.to_string(),
        })
    }
}

/// Headers carrying a request's HTTP signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedHeaders {
    /// `Host` header the signature covers.
    pub host: String,
    /// `Date` header the signature covers.
    pub date: String,
    /// `Digest` header, present only when the request has a body.
    pub digest: Option<String>,
    /// Complete `Signature` header value.
    pub signature: String,
}

impl SignedHeaders {
    /// Attach these headers to a request.
    pub fn apply(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let mut request = request
            .header("host", &self.host)
            .header("date", &self.date)
            .header("signature", &self.signature);
        if let Some(digest) = &self.digest {
            request = request.header("digest", digest);
        }
        request
    }
}

fn host_header(url: &Url) -> Option<String> {
    let host = url.host_str()?;
    Some(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    })
}

fn request_target(method: &str, url: &Url) -> String {
    let mut target = url.path().to_owned();
    if let Some(query) = url.query() {
        target.push('?');
        target.push_str(query);
    }
    format!("{} {target}", method.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodus_federation::Verifier;
    use nodus_federation::signature::crypto::RsaVerifier;

    const KEY_ID: &str = "https://example.social/users/a#main-key";

    const PRIVATE_KEY: &str = "\
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

    const PUBLIC_KEY: &str = "\
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

    fn signer() -> HttpSigner {
        HttpSigner::from_pkcs8_pem(KEY_ID, PRIVATE_KEY).expect("signer")
    }

    fn date() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-12T03:00:00Z")
            .expect("date")
            .with_timezone(&Utc)
    }

    fn inbox() -> Url {
        Url::parse("https://example.social/inbox").expect("url")
    }

    fn signature_of(signed: &SignedHeaders) -> HttpSignature {
        HttpSignature::parse(&signed.signature).expect("signature")
    }

    #[test]
    fn body_digest_is_deterministic() {
        let body = br#"{"type":"Create"}"#;
        let first = signer()
            .sign("POST", &inbox(), Some(body), date())
            .expect("sign");
        let second = signer()
            .sign("POST", &inbox(), Some(body), date())
            .expect("sign");

        assert_eq!(
            first.digest.as_deref(),
            Some("SHA-256=JeE18werLvQnEoHViKDam+ZK1D8E27TBC2kIISI7pIY=")
        );
        assert_eq!(first, second);
    }

    #[test]
    fn signature_names_algorithm_and_key_id() {
        let signed = signer()
            .sign("POST", &inbox(), Some(b"body"), date())
            .expect("sign");
        let signature = signature_of(&signed);

        assert_eq!(signature.key_id, KEY_ID);
        assert_eq!(signature.algorithm, "rsa-sha256");
        assert_eq!(
            signature.headers,
            ["(request-target)", "host", "date", "digest"]
        );
    }

    #[test]
    fn signature_verifies_over_target_host_date_and_digest() {
        let signed = signer()
            .sign("POST", &inbox(), Some(b"body"), date())
            .expect("sign");
        let signature = signature_of(&signed);
        let message = signature
            .signing_string(|name| match name {
                "(request-target)" => Some("post /inbox".to_owned()),
                "host" => Some("example.social".to_owned()),
                "date" => Some("Thu, 12 Nov 2026 03:00:00 GMT".to_owned()),
                "digest" => signed.digest.clone(),
                _ => None,
            })
            .expect("signing string");

        assert!(message.starts_with("(request-target): post /inbox\nhost: example.social\n"));
        RsaVerifier::from_public_key_pem(PUBLIC_KEY)
            .expect("verifier")
            .verify(
                KEY_ID,
                "rsa-sha256",
                message.as_bytes(),
                &signature.signature,
            )
            .expect("verify");
    }

    #[test]
    fn a_request_without_a_body_carries_no_digest() {
        let url = Url::parse("https://example.social/users/a").expect("url");
        let signed = signer().sign("GET", &url, None, date()).expect("sign");

        assert!(signed.digest.is_none());
        assert_eq!(
            signature_of(&signed).headers,
            ["(request-target)", "host", "date"]
        );

        let request = signed
            .apply(reqwest::Client::new().get(url))
            .build()
            .expect("request");
        assert!(request.headers().get("digest").is_none());
        assert_eq!(request.headers()["signature"], signed.signature.as_str());
    }

    #[test]
    fn applies_a_digest_when_a_body_is_signed() {
        let signed = signer()
            .sign("POST", &inbox(), Some(b"body"), date())
            .expect("sign");
        let request = signed
            .apply(reqwest::Client::new().post(inbox()))
            .build()
            .expect("request");

        assert_eq!(request.headers()["digest"], signed.digest.unwrap());
        assert_eq!(request.headers()["host"], "example.social");
        assert_eq!(request.headers()["date"], "Thu, 12 Nov 2026 03:00:00 GMT");
    }
}
