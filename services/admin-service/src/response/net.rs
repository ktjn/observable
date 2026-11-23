//! Guarded outbound sender for exchange targets.
//!
//! Every outbound call goes to a registered target URL, and the URL is checked twice: at
//! registration and again on every send, because DNS answers change. A send resolves the
//! host once, refuses the request if any resolved address is not public, and pins the
//! connection to the checked addresses so a second lookup cannot redirect it (DNS
//! rebinding). Redirects are never followed. Plain `http` and private addresses are refused
//! unless the deployment opts in (`RESPONSE_TARGETS_ALLOW_HTTP`,
//! `RESPONSE_TARGETS_ALLOW_PRIVATE`), which only local stacks and tests should do.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use reqwest::Url;
use serde::Serialize;

#[derive(Clone, Debug)]
pub struct TargetPolicy {
    pub allow_http: bool,
    pub allow_private: bool,
    pub timeout: Duration,
}

impl Default for TargetPolicy {
    fn default() -> Self {
        Self {
            allow_http: false,
            allow_private: false,
            timeout: Duration::from_secs(10),
        }
    }
}

/// Why a target URL is refused. The code is returned to the caller; the resolved addresses are not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetRejected {
    Malformed,
    SchemeNotAllowed,
    CredentialsInUrl,
    Unresolvable,
    AddressNotAllowed,
}

impl TargetRejected {
    pub fn code(self) -> &'static str {
        match self {
            Self::Malformed => "target_url_malformed",
            Self::SchemeNotAllowed => "target_scheme_not_allowed",
            Self::CredentialsInUrl => "target_url_has_credentials",
            Self::Unresolvable => "target_unresolvable",
            Self::AddressNotAllowed => "target_address_not_allowed",
        }
    }
}

/// Outcome of one delivery attempt.
#[derive(Clone, Debug)]
pub enum SendOutcome {
    Delivered {
        http_status: u16,
    },
    Failed {
        http_status: Option<u16>,
        error: String,
    },
}

impl TargetPolicy {
    /// Syntactic check: scheme, host, no userinfo.
    pub fn check_url(&self, raw: &str) -> Result<Url, TargetRejected> {
        let url = Url::parse(raw.trim()).map_err(|_| TargetRejected::Malformed)?;
        match url.scheme() {
            "https" => {}
            "http" if self.allow_http => {}
            _ => return Err(TargetRejected::SchemeNotAllowed),
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(TargetRejected::CredentialsInUrl);
        }
        if url.host_str().is_none_or(str::is_empty) {
            return Err(TargetRejected::Malformed);
        }
        Ok(url)
    }

    /// Resolves the host and refuses any non-public address.
    pub async fn resolve(&self, url: &Url) -> Result<Vec<SocketAddr>, TargetRejected> {
        let host = url.host_str().ok_or(TargetRejected::Malformed)?;
        let port = url
            .port_or_known_default()
            .ok_or(TargetRejected::Malformed)?;
        let host = host.trim_start_matches('[').trim_end_matches(']');
        let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| TargetRejected::Unresolvable)?
            .collect();
        if addrs.is_empty() {
            return Err(TargetRejected::Unresolvable);
        }
        if !self.allow_private && addrs.iter().any(|a| !is_public(a.ip())) {
            return Err(TargetRejected::AddressNotAllowed);
        }
        Ok(addrs)
    }

    /// Full check used at registration.
    pub async fn validate(&self, raw: &str) -> Result<Url, TargetRejected> {
        let url = self.check_url(raw)?;
        self.resolve(&url).await?;
        Ok(url)
    }

    /// One POST of `body` to `raw`, re-validated and pinned to the checked addresses.
    pub async fn post_json<T: Serialize + ?Sized>(&self, raw: &str, body: &T) -> SendOutcome {
        let url = match self.check_url(raw) {
            Ok(u) => u,
            Err(r) => return refused(r),
        };
        let addrs = match self.resolve(&url).await {
            Ok(a) => a,
            Err(r) => return refused(r),
        };
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(self.timeout)
            .no_proxy();
        if let Some(domain) = url.domain() {
            builder = builder.resolve_to_addrs(domain, &addrs);
        }
        let client = match builder.build() {
            Ok(c) => c,
            Err(e) => {
                return SendOutcome::Failed {
                    http_status: None,
                    error: format!("client: {e}"),
                };
            }
        };
        match client.post(url).json(body).send().await {
            Ok(resp) if resp.status().is_success() => SendOutcome::Delivered {
                http_status: resp.status().as_u16(),
            },
            Ok(resp) => SendOutcome::Failed {
                http_status: Some(resp.status().as_u16()),
                error: format!("target returned HTTP {}", resp.status().as_u16()),
            },
            Err(e) => SendOutcome::Failed {
                http_status: None,
                error: if e.is_timeout() {
                    "target timed out".to_owned()
                } else {
                    "target unreachable".to_owned()
                },
            },
        }
    }
}

fn refused(r: TargetRejected) -> SendOutcome {
    SendOutcome::Failed {
        http_status: None,
        error: r.code().to_owned(),
    }
}

/// Globally routable unicast only.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_public_v4(v4),
            None => is_public_v6(v6),
        },
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0
        || (a == 100 && (b & 0xc0) == 64)
        || (a == 192 && b == 0 && ip.octets()[2] == 0)
        || (a == 198 && (b & 0xfe) == 18)
        || a >= 240)
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
        || first == 0x2001 && ip.segments()[1] == 0x0db8
        || first == 0x0064 && ip.segments()[1] == 0xff9b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_public_addresses_are_refused() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "64:ff9b::a9fe:a9fe",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip} must not be public");
        }
        for ip in ["93.184.216.34", "2606:4700::1111"] {
            assert!(is_public(ip.parse().unwrap()), "{ip} is public");
        }
    }

    #[test]
    fn strict_policy_refuses_http_and_credentials() {
        let p = TargetPolicy::default();
        assert_eq!(
            p.check_url("http://example.com/hook").unwrap_err(),
            TargetRejected::SchemeNotAllowed
        );
        assert_eq!(
            p.check_url("https://user:pw@example.com/hook").unwrap_err(),
            TargetRejected::CredentialsInUrl
        );
        assert_eq!(
            p.check_url("file:///etc/passwd").unwrap_err(),
            TargetRejected::SchemeNotAllowed
        );
        assert!(p.check_url("https://example.com/hook").is_ok());
    }

    #[tokio::test]
    async fn strict_policy_refuses_literal_private_hosts() {
        let p = TargetPolicy::default();
        for url in [
            "https://127.0.0.1/hook",
            "https://[::1]/hook",
            "https://169.254.169.254/latest/meta-data",
            "https://localhost/hook",
        ] {
            assert_eq!(
                p.validate(url).await.unwrap_err(),
                TargetRejected::AddressNotAllowed,
                "{url}"
            );
        }
    }
}
