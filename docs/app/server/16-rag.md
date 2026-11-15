# rag 域 HTTP 端点
> 操作数：3
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| GET | `/api/v1/rag/diagnose` | `handle_rag_diagnose` | query (q), top_k (q), filter_fields (q), score_threshold (q), collection (q), namespace (q) |
| GET | `/api/v1/rag/preview` | `handle_rag_preview` | collection (q), namespace (q), query (q), top_k (q), excerpt_chars (q) |
| GET | `/api/v1/rag/stats` | `handle_rag_stats` | collection (q), namespace (q) |
