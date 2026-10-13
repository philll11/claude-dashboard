//! The one way claude-dashboard talks to the network: HTTPS only, through the proxy the
//! environment names (or on Windows the system's), checked against the operating system's
//! certificates, within a time limit, and reading at most a megabyte.

use std::sync::Arc;
use std::time::Duration;

/// How long one request may take, redirects included.
const TIMEOUT: Duration = Duration::from_secs(10);

/// The most of an answer that is read.
const LONGEST_BODY: u64 = 1 << 20;

/// The most redirects followed, each to a host the request allows.
const MOST_REDIRECTS: usize = 3;

/// A GET request.
#[derive(Debug, Clone, Copy)]
pub struct Get<'a> {
    pub url: &'a str,
    pub headers: &'a [(&'a str, &'a str)],
    /// The hosts a redirect may lead to, the request's own included; with none, no redirect is
    /// followed, so whatever the headers carry goes only to `url`.
    pub redirect_hosts: &'a [&'a str],
}

/// What a server answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    pub body: String,
    /// The `ETag` header.
    pub etag: Option<String>,
    /// The `Retry-After` header, in seconds.
    pub retry_after_s: Option<u64>,
}

/// Sends a GET request and reads the answer.
///
/// # Errors
///
/// Why the server could not be reached or its answer read, or that it redirected somewhere the
/// request does not allow.
pub fn get(request: &Get<'_>) -> Result<Answer, String> {
    use ureq::tls::{RootCerts, TlsConfig};

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(TIMEOUT))
        .user_agent(concat!(
            "claude-dashboard/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/philll11/claude-dashboard)"
        ))
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .unversioned_rustls_crypto_provider(Arc::new(
                    rustls::crypto::ring::default_provider(),
                ))
                .build(),
        )
        .build()
        .into();
    let mut url = request.url.to_owned();
    for _ in 0..=MOST_REDIRECTS {
        let mut call = agent.get(&url);
        for (name, value) in request.headers {
            call = call.header(*name, *value);
        }
        let mut response = call.call().map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(|value| value.trim().to_owned())
        };
        if matches!(status, 301 | 302 | 303 | 307 | 308) && !request.redirect_hosts.is_empty() {
            let location = header("location").ok_or("a redirect without a Location")?;
            url = allowed_redirect(&url, &location, request.redirect_hosts)?;
            continue;
        }
        let etag = header("etag");
        let retry_after_s = header("retry-after").and_then(|value| value.parse().ok());
        let body = response
            .body_mut()
            .with_config()
            .limit(LONGEST_BODY)
            .read_to_string()
            .map_err(|error| error.to_string())?;
        return Ok(Answer {
            status,
            body,
            etag,
            retry_after_s,
        });
    }
    Err(format!("more than {MOST_REDIRECTS} redirects"))
}

/// Where a redirect from `from` to `location` leads, when that is HTTPS on one of `hosts`.
fn allowed_redirect(from: &str, location: &str, hosts: &[&str]) -> Result<String, String> {
    let target = if location.starts_with('/') {
        let origin_end = from
            .strip_prefix("https://")
            .and_then(|rest| rest.find('/'))
            .map_or(from.len(), |at| at + "https://".len());
        format!("{}{location}", &from[..origin_end])
    } else {
        location.to_owned()
    };
    let host = target
        .strip_prefix("https://")
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .unwrap_or_default();
    if hosts
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(host))
    {
        Ok(target)
    } else {
        Err(format!("a redirect to {target}, which is not allowed"))
    }
}

#[cfg(test)]
mod tests {
    use super::allowed_redirect;

    const FROM: &str = "https://raw.githubusercontent.com/old/name/main/pricing/prices.json";
    const HOSTS: &[&str] = &["raw.githubusercontent.com"];

    #[test]
    fn a_redirect_stays_on_the_hosts_it_is_allowed() {
        assert_eq!(
            allowed_redirect(
                FROM,
                "https://raw.githubusercontent.com/new/name/main/pricing/prices.json",
                HOSTS
            ),
            Ok("https://raw.githubusercontent.com/new/name/main/pricing/prices.json".to_owned())
        );
        assert_eq!(
            allowed_redirect(FROM, "/new/name/main/pricing/prices.json", HOSTS),
            Ok("https://raw.githubusercontent.com/new/name/main/pricing/prices.json".to_owned())
        );
    }

    #[test]
    fn a_redirect_elsewhere_or_without_https_is_refused() {
        for location in [
            "https://example.com/prices.json",
            "http://raw.githubusercontent.com/new/name/main/pricing/prices.json",
            "https://raw.githubusercontent.com.example.com/prices.json",
        ] {
            assert!(
                allowed_redirect(FROM, location, HOSTS).is_err(),
                "{location}"
            );
        }
    }
}
