use super::*;

impl ElasticKeywordIndex {
    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.config.base_url.trim_end_matches('/'))
    }

    pub(crate) fn batch_size(&self) -> usize {
        self.config.batch_size.max(1)
    }

    pub(crate) async fn send(
        &self,
        request: reqwest::RequestBuilder,
    ) -> KeywordResult<reqwest::Response> {
        let mut request = request;
        if let Some(api_key) = self.config.api_key.as_deref().filter(|key| !key.is_empty()) {
            request = request.header("Authorization", format!("ApiKey {api_key}"));
        }
        request
            .send()
            .await
            .map_err(|err| KeywordError::backend(err.to_string()))
    }

    pub(crate) fn is_retryable_status(status: reqwest::StatusCode) -> bool {
        matches!(
            status,
            reqwest::StatusCode::TOO_MANY_REQUESTS
                | reqwest::StatusCode::BAD_GATEWAY
                | reqwest::StatusCode::SERVICE_UNAVAILABLE
                | reqwest::StatusCode::GATEWAY_TIMEOUT
        )
    }

    /// Sends one idempotent read with bounded retries on rate limiting,
    /// transient server errors and transport failures.
    pub(crate) async fn send_read(
        &self,
        build: impl Fn() -> reqwest::RequestBuilder,
    ) -> KeywordResult<reqwest::Response> {
        let mut attempt = 0;
        loop {
            match self.send(build()).await {
                Ok(response) => {
                    if Self::is_retryable_status(response.status()) && attempt < MAX_READ_RETRIES {
                        attempt += 1;
                        tracing::warn!(
                            backend = "elastic",
                            attempt = attempt,
                            status = %response.status(),
                            "retrying transient remote failure"
                        );
                        tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                        continue;
                    }
                    return Ok(response);
                }
                Err(KeywordError::Backend(ref error)) if attempt < MAX_READ_RETRIES => {
                    attempt += 1;
                    tracing::warn!(
                        backend = "elastic",
                        attempt = attempt,
                        error = %error,
                        "retrying transient transport failure"
                    );
                    tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                    continue;
                }
                Err(other) => return Err(other),
            }
        }
    }

    pub(crate) fn status_error(
        context: &str,
        status: reqwest::StatusCode,
        body: &str,
    ) -> KeywordError {
        KeywordError::backend(format!(
            "elastic {context} failed (status {status}): {}",
            diagnostic_body(body)
        ))
    }

    pub(crate) fn auth_error(status: reqwest::StatusCode, body: &str) -> Option<KeywordError> {
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            Some(KeywordError::backend(format!(
                "elastic unauthorized (status {status}): {}",
                diagnostic_body(body)
            )))
        } else {
            None
        }
    }

    pub(crate) fn bad_request_error(
        status: reqwest::StatusCode,
        body: &str,
    ) -> Option<KeywordError> {
        if status == reqwest::StatusCode::BAD_REQUEST {
            Some(KeywordError::invalid_input(format!(
                "elastic invalid request (status {status}): {}",
                diagnostic_body(body)
            )))
        } else {
            None
        }
    }

    pub(crate) fn decode_error(err: impl std::fmt::Display) -> KeywordError {
        KeywordError::backend(format!("elastic decode error: {err}"))
    }

    /// Checks server reachability (proprietary extension, not part of the contract).
    pub async fn health_check(&self) -> KeywordResult<()> {
        let response = self.send_read(|| self.client.get(self.url("/"))).await?;
        if response.status().is_success() {
            return Ok(());
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if let Some(err) = Self::auth_error(status, &body) {
            return Err(err);
        }
        Err(Self::status_error("health check", status, &body))
    }

    /// Returns the server version string (proprietary extension).
    pub async fn version(&self) -> KeywordResult<String> {
        let response = self.send_read(|| self.client.get(self.url("/"))).await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if let Some(err) = Self::auth_error(status, &body) {
                return Err(err);
            }
            return Err(Self::status_error("version probe", status, &body));
        }
        let body = response.text().await.unwrap_or_default();
        let parsed: serde_json::Value = serde_json::from_str(&body).map_err(Self::decode_error)?;
        parsed
            .pointer("/version/number")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .ok_or_else(|| {
                KeywordError::backend("elastic version response has no number".to_string())
            })
    }

    /// Warns on major-version drift without blocking startup: the major
    /// must match the expected line, minors drift by at most one.
    pub fn version_drift_warning(version: &str, expected_major: u64) -> Option<String> {
        let mut parts = version.split('.');
        let major: u64 = parts.next()?.trim().parse().ok()?;
        if major != expected_major {
            return Some(format!(
                "elastic version drift: server {version} major differs from expected {expected_major}"
            ));
        }
        None
    }
}
