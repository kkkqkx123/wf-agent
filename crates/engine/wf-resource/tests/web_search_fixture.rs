//! Integration fixtures for the web search backend family.
//!
//! The HTML fixture is a real DuckDuckGo response captured through the
//! configured proxy. Tests here replay it locally so the parsed contract is
//! stable without depending on the live search service.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use wf_config::processor::network::{
    HttpMethod, HttpRequestConfig, HttpResponseConfig, HttpResponseFormat,
};
use wf_resource::network::HttpSearchBackend;
use wf_resource::ResolvedProxy;
use wf_tools::{WebSearchBackend, WebSearchQuery, WebSearchResult};

#[derive(Debug, Deserialize)]
struct ExpectedFixture {
    query: String,
    endpoint: String,
    results: Vec<WebSearchResult>,
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/web_search")
        .join(name)
}

fn expected() -> ExpectedFixture {
    let text = fs::read_to_string(fixture_path("duckduckgo.expected.json"))
        .expect("expected fixture must be readable");
    serde_json::from_str(&text).expect("expected fixture must be valid JSON")
}

#[test]
fn real_html_matches_expected_results() {
    let html = fs::read_to_string(fixture_path("duckduckgo.html"))
        .expect("HTML fixture must be readable");
    let fixture = expected();
    assert_eq!(fixture.endpoint, "https://html.duckduckgo.com/html/");
    assert_eq!(fixture.query, "Rust async programming");

    let results = wf_resource::network::duckduckgo::parse_duckduckgo_results(&html);
    assert_eq!(results, fixture.results);
}

#[tokio::test]
async fn declarative_backend_replays_fixture_over_local_http() {
    let html = fs::read(fixture_path("duckduckgo.html"))
        .expect("HTML fixture must be readable");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local fixture server");
    let address = listener.local_addr().expect("resolve local fixture address");
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                html.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(&html);
        }
    });

    let backend = HttpSearchBackend::new(
        "fixture".into(),
        HttpRequestConfig {
            url: format!("http://{address}/search"),
            method: HttpMethod::Get,
            headers: Default::default(),
            query: Default::default(),
            json_body: None,
        },
        HttpResponseConfig {
            format: HttpResponseFormat::DuckduckgoHtml,
            items_path: None,
            map: None,
            url_prefix: None,
        },
        5_000,
        ResolvedProxy::default(),
    )
    .expect("fixture backend builds");
    let response = backend
        .search(WebSearchQuery {
            query: "Rust async programming".into(),
            max_results: 5,
            backend: None,
        })
        .await
        .expect("fixture search succeeds");
    assert_eq!(response.results, expected().results);
}
