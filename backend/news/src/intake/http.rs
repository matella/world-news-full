//! Shared outbound policy. Every hop resolves, validates and pins its own addresses.
use ipnet::IpNet;
use reqwest::{
    Client, StatusCode,
    header::{self, HeaderMap, HeaderValue},
};
use std::{
    collections::BTreeSet,
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchError {
    InvalidUrl,
    BlockedTarget,
    Resolution,
    Busy,
    Timeout,
    Network,
    TooLarge,
    RedirectLimit,
    InvalidResponse,
    UnsupportedEncoding,
    HttpStatus(u16, Option<u64>),
    InvalidFeed,
}
impl FetchError {
    pub fn category(self) -> &'static str {
        match self {
            Self::InvalidUrl => "invalid_url",
            Self::BlockedTarget => "blocked_target",
            Self::Resolution => "resolution",
            Self::Busy => "busy",
            Self::Timeout => "timeout",
            Self::Network => "network",
            Self::TooLarge => "too_large",
            Self::RedirectLimit => "redirect_limit",
            Self::InvalidResponse => "invalid_response",
            Self::UnsupportedEncoding => "unsupported_encoding",
            Self::HttpStatus(_, _) => "http_status",
            Self::InvalidFeed => "invalid_feed",
        }
    }
}
#[derive(Clone, Default)]
pub struct Validators {
    pub url: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}
pub struct Download {
    pub final_url: String,
    pub bytes: Vec<u8>,
    pub validators: Validators,
    pub content_type: Option<String>,
}
pub enum DownloadResult {
    Fetched(Download),
    NotModified(Validators),
}
#[derive(Clone)]
pub struct SafeHttp {
    user_agent: HeaderValue,
    slots: Arc<Semaphore>,
    dns_slots: Arc<Semaphore>,
    budget: Duration,
    #[cfg(test)]
    fixture: Option<FixtureTarget>,
    #[cfg(test)]
    dns_answers: std::collections::BTreeMap<String, Vec<SocketAddr>>,
}
#[cfg(test)]
#[derive(Clone)]
struct FixtureTarget {
    host: String,
    address: SocketAddr,
}
impl SafeHttp {
    pub fn new(public_url: &Url) -> Result<Self, FetchError> {
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            // Another constructor may win this race; never replace an installed provider.
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
        if !matches!(public_url.scheme(), "http" | "https") || public_url.host_str().is_none() {
            return Err(FetchError::InvalidUrl);
        }
        let user_agent = HeaderValue::from_str(&format!(
            "WorldNewsRust/0.1 (+{})",
            public_url.origin().ascii_serialization()
        ))
        .map_err(|_| FetchError::InvalidUrl)?;
        Ok(Self {
            user_agent,
            slots: Arc::new(Semaphore::new(4)),
            dns_slots: Arc::new(Semaphore::new(2)),
            budget: Duration::from_secs(15),
            #[cfg(test)]
            fixture: None,
            #[cfg(test)]
            dns_answers: std::collections::BTreeMap::new(),
        })
    }
    pub async fn get(
        &self,
        url: &str,
        max_bytes: usize,
        validators: &Validators,
    ) -> Result<DownloadResult, FetchError> {
        self.get_scoped(url, max_bytes, validators, false).await
    }
    /// Extraction rights are scoped to an origin. Check before resolving or
    /// contacting every redirect destination, in addition to the shared IP policy.
    pub async fn get_same_origin(
        &self,
        url: &str,
        max_bytes: usize,
    ) -> Result<DownloadResult, FetchError> {
        self.get_scoped(url, max_bytes, &Validators::default(), true)
            .await
    }
    async fn get_scoped(
        &self,
        url: &str,
        max_bytes: usize,
        validators: &Validators,
        same_origin: bool,
    ) -> Result<DownloadResult, FetchError> {
        if !(16384..=10485760).contains(&max_bytes) {
            return Err(FetchError::TooLarge);
        }
        let _permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| FetchError::Busy)?;
        tokio::time::timeout(
            self.budget,
            self.hops(url, max_bytes, validators, same_origin),
        )
        .await
        .map_err(|_| FetchError::Timeout)?
    }
    async fn hops(
        &self,
        input: &str,
        max_bytes: usize,
        validators: &Validators,
        same_origin: bool,
    ) -> Result<DownloadResult, FetchError> {
        let mut url = target_url(input)?;
        let original_origin = url.origin();
        let mut visited = BTreeSet::new();
        for hop in 0..=3 {
            if same_origin && url.origin() != original_origin {
                return Err(FetchError::BlockedTarget);
            }
            if !visited.insert(url.as_str().to_owned()) {
                return Err(FetchError::RedirectLimit);
            }
            let addresses = self.addresses(&url).await?;
            let host = url.host_str().ok_or(FetchError::InvalidUrl)?;
            // Product egress is direct and DNS-pinned; ambient proxies could bypass
            // that policy or attach credentials. Proxy support requires a separate
            // policy-aware adapter; no ambient credentials may be forwarded.
            let client = Client::builder()
                .tls_backend_rustls()
                .no_proxy()
                .no_hickory_dns()
                .no_gzip()
                .no_brotli()
                .no_deflate()
                .no_zstd()
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .resolve_to_addrs(host, &addresses)
                .user_agent(self.user_agent.clone())
                .connect_timeout(Duration::from_secs(3))
                .timeout(self.budget)
                .pool_max_idle_per_host(0)
                .build()
                .map_err(|_| FetchError::Network)?;
            let mut request = client
                .get(url.clone())
                .header(header::ACCEPT_ENCODING, "identity");
            let conditional = validators.url.as_deref() == Some(url.as_str())
                && (validators.etag.is_some() || validators.last_modified.is_some());
            if conditional {
                if let Some(value) = &validators.etag {
                    request = request.header(header::IF_NONE_MATCH, validator_header(value)?);
                }
                if let Some(value) = &validators.last_modified {
                    request = request.header(header::IF_MODIFIED_SINCE, validator_header(value)?);
                }
            }
            let mut response = request.send().await.map_err(network_error)?;
            let status = response.status();
            if matches!(
                status,
                StatusCode::MOVED_PERMANENTLY
                    | StatusCode::FOUND
                    | StatusCode::SEE_OTHER
                    | StatusCode::TEMPORARY_REDIRECT
                    | StatusCode::PERMANENT_REDIRECT
            ) {
                if hop == 3 {
                    return Err(FetchError::RedirectLimit);
                }
                let location = response
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|h| h.to_str().ok())
                    .filter(|v| v.len() <= 2048)
                    .ok_or(FetchError::InvalidResponse)?;
                let redirect = url.join(location).map_err(|_| FetchError::InvalidUrl)?;
                if url.scheme() == "https" && redirect.scheme() != "https" {
                    return Err(FetchError::BlockedTarget);
                }
                url = target_url(redirect.as_str())?;
                continue;
            }
            if status == StatusCode::NOT_MODIFIED {
                if !conditional {
                    return Err(FetchError::InvalidResponse);
                }
                let mut updated = read_validators(response.headers(), &url)?;
                updated.etag = updated.etag.or_else(|| validators.etag.clone());
                updated.last_modified = updated
                    .last_modified
                    .or_else(|| validators.last_modified.clone());
                return Ok(DownloadResult::NotModified(updated));
            }
            if status != StatusCode::OK {
                return Err(FetchError::HttpStatus(
                    status.as_u16(),
                    retry_after(response.headers()),
                ));
            }
            if response
                .headers()
                .get_all(header::CONTENT_ENCODING)
                .iter()
                .any(|h| h.as_bytes() != b"identity")
            {
                return Err(FetchError::UnsupportedEncoding);
            }
            if response
                .content_length()
                .is_some_and(|n| n > max_bytes as u64)
            {
                return Err(FetchError::TooLarge);
            }
            let validators = read_validators(response.headers(), &url)?;
            let content_type = response
                .headers()
                .get(header::CONTENT_TYPE)
                .map(|h| {
                    h.to_str()
                        .ok()
                        .filter(|s| s.len() <= 256)
                        .map(str::to_owned)
                        .ok_or(FetchError::InvalidResponse)
                })
                .transpose()?;
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(network_error)? {
                if chunk.len() > max_bytes.saturating_sub(bytes.len()) {
                    return Err(FetchError::TooLarge);
                }
                bytes.extend_from_slice(&chunk);
            }
            return Ok(DownloadResult::Fetched(Download {
                final_url: url.to_string(),
                bytes,
                validators,
                content_type,
            }));
        }
        Err(FetchError::RedirectLimit)
    }
    async fn addresses(&self, url: &Url) -> Result<Vec<SocketAddr>, FetchError> {
        let host = url.host_str().ok_or(FetchError::InvalidUrl)?;
        let port = url.port_or_known_default().ok_or(FetchError::InvalidUrl)?;
        #[cfg(test)]
        if let Some(fixture) = &self.fixture
            && host == fixture.host
            && port == fixture.address.port()
        {
            return Ok(vec![fixture.address]);
        }
        if !matches!((url.scheme(), port), ("http", 80) | ("https", 443)) {
            return Err(FetchError::BlockedTarget);
        }
        if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
            return if public_address(ip) {
                Ok(vec![SocketAddr::new(ip, port)])
            } else {
                Err(FetchError::BlockedTarget)
            };
        }
        let normalized = host.trim_end_matches('.');
        if !normalized.contains('.')
            || ["localhost", "local", "internal", "lan", "home"]
                .iter()
                .any(|suffix| normalized == *suffix || normalized.ends_with(&format!(".{suffix}")))
        {
            return Err(FetchError::BlockedTarget);
        }
        #[cfg(test)]
        if let Some(answers) = self.dns_answers.get(host) {
            return checked_addresses(answers.clone());
        }
        let permit = self
            .dns_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| FetchError::Busy)?;
        let host = host.to_owned();
        let addresses = tokio::task::spawn_blocking(move || resolve(host, port, permit))
            .await
            .map_err(|_| FetchError::Resolution)??;
        checked_addresses(addresses)
    }
    #[cfg(test)]
    pub(super) fn fixture(host: &str, address: SocketAddr) -> Self {
        assert!(address.ip().is_loopback());
        let mut client = Self::new(&Url::parse("https://news.invalid").unwrap()).unwrap();
        client.fixture = Some(FixtureTarget {
            host: host.into(),
            address,
        });
        client
    }
}
fn resolve(
    host: String,
    port: u16,
    permit: OwnedSemaphorePermit,
) -> Result<Vec<SocketAddr>, FetchError> {
    let _permit = permit; // Retained until OS DNS finishes, even after HTTP cancellation.
    let addresses = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|_| FetchError::Resolution)?;
    let mut result = Vec::new();
    for address in addresses {
        if result.len() == 16 {
            return Err(FetchError::Resolution);
        }
        if !result.contains(&address) {
            result.push(address);
        }
    }
    Ok(result)
}
fn checked_addresses(addresses: Vec<SocketAddr>) -> Result<Vec<SocketAddr>, FetchError> {
    if addresses.is_empty() {
        return Err(FetchError::Resolution);
    }
    if addresses.iter().any(|a| !public_address(a.ip())) {
        return Err(FetchError::BlockedTarget);
    }
    Ok(addresses)
}
fn target_url(value: &str) -> Result<Url, FetchError> {
    if value.len() > 2048 {
        return Err(FetchError::InvalidUrl);
    }
    let mut url = Url::parse(value).map_err(|_| FetchError::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || !crate::content::public_url_parameters(&url)
    {
        return Err(FetchError::InvalidUrl);
    }
    url.set_fragment(None);
    Ok(url)
}
fn public_address(ip: IpAddr) -> bool {
    const V4: &[&str] = &[
        "0.0.0.0/8",
        "10.0.0.0/8",
        "100.64.0.0/10",
        "127.0.0.0/8",
        "169.254.0.0/16",
        "172.16.0.0/12",
        "192.0.0.0/24",
        "192.0.2.0/24",
        "192.88.99.0/24",
        "192.168.0.0/16",
        "198.18.0.0/15",
        "198.51.100.0/24",
        "203.0.113.0/24",
        "224.0.0.0/4",
        "240.0.0.0/4",
    ];
    match ip {
        IpAddr::V4(_) => !V4
            .iter()
            .any(|cidr| cidr.parse::<IpNet>().expect("static network").contains(&ip)),
        IpAddr::V6(_) => {
            "2000::/3"
                .parse::<IpNet>()
                .expect("static network")
                .contains(&ip)
                && !["2001::/23", "2001:db8::/32", "2002::/16", "3fff::/20"]
                    .iter()
                    .any(|cidr| cidr.parse::<IpNet>().expect("static network").contains(&ip))
        }
    }
}
fn validator_header(value: &str) -> Result<HeaderValue, FetchError> {
    if value.len() > 1024 {
        return Err(FetchError::InvalidResponse);
    }
    HeaderValue::from_str(value).map_err(|_| FetchError::InvalidResponse)
}
fn read_validators(headers: &HeaderMap, url: &Url) -> Result<Validators, FetchError> {
    let read = |name| {
        headers
            .get(name)
            .map(|h| {
                h.to_str()
                    .ok()
                    .filter(|v| v.len() <= 1024)
                    .map(str::to_owned)
                    .ok_or(FetchError::InvalidResponse)
            })
            .transpose()
    };
    Ok(Validators {
        url: Some(url.to_string()),
        etag: read(header::ETAG)?,
        last_modified: read(header::LAST_MODIFIED)?,
    })
}
fn network_error(error: reqwest::Error) -> FetchError {
    if error.is_timeout() {
        FetchError::Timeout
    } else {
        FetchError::Network
    }
}
fn retry_after(headers: &HeaderMap) -> Option<u64> {
    let value = headers
        .get(header::RETRY_AFTER)?
        .to_str()
        .ok()
        .filter(|v| v.len() <= 128)?;
    value.parse().ok().or_else(|| {
        httpdate::parse_http_date(value).ok().map(|date| {
            date.duration_since(std::time::SystemTime::now())
                .unwrap_or_default()
                .as_secs()
        })
    })
}

#[cfg(test)]
mod tests;
