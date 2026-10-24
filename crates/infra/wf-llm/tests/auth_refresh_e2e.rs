//! Integration coverage for DeclaredOAuth::refresh credential handling.

use std::collections::BTreeMap;

use wf_llm::http_mock::{MockRequest, MockResponse, MockServer};

use wf_llm::auth::{DeclaredFlow, DeclaredOAuth, DeclaredOAuthConfig};
use wf_llm::OAuthAuth;
use wf_llm::OAuthCredential;

fn device_config(token_url: String) -> DeclaredOAuthConfig {
    DeclaredOAuthConfig {
        display_name: "Example Sub".to_string(),
        flow: DeclaredFlow::Device,
        is_subscription: true,
        device_authorization_url: Some("https://example.test/device".to_string()),
        token_url,
        authorize_url: None,
        client_id: "cid".to_string(),
        scope: Some("api".to_string()),
        redirect_uri: None,
        loopback: false,
        login_label: Some("Sign in".to_string()),
    }
}

#[tokio::test]
async fn refresh_reuses_old_refresh_token_when_server_omits_it() {
    let server = MockServer::spawn(|_: &MockRequest| {
        MockResponse::ok_json(r#"{"access_token":"at2","token_type":"Bearer","expires_in":3600}"#)
    })
    .await;
    let auth = DeclaredOAuth::new(device_config(server.url("/token")));
    let mut extra = BTreeMap::new();
    extra.insert("tenant".to_string(), serde_json::json!("acme"));
    let credential = OAuthCredential {
        refresh: "rt-old".to_string(),
        access: "at1".to_string(),
        expires: 0,
        is_subscription: true,
        extra: extra.clone(),
    };
    let refreshed = auth.refresh(&credential).await.expect("refresh");
    assert_eq!(refreshed.access, "at2");
    assert_eq!(
        refreshed.refresh, "rt-old",
        "old refresh token must be reused"
    );
    assert_eq!(refreshed.extra, extra, "extra must be preserved");
}

#[tokio::test]
async fn refresh_uses_server_refresh_token_when_provided() {
    let server = MockServer::spawn(|_: &MockRequest| {
        MockResponse::ok_json(
            r#"{"access_token":"at2","refresh_token":"rt-new","token_type":"Bearer","expires_in":3600}"#,
        )
    })
    .await;
    let auth = DeclaredOAuth::new(device_config(server.url("/token")));
    let credential = OAuthCredential {
        refresh: "rt-old".to_string(),
        access: "at1".to_string(),
        expires: 0,
        is_subscription: true,
        extra: Default::default(),
    };
    let refreshed = auth.refresh(&credential).await.expect("refresh");
    assert_eq!(refreshed.refresh, "rt-new");
}

#[tokio::test]
async fn refresh_stores_verbatim_expiry_without_safety_margin() {
    let server = MockServer::spawn(|_: &MockRequest| {
        MockResponse::ok_json(r#"{"access_token":"at2","refresh_token":"rt2","token_type":"Bearer","expires_in":3600}"#)
    })
    .await;
    let auth = DeclaredOAuth::new(device_config(server.url("/token")));
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let credential = OAuthCredential {
        refresh: "rt-old".to_string(),
        access: "at1".to_string(),
        expires: 0,
        is_subscription: true,
        extra: Default::default(),
    };
    let refreshed = auth.refresh(&credential).await.expect("refresh");
    assert!(refreshed.expires >= before + 3_600_000);
    assert!(refreshed.expires <= before + 3_600_000 + 5_000);
}
