use super::*;

impl QdrantVectorStore {
    /// Collection dimensions and distance (专有扩展, not part of the contract).
    pub async fn collection_info(&self, collection: &str) -> Result<(usize, DistanceMetric)> {
        let info = self.collection_config(collection).await?;
        Ok((info.dimension, info.distance))
    }

    /// Lists collection names (专有扩展 for operations and tests).
    pub async fn list_collections(&self) -> Result<Vec<String>> {
        let response = self
            .send_read(|| self.client.get(self.url("/collections")))
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(VectorError::Backend(format!(
                "qdrant list failed with status {status}: {}",
                diagnostic_body(&body)
            )));
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|err| VectorError::Decode(err.to_string()))?;
        let names = parsed
            .pointer("/result/collections")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        item.get("name")
                            .and_then(|n| n.as_str())
                            .map(str::to_string)
                    })
                    .collect::<Vec<_>>()
            })
            .ok_or_else(|| {
                VectorError::Decode("qdrant list response has no result.collections".to_string())
            })?;
        Ok(names)
    }

    /// Checks server reachability (专有扩展).
    pub async fn health_check(&self) -> Result<()> {
        let started = Instant::now();
        let response = self
            .send_read(|| self.client.get(self.url("/healthz")))
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        tracing::debug!(
            backend = "qdrant",
            elapsed_ms = started.elapsed().as_millis() as u64,
            "qdrant health check"
        );
        if status.is_success() {
            return Ok(());
        }
        Err(VectorError::Transport(format!(
            "qdrant health check failed with status {status}: {}",
            diagnostic_body(&body)
        )))
    }

    /// Returns the server version string (专有扩展).
    pub async fn version(&self) -> Result<String> {
        let response = self.send_read(|| self.client.get(self.url("/"))).await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(VectorError::Backend(format!(
                "qdrant version probe failed with status {status}: {}",
                diagnostic_body(&body)
            )));
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|err| VectorError::Decode(err.to_string()))?;
        parsed
            .get("version")
            .and_then(|v| v.as_str())
            .or_else(|| parsed.pointer("/result/version").and_then(|v| v.as_str()))
            .map(str::to_string)
            .ok_or_else(|| {
                VectorError::Decode("qdrant version response has no version".to_string())
            })
    }

    /// Warns on version drift without blocking (major must match, minor drift at most one).
    pub async fn check_compatibility(&self, client_version: &str) -> Result<bool> {
        let server = self.version().await?;
        let compatible = Self::is_compatible_version(client_version, &server);
        if !compatible {
            tracing::warn!(backend = "qdrant",
                client_version = client_version,
                server_version = %server,
                "qdrant version drift detected"
            );
        }
        Ok(compatible)
    }

    /// Checks whether the server version supports a retrieval kind. Sparse
    /// needs 1.7 and multi vectors need 1.10; dense is always supported.
    /// The probe is explicit so callers can fail fast before creating a
    /// collection the server cannot serve.
    /// Checks whether the server version supports Qdrant named vectors.
    /// Named layout is a capability gate independent of retrieval kinds.
    pub async fn check_named_space_support(&self) -> Result<bool> {
        let server = self.version().await?;
        let mut parts = server.split('.');
        let major = parts
            .next()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(0);
        let minor = parts
            .next()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(0);
        Ok((major, minor) >= (1, 0))
    }

    pub async fn check_kind_support(&self, kind: VectorKind) -> Result<bool> {
        let minimum = match kind {
            VectorKind::Dense => return Ok(true),
            VectorKind::Sparse => (1, 7),
            VectorKind::Multi => (1, 10),
        };
        let server = self.version().await?;
        let mut parts = server.split('.');
        let major = parts
            .next()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(0);
        let minor = parts
            .next()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(0);
        Ok((major, minor) >= minimum)
    }
}
