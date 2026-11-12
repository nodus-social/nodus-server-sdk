//! Shared-secret authentication for internal service calls.
//!
//! Every gRPC service validates the secret on every RPC. In-process adapters do
//! not carry a secret; the moment a call crosses a process boundary it must use
//! [`SharedSecret::verify`] in the server interceptor and [`SharedSecret::attach`]
//! on the client.

use std::fmt;

use thiserror::Error;

/// Metadata key carrying the shared secret on internal gRPC calls.
pub const SHARED_SECRET_HEADER: &str = "x-nodus-service-secret";

/// A service-to-service shared secret.
#[derive(Clone, PartialEq, Eq)]
pub struct SharedSecret(String);

impl SharedSecret {
    /// Build from a configured value. Empty secrets are rejected so a missing
    /// environment variable cannot silently disable authentication.
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if value.is_empty() {
            return Err(Error::Empty);
        }
        Ok(Self(value))
    }

    /// Read a secret from the environment.
    pub fn from_env(name: &str) -> Result<Self, Error> {
        let value = std::env::var(name).map_err(|_| Error::Missing(name.to_owned()))?;
        Self::new(value)
    }

    /// The metadata value to attach to an outgoing call.
    #[must_use]
    pub fn attach(&self) -> &str {
        &self.0
    }

    /// Validate the value presented by a peer in constant time.
    pub fn verify(&self, presented: Option<&str>) -> Result<(), Error> {
        let Some(presented) = presented else {
            return Err(Error::Unauthenticated);
        };
        let expected = self.0.as_bytes();
        let actual = presented.as_bytes();
        let mut diff = u8::from(expected.len() != actual.len());
        for (left, right) in expected.iter().zip(actual) {
            diff |= left ^ right;
        }
        if diff == 0 {
            Ok(())
        } else {
            Err(Error::Unauthenticated)
        }
    }
}

impl fmt::Debug for SharedSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SharedSecret(<redacted>)")
    }
}

/// gRPC transport integration.
///
/// The interceptor attaches the secret on the client and verifies it on the
/// server. The secret never appears in a status message or log line.
#[cfg(feature = "grpc")]
pub mod grpc {
    use tonic::metadata::MetadataValue;
    use tonic::service::Interceptor;
    use tonic::{Request, Status};

    use super::{SHARED_SECRET_HEADER, SharedSecret};

    /// Client interceptor that attaches the configured secret to every call.
    #[derive(Clone, Debug)]
    pub struct ClientSecret {
        secret: SharedSecret,
    }

    impl ClientSecret {
        /// Build a client interceptor.
        #[must_use]
        pub fn new(secret: SharedSecret) -> Self {
            Self { secret }
        }
    }

    impl Interceptor for ClientSecret {
        fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
            let value: MetadataValue<tonic::metadata::Ascii> = self
                .secret
                .attach()
                .parse()
                .map_err(|_| Status::internal("service secret is not valid metadata"))?;
            request.metadata_mut().insert(SHARED_SECRET_HEADER, value);
            Ok(request)
        }
    }

    /// Server-side check for one request.
    pub fn verify<T>(secret: &SharedSecret, request: &Request<T>) -> Result<(), Status> {
        let presented = request
            .metadata()
            .get(SHARED_SECRET_HEADER)
            .and_then(|value| value.to_str().ok());
        secret
            .verify(presented)
            .map_err(|_| Status::unauthenticated("service is not authenticated"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_client_attaches_the_secret_and_the_server_verifies_it() {
            let secret = SharedSecret::new("s3cret").expect("secret");
            let mut interceptor = ClientSecret::new(secret.clone());
            let request = interceptor.call(Request::new(())).expect("call");
            assert!(verify(&secret, &request).is_ok());
        }

        #[test]
        fn a_request_without_the_secret_is_rejected() {
            let secret = SharedSecret::new("s3cret").expect("secret");
            assert!(verify(&secret, &Request::new(())).is_err());
        }
    }
}

/// Why service authentication failed.
#[derive(Debug, Error)]
pub enum Error {
    /// No secret was configured.
    #[error("service secret is empty")]
    Empty,
    /// The configured environment variable is absent.
    #[error("service secret {0} is not set")]
    Missing(String),
    /// The peer did not present the expected secret.
    #[error("service is not authenticated")]
    Unauthenticated,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_matching_secret_is_accepted() {
        let secret = SharedSecret::new("s3cret").expect("secret");
        assert!(secret.verify(Some("s3cret")).is_ok());
    }

    #[test]
    fn a_missing_or_wrong_secret_is_rejected() {
        let secret = SharedSecret::new("s3cret").expect("secret");
        assert!(matches!(secret.verify(None), Err(Error::Unauthenticated)));
        assert!(matches!(
            secret.verify(Some("other")),
            Err(Error::Unauthenticated)
        ));
        assert!(matches!(
            secret.verify(Some("s3cret ")),
            Err(Error::Unauthenticated)
        ));
    }

    #[test]
    fn an_empty_secret_is_rejected() {
        assert!(matches!(SharedSecret::new(""), Err(Error::Empty)));
    }
}
