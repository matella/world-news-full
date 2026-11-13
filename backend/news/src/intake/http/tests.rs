use super::*;
use axum::{Router, http::StatusCode as AxumStatus, response::IntoResponse, routing::get};
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn nonpublic_and_mixed_dns_answers_are_rejected() {
    for ip in [
        "127.0.0.1",
        "0.0.0.0",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "169.254.169.254",
        "100.100.100.200",
        "198.18.0.1",
        "192.0.2.1",
        "224.0.0.1",
        "255.255.255.255",
        "::1",
        "::",
        "fc00::1",
        "fe80::1",
        "::ffff:127.0.0.1",
        "64:ff9b::a00:1",
        "2002:7f00:1::",
        "2001:db8::1",
    ] {
        assert!(!public_address(ip.parse().unwrap()), "{ip}");
    }
    assert!(public_address("8.8.8.8".parse().unwrap()));
    assert!(public_address("2606:4700:4700::1111".parse().unwrap()));
    assert!(matches!(
        checked_addresses(vec![
            "8.8.8.8:443".parse().unwrap(),
            "127.0.0.1:443".parse().unwrap()
        ]),
        Err(FetchError::BlockedTarget)
    ));
}
#[tokio::test]
async fn real_fixture_requests_obey_pinning_redirect_limits_and_conditional_binding() {
    let hits = Arc::new(AtomicUsize::new(0));
    let private_hits = hits.clone();
    let routes = Router::new()
        .route(
            "/ok",
            get(|| async {
                (
                    [
                        ("etag", "\"synthetic-tag\""),
                        ("last-modified", "Fri, 13 Nov 2026 12:00:00 GMT"),
                    ],
                    "synthetic body",
                )
            }),
        )
        .route(
            "/conditional",
            get(|headers: axum::http::HeaderMap| async move {
                if headers
                    .get("if-none-match")
                    .is_some_and(|h| h == "\"synthetic-tag\"")
                {
                    assert_eq!(
                        headers.get("if-modified-since").unwrap(),
                        "Fri, 13 Nov 2026 12:00:00 GMT"
                    );
                    AxumStatus::NOT_MODIFIED.into_response()
                } else {
                    ([("etag", "\"synthetic-tag\"")], "new body").into_response()
                }
            }),
        )
        .route(
            "/relative",
            get(|| async { (AxumStatus::FOUND, [("location", "/ok")]) }),
        )
        .route(
            "/cycle",
            get(|| async { (AxumStatus::FOUND, [("location", "/cycle")]) }),
        )
        .route(
            "/private",
            get(move || {
                let hits = private_hits.clone();
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    "must not request"
                }
            }),
        )
        .route("/oversized", get(|| async { vec![b'x'; 17000] }))
        .route(
            "/limited",
            get(|| async { (AxumStatus::TOO_MANY_REQUESTS, [("retry-after", "7200")]) }),
        )
        .route(
            "/chunked",
            get(|| async {
                axum::body::Body::from_stream(futures_util::stream::iter([
                    Ok::<_, std::convert::Infallible>(vec![b'x'; 10000]),
                    Ok(vec![b'x'; 10000]),
                ]))
            }),
        )
        .route(
            "/dns-ssrf",
            get(|| async {
                (
                    AxumStatus::FOUND,
                    [("location", "http://rebind.invalid/private")],
                )
            }),
        )
        .route(
            "/compressed",
            get(|| async { ([("content-encoding", "gzip")], "not accepted") }),
        )
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                "late"
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let routes = routes.route(
        "/ssrf",
        get(move || async move {
            (
                AxumStatus::FOUND,
                [(
                    "location",
                    format!("http://127.0.0.1:{}/private", address.port()),
                )],
            )
        }),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, routes).await.unwrap();
    });
    let mut client = SafeHttp::fixture("fixture.invalid", address);
    client
        .dns_answers
        .insert("rebind.invalid".into(), vec![address]);
    let url = |path: &str| format!("http://fixture.invalid:{}{path}", address.port());
    let empty = Validators::default();
    assert!(matches!(
        client
            .get(
                &format!("{}?access_token=not-public", url("/private")),
                16384,
                &empty
            )
            .await,
        Err(FetchError::InvalidUrl)
    ));
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    let fetched = client.get(&url("/ok"), 16384, &empty).await.unwrap();
    let DownloadResult::Fetched(body) = fetched else {
        panic!("body expected")
    };
    assert_eq!(body.bytes, b"synthetic body");
    assert_eq!(body.validators.etag.as_deref(), Some("\"synthetic-tag\""));
    let mut cache = body.validators;
    cache.url = Some(url("/conditional"));
    assert!(matches!(
        client.get(&url("/conditional"), 16384, &cache).await,
        Ok(DownloadResult::NotModified(_))
    ));
    cache.url = Some(url("/different"));
    assert!(matches!(
        client.get(&url("/conditional"), 16384, &cache).await,
        Ok(DownloadResult::Fetched(_))
    ));
    assert!(matches!(
        client.get(&url("/relative"), 16384, &empty).await,
        Ok(DownloadResult::Fetched(_))
    ));
    assert!(matches!(
        client.get(&url("/cycle"), 16384, &empty).await,
        Err(FetchError::RedirectLimit)
    ));
    assert!(matches!(
        client.get(&url("/ssrf"), 16384, &empty).await,
        Err(FetchError::BlockedTarget)
    ));
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    assert!(matches!(
        client.get(&url("/dns-ssrf"), 16384, &empty).await,
        Err(FetchError::BlockedTarget)
    ));
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    assert!(matches!(
        client.get(&url("/oversized"), 16384, &empty).await,
        Err(FetchError::TooLarge)
    ));
    assert!(matches!(
        client.get(&url("/chunked"), 16384, &empty).await,
        Err(FetchError::TooLarge)
    ));
    assert!(matches!(
        client.get(&url("/compressed"), 16384, &empty).await,
        Err(FetchError::UnsupportedEncoding)
    ));
    assert!(matches!(
        client.get(&url("/limited"), 16384, &empty).await,
        Err(FetchError::HttpStatus(429, Some(7200)))
    ));
    let dns = client
        .dns_slots
        .clone()
        .acquire_many_owned(2)
        .await
        .unwrap();
    assert!(matches!(
        client
            .get("https://no-query.invalid/feed", 16384, &empty)
            .await,
        Err(FetchError::Busy)
    ));
    drop(dns);
    let permit = client.slots.clone().acquire_many_owned(4).await.unwrap();
    assert!(matches!(
        client.get(&url("/ok"), 16384, &empty).await,
        Err(FetchError::Busy)
    ));
    drop(permit);
    client.budget = Duration::from_millis(30);
    assert!(matches!(
        client.get(&url("/slow"), 16384, &empty).await,
        Err(FetchError::Timeout)
    ));
    server.abort();
}
