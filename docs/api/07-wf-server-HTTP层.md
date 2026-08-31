# wf-server HTTP 传输层

对应 `crates/wf-server`（约 11,100 行）。框架：**axum**（+ tokio/tower/futures/serde）。定位：**纯 HTTP 传输层**——无业务逻辑，每个 handler 调用 wf-api 并做信封封装，约 365 条路由（api/ 362 + 顶层 3）。

## 1. 路由结构（router.rs，95 行）

- `ApiState` = `Arc<wf_api::ApiContext>` + `Arc<ServerMiddlewareConfig>`。
- `api_router(ctx)`：域路由合并后嵌套于 `.nest("/api/v1", ...)`。
- 系统面在根路径：`/`、`/health`、`/api/v1/info`、`/api/v1/storage/*`、`/system/*`、`/metrics`（Prometheus 抓取）。
- 子模块经 `Router::merge` 组合（如 `workflows.rs` 合并 versions + graphs；`executions.rs` 合并 checkpoints + execution_state + execution_analysis；`agents.rs` 合并 profiles/loops/executions/variables/triggers）。

## 2. HTTP 端点清单（均位于 `/api/v1` 前缀下）

### 2a. workflow 域（14 文件）

**workflows.rs — 工作流 CRUD**：`GET|POST /workflows`；`GET|PUT|DELETE /workflows/{id}`；`POST /workflows/{id}/clone`；`POST /workflows/validate`、`/validate/node`；`POST /workflows/parse`、`/transform`；`GET /workflows/summaries`、`/search`、`/by-name/{name}`、`/by-tags`、`/by-category/{category}`、`/by-author/{author}`；`POST /workflows/export-all`、`GET /workflows/{id}/export?format=json|toml`、`POST /workflows/import`、`/import-many`；`PATCH /workflows/{id}/metadata`。

**versions.rs**：`GET|POST /workflows/{id}/versions`；`GET /workflows/{id}/versions/{version}`；`POST /workflows/{id}/rollback`、`/versions/increment?level=patch|minor|major`。

**graphs.rs（246）— 图查询**：`GET /workflows/{id}/graph`、`/graph/summary`、`/graph/nodes?node_type=`、`/graph/edges`、`/graph/neighbors/{nodeId}`、`/graph/analysis`、`/graph/cycles`、`/graph/topology`、`/graph/reachability`。

**executions.rs（439）— 执行控制**：`POST /workflows/{id}/execute`；`POST /workflows/{id}/execute/stream`（**SSE**）；`GET /executions`、`GET|DELETE /executions/{id}`；`POST /executions/{id}/pause|resume|cancel`；`GET /executions/{id}/status`、`/triggers`；`POST /execution-triggers/{id}/enable|disable`。

**checkpoints.rs（448）**：`POST /executions/{id}/checkpoints`；`GET /executions/{id}/checkpoints/chain`；`POST /executions/checkpoints/{cid}/restore|resume`；`GET|POST /checkpoints`；`GET|DELETE /checkpoints/{id}`；`GET|DELETE /checkpoints/entity/{entityId}`；`GET|PUT /checkpoints/entity/{entityId}/metadata`；`GET /checkpoints/entity/{entityId}/latest`；`GET /checkpoints/entities?entity_ids=`；`GET /checkpoints/time-range?workflowId=&start=&end=`；文件检查点：`GET|POST /file-checkpoints`；`GET|DELETE /file-checkpoints/{id}`；`GET /file-checkpoints/by-path`；`GET|DELETE /file-checkpoints/by-entity/{entityId}?keep_latest=`。

**execution_state.rs（428）**：`GET /executions/{id}/state|variables|transitions|context|call-stack|memory|variable-snapshots|context-evolution|state-analysis|context-transitions|context-snapshots|agent-state|agent-iterations|agent-variables`；`GET|DELETE /executions/{id}/state-records`；`/state-records/iterations/{iteration}`、`/snapshots/{timestamp}`、`/variables/{name}/history`、`/most-changed`、`/mutation-count`、`/call-stack`、`/memory`、`/memory/peak`。

**execution_analysis.rs（476）**：`GET /executions/{id}/graph`、`/graph/nodes|edges|neighbors/{nodeId}|path-stats|reachability`；`POST /executions/{id}/graph/clear`；`GET /executions/{id}/analysis/paths`、`/paths/enumerate`、`/decision-points`、`/slow-nodes?percentile=`、`/efficiency`、`/alternatives`、`/probabilities`；`GET /executions/{id}/nodes`、`/nodes/{nodeId}`、`/nodes/by-type/{nodeType}`、`/nodes/{nodeId}/input-context`、`/nodes/{nodeId}/transitions`、`/tool-chain/{nodeId}`、`/path`、`/optimizations`、`/node-stats`、`/failed-nodes`、`/iterations`、`/llm-reasoning-path/{nodeId}`。

**analysis.rs**：`GET /executions/{id}/progress`；`GET /search?q=&types=`（统一搜索）；`GET /analysis/llm-metrics`；`GET /analysis/performance/compare?baseline=&compared=`；`GET /analysis/stats`、`/stats/top-workflows`、`/top-node-types`、`/agent-profiles`；`GET /executions/{id}/error-analysis` + `/advanced`、`/root-cause`、`/context`、`/context/{errorId}`、`/recovery/{errorId}`、`/recovery-recommendations`、`/similar`、`/stream`（SSE）；`GET /executions/{id}/performance` + `/summary`、`/bottlenecks`、`/iteration-comparison`。

**approvals.rs（359）— 人工审批**：`POST /approvals/request`（默认 30s 超时，无 handler 快速失败）、`/approvals/check`、`/approvals/execute-tool`；交互：`GET|POST /interactions`（POST 直接落一条交互记录）；`GET /interactions/by-execution/{executionId}`、`/by-status/{status}`；`GET|DELETE /interactions/{id}`；`POST /interactions/{id}/respond`；`GET /interactions/stats`。

**audit.rs（153）**：`GET /executions/{id}/audit/summary|report|timeline|iterations|tool-calls|llm-calls|node-executions`。

**events.rs（524）**：`GET|DELETE /events`（删除需 `?force=true`）；`GET /events/stats`、`/search`、`/size`、`/time-range`；`GET /events/stream`（**SSE**，最多 100 并发、30s keepalive、初始 connected 事件）；`GET /events/timeline/{executionId}`、`/agent-timeline/{id}`、`/execution-timeline/{executionId}` + `/summary`、`/listener-stats/{executionId}`；`GET /events/agent/stats`、`/events/agent/{agentLoopId}` + `/turns`、`/tool-executions`。

**messages.rs（153）**：`GET|POST /messages`；`GET /messages/stats`、`/search`、`/by-execution/{executionId}`、`/conversation/{executionId}`；`GET|DELETE /messages/{id}`。

**query.rs**：`POST /query`；`POST /query/evaluate`（单条 JSON 表达式求值）；`POST /query/export`（json/csv/xml）；`POST /query/aggregate`；`GET /query/distinct?field=`；`POST /query/group-by`。

**tasks.rs（129）**：`GET|POST /tasks`；`GET /tasks/stats`、`/by-execution/{executionId}`；`POST /tasks/cleanup`；`GET|DELETE /tasks/{id}`；`POST /tasks/{id}/cancel`。

### 2b. agent 域（10 文件）

**profiles.rs**：`GET|POST /agents`；`GET|PUT|DELETE /agents/{id}`；`POST /agents/validate`。

**loops.rs — Agent Loop 控制**：`GET|POST /agent-loops`；`POST /agent-loops/cleanup-completed`；`GET /agent-loops/summaries`（live-first 摘要，`?status=&profile_id=`）、`GET /agent-loops/stats`；`GET|PUT|DELETE /agent-loops/{id}`；`PATCH|GET /agent-loops/{id}/status`；`POST /agent-loops/{id}/status/transition`；`POST /agent-loops/{id}/run`；`POST /agent-loops/{id}/stream`（**SSE**）；`POST /agent-loops/{id}/pause|resume|cancel`；`GET /agent-loops/{id}/summary`、`/iteration-history` + `/summary`、`/timeline`、`/variable-history/{name}`、`/context-evolution`、`/execution-path`。

**executions.rs（215）**：`GET /agent-executions`；`GET|DELETE /agent-executions/{id}`；`GET /agent-executions/by-definition/{defId}`；`GET /agent-executions/stats`；`GET /agent-executions/by-status/{status}`；Agent 检查点：`GET|POST /agent-loops/{id}/checkpoints`；`POST /agent-loops/{id}/checkpoints/{cid}/restore`；`GET /agent-loops/{id}/checkpoints/chain`；`DELETE /agent-loops/{id}/checkpoints`；`GET /agent-checkpoints/stats`。

**analysis.rs（231）**：`GET /agent-executions/{id}/errors` + `/chain`、`/root-cause`、`/statistics`、`/statistics/advanced`、`/recovery/{errorId}`、`/similar/{errorId}`；`GET /agent-loops/{id}/performance` + `/comparison`。

**graphs.rs（294）— Agent 决策图**：`GET /agent-loops/{id}/graph` + `/nodes`、`/edges`、`/paths`、`/paths/execution-path`、`/paths/path-stats`、`/paths/critical-path`、`/paths/steps`、`/alternatives`、`/alternatives/iterations/{iteration}`、`/sequences`、`/sequences/iterations/{iteration}`、`/sequences/types/{decisionType}`、`/unexplored`、`/unexplored/best`、`/tool-frequency`、`/patterns`、`/efficiency`、`/probabilities`。

**llm.rs（387）**：`POST /llm/generate`、`/generate-batch`、`/generate-stream`（**SSE**）、`/count-tokens`；`GET|POST /llm/profiles`；`GET|PUT|DELETE /llm/profiles/{id}`；`POST /llm/profiles/{id}/default`；`GET /llm/profiles/default`；`GET /llm/profiles/{id}/export`；`POST /llm/profiles/import`；`GET /llm/profiles/export-all`；`POST /llm/profiles/import-all`；`POST /llm/profiles/validate`；`GET|POST /llm/profile-templates`；`GET /llm/profile-templates/{name}`；`DELETE /llm/profile-templates?name=`；`POST /llm/profiles/from-template`。

**skills.rs**：`GET /skills`、`/prompt`、`/query`；`POST /skills/scan`、`/reload`；`GET /skills/enabled`、`/disabled`；`POST /skills/cache/clear` + `/clear/{name}`；`GET /skills/{name}`；`POST /skills/{name}/enable|disable`；`GET /skills/{name}/content`、`/resources?resource_type=`。

**triggers.rs**：`GET|POST /agent-triggers`（GET 列表 `?event=`，POST 注册新触发器，重复 id 409）；`GET /agent-triggers/{tid}`、`/{tid}/enabled`；`GET /agent-triggers/export`；`GET /agent-triggers/history?execution_id=&trigger_name=`；`POST /agent-triggers/{tid}/enable|disable`；`GET /agent-triggers/stats`；交互：`GET /agent-loops/{id}/interactions`；`GET /agent-interactions/{id}`；`POST /agent-interactions/{id}/respond`。

**variables.rs**：`GET /agent-loops/{id}/messages` + `/search`、`/stats`、`/conversation`；`POST /agent-loops/{id}/messages/dedupe`；`GET /agent-loops/{id}/variables` + `/stats`、`/export`；`GET|PUT|DELETE /agent-loops/{id}/variables/{name}`。

### 2c. resource 域（11 文件）

**health.rs（208）— 系统面**：`GET /`（服务索引 + 端点地图）、`GET /health`（就绪：ready/persistence/storage 操作计数）、`GET /api/v1/info`（name/version/apiVersion/timestamp）、`GET /api/v1/storage/diagnose|health|stats`、`GET /system/diagnostics`、`/system/event-health`。

**openapi.rs — API 发现**：`GET /api/v1/openapi.json`（静态 OpenAPI 风格服务信息与路由清单）。

**metrics.rs（139）**：`GET /api/v1/metrics/workflow?workflow_id=`、`/node-templates?top_n=`、`/agents?profile_id=`、`/report`、`/export?format=json|prometheus`、`/collectors`（顶层另有 `GET /metrics` Prometheus 文本格式）。

**scripts.rs（210）**：`POST /scripts/execute`、`/validate`；`GET|POST /scripts`；`GET /scripts/search?q=`；`GET|PUT|DELETE /scripts/{id}`（删除带 `?force=` 引用检查）；`POST /scripts/{id}/enable|disable`。

**tools.rs（190）**：`GET /tools`、`/search`；`POST /tools/execute`、`/validate-params`；`GET /tools/{id}`；`POST /tools/{id}/enable|disable`；`GET|POST /tool-registry`；`DELETE /tool-registry/{id}?force=`；`GET /tool-registry/stats`。

**templates.rs（432）— 模板 CRUD**：节点 `GET|POST /templates/node` + `/{id}`、`POST /templates/node/import`、`GET /templates/node/{id}/export`；Hook 同构（`/templates/hook*`）；触发器同构（`/templates/trigger*`）；Agent Hook 模板：`POST /templates/agent-hook`、`GET|PUT|DELETE /templates/agent-hook/{id}`、`POST /templates/agent-hook/import`、`GET /templates/agent-hook/export?name=`（按名称导出，对齐 TS `exportTemplate(name)`）。

**template_queries.rs（126）**：`GET /templates/agent-hook` + `/summaries`；`GET /templates/agent-trigger` + `/summaries`；`GET /templates/agent` + `/summaries`、`/featured`、`/popular`。

**template_library.rs（242）**：`GET /templates/library`、`/featured`、`/popular`；`POST /templates/library/{id}/usage`、`/clone`；`GET|POST /templates/library/workflows` + `GET|DELETE /{id}`；`GET|POST /templates/library/agents` + `GET|DELETE /{id}`。

**triggers.rs**：`GET|POST /triggers`；`GET /triggers/stats`、`/search`；`GET|DELETE /triggers/{id}`、`/{id}/enabled`；`POST /triggers/{id}/enable|disable`；`GET|POST /trigger-executions`；`POST /trigger-executions/cleanup`；`GET /trigger-executions/stats`、`/by-execution/{executionId}`、`/by-trigger/{name}`、`/by-workflow/{id}`；`GET|DELETE /trigger-executions/{id}`。

**variables.rs（287）**：`GET|POST /variables`；`POST /variables/batch`、`/import`；`GET /variables/scopes/{executionId}`、`/scope/{scope}`、`/by-node/{executionId}/{nodeId}`；`GET /variables/stats`、`/history?name=`；`GET /variables/export/{executionId}`；`GET|DELETE /variables/{name}?scope=&execution_id=`。

### 2d. WebSocket（ws.rs，525 行）

`GET /api/v1/ws`：客户端 `subscribe`/`unsubscribe`/`ping`（带 executionId）；服务端 `connection`（clientId）/`execution_event`/`subscribed`/`unsubscribed`/`pong`/`error`。每个订阅 spawn 转发任务（`wf_api::infra::events::subscribe`），终态事件结束订阅；连接关闭 abort 全部转发任务。认证经 `api_key` 查询参数（失败关闭码 4001）。

## 3. 与 wf-api 的关系

- 服务器状态即 `Arc<wf_api::ApiContext>`；每个 handler 调用 `wf_api::...` 自由函数并包装结果。
- 例外（wf-server 自有逻辑）：SSE/WS 帧、统计 handler 聚合 wf-metrics 收集器、少量视图整形（`ExecuteView`/`AgentRunView`/`StreamMetadataView`）。
- 直接依赖 wf-types（线类型）、wf-storage（list-options）、wf-tools、wf-common、wf-metrics。

## 4. 信封 / SSE / 中间件

**信封（envelope.rs，210 行）**——所有 JSON handler 经 `ok()`/`err()`/`error_response()`：

```json
{ "success": true,  "data": {...},  "error": null }
{ "success": false, "data": null,   "error": { "code": "...", "message": "..." } }
```

错误映射（`wf_api::ApiError` → HTTP）：NotFound/ExecutionNotFound → 404 `NOT_FOUND`；Validation → 400 `INVALID_PARAMS`；AlreadyExists → 409 `ALREADY_EXISTS`；Conflict → 409 `CONFLICT`；Timeout → 504 `TIMEOUT`；Storage → 500 `STORAGE_ERROR`；Execution → 500 `INTERNAL_ERROR`。另有 `unauthorized`(401)/`forbidden`(403)/`rate_limited`(429 + Retry-After)/`service_unavailable`(503，SSE 限额用)。

**SSE（sse.rs，27 行）**：`text/event-stream` + `Cache-Control: no-cache`；帧 `data: {json}\n\n`；`/events/stream` 追加 connected 事件与 30s 注释 keepalive。用于：`/events/stream`、`/workflows/{id}/execute/stream`、`/agent-loops/{id}/stream`、`/llm/generate-stream`。

**中间件（middleware.rs，594 行）**——层序（外→内）：请求日志 → CORS → API-key 认证 → 每 IP 限流。

- 日志：按状态类分级 tracing（method/path/status/duration）。
- CORS：OPTIONS 预检、来源白名单（默认 `*`）。
- 认证：`x-api-key` 头或 `api_key` 查询参数；排除 `/health`、`/api/v1/info`、`/`、`/api/v1/ws`、`/api/v1/events/stream`；env 驱动（`AUTH_ENABLED`/`API_KEYS`），默认关闭。
- 限流：内存 `HashMap<ip, entry>`，默认 60s / 100 请求，429 + `x-ratelimit-*` 头；按 `x-forwarded-for` 键控。

**启动（server.rs，74 行）**：`serve_with_router` 绑定 TCP 监听 + `axum::serve` + oneshot 优雅关停；返回 `ServerHandle`（实际绑定地址 + 关停句柄）。

## 5. 小结

- 约 365 条路由（方法 × 路径），handler 多为 5–15 行 wf-api 薄适配器。
- 纯传输层：错误映射、信封、帧协议是唯一职责。
- 流式能力三件套：SSE（执行/事件/LLM 生成）、WebSocket（事件订阅）、Prometheus `/metrics`。
