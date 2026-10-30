//! End-to-end coverage for the in-tree provider drivers against a mock
//! HTTP server: full login and refresh cycles with no real network.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use serde_json::{json, Value};
use wf_llm::http_mock::{MockRequest, MockResponse, MockServer};
use wf_llm::llm_auth::credential::Credential;
use wf_llm::llm_auth::interaction::{AuthEvent, AuthPrompt};
use wf_llm::llm_auth::session::{ExternalFlowDriver, ExternalStep};
use wf_llm::provider_flows::{
    AnthropicAuth, AnthropicParams, CopilotUrls, GithubCopilotAuth, GithubCopilotParams,
    MetaMuseAuth, MetaMuseParams,
};
use wf_llm::{OAuthAuth, OAuthCredential};

fn json_body(text: &str) -> Value {
    serde_json::from_str(text).expect("mock request body is JSON")
}

#[tokio::test]
async fn copilot_full_login_exchanges_then_refreshes() {
    let polls = Arc::new(AtomicUsize::new(0));
    let polls_in_handler = Arc::clone(&polls);
    let server = MockServer::spawn(move |req: &MockRequest| match req.path.as_str() {
        "/login/device/code" => MockResponse::ok_json(
            r#"{"device_code":"dc-1","user_code":"WDJB-MJHT","verification_uri":"https://github.com/login/device","interval":1,"expires_in":900}"#,
        ),
        "/login/oauth/access_token" => {
            if polls_in_handler.fetch_add(1, Ordering::SeqCst) == 0 {
                MockResponse::ok_json(r#"{"error":"authorization_pending"}"#)
            } else {
                MockResponse::ok_json(r#"{"access_token":"github-token-1","token_type":"bearer"}"#)
            }
        }
        "/copilot_internal/v2/token" => {
            assert!(req.body.is_empty(), "copilot mint is a GET with no body");
            MockResponse::ok_json(
                r#"{"token":"tid=1;exp=2;proxy-ep=proxy.individual.githubcopilot.com;","expires_at":1900000000}"#,
            )
        }
        other => MockResponse::status(404, format!("unexpected path {other}")),
    })
    .await;

    let mut params = GithubCopilotParams::from_declared(None, None);
    params.endpoints = Some(CopilotUrls {
        device_code_url: server.url("/login/device/code"),
        access_token_url: server.url("/login/oauth/access_token"),
        copilot_token_url: server.url("/copilot_internal/v2/token"),
    });
    let auth = GithubCopilotAuth::new("GitHub Copilot", None, params);

    // Domain prompt, then device start.
    let initial = auth.initial(&Value::Null).await.expect("initial");
    let ExternalStep::AwaitingInput { state, prompt, .. } = initial else {
        panic!("expected domain prompt");
    };
    assert!(matches!(prompt, AuthPrompt::Text { .. }));
    let pending = auth.submit(&state, String::new()).await.expect("submit");
    let ExternalStep::Pending { state, events } = pending else {
        panic!("expected device pending");
    };
    assert!(events.iter().any(|e| matches!(
        e,
        AuthEvent::DeviceCode { user_code, .. } if user_code == "WDJB-MJHT"
    )));

    // First poll pends, second completes with the exchanged token.
    let first = auth.poll(&state).await.expect("poll 1");
    assert!(matches!(first, ExternalStep::Pending { .. }));
    let completed = auth
        .poll(&state_after_first_poll(&state))
        .await
        .expect("poll 2");
    let ExternalStep::Completed { credential } = completed else {
        panic!("expected completed login");
    };
    let Credential::OAuth(credential) = credential else {
        panic!("expected oauth credential");
    };
    assert_eq!(credential.refresh, "github-token-1");
    assert!(credential.access.contains("proxy-ep="));
    assert!(credential.is_subscription);

    // resolve derives the per-request base URL from the token.
    let model_auth = auth.to_auth(&credential).await.expect("to_auth");
    assert_eq!(
        model_auth.base_url.as_deref(),
        Some("https://api.individual.githubcopilot.com")
    );

    // Refresh re-mints through the Copilot endpoint.
    let refreshed = auth.refresh(&credential).await.expect("refresh");
    assert_eq!(refreshed.refresh, "github-token-1");
    assert!(refreshed.access.contains("proxy-ep="));
}

/// The driver stamps `next_poll_at_ms` into the session; step past it so
/// the second poll performs network I/O instead of short-circuiting.
fn state_after_first_poll(state: &Value) -> Value {
    let mut state = state.clone();
    if let Some(object) = state.as_object_mut() {
        object.insert("next_poll_at_ms".to_owned(), json!(0));
    }
    state
}

#[tokio::test]
async fn meta_full_login_mints_then_refreshes() {
    let server = MockServer::spawn(|req: &MockRequest| match req.path.as_str() {
        "/device" => MockResponse::ok_json(
            r#"{"device_code":"dc-9","user_code":"ABCD-EFGH","verification_uri":"https://auth.meta.com/activate","verification_uri_complete":"https://auth.meta.com/activate?code=ABCD-EFGH","interval":1,"expires_in":900}"#,
        ),
        "/token" => MockResponse::ok_json(r#"{"access_token":"identity-1"}"#),
        "/mint" => {
            assert_eq!(json_body(&req.body), json!({}));
            MockResponse::ok_json(r#"{"api_key":"meta-key-1"}"#)
        }
        other => MockResponse::status(404, format!("unexpected path {other}")),
    })
    .await;

    let mut params = MetaMuseParams::from_declared(None, None, None);
    params.device_authorization_url = server.url("/device");
    params.token_url = server.url("/token");
    params.mint_url = Some(server.url("/mint"));
    let auth = MetaMuseAuth::new("Meta", None, params);

    // No prompt: login starts the device flow immediately.
    let initial = auth.initial(&Value::Null).await.expect("initial");
    let ExternalStep::Pending { state, events } = initial else {
        panic!("expected device pending");
    };
    assert!(events.iter().any(|e| matches!(
        e,
        AuthEvent::DeviceCode {
            verification_uri,
            ..
        } if verification_uri.contains("code=ABCD-EFGH")
    )));

    // Poll mints the API key and completes.
    let completed = auth
        .poll(&state_after_first_poll(&state))
        .await
        .expect("poll");
    let ExternalStep::Completed { credential } = completed else {
        panic!("expected completed login, got {completed:?}");
    };
    let Credential::OAuth(credential) = credential else {
        panic!("expected oauth credential");
    };
    assert_eq!(credential.access, "meta-key-1");
    assert_eq!(credential.refresh, "identity-1");
    assert!(credential.is_subscription);

    let model_auth = auth.to_auth(&credential).await.expect("to_auth");
    assert_eq!(model_auth.api_key.as_deref(), Some("meta-key-1"));

    let refreshed = auth.refresh(&credential).await.expect("refresh");
    assert_eq!(refreshed.access, "meta-key-1");
}

#[tokio::test]
async fn anthropic_copy_code_login_exchanges_json_with_state() {
    let server = MockServer::spawn(|req: &MockRequest| match req.path.as_str() {
        "/token" => {
            let body = json_body(&req.body);
            assert_eq!(body["grant_type"], "authorization_code");
            assert_eq!(body["code"], "paste-code");
            assert_eq!(body["state"], body["code_verifier"]);
            assert!(body["code_verifier"].as_str().is_some());
            MockResponse::ok_json(
                r#"{"access_token":"at-1","refresh_token":"rt-1","expires_in":3600}"#,
            )
        }
        other => MockResponse::status(404, format!("unexpected path {other}")),
    })
    .await;

    let params = AnthropicParams {
        token_url: server.url("/token"),
        ..AnthropicParams::from_declared(None, None, None, None, None)
    };
    let auth = AnthropicAuth::new("Anthropic", None, params);

    let initial = auth.initial(&Value::Null).await.expect("initial");
    let ExternalStep::AwaitingInput { state, prompt, .. } = initial else {
        panic!("expected method choice");
    };
    assert!(matches!(prompt, AuthPrompt::Select { .. }));

    let awaiting = auth
        .submit(&state, "copy_code".to_owned())
        .await
        .expect("method submit");
    let ExternalStep::AwaitingInput { state, .. } = awaiting else {
        panic!("expected paste prompt");
    };
    let verifier = state["verifier"].as_str().expect("verifier").to_owned();

    // The pasted `code#state` round-trips through the JSON exchange.
    let completed = auth
        .submit(&state, format!("paste-code#{verifier}"))
        .await
        .expect("code submit");
    let ExternalStep::Completed { credential } = completed else {
        panic!("expected completed login, got {completed:?}");
    };
    let Credential::OAuth(credential) = credential else {
        panic!("expected oauth credential");
    };
    assert_eq!(credential.access, "at-1");
    assert_eq!(credential.refresh, "rt-1");
}

#[tokio::test]
async fn anthropic_browser_login_receives_loopback_callback() {
    let server = MockServer::spawn(|req: &MockRequest| match req.path.as_str() {
        "/token" => {
            let body = json_body(&req.body);
            assert_eq!(body["grant_type"], "authorization_code");
            assert_eq!(body["code"], "loop-code");
            MockResponse::ok_json(
                r#"{"access_token":"at-loop","refresh_token":"rt-loop","expires_in":3600}"#,
            )
        }
        other => MockResponse::status(404, format!("unexpected path {other}")),
    })
    .await;

    let params = AnthropicParams {
        token_url: server.url("/token"),
        ..AnthropicParams::from_declared(None, None, None, None, None)
    };
    let auth = AnthropicAuth::new("Anthropic", None, params);

    let initial = auth.initial(&Value::Null).await.expect("initial");
    let ExternalStep::AwaitingInput { state, .. } = initial else {
        panic!("expected method choice");
    };
    let pending = auth
        .submit(&state, "browser".to_owned())
        .await
        .expect("browser submit");
    let ExternalStep::Pending { state, events } = pending else {
        panic!("expected callback pending, got {pending:?}");
    };
    let redirect_uri = state["redirect_uri"]
        .as_str()
        .expect("redirect uri")
        .to_owned();
    let verifier = state["verifier"].as_str().expect("verifier").to_owned();
    assert!(events.iter().any(|e| matches!(
        e,
        AuthEvent::AuthUrl { url, .. } if url.contains("code=true")
    )));

    // Simulate the browser completing authorization against loopback.
    let callback = format!("{redirect_uri}?code=loop-code&state={verifier}");
    reqwest::Client::new()
        .get(callback)
        .send()
        .await
        .expect("callback request");

    // Poll until the listener task records the code.
    let mut completed = None;
    for _ in 0..50 {
        match auth.poll(&state).await.expect("poll") {
            ExternalStep::Completed { credential } => {
                completed = Some(credential);
                break;
            }
            ExternalStep::Pending { .. } => {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            other => panic!("unexpected poll step: {other:?}"),
        }
    }
    let Credential::OAuth(credential) = completed.expect("browser login completes after callback")
    else {
        panic!("expected oauth credential");
    };
    assert_eq!(credential.access, "at-loop");
    assert_eq!(credential.refresh, "rt-loop");
}

#[tokio::test]
async fn anthropic_refresh_reuses_previous_refresh_token() {
    let server = MockServer::spawn(|_: &MockRequest| {
        MockResponse::ok_json(r#"{"access_token":"at-2","expires_in":3600}"#)
    })
    .await;
    let params = AnthropicParams {
        token_url: server.url("/token"),
        ..AnthropicParams::from_declared(None, None, None, None, None)
    };
    let auth = AnthropicAuth::new("Anthropic", None, params);
    let credential = OAuthCredential {
        refresh: "rt-old".to_string(),
        access: "at-1".to_string(),
        expires: 0,
        is_subscription: true,
        extra: Default::default(),
    };
    let refreshed = auth.refresh(&credential).await.expect("refresh");
    assert_eq!(refreshed.access, "at-2");
    assert_eq!(refreshed.refresh, "rt-old");
}
