use super::*;

impl QdrantVectorStore {
    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.base_url.trim_end_matches('/'), path)
    }

    pub(crate) fn wait_suffix(&self) -> &'static str {
        if self.config.wait {
            "?wait=true"
        } else {
            "?wait=false"
        }
    }

    pub(crate) fn batch_size(&self) -> usize {
        self.config.batch_size.max(1)
    }

    pub(crate) async fn send(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        let mut request = request;
        if let Some(api_key) = &self.config.api_key {
            request = request.header("api-key", api_key.clone());
        }
        for (key, value) in &self.config.headers {
            request = request.header(key.clone(), value.clone());
        }
        request
            .send()
            .await
            .map_err(|err| VectorError::Transport(err.to_string()))
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
    ) -> Result<reqwest::Response> {
        let mut attempt = 0;
        loop {
            match self.send(build()).await {
                Ok(response) => {
                    let status = response.status();
                    if Self::is_retryable_status(status) {
                        if attempt < MAX_READ_RETRIES {
                            attempt += 1;
                            tracing::warn!(backend = "qdrant",
                                attempt,
                                status = %status,
                                "qdrant read retry on transient status"
                            );
                            tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                            continue;
                        }
                        tracing::warn!(backend = "qdrant",
                            attempt,
                            status = %status,
                            "qdrant read retries exhausted on transient status"
                        );
                    }
                    return Ok(response);
                }
                Err(VectorError::Transport(err)) => {
                    if attempt < MAX_READ_RETRIES {
                        attempt += 1;
                        tracing::warn!(backend = "qdrant",
                            attempt,
                            error = %err,
                            "qdrant read transport failure; retrying"
                        );
                        tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                        continue;
                    }
                    tracing::warn!(backend = "qdrant",
                        attempt,
                        error = %err,
                        "qdrant read transport retries exhausted"
                    );
                    return Err(VectorError::Transport(err));
                }
                Err(other) => return Err(other),
            }
        }
    }

    pub(crate) fn map_status(
        status: reqwest::StatusCode,
        body: &str,
        collection: &str,
    ) -> Result<()> {
        if status.is_success() {
            return Ok(());
        }
        let diagnostic = diagnostic_body(body);
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(VectorError::Unauthorized(format!(
                "qdrant rejected credentials (status {status})"
            )));
        }
        if status == reqwest::StatusCode::BAD_REQUEST
            || status == reqwest::StatusCode::UNPROCESSABLE_ENTITY
        {
            return Err(VectorError::InvalidInput(format!(
                "qdrant rejected request (status {status}): {diagnostic}"
            )));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
            return Err(VectorError::Transport(format!(
                "qdrant transient failure (status {status}): {diagnostic}"
            )));
        }
        Err(VectorError::Backend(format!(
            "qdrant status {status}: {diagnostic}"
        )))
    }

    pub(crate) fn is_dimension_body(body: &str) -> bool {
        let lower = body.to_ascii_lowercase();
        lower.contains("dimension")
            || lower.contains("vector size")
            || lower.contains("size") && lower.contains("vector")
    }
}
