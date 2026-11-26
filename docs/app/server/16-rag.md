# rag 域 HTTP 端点
> 操作数：7
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| GET | `/api/v1/rag/diagnose` | `handle_rag_diagnose` | query (q), top_k (q), filter_fields (q), score_threshold (q), collection (q), namespace (q) |
| GET | `/api/v1/rag/preview` | `handle_rag_preview` | collection (q), namespace (q), query (q), top_k (q), excerpt_chars (q) |
| GET | `/api/v1/rag/stats` | `handle_rag_stats` | collection (q), namespace (q) |
| GET | `/api/v1/rag/sources` | `handle_rag_sources` | collection (q), namespace (q) |
| POST | `/api/v1/rag/sources` | `handle_rag_ingest_source` | body (`RagSourceWrite`) |
| DELETE | `/api/v1/rag/sources` | `handle_rag_delete_source` | body (`RagSourceDelete`) |
| POST | `/api/v1/rag/sources/reindex` | `handle_rag_reindex_source` | body (`RagSourceWrite`) |

响应数据采用 `ApiEnvelope<T>`：统计、预览命中、诊断原因、source 状态条目与摄取结果均有独立 schema；删除操作返回受影响数量。RAG 路由整体受 `rag-vector` feature 控制，未注入检索句柄或 source service 时返回 `SERVICE_UNAVAILABLE`。

域服务与装配边界见 [../api/10-rag-domain.md](../api/10-rag-domain.md)；前端与 CLI 的消费方式分别见 [../webapp/02-rag.md](../webapp/02-rag.md) 与 [../cli/07-rag.md](../cli/07-rag.md)。
