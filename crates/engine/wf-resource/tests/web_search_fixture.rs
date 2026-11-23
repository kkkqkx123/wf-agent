//! Integration fixtures for the web search backend family.
//!
//! The HTML fixture is a real DuckDuckGo response captured through the
//! configured proxy. Tests here replay it locally so the parsed contract is
//! stable without depending on the live search service.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use wf_config::processor::network::{
    HttpMethod, HttpRequestConfig, HttpResponseConfig, HttpResponseFormat,
};
use wf_resource::network::HttpSearchBackend;
use wf_tools::{
    NetworkDomainPolicy, NetworkProxy, WebSearchBackend, WebSearchQuery, WebSearchResult,
};

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
    let html =
        fs::read_to_string(fixture_path("duckduckgo.html")).expect("HTML fixture must be readable");
    let fixture = expected();
    assert_eq!(fixture.endpoint, "https://html.duckduckgo.com/html/");
    assert_eq!(fixture.query, "Rust async programming");

    let results = wf_resource::network::duckduckgo::parse_duckduckgo_results(&html);
    assert!(results.len() >= fixture.results.len());
    assert_eq!(
        &results[..fixture.results.len()],
        fixture.results.as_slice()
    );
}

#[tokio::test]
async fn declarative_backend_replays_fixture_over_local_http() {
    let html = fs::read(fixture_path("duckduckgo.html")).expect("HTML fixture must be readable");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local fixture server");
    let address = listener
        .local_addr()
        .expect("resolve local fixture address");
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
        10,
        NetworkProxy::default(),
        NetworkDomainPolicy::default(),
    )
    .expect("fixture backend builds");
    let response = backend
        .search(WebSearchQuery {
            query: "Rust async programming".into(),
            max_results: 5,
            backend: None,
            ..Default::default()
        })
        .await
        .expect("fixture search succeeds");
    assert_eq!(response.results, expected().results);
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut data = Vec::new();
    let mut buffer = [0u8; 1024];
    loop {
        let count = stream.read(&mut buffer).expect("read mock request");
        if count == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..count]);
        let text = String::from_utf8_lossy(&data);
        let Some(header_end) = text.find("\r\n\r\n") else {
            continue;
        };
        let content_length = text.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())?
        });
        if data.len() >= header_end + 4 + content_length.unwrap_or(0) {
            break;
        }
    }
    String::from_utf8_lossy(&data).into_owned()
}

#[tokio::test]
async fn declarative_backend_renders_post_request_and_maps_json() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local JSON server");
    let address = listener.local_addr().expect("resolve local JSON address");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let request = read_request(&mut stream);
            captured.lock().expect("capture request").push(request);
            let body = br#"{"data":{"items":[{"title":"T","url":"/p","snippet":"S"}]}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(body);
        }
    });

    let backend = HttpSearchBackend::new(
        "json_api".into(),
        HttpRequestConfig {
            url: format!("http://{address}/search?fixed=1"),
            method: HttpMethod::Post,
            headers: BTreeMap::from([("X-Query".to_string(), "$query".to_string())]),
            query: BTreeMap::from([
                ("q".to_string(), "$query".to_string()),
                ("n".to_string(), "$max_results".to_string()),
            ]),
            json_body: Some(serde_json::json!({
                "q": "$query",
                "count": "$max_results"
            })),
        },
        HttpResponseConfig {
            format: HttpResponseFormat::Json,
            items_path: Some("data.items".into()),
            map: Some(wf_config::processor::network::ResultFieldMap {
                title: "title".into(),
                url: "url".into(),
                snippet: "snippet".into(),
            }),
            url_prefix: Some("https://example.com".into()),
        },
        5_000,
        10,
        NetworkProxy::default(),
        NetworkDomainPolicy::default(),
    )
    .expect("JSON backend builds");
    let response = backend
        .search(WebSearchQuery {
            query: "rust async".into(),
            max_results: 3,
            backend: None,
            ..Default::default()
        })
        .await
        .expect("JSON search succeeds");

    assert_eq!(response.results.len(), 1);
    assert_eq!(response.results[0].url, "https://example.com/p");
    assert_eq!(response.results[0].snippet, "S");
    let captured = requests.lock().expect("read captured requests");
    let request = captured.first().expect("request captured");
    assert!(request.starts_with("POST /search?fixed=1&"));
    let request_line = request.lines().next().expect("request line captured");
    assert!(request_line.contains("&n=3&"));
    assert!(request_line.contains("&q=rust+async"));
    assert!(request.to_ascii_lowercase().contains("x-query: rust async"));
    assert!(request.contains("\"q\":\"rust async\""));
    assert!(request.contains("\"count\":\"3\""));
}
