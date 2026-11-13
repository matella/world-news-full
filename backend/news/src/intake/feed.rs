//! Bounded RSS/Atom metadata parsing. Article bodies/summaries are not retained.
use super::http::{Download, DownloadResult, FetchError, SafeHttp, Validators};
use chrono::{DateTime, Utc};
use quick_xml::{Reader, events::Event};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

#[derive(Serialize, Deserialize)]
pub struct Entry {
    /// feed-rs may generate this when the publisher supplied none. Never certify
    /// it as an original GUID or use it alone for irreversible deduplication.
    pub parser_id: String,
    pub title: Option<String>,
    pub links: Vec<String>,
    pub published: Option<DateTime<Utc>>,
    /// Parser-normalised; RSS can copy this from published when no updated exists.
    pub updated: Option<DateTime<Utc>>,
    pub language: Option<String>,
    pub authors: Vec<String>,
}
#[derive(Serialize, Deserialize)]
pub struct FeedData {
    pub final_url: String,
    pub format: String,
    pub response_bytes: usize,
    pub title: Option<String>,
    pub entries: Vec<Entry>,
    #[serde(skip)]
    pub validators: Validators,
}
pub enum FeedResult {
    Fetched(FeedData),
    NotModified(Validators),
}
#[derive(Clone)]
pub struct FeedClient {
    http: SafeHttp,
    parse_slots: Arc<Semaphore>,
}
impl FeedClient {
    pub fn new(http: SafeHttp) -> Self {
        Self {
            http,
            parse_slots: Arc::new(Semaphore::new(2)),
        }
    }
    pub async fn fetch(
        &self,
        url: &str,
        max_bytes: usize,
        validators: &Validators,
    ) -> Result<FeedResult, FetchError> {
        match self.http.get(url, max_bytes, validators).await? {
            DownloadResult::NotModified(cache) => Ok(FeedResult::NotModified(cache)),
            DownloadResult::Fetched(download) => {
                let permit = self
                    .parse_slots
                    .clone()
                    .try_acquire_owned()
                    .map_err(|_| FetchError::Busy)?;
                let parse = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    parse(download)
                });
                tokio::time::timeout(Duration::from_secs(3), parse)
                    .await
                    .map_err(|_| FetchError::Timeout)?
                    .map_err(|_| FetchError::InvalidFeed)?
                    .map(FeedResult::Fetched)
            }
        }
    }
}
fn validate_xml(bytes: &[u8]) -> Result<(), FetchError> {
    if bytes.contains(&0) {
        return Err(FetchError::UnsupportedEncoding);
    }
    let mut reader = Reader::from_reader(bytes);
    let mut depth = 0usize;
    let mut events = 0usize;
    let mut roots = 0usize;
    loop {
        events += 1;
        if events > 100000 {
            return Err(FetchError::TooLarge);
        }
        match reader.read_event().map_err(|_| FetchError::InvalidFeed)? {
            Event::DocType(_) => return Err(FetchError::InvalidFeed),
            Event::Start(_) => {
                if depth == 0 {
                    roots += 1;
                }
                depth += 1;
                if depth > 64 {
                    return Err(FetchError::TooLarge);
                }
            }
            Event::Empty(_) => {
                if depth == 0 {
                    roots += 1;
                }
            }
            Event::End(_) => {
                depth = depth.checked_sub(1).ok_or(FetchError::InvalidFeed)?;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if depth != 0 || roots != 1 {
        return Err(FetchError::InvalidFeed);
    }
    Ok(())
}
fn parse(download: Download) -> Result<FeedData, FetchError> {
    validate_xml(&download.bytes)?;
    let feed = feed_rs::parser::Builder::new()
        .base_uri(Some(&download.final_url))
        .build()
        .parse(download.bytes.as_slice())
        .map_err(|_| FetchError::InvalidFeed)?;
    if feed.entries.len() > 500 {
        return Err(FetchError::TooLarge);
    }
    let mut entries = Vec::with_capacity(feed.entries.len());
    for entry in feed.entries {
        let title = entry.title.map(|t| t.content);
        let links: Vec<_> = entry
            .links
            .into_iter()
            .filter(|link| link.rel.as_deref().is_none_or(|rel| rel == "alternate"))
            .map(|link| link.href)
            .collect();
        let authors: Vec<_> = entry
            .authors
            .into_iter()
            .filter_map(|author| author.name)
            .collect();
        if entry.id.len() > 2048
            || entry.id.contains('\0')
            || title
                .as_ref()
                .is_some_and(|t| t.len() > 4096 || t.contains('\0'))
            || links.len() > 20
            || links.iter().any(|v| v.len() > 2048 || v.contains('\0'))
            || authors.len() > 20
            || authors.iter().any(|a| a.len() > 500 || a.contains('\0'))
            || entry
                .language
                .as_ref()
                .is_some_and(|l| l.len() > 100 || l.contains('\0'))
        {
            return Err(FetchError::TooLarge);
        }
        entries.push(Entry {
            parser_id: entry.id,
            title,
            links,
            published: entry.published,
            updated: entry.updated,
            language: entry.language,
            authors,
        });
    }
    let title = feed.title.map(|t| t.content);
    if title
        .as_ref()
        .is_some_and(|t| t.len() > 4096 || t.contains('\0'))
    {
        return Err(FetchError::TooLarge);
    }
    Ok(FeedData {
        final_url: download.final_url,
        format: match feed.feed_type {
            feed_rs::model::FeedType::Atom => "atom",
            feed_rs::model::FeedType::RSS0 => "rss0",
            feed_rs::model::FeedType::RSS1 => "rss1",
            feed_rs::model::FeedType::RSS2 => "rss2",
            feed_rs::model::FeedType::JSON => return Err(FetchError::InvalidFeed),
        }
        .into(),
        response_bytes: download.bytes.len(),
        title,
        entries,
        validators: download.validators,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn download(bytes: &[u8]) -> Download {
        Download {
            final_url: "https://source.invalid/feed".into(),
            bytes: bytes.to_vec(),
            validators: Validators::default(),
            content_type: None,
        }
    }
    #[test]
    fn rss_and_atom_keep_metadata_without_copying_full_article_content() {
        let rss=br#"<?xml version="1.0"?><rss version="2.0"><channel><title>Fixture</title><link>https://source.invalid</link><description>Synthetic</description><item><guid>source-item</guid><title>Synthetic title</title><link>https://source.invalid/article</link><description>private-full-body-marker</description><pubDate>Fri, 13 Nov 2026 12:00:00 GMT</pubDate></item></channel></rss>"#;
        let feed = parse(download(rss)).unwrap();
        assert_eq!(feed.entries.len(), 1);
        assert_eq!(feed.entries[0].links, ["https://source.invalid/article"]);
        let encoded = serde_json::to_string(&feed.entries).unwrap();
        assert!(!encoded.contains("private-full-body-marker"));
        assert_eq!(
            feed.entries[0].published.unwrap().to_rfc3339(),
            "2026-11-13T12:00:00+00:00"
        );
        let atom=br#"<feed xmlns="http://www.w3.org/2005/Atom"><id>urn:fixture</id><title>Atom fixture</title><updated>2026-11-13T12:00:00Z</updated><entry><id>urn:item</id><title>Atom item</title><link href="/article"/><updated>2026-11-13T12:00:00Z</updated></entry></feed>"#;
        let feed = parse(download(atom)).unwrap();
        assert_eq!(feed.entries[0].links, ["https://source.invalid/article"]);
    }
    #[test]
    fn malformed_external_entities_deep_and_utf16_xml_fail_closed() {
        for bytes in [b"<rss><channel>".as_slice(),br#"<!DOCTYPE rss [<!ENTITY ext SYSTEM "file:///etc/passwd">]><rss><channel><title>&ext;</title></channel></rss>"#.as_slice(),br#"<!DOCTYPE rss SYSTEM "https://private.invalid/xxe"><rss/>"#.as_slice(),b"not XML".as_slice()] {assert!(parse(download(bytes)).is_err());}
        let deep = format!("{}{}", "<node>".repeat(65), "</node>".repeat(65));
        assert!(matches!(
            parse(download(deep.as_bytes())),
            Err(FetchError::TooLarge)
        ));
        assert!(matches!(
            parse(download(b"\xff\xfe<\0r\0s\0s\0")),
            Err(FetchError::UnsupportedEncoding)
        ));
    }
    #[tokio::test]
    async fn feed_client_uses_real_outbound_policy_and_bounded_parser() {
        use axum::{Router, routing::get};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener,Router::new().route("/rss",get(||async{r#"<rss version="2.0"><channel><title>Fixture</title><link>https://source.invalid</link><description>Fixture</description><item><title>Item</title><link>https://source.invalid/article</link></item></channel></rss>"#}))).await.unwrap();
        });
        let client = FeedClient::new(SafeHttp::fixture("fixture.invalid", address));
        let url = format!("http://fixture.invalid:{}/rss", address.port());
        let FeedResult::Fetched(data) = client
            .fetch(&url, 16384, &Validators::default())
            .await
            .unwrap()
        else {
            panic!("metadata expected")
        };
        assert_eq!(data.format, "rss2");
        assert_eq!(data.entries.len(), 1);
        assert!(data.response_bytes > 0);
        let permit = client
            .parse_slots
            .clone()
            .acquire_many_owned(2)
            .await
            .unwrap();
        assert!(matches!(
            client.fetch(&url, 16384, &Validators::default()).await,
            Err(FetchError::Busy)
        ));
        drop(permit);
        server.abort();
    }
}
