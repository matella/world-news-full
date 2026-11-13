//! Bounded, inert plain-text extraction. No JS, credentials, hidden body or paywall workarounds.
use super::http::{Download, DownloadResult, FetchError, SafeHttp};
use chrono::{DateTime, Utc};
use html_scraper::{ElementRef, Html, Node, Selector};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtractionError {
    Fetch(FetchError),
    UnsupportedDocument,
    TooLarge,
    AccessRestricted,
    NoArticle,
    Busy,
    Timeout,
}
impl ExtractionError {
    pub fn category(self) -> &'static str {
        match self {
            Self::Fetch(e) => e.category(),
            Self::UnsupportedDocument => "unsupported_document",
            Self::TooLarge => "article_too_large",
            Self::AccessRestricted => "access_restricted",
            Self::NoArticle => "no_article",
            Self::Busy => "parser_busy",
            Self::Timeout => "parser_timeout",
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extracted {
    pub final_url: String,
    pub response_hash: String,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub published: Option<DateTime<Utc>>,
    pub paragraphs: Vec<String>,
}
impl Extracted {
    pub(crate) fn valid(&self) -> bool {
        self.response_hash.len() == 64
            && self.response_hash.bytes().all(|b| b.is_ascii_hexdigit())
            && self.title.as_ref().is_none_or(|s| plain(s, 500))
            && self.authors.len() <= 20
            && self.authors.iter().all(|s| plain(s, 500))
            && !self.paragraphs.is_empty()
            && self.paragraphs.len() <= 500
            && self.paragraphs.iter().all(|s| plain(s, 20000))
            && self.paragraphs.iter().map(String::len).sum::<usize>() <= 200000
    }
}
fn plain(s: &str, max: usize) -> bool {
    !s.trim().is_empty()
        && s.chars().count() <= max
        && !s
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
}
#[derive(Clone)]
pub struct ArticleClient {
    http: SafeHttp,
    slots: Arc<Semaphore>,
}
impl ArticleClient {
    pub fn new(http: SafeHttp) -> Self {
        Self {
            http,
            slots: Arc::new(Semaphore::new(2)),
        }
    }
    pub async fn extract(&self, url: &str) -> Result<Extracted, ExtractionError> {
        let download = match self
            .http
            .get_same_origin(url, 2 * 1024 * 1024)
            .await
            .map_err(ExtractionError::Fetch)?
        {
            DownloadResult::Fetched(d) => d,
            _ => return Err(ExtractionError::UnsupportedDocument),
        };
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| ExtractionError::Busy)?;
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            parse(download)
        });
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .map_err(|_| ExtractionError::Timeout)?
            .map_err(|_| ExtractionError::UnsupportedDocument)?
    }
}
fn selector(s: &str) -> Selector {
    Selector::parse(s).expect("fixed selector")
}
fn excluded(element: ElementRef<'_>) -> bool {
    let value = element.value();
    let style = value
        .attr("style")
        .unwrap_or("")
        .split_whitespace()
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(
        value.name(),
        "script"
            | "style"
            | "template"
            | "noscript"
            | "iframe"
            | "object"
            | "embed"
            | "svg"
            | "math"
            | "nav"
            | "footer"
            | "aside"
            | "form"
    ) || value.attr("hidden").is_some()
        || value.attr("inert").is_some()
        || value
            .attr("aria-hidden")
            .is_some_and(|v| v.eq_ignore_ascii_case("true"))
        || style.contains("display:none")
        || style.contains("visibility:hidden")
}
fn visible(element: ElementRef<'_>) -> bool {
    !excluded(element)
        && !element
            .ancestors()
            .filter_map(ElementRef::wrap)
            .any(excluded)
}
fn text(element: ElementRef<'_>) -> String {
    let mut output = String::new();
    for node in element.descendants() {
        if let Node::Text(value) = node.value() {
            if !node.ancestors().filter_map(ElementRef::wrap).any(excluded) {
                output.push_str(value);
            }
        } else if let Some(el) = ElementRef::wrap(node)
            && el.value().name() == "br"
        {
            output.push(' ');
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn paid_json(v: &serde_json::Value, depth: usize) -> bool {
    if depth > 32 {
        return true;
    }
    match v {
        serde_json::Value::Object(o) => o.iter().any(|(k, v)| {
            k == "isAccessibleForFree" && (v == false || v == "false") || paid_json(v, depth + 1)
        }),
        serde_json::Value::Array(a) => a.iter().any(|v| paid_json(v, depth + 1)),
        _ => false,
    }
}
fn parse(download: Download) -> Result<Extracted, ExtractionError> {
    if download.bytes.len() > 2 * 1024 * 1024 {
        return Err(ExtractionError::TooLarge);
    }
    let kind = download
        .content_type
        .as_deref()
        .ok_or(ExtractionError::UnsupportedDocument)?;
    let mut parts = kind.split(';');
    if !parts
        .next()
        .unwrap_or("")
        .trim()
        .eq_ignore_ascii_case("text/html")
        || parts.any(|p| {
            let p = p.trim().to_ascii_lowercase();
            p.starts_with("charset=")
                && !["charset=utf-8", "charset=utf8", "charset=\"utf-8\""].contains(&p.as_str())
        })
    {
        return Err(ExtractionError::UnsupportedDocument);
    }
    let input =
        std::str::from_utf8(&download.bytes).map_err(|_| ExtractionError::UnsupportedDocument)?;
    if input.contains('\0') {
        return Err(ExtractionError::UnsupportedDocument);
    }
    let doc = Html::parse_document(input);
    let root = doc.root_element();
    for (i, node) in root.descendants().enumerate() {
        if i > 100000 || node.ancestors().take(129).count() > 128 {
            return Err(ExtractionError::TooLarge);
        }
    }
    for meta in doc.select(&selector("meta[charset]")) {
        if !["utf-8", "utf8"].contains(
            &meta
                .value()
                .attr("charset")
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
        ) {
            return Err(ExtractionError::UnsupportedDocument);
        }
    }
    if doc
        .select(&selector(
            "[data-paywall], .paywall, [itemprop='isAccessibleForFree'][content='false']",
        ))
        .next()
        .is_some()
    {
        return Err(ExtractionError::AccessRestricted);
    }
    for node in doc.select(&selector("script[type='application/ld+json']")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&node.text().collect::<String>())
            && paid_json(&v, 0)
        {
            return Err(ExtractionError::AccessRestricted);
        }
    }
    let containers: Vec<_> = doc
        .select(&selector("article"))
        .filter(|e| visible(*e))
        .collect();
    let containers = if containers.is_empty() {
        doc.select(&selector("main"))
            .filter(|e| visible(*e))
            .collect()
    } else {
        containers
    };
    // Ambiguous page aggregations need manual intake, not concatenated articles.
    if containers.len() != 1 {
        return Err(ExtractionError::NoArticle);
    }
    let article = containers[0];
    let mut paragraphs = Vec::new();
    for paragraph in article.select(&selector("p")) {
        if visible(paragraph) {
            let text = text(paragraph);
            if !text.is_empty() {
                paragraphs.push(text);
            }
        }
    }
    let title = article
        .select(&selector("h1"))
        .find(|e| visible(*e))
        .map(text)
        .filter(|s| !s.is_empty());
    let authors = doc
        .select(&selector("meta[name='author']"))
        .filter_map(|e| e.value().attr("content").map(str::to_owned))
        .collect();
    let published = doc
        .select(&selector("meta[property='article:published_time']"))
        .filter_map(|e| e.value().attr("content"))
        .find_map(|v| DateTime::parse_from_rfc3339(v).ok())
        .map(|v| v.with_timezone(&Utc));
    let result = Extracted {
        final_url: download.final_url,
        response_hash: format!("{:x}", Sha256::digest(&download.bytes)),
        title,
        authors,
        published,
        paragraphs,
    };
    if !result.valid() {
        return Err(if result.paragraphs.is_empty() {
            ExtractionError::NoArticle
        } else {
            ExtractionError::TooLarge
        });
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn document(html: &str) -> Download {
        Download {
            final_url: "https://source.invalid/a".into(),
            bytes: html.as_bytes().to_vec(),
            validators: Default::default(),
            content_type: Some("text/html; charset=utf-8".into()),
        }
    }
    #[test]
    fn extracts_inert_paragraphs_and_explicit_metadata() {
        let result=parse(document(r#"<meta name="author" content="Fixture writer"><meta property="article:published_time" content="2026-11-13T12:00:00Z"><article><h1>Title &amp; context</h1><p>Hello <strong>world</strong>.<script>secret-script</script><iframe>secret-frame</iframe></p><p hidden>secret-hidden</p><p style="display: none">secret-style</p><aside><p>secret-aside</p></aside><p>Second<br>paragraph &amp; context.</p></article>"#)).unwrap();
        assert_eq!(result.title.as_deref(), Some("Title & context"));
        assert_eq!(
            result.paragraphs,
            ["Hello world.", "Second paragraph & context."]
        );
        assert_eq!(result.authors, ["Fixture writer"]);
        assert!(result.published.is_some());
        assert!(!serde_json::to_string(&result).unwrap().contains("secret-"));
    }
    #[test]
    fn inaccessible_ambiguous_oversize_and_non_html_fail_closed() {
        for html in [
            "<article><p>Teaser</p><div data-paywall>Pay</div></article>",
            r#"<script type="application/ld+json">{"isAccessibleForFree":false,"articleBody":"never copied"}</script><article><p>Teaser</p></article>"#,
        ] {
            assert!(matches!(
                parse(document(html)),
                Err(ExtractionError::AccessRestricted)
            ));
        }
        for html in [
            "<p>Body without article boundary</p>",
            "<article><p>One</p></article><article><p>Two</p></article>",
            "<article hidden><p>Hidden</p></article>",
        ] {
            assert!(matches!(
                parse(document(html)),
                Err(ExtractionError::NoArticle)
            ));
        }
        assert!(
            parse(document(&format!(
                "<article><p>{}</p></article>",
                "x".repeat(20001)
            )))
            .is_err()
        );
        let mut d = document("<article><p>Text</p></article>");
        d.content_type = Some("application/json".into());
        assert!(parse(d).is_err());
        assert!(
            parse(document(
                "<meta charset='iso-8859-1'><article><p>Text</p></article>"
            ))
            .is_err()
        );
    }
    #[tokio::test]
    async fn real_client_uses_shared_policy_and_blocks_cross_origin_before_contact() {
        use axum::{Router, response::Redirect, routing::get};
        use std::sync::atomic::{AtomicUsize, Ordering};
        let hits = Arc::new(AtomicUsize::new(0));
        let seen = hits.clone();
        let other = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let other_addr = other.local_addr().unwrap();
        let other_task = tokio::spawn(async move {
            axum::serve(
                other,
                Router::new().route(
                    "/forbidden",
                    get(move || {
                        seen.fetch_add(1, Ordering::SeqCst);
                        async { "forbidden" }
                    }),
                ),
            )
            .await
            .unwrap()
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new()
                    .route("/start", get(|| async { Redirect::temporary("/article") }))
                    .route(
                        "/article",
                        get(|| async {
                            axum::response::Html(
                                "<article><h1>Fixture</h1><p>Plain text</p></article>",
                            )
                        }),
                    )
                    .route(
                        "/external",
                        get(|| async { Redirect::temporary("https://other.invalid/article") }),
                    )
                    .route(
                        "/away",
                        get(move || async move {
                            Redirect::temporary(&format!(
                                "http://127.0.0.1:{}/forbidden",
                                other_addr.port()
                            ))
                        }),
                    ),
            )
            .await
            .unwrap()
        });
        let client = ArticleClient::new(SafeHttp::fixture("fixture.invalid", address));
        let base = format!("http://fixture.invalid:{}", address.port());
        assert_eq!(
            client
                .extract(&format!("{base}/start"))
                .await
                .unwrap()
                .paragraphs,
            ["Plain text"]
        );
        assert!(matches!(
            client.extract(&format!("{base}/away")).await,
            Err(ExtractionError::Fetch(FetchError::BlockedTarget))
        ));
        assert!(matches!(
            client.extract(&format!("{base}/external")).await,
            Err(ExtractionError::Fetch(FetchError::BlockedTarget))
        ));
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        task.abort();
        other_task.abort();
    }
}
