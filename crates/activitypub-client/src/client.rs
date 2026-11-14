//! Public reads over HTTPS. No signing, no delivery.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;

use serde_json::Value;
use url::Url;

use nodus_activitypub_core::actor::link;
use nodus_activitypub_core::{Actor, Collection, CollectionPage, Error, Note};

/// Maximum permitted HTTP response body size (4 MiB).
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// A read-only ActivityPub client.
#[derive(Debug, Clone)]
pub struct ActivityPub {
    client: reqwest::Client,
}

impl Default for ActivityPub {
    fn default() -> Self {
        Self::new()
    }
}

impl ActivityPub {
    /// A client with a short timeout, an ActivityPub accept header, and SSRF protections.
    #[must_use]
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("nodus-activitypub/0.1")
            .redirect(safe_redirect_policy())
            .dns_resolver(Arc::new(SafeDnsResolver))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    /// Construct an ActivityPub client using an existing `reqwest::Client`.
    #[must_use]
    pub fn from_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    async fn get(&self, url: &str) -> Result<Value, Error> {
        check(url)?;
        let response = self
            .client
            .get(url)
            .header("accept", "application/activity+json, application/ld+json")
            .send()
            .await
            .map_err(|error| Error::Transport(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::Status {
                status: status.as_u16(),
                url: url.to_owned(),
            });
        }
        read_json_limited(response, MAX_RESPONSE_BYTES).await
    }

    /// Resolve `user@host` to its actor URL.
    ///
    /// # Errors
    /// [`Error::Handle`] for a malformed handle, [`Error::NoActor`] when WebFinger has no
    /// ActivityPub link.
    pub async fn webfinger(&self, acct: &str) -> Result<String, Error> {
        let handle = acct.strip_prefix('@').unwrap_or(acct);
        let (user, host) = handle
            .split_once('@')
            .filter(|(user, host)| !user.is_empty() && !host.is_empty())
            .ok_or_else(|| Error::Handle(acct.to_owned()))?;
        validate_webfinger_host(host).map_err(|()| Error::Handle(acct.to_owned()))?;
        let resource = format!("acct:{user}@{host}");
        let url = format!(
            "https://{host}/.well-known/webfinger?resource={}",
            urlencoding::encode(&resource)
        );
        let value = self.get(&url).await?;
        value
            .get("links")
            .and_then(Value::as_array)
            .and_then(|links| {
                links.iter().find(|link| {
                    link.get("rel").and_then(Value::as_str) == Some("self")
                        && link
                            .get("type")
                            .and_then(Value::as_str)
                            .is_some_and(|kind| kind.contains("activity"))
                })
            })
            .and_then(|link| link.get("href"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| Error::NoActor(acct.to_owned()))
    }

    /// Read an actor by handle.
    ///
    /// # Errors
    /// Propagates WebFinger and transport errors.
    pub async fn actor(&self, acct: &str) -> Result<Actor, Error> {
        let url = self.webfinger(acct).await?;
        self.actor_url(&url).await
    }

    /// Read an actor by actor URL.
    ///
    /// # Errors
    /// Propagates transport and object errors.
    pub async fn actor_url(&self, url: &str) -> Result<Actor, Error> {
        Actor::from_json(&self.get(url).await?)
    }

    /// Read a collection header.
    ///
    /// # Errors
    /// Propagates transport and object errors.
    pub async fn collection(&self, url: &str) -> Result<Collection, Error> {
        Collection::from_json(&self.get(url).await?)
    }

    /// Read a collection page.
    ///
    /// # Errors
    /// Propagates transport and object errors.
    pub async fn page(&self, url: &str) -> Result<CollectionPage, Error> {
        CollectionPage::from_json(&self.get(url).await?)
    }

    /// Read the first page of an actor's outbox.
    ///
    /// # Errors
    /// [`Error::Object`] when the actor publishes no outbox.
    pub async fn outbox(&self, actor: &Actor) -> Result<CollectionPage, Error> {
        let url = actor
            .outbox
            .as_deref()
            .ok_or_else(|| Error::Object("actor has no outbox".to_owned()))?;
        self.first_page(url).await
    }

    /// Read the first page of a collection, following `first` when needed.
    ///
    /// # Errors
    /// Propagates transport and object errors.
    pub async fn first_page(&self, url: &str) -> Result<CollectionPage, Error> {
        let value = self.get(url).await?;
        if value.get("orderedItems").is_some() || value.get("items").is_some() {
            return CollectionPage::from_json(&value);
        }
        let collection = Collection::from_json(&value)?;
        match collection.first {
            Some(first) => self.page(&first).await,
            None => Ok(CollectionPage {
                items: Vec::new(),
                next: None,
                total_items: collection.total_items,
            }),
        }
    }

    /// Read a note by URL.
    ///
    /// # Errors
    /// Propagates transport and object errors.
    pub async fn note(&self, url: &str) -> Result<Note, Error> {
        Note::from_json(&self.get(url).await?)
    }

    /// Read the reply collection of a note, one page.
    ///
    /// # Errors
    /// [`Error::Object`] when the note has no replies collection.
    pub async fn replies(&self, note: &Note) -> Result<CollectionPage, Error> {
        let url = note
            .replies
            .as_deref()
            .ok_or_else(|| Error::Object("note has no replies collection".to_owned()))?;
        self.first_page(url).await
    }
}

/// Validate that a host string in a WebFinger handle is a valid, publicly routable domain or IP.
///
/// Ports, slashes, backslashes, query parameters, fragments, userinfo `@`, whitespace, and private
/// destinations are rejected.
fn validate_webfinger_host(host: &str) -> Result<(), ()> {
    if host.is_empty()
        || host.contains(|c: char| {
            c == '/'
                || c == '\\'
                || c == '?'
                || c == '#'
                || c == ':'
                || c == '@'
                || c.is_whitespace()
        })
    {
        return Err(());
    }
    let parsed = url::Host::parse(host).map_err(|_| ())?;
    match parsed {
        url::Host::Domain(domain) if !is_disallowed_domain(&domain) => Ok(()),
        url::Host::Ipv4(v4) if !is_disallowed_ipv4(v4) => Ok(()),
        url::Host::Ipv6(v6) if !is_disallowed_ipv6(v6) => Ok(()),
        _ => Err(()),
    }
}

/// Whether an IPv4 address belongs to a loopback, private, link-local, broadcast, multicast,
/// or reserved range.
fn is_disallowed_ipv4(v4: Ipv4Addr) -> bool {
    let octets = v4.octets();
    v4.is_loopback() // 127.0.0.0/8 (RFC 1122)
        || v4.is_private() // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16 (RFC 1918)
        || v4.is_link_local() // 169.254.0.0/16 (RFC 3927)
        || v4.is_unspecified() // 0.0.0.0 (RFC 1122)
        || v4.is_broadcast() // 255.255.255.255 (RFC 919)
        || v4.is_multicast() // 224.0.0.0/4 (RFC 5771)
        || octets[0] == 0 // 0.0.0.0/8 Current network (RFC 1122)
        || (octets[0] == 100 && (octets[1] & 0xc0) == 64) // 100.64.0.0/10 Shared address space / CGNAT (RFC 6598)
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0) // 192.0.0.0/24 IETF Protocol Assignments (RFC 6890)
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 2) // 192.0.2.0/24 TEST-NET-1 (RFC 5737)
        || (octets[0] == 198 && (octets[1] & 0xfe) == 18) // 198.18.0.0/15 Benchmarking (RFC 2544)
        || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100) // 198.51.100.0/24 TEST-NET-2 (RFC 5737)
        || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113) // 203.0.113.0/24 TEST-NET-3 (RFC 5737)
        || octets[0] >= 240 // 240.0.0.0/4 Reserved (Class E) (RFC 1112)
}

/// Whether an IPv6 address belongs to a loopback, unique local, link-local, multicast,
/// or reserved range (including IPv4-mapped/translated variants).
fn is_disallowed_ipv6(v6: Ipv6Addr) -> bool {
    if let Some(v4) = v6.to_ipv4() {
        if is_disallowed_ipv4(v4) {
            return true;
        }
    }
    let segments = v6.segments();
    // 64:ff9b::/96 IPv4/IPv6 translation (RFC 6052)
    if segments[0] == 0x0064
        && segments[1] == 0xff9b
        && segments[2] == 0
        && segments[3] == 0
        && segments[4] == 0
        && segments[5] == 0
    {
        let octets = v6.octets();
        let v4 = Ipv4Addr::new(octets[12], octets[13], octets[14], octets[15]);
        if is_disallowed_ipv4(v4) {
            return true;
        }
    }
    v6.is_loopback() // ::1 (RFC 4291)
        || v6.is_unspecified() // :: (RFC 4291)
        || v6.is_multicast() // ff00::/8 (RFC 4291)
        // Unique Local Address fc00::/7 (RFC 4193)
        || (segments[0] & 0xfe00) == 0xfc00
        // Link-Local Unicast fe80::/10 (RFC 4291)
        || (segments[0] & 0xffc0) == 0xfe80
        // Documentation 2001:db8::/32 (RFC 3849)
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
        // Discard prefix 100::/64 (RFC 6666)
        || segments[0] == 0x0100
}

/// Whether an IP address is disallowed.
fn is_disallowed_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_disallowed_ipv4(v4),
        IpAddr::V6(v6) => is_disallowed_ipv6(v6),
    }
}

/// Whether a domain name points to a local, loopback, or private destination.
fn is_disallowed_domain(domain: &str) -> bool {
    let d = domain.trim().trim_end_matches('.');
    if d.is_empty() {
        return true;
    }
    d.eq_ignore_ascii_case("localhost")
        || d.to_ascii_lowercase().ends_with(".localhost")
        || d.eq_ignore_ascii_case("local")
        || d.to_ascii_lowercase().ends_with(".local")
        || d.eq_ignore_ascii_case("internal")
        || d.to_ascii_lowercase().ends_with(".internal")
}

/// Check if a parsed URL host is allowed.
fn check_url_host(host: &url::Host<&str>) -> bool {
    match host {
        url::Host::Domain(domain) => !is_disallowed_domain(domain),
        url::Host::Ipv4(v4) => !is_disallowed_ipv4(*v4),
        url::Host::Ipv6(v6) => !is_disallowed_ipv6(*v6),
    }
}

/// Validate that a URL is an absolute HTTPS URL pointing to a publicly routable destination.
///
/// # Errors
/// [`Error::Url`] if the URL is not HTTPS, is malformed, lacks a host, or points to a loopback,
/// private, link-local, unspecified, or local destination.
pub fn check(url: &str) -> Result<(), Error> {
    let parsed = Url::parse(url).map_err(|_| Error::Url(url.to_owned()))?;
    if parsed.scheme() != "https" {
        return Err(Error::Url(url.to_owned()));
    }
    match parsed.host() {
        Some(ref host) if check_url_host(host) => Ok(()),
        _ => Err(Error::Url(url.to_owned())),
    }
}

/// Validate a redirect URL and hop count.
pub(crate) fn validate_redirect(url: &Url, previous_count: usize) -> Result<(), &'static str> {
    if previous_count >= 10 {
        return Err("too many redirects");
    }
    if url.scheme() != "https" {
        return Err("redirect to non-https URL is forbidden");
    }
    match url.host() {
        Some(ref host) if check_url_host(host) => Ok(()),
        Some(_) => Err("redirect to private or local destination is forbidden"),
        None => Err("redirect URL missing host"),
    }
}

/// A reqwest redirect policy that enforces HTTPS and forbids internal/private destinations.
pub(crate) fn safe_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        match validate_redirect(attempt.url(), attempt.previous().len()) {
            Ok(()) => attempt.follow(),
            Err(err) => attempt.error(err),
        }
    })
}

/// Validate resolved socket addresses from DNS lookup.
pub(crate) fn validate_resolved_addrs(
    addrs: Vec<SocketAddr>,
) -> Result<Vec<SocketAddr>, &'static str> {
    if addrs.is_empty() {
        return Err("SSRF protection: no IP addresses found");
    }
    for addr in &addrs {
        if is_disallowed_ip(addr.ip()) {
            return Err("SSRF protection: host resolved to private or local IP address");
        }
    }
    Ok(addrs)
}

/// A custom DNS resolver for reqwest that blocks private, loopback, link-local, and unspecified IPs.
#[derive(Clone, Default, Debug)]
pub(crate) struct SafeDnsResolver;

impl reqwest::dns::Resolve for SafeDnsResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            if is_disallowed_domain(&host) {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "SSRF protection: domain is forbidden",
                ))
                    as Box<dyn std::error::Error + Send + Sync>);
            }
            if let Ok(ip) = host.parse::<IpAddr>() {
                if is_disallowed_ip(ip) {
                    return Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "SSRF protection: IP address is forbidden",
                    ))
                        as Box<dyn std::error::Error + Send + Sync>);
                }
            }
            let addrs: Vec<SocketAddr> = ToSocketAddrs::to_socket_addrs(&(host.as_str(), 0))
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
                .collect();
            let safe_addrs = validate_resolved_addrs(addrs).map_err(|err| {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    err,
                )) as Box<dyn std::error::Error + Send + Sync>
            })?;
            let iter: reqwest::dns::Addrs = Box::new(safe_addrs.into_iter());
            Ok(iter)
        })
    }
}

/// Read and parse a JSON response body without exceeding `max_bytes`.
async fn read_json_limited(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> Result<Value, Error> {
    let max_u64 = u64::try_from(max_bytes).unwrap_or(u64::MAX);
    let content_length = response.content_length();
    if let Some(len) = content_length.filter(|&len| len > max_u64) {
        return Err(Error::Transport(format!(
            "response body of {len} bytes exceeds {max_bytes} byte limit"
        )));
    }

    let initial_cap = content_length
        .and_then(|len| usize::try_from(len).ok())
        .map_or(0, |len| len.min(64 * 1024).min(max_bytes));
    let mut body = Vec::with_capacity(initial_cap);

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| Error::Transport(error.to_string()))?
    {
        if chunk.len() > max_bytes - body.len() {
            return Err(Error::Transport(format!(
                "response body exceeded {max_bytes} byte limit"
            )));
        }
        body.extend_from_slice(&chunk);
    }

    serde_json::from_slice(&body).map_err(|error| Error::Transport(error.to_string()))
}

/// The URL a note object points at, when callers only have a page.
#[must_use]
pub fn note_url(value: &Value) -> Option<String> {
    link(value.get("url")).or_else(|| link(value.get("id")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_rejects_non_https() {
        let bad_urls = [
            "http://example.com/actor",
            "http://127.0.0.1:8091/healthz",
            "http://169.254.169.254/latest/meta-data",
            "ftp://example.com/actor",
            "file:///etc/passwd",
            "gopher://evil.com/",
            "//example.com/actor",
            "",
            "not a url",
        ];
        for url in bad_urls {
            assert!(
                check(url).is_err(),
                "expected check to reject non-https URL: {url}"
            );
        }
    }

    #[test]
    fn check_rejects_loopback_and_private_ips() {
        let bad_ips = [
            // IPv4 loopback
            "https://127.0.0.1/actor",
            "https://127.0.0.2:8443/actor",
            "https://127.255.255.255/actor",
            "https://0x7f000001/actor",
            "https://2130706433/actor",
            // IPv4 private RFC 1918
            "https://10.0.0.1/actor",
            "https://10.255.255.255/actor",
            "https://172.16.0.1/actor",
            "https://172.31.255.255/actor",
            "https://192.168.0.1/actor",
            "https://192.168.1.1/actor",
            // IPv4 link-local RFC 3927
            "https://169.254.169.254/latest/meta-data",
            "https://169.254.1.1/actor",
            // IPv4 unspecified and broadcast
            "https://0.0.0.0/actor",
            "https://255.255.255.255/actor",
            // IPv4 multicast
            "https://224.0.0.1/actor",
            // IPv4 CGNAT
            "https://100.64.0.1/actor",
            // IPv4 reserved Class E
            "https://240.0.0.1/actor",
            // IPv6 loopback
            "https://[::1]/actor",
            "https://[::1]:8443/actor",
            // IPv6 unspecified
            "https://[::]/actor",
            // IPv6 unique local (ULA)
            "https://[fc00::1]/actor",
            "https://[fd00::1]/actor",
            "https://[fd12:3456:789a::1]/actor",
            // IPv6 link-local
            "https://[fe80::1]/actor",
            // IPv6 multicast
            "https://[ff02::1]/actor",
            // IPv6 documentation
            "https://[2001:db8::1]/actor",
            // IPv4-mapped IPv6
            "https://[::ffff:127.0.0.1]/actor",
            "https://[::ffff:10.0.0.1]/actor",
            "https://[::ffff:169.254.169.254]/actor",
            "https://[::ffff:192.168.1.1]/actor",
            // IPv4-translated IPv6
            "https://[64:ff9b::127.0.0.1]/actor",
            "https://[64:ff9b::10.0.0.1]/actor",
        ];
        for url in bad_ips {
            assert!(
                check(url).is_err(),
                "expected check to reject private/loopback URL: {url}"
            );
        }
    }

    #[test]
    fn check_rejects_localhost_and_internal_domains() {
        let bad_domains = [
            "https://localhost/actor",
            "https://localhost:8443/actor",
            "https://sub.localhost/actor",
            "https://foo.bar.localhost/actor",
            "https://localhost./actor",
            "https://service.local/actor",
            "https://node.internal/actor",
            "https://corp.internal/actor",
        ];
        for url in bad_domains {
            assert!(
                check(url).is_err(),
                "expected check to reject localhost/internal domain: {url}"
            );
        }
    }

    #[test]
    fn check_accepts_valid_public_urls() {
        let good_urls = [
            "https://example.com/actor",
            "https://social.example.org:8443/users/alice",
            "https://sub.domain.example.com/inbox",
            "https://93.184.216.34/actor",
            "https://[2606:2800:220:1:248:1893:25c8:1946]/actor",
        ];
        for url in good_urls {
            assert!(
                check(url).is_ok(),
                "expected check to accept valid public URL: {url}"
            );
        }
    }

    #[test]
    fn webfinger_rejects_path_query_fragment_port_injection() {
        let bad_hosts = [
            // Exact exploit scenario from issue description
            "target.example/arbitrary/path?foo=bar#",
            "target.example/path",
            "target.example?foo=bar",
            "target.example#fragment",
            "target.example:8443",
            "target.example\\path",
            "target.example@evil.example",
            "target.example/arbitrary",
            "target.example?",
            "target.example#",
            "target.example:",
            "target.example ",
            "target.example\t",
            // Localhost / private IP in WebFinger
            "localhost",
            "foo.localhost",
            "127.0.0.1",
            "10.0.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "[::1]",
            "",
        ];
        for host in bad_hosts {
            assert!(
                validate_webfinger_host(host).is_err(),
                "expected validate_webfinger_host to reject host: {host}"
            );
        }

        let bad_handles = [
            "user@target.example/arbitrary/path?foo=bar#",
            "user@target.example:8443",
            "user@target.example/path",
            "user@target.example?foo=bar",
            "user@target.example#fragment",
            "user@target.example\\path",
            "user@localhost",
            "user@127.0.0.1",
            "user@10.0.0.1",
            "user@169.254.169.254",
            "user@[::1]",
            "no-at-sign",
            "@no-user",
            "no-host@",
            "",
            "@@",
            "@@user@example.com",
            "user@@example.com",
        ];
        let client = ActivityPub::new();
        for handle in bad_handles {
            let res = block_on(client.webfinger(handle));
            match res {
                Err(Error::Handle(h)) => {
                    assert_eq!(h, handle, "expected handle in error to match input");
                }
                other => panic!("expected Error::Handle for {handle}, got {other:?}"),
            }
        }
    }

    #[test]
    fn webfinger_validates_clean_domains() {
        let good_hosts = [
            "example.com",
            "sub.example.com",
            "mastodon.social",
            "social.test-domain.org",
            "93.184.216.34",
        ];
        for host in good_hosts {
            assert!(
                validate_webfinger_host(host).is_ok(),
                "expected valid host to pass: {host}"
            );
        }
    }

    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        let mut pinned = std::pin::pin!(fut);
        match pinned.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(val) => val,
            std::task::Poll::Pending => panic!("future unexpectedly pending"),
        }
    }

    #[test]
    fn webfinger_client_returns_handle_error_for_injected_handle() {
        let client = ActivityPub::new();
        let res = block_on(client.webfinger("user@target.example/arbitrary/path?foo=bar#"));
        match res {
            Err(Error::Handle(h)) => {
                assert_eq!(h, "user@target.example/arbitrary/path?foo=bar#");
            }
            other => panic!("expected Error::Handle, got {other:?}"),
        }

        let res_port = block_on(client.webfinger("user@target.example:8443"));
        match res_port {
            Err(Error::Handle(h)) => {
                assert_eq!(h, "user@target.example:8443");
            }
            other => panic!("expected Error::Handle, got {other:?}"),
        }

        let res_local = block_on(client.webfinger("user@localhost"));
        match res_local {
            Err(Error::Handle(h)) => {
                assert_eq!(h, "user@localhost");
            }
            other => panic!("expected Error::Handle, got {other:?}"),
        }
    }

    #[test]
    fn validate_redirect_enforces_https_and_forbids_private_destinations() {
        // HTTP downgrade rejection
        let downgrade_target = Url::parse("http://127.0.0.1:8091/healthz").unwrap();
        assert_eq!(
            validate_redirect(&downgrade_target, 0),
            Err("redirect to non-https URL is forbidden")
        );

        let http_remote = Url::parse("http://remote.example/actor").unwrap();
        assert_eq!(
            validate_redirect(&http_remote, 0),
            Err("redirect to non-https URL is forbidden")
        );

        // HTTPS loopback / private IP rejection
        let loopback_target = Url::parse("https://127.0.0.1:8091/healthz").unwrap();
        assert_eq!(
            validate_redirect(&loopback_target, 0),
            Err("redirect to private or local destination is forbidden")
        );

        let https_link_local = Url::parse("https://169.254.169.254/latest/meta-data/").unwrap();
        assert_eq!(
            validate_redirect(&https_link_local, 0),
            Err("redirect to private or local destination is forbidden")
        );

        let https_private = Url::parse("https://10.0.0.1/admin").unwrap();
        assert_eq!(
            validate_redirect(&https_private, 0),
            Err("redirect to private or local destination is forbidden")
        );

        let https_localhost = Url::parse("https://localhost/admin").unwrap();
        assert_eq!(
            validate_redirect(&https_localhost, 0),
            Err("redirect to private or local destination is forbidden")
        );

        let https_v6_loopback = Url::parse("https://[::1]/admin").unwrap();
        assert_eq!(
            validate_redirect(&https_v6_loopback, 0),
            Err("redirect to private or local destination is forbidden")
        );

        // Max hops exceeded
        let https_valid = Url::parse("https://cdn.example.com/actor").unwrap();
        assert_eq!(
            validate_redirect(&https_valid, 10),
            Err("too many redirects")
        );

        // Valid HTTPS redirect
        assert_eq!(validate_redirect(&https_valid, 1), Ok(()));
    }

    #[test]
    fn validate_resolved_addrs_rejects_private_and_empty() {
        let loopback: SocketAddr = "127.0.0.1:443".parse().unwrap();
        assert!(validate_resolved_addrs(vec![loopback]).is_err());

        let private: SocketAddr = "10.0.0.1:443".parse().unwrap();
        assert!(validate_resolved_addrs(vec![private]).is_err());

        let link_local: SocketAddr = "169.254.169.254:443".parse().unwrap();
        assert!(validate_resolved_addrs(vec![link_local]).is_err());

        let v6_loopback: SocketAddr = "[::1]:443".parse().unwrap();
        assert!(validate_resolved_addrs(vec![v6_loopback]).is_err());

        // Mixed results (SSRF rebinding/dual-homed attempt)
        let public: SocketAddr = "93.184.216.34:443".parse().unwrap();
        assert!(validate_resolved_addrs(vec![public, loopback]).is_err());

        // Empty addresses
        assert!(validate_resolved_addrs(vec![]).is_err());

        // Pure public addresses
        let v6_public: SocketAddr = "[2606:2800:220:1:248:1893:25c8:1946]:443".parse().unwrap();
        assert!(validate_resolved_addrs(vec![public, v6_public]).is_ok());
    }

    #[test]
    fn dns_resolver_rejects_forbidden_hostnames() {
        use reqwest::dns::Resolve;

        let resolver = SafeDnsResolver;
        assert!(block_on(resolver.resolve("localhost".parse().unwrap())).is_err());
        assert!(block_on(resolver.resolve("127.0.0.1".parse().unwrap())).is_err());
        assert!(block_on(resolver.resolve("10.0.0.1".parse().unwrap())).is_err());
        assert!(block_on(resolver.resolve("169.254.169.254".parse().unwrap())).is_err());
        assert!(block_on(resolver.resolve("[::1]".parse().unwrap())).is_err());
    }

    struct ChunkedBody {
        chunks: std::collections::VecDeque<bytes::Bytes>,
        advertised_size: Option<u64>,
        polls: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl http_body::Body for ChunkedBody {
        type Data = bytes::Bytes;
        type Error = std::io::Error;

        fn poll_frame(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
            let this = self.get_mut();
            match this.chunks.pop_front() {
                Some(chunk) => {
                    this.polls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    std::task::Poll::Ready(Some(Ok(http_body::Frame::data(chunk))))
                }
                None => std::task::Poll::Ready(None),
            }
        }

        fn size_hint(&self) -> http_body::SizeHint {
            match self.advertised_size {
                Some(len) => http_body::SizeHint::with_exact(len),
                None => http_body::SizeHint::default(),
            }
        }
    }

    fn build_test_response(
        content_length: Option<usize>,
        chunks: Vec<bytes::Bytes>,
    ) -> (reqwest::Response, Arc<std::sync::atomic::AtomicUsize>) {
        let polls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let advertised_size = content_length.map(|len| u64::try_from(len).unwrap_or(u64::MAX));
        let body = reqwest::Body::wrap(ChunkedBody {
            chunks: chunks.into(),
            advertised_size,
            polls: Arc::clone(&polls),
        });
        let mut builder = http::Response::builder().status(200);
        if let Some(len) = content_length {
            builder = builder.header(http::header::CONTENT_LENGTH, len.to_string());
        }
        let http_res = builder.body(body).unwrap();
        (reqwest::Response::from(http_res), polls)
    }

    fn make_json_payload(total_len: usize) -> Vec<u8> {
        // Build `{"k":"aaa..."}` of exact length `total_len` (requires total_len >= 9).
        let prefix = b"{\"k\":\"";
        let suffix = b"\"}";
        let pad_len = total_len - prefix.len() - suffix.len();
        let mut out = Vec::with_capacity(total_len);
        out.extend_from_slice(prefix);
        out.resize(prefix.len() + pad_len, b'a');
        out.extend_from_slice(suffix);
        out
    }

    #[test]
    fn read_json_limited_rejects_oversized_content_length() {
        let payload = bytes::Bytes::from(make_json_payload(1025));
        let (response, polls) = build_test_response(Some(payload.len()), vec![payload]);
        let err = block_on(read_json_limited(response, 1024)).unwrap_err();
        assert!(
            matches!(err, Error::Transport(_)),
            "expected Transport error, got {err:?}"
        );
        assert_eq!(
            polls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "body should not be polled when Content-Length exceeds limit"
        );

        let (max_res, max_polls) = build_test_response(Some(MAX_RESPONSE_BYTES + 1), vec![]);
        let max_err = block_on(read_json_limited(max_res, MAX_RESPONSE_BYTES)).unwrap_err();
        assert!(matches!(max_err, Error::Transport(_)));
        assert_eq!(max_polls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn read_json_limited_rejects_oversized_chunked_body_without_content_length() {
        let payload = make_json_payload(1500);
        let chunks: Vec<bytes::Bytes> = payload
            .chunks(300)
            .map(bytes::Bytes::copy_from_slice)
            .collect();
        assert_eq!(chunks.len(), 5);

        let (response, polls) = build_test_response(None, chunks);
        let err = block_on(read_json_limited(response, 1024)).unwrap_err();
        assert!(
            matches!(err, Error::Transport(_)),
            "expected Transport error, got {err:?}"
        );
        assert_eq!(
            polls.load(std::sync::atomic::Ordering::SeqCst),
            4,
            "streaming should stop as soon as accumulated bytes exceed limit"
        );
    }

    #[test]
    fn read_json_limited_rejects_spoofed_small_content_length() {
        let payload = make_json_payload(1500);
        let chunks: Vec<bytes::Bytes> = payload
            .chunks(300)
            .map(bytes::Bytes::copy_from_slice)
            .collect();
        let (response, polls) = build_test_response(Some(16), chunks);
        let err = block_on(read_json_limited(response, 1024)).unwrap_err();
        assert!(
            matches!(err, Error::Transport(_)),
            "expected Transport error, got {err:?}"
        );
        assert_eq!(polls.load(std::sync::atomic::Ordering::SeqCst), 4);
    }

    #[test]
    fn read_json_limited_accepts_chunked_json_at_exact_limit() {
        let payload = make_json_payload(1024);
        let chunks: Vec<bytes::Bytes> = payload
            .chunks(256)
            .map(bytes::Bytes::copy_from_slice)
            .collect();
        let (response, polls) = build_test_response(None, chunks);
        let value = block_on(read_json_limited(response, 1024)).expect("exact limit should pass");
        assert_eq!(
            value.get("k").and_then(Value::as_str).map(str::len),
            Some(1024 - 8)
        );
        assert_eq!(polls.load(std::sync::atomic::Ordering::SeqCst), 4);
    }

    #[test]
    fn read_json_limited_rejects_invalid_json_within_limit() {
        let (response, _polls) =
            build_test_response(Some(14), vec![bytes::Bytes::from_static(b"not valid json")]);
        let err = block_on(read_json_limited(response, 1024)).unwrap_err();
        assert!(matches!(err, Error::Transport(_)));
    }
}
