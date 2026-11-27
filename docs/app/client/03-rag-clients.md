# RAG 客户端接入

> 范围：`apps/web-app/src/lib/api` 中的 RAG 类型与服务、`crates/app/wf-client` 中构建期生成的类型化调用
> 上游契约：[../api/10-rag-domain.md](../api/10-rag-domain.md) 与 [../server/16-rag.md](../server/16-rag.md)

两个客户端都不理解检索算法，只负责把 RAG HTTP 契约变成易用的调用入口。Web 侧按领域封装服务函数，Rust 侧全部走构建期生成的类型化客户端，没有手拼 URL 的调用点。

## 1. 契约来源

RAG 响应注解已改为专用 DTO，OpenAPI 快照包含统计、预览命中、诊断、source 状态条目与摄取结果等 schema。Web 类型文件由 `tools/openapi-codegen` 从快照生成并提交入库；Rust 客户端由 `build.rs` 从同一快照在构建期生成，产物不入库。

新增或调整 RAG 字段时的顺序是：改服务端 DTO 与注解 → 刷新快照 → 重新生成 Web 类型 → Rust 下次构建自动跟随。任一步跳过都会让客户端类型落后于运行时 JSON。

## 2. Web 类型客户端

`apps/web-app/src/lib/services/rag.ts` 是 RAG 领域服务的唯一入口，类型直接取自生成 schema 的构件：

| 服务函数 | HTTP 操作 | 返回 |
|---|---|---|
| `ragStats` | `GET /rag/stats` | 集合统计 |
| `ragPreview` | `GET /rag/preview` | 预览命中数组 |
| `ragDiagnose` | `GET /rag/diagnose` | 可能原因数组 |
| `ragSources` | `GET /rag/sources` | source 状态条目数组 |
| `ingestRagSource` | `POST /rag/sources` | 摄取结果 |
| `reindexRagSource` | `POST /rag/sources/reindex`（`RagSourceRef`） | 摄取结果 |
| `deleteRagSource` | `DELETE /rag/sources`（`RagSourceRef`） | 受影响数量 |

所有调用统一经过既有信封拆包与缺载保护。服务层不做命名风格转换：后端字段已经是 snake_case，页面直接消费 DTO。错误由统一 `ApiHttpError` 抛出，页面通过错误码区分服务不可用与真实失败。

## 3. Web 预览替换

预览模式通过构建别名把 HTTP 客户端换成固件实现，服务层没有分支。RAG 固件补齐了：

- 固定集合统计、三条预览命中与三个 source 状态的固件数据；
- stats、preview、diagnose、sources 列表、摄取、重建、删除的 mock 路由；
- 可变 source 列表，使添加、重建和删除在预览中可以连续操作；
- 空命中查询词，用于演示无结果提示。

固件客户端的 DELETE 实现已改为透传请求体，否则 RAG source 删除在预览模式下无法携带 collection、namespace 和 source 三元组。

## 4. Rust 生成客户端

`wf-client` 的 RAG 调用全部走生成客户端：stats、preview、diagnose、sources 列举、摄取、重建、删除均使用 `handle_rag_*` 类型化方法，查询参数由 progenitor 序列化，请求体使用 `models::RagSourceWrite` / `models::RagSourceRef`，信封统一经 `unwrap_generated_envelope` 拆解。这条路径不存在手拼 URL，因此不会出现保留字符未编码导致的拆参问题。

`delete_json_with_body` 仍保留，用于其他领域带 body 的 DELETE；RAG 的删除已经改为类型化调用。

客户端不缓存 RAG 状态，也不在连接阶段探测 RAG 能力——能力判断留给具体命令的响应错误码，避免把领域可用性误当成连接可用性。

## 5. 边界与约束

- 两个客户端都不直接访问向量库、嵌入服务或文件系统。
- 生成产物不手改；OpenAPI 快照、Web 类型文件与 Rust 构建产物保持单向同步。
- RAG 错误沿用统一错误码：服务不可用表示宿主未装配能力，校验错误表示请求形态非法，二者都不应被渲染成空列表。
