// SPDX-License-Identifier: Apache-2.0

//! HTTPS for Update posters, and nothing else. A build without the `network`
//! feature leaves this module out.
//!
//! Only FBI addresses are requested (`update::fbi_url`). A redirect is followed
//! by hand, only to another FBI address, at most three times. Bodies are capped
//! and every request has a deadline. The request names OpenWorld in its
//! User-Agent and carries nothing about the user: no cookies, no other headers.
//! Certificates are checked against the Mozilla roots built into this library.

use crate::update::{fbi_url, FetchError, Transport};
use std::time::Duration;

const USER_AGENT: &str = concat!("OpenWorld/", env!("CARGO_PKG_VERSION"), " (+https://github.com/isaacy13/openworld)");
const MAX_REDIRECTS: usize = 3;
/// One request, from connecting to the last byte.
const DEADLINE: Duration = Duration::from_secs(60);
const CONNECT: Duration = Duration::from_secs(15);

pub struct Https {
    agent: ureq::Agent,
}

impl Https {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .http_status_as_error(false)
            .user_agent(USER_AGENT)
            .timeout_global(Some(DEADLINE))
            .timeout_connect(Some(CONNECT))
            .build();
        Self { agent: config.into() }
    }
}

impl Default for Https {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for Https {
    fn get(&mut self, url: &str, limit: usize) -> Result<Vec<u8>, FetchError> {
        let mut url = url.to_string();
        for _ in 0..=MAX_REDIRECTS {
            if !fbi_url(&url) {
                return Err(FetchError::Refused);
            }
            let mut response = self.agent.get(&url).call().map_err(|err| FetchError::Network(err.to_string()))?;
            let status = response.status().as_u16();
            if (300..400).contains(&status) {
                let location = response.headers().get("location").and_then(|v| v.to_str().ok()).unwrap_or("");
                url = redirect(&url, location).ok_or(FetchError::Refused)?;
                continue;
            }
            if status != 200 {
                return Err(FetchError::Status(status));
            }
            return response.body_mut().with_config().limit(limit as u64).read_to_vec().map_err(|err| match err {
                ureq::Error::BodyExceedsLimit(_) => FetchError::TooLarge,
                other => FetchError::Network(other.to_string()),
            });
        }
        Err(FetchError::Refused)
    }

    fn pause(&mut self, wait: Duration) {
        std::thread::sleep(wait);
    }
}

/// Where a redirect points: an absolute `https://` address, or a path on the
/// same host. `get` checks the host before it follows.
fn redirect(from: &str, location: &str) -> Option<String> {
    if location.starts_with("https://") {
        return Some(location.to_string());
    }
    if !location.starts_with('/') || location.starts_with("//") {
        return None;
    }
    let host = from.strip_prefix("https://")?.split(['/', '?', '#']).next()?;
    Some(format!("https://{host}{location}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_redirect_is_followed_only_to_an_fbi_address() {
        let from = "https://www.fbi.gov/wanted/a/@@images/image/original";
        assert_eq!(redirect(from, "/wanted/a/photo.jpg").as_deref(), Some("https://www.fbi.gov/wanted/a/photo.jpg"));
        assert_eq!(redirect(from, "https://fbi.gov/x").as_deref(), Some("https://fbi.gov/x"));
        assert!(fbi_url(&redirect(from, "/x").unwrap()));
        assert!(!fbi_url(&redirect(from, "https://evil.example/x").unwrap()));
        for refused in ["//evil.example/x", "http://www.fbi.gov/x", "photo.jpg", ""] {
            assert_eq!(redirect(from, refused), None, "{refused}");
        }
    }

    #[test]
    fn another_host_is_refused_without_a_request() {
        let mut https = Https::new();
        assert_eq!(https.get("https://evil.example/", 10), Err(FetchError::Refused));
        assert_eq!(https.get("http://www.fbi.gov/", 10), Err(FetchError::Refused));
        assert!(USER_AGENT.starts_with("OpenWorld/"));
    }
}
