# wf-server HTTP 传输层

对应 `crates/app/wf-server`。框架：**axum**（+ tokio/tower/futures/serde/clap）。定位：**纯 HTTP 传输层**——无业务逻辑，handler 调用 wf-api 并做信封封装。**447 条路由**（方法 × 路径，以契约清单为准，由生成器从路由声明派生）。

## 1. 结构与路由组合

- 目录已从平铺重组为 **`api/{workflow,agent,resource}/` 三子目录** + 三个域声明文件（无 mod.rs）：workflow/ 17 文件（204 端点）、agent/ 10 文件（125）、resource/ 11 文件（90）。
- `ApiState = { ctx: Arc<wf_api::ApiContext>, config: Arc<ServerMiddlewareConfig> }`；metrics 面另用 `RegistryState`。
- `router.rs`（113 行）：域路由 merge 后 `nest("/api/v1", ...)`；系统面在根路径挂 `api/resource/health.rs`（路径字符串自带前缀）。`full_router_with_middleware` = `src/metrics.rs` 的 router（`GET /metrics` + nest `/api/v1/metrics`）merge api_router，再 route_layer 请求指标。
- 二级组合（域文件内部 merge）：workflows ⊇ versions+graphs；executions ⊇ checkpoints+execution_state+execution_analysis；agents ⊇ profiles+loops+executions+variables+triggers；agent/analysis ⊇ agent/graphs；agent/llm ⊇ resource/scripts+tools；templates ⊇ template_queries+template_library；entities ⊇ tasks+triggers+variables+messages+skills。
- `extract.rs`（93 行）：共享路径参数提取器（IdPath/IdNodePath/IdVersionPath/IdCidPath/CidPath 等）。
- `server_config.rs`（180 行）+ `main.rs`（163 行）：`ServerConfig{bind_addr, middleware}` 加载优先级 **CLI `--addr` > env（`WF_SERVER_BIND_ADDR` 等）> `configs/server/*.toml` > 默认 127.0.0.1:3000**（文件层宽松加载，损坏 warn 跳过；secrets 不进文件）。main 经 `wf_runtime::bootstrap::Runtime` 装配后 serve，支持 `--config/--storage/--log-level`。

## 2. HTTP 端点清单（均位于 `/api/v1` 前缀下，除注明）

### 2a. workflow 域（api/workflow/）

**workflows.rs（517，21）**：`GET|POST /workflows`；`GET|PUT|DELETE /workflows/{id}`；`POST /workflows/{id}/clone`；`POST /workflows/validate`、`/validate/node`；`POST /workflows/parse`、`/transform`；`GET /workflows/summaries`、`/search`、`/by-name/{name}`、`/by-tags`、`/by-category/{category}`、`/by-author/{author}`；`POST /workflows/export-all`、`GET /workflows/{id}/export`、`POST /workflows/import`、`/import-many`；`PATCH /workflows/{id}/metadata`。

**versions.rs（139，5）**：`GET|POST /workflows/{id}/versions`；`GET /workflows/{id}/versions/{version}`；`POST /workflows/{id}/rollback`、`/versions/increment?level=`。

**graphs.rs（245，9）**：`GET /workflows/{id}/graph` + `/summary`、`/nodes?node_type=`、`/edges`、`/neighbors/{nodeId}`、`/analysis`、`/cycles`、`/topology`、`/reachability`。

**executions.rs（407，10）**：`POST /workflows/{id}/execute`；`POST /workflows/{id}/execute/stream`（**SSE**）；`GET /executions`、`GET|DELETE /executions/{id}`；`POST /executions/{id}/pause|resume|cancel`；`GET /executions/{id}/status`、`/triggers`。（旧版的 `POST /execution-triggers/{id}/enable|disable` 随每执行触发器模型删除。）

**checkpoints.rs（329，14）**：执行级（先 `ensure_execution_domain(Workflow)`）：`POST /executions/{id}/checkpoints`；`GET /executions/{id}/checkpoints/chain`（ExecutionNotFound 降级空链 200）；`POST /executions/checkpoints/{cid}/restore|resume`（先 `ensure_checkpoint_domain`）。记录级（直用 `checkpoint::record`）：`GET|POST /checkpoints`；`GET|DELETE /checkpoints/{id}`；`GET|DELETE /checkpoints/entity/{entityId}`；`GET|PUT /checkpoints/entity/{entityId}/metadata`；`GET /checkpoints/entity/{entityId}/latest`；`GET /checkpoints/entities?entity_ids=`；`GET /checkpoints/time-range?workflowId=&start=&end=`。（旧版 `/file-checkpoints*` CRUD 端点已不存在，文件面改由下列两文件承载。）

**file_provenance.rs（262，14，新）**：`GET /file-checkpoint/partitions`、`/sessions`、`/changes/actor/{id}`、`/changes/path/{id}`、`/diff/actors/{a}/{b}`、`/diff/staged/{id}`、`/workspace/{id}`、`/timeline/{id}`；`POST /file-checkpoint/sessions`、`/sessions/{id}/rollback/{actor}`、`/rename`、`/undo/{id}`、`/redo/{id}`、`/gc`。

**file_approvals.rs（87，3，新）**：`GET /file-checkpoint/approvals/pending`；`POST /file-checkpoint/approvals/{id}/approve|reject`。

**execution_state.rs（428，24）**：`GET /executions/{id}/state|variables|transitions|context|call-stack|memory|variable-snapshots|context-evolution|state-analysis|context-transitions|context-snapshots|agent-state|agent-variables|agent-iterations`；`GET|DELETE /executions/{id}/state-records` + `/iterations/{iteration}`、`/snapshots/{timestamp}`、`/variables/{name}/history`、`/most-changed`、`/mutation-count`、`/call-stack`、`/memory`、`/memory/peak`。

**execution_analysis.rs（476，26）**：`GET /executions/{id}/graph` + `/nodes|edges|neighbors/{nodeId}|path-stats|reachability`；`POST /executions/{id}/graph/clear`；`GET …/analysis/paths|paths/enumerate|decision-points|slow-nodes?percentile=|efficiency|alternatives|probabilities`；`GET …/nodes`、`/nodes/{nodeId}`、`/nodes/by-type/{t}`、`/nodes/{nodeId}/input-context|transitions`、`/tool-chain/{nodeId}`、`/llm-reasoning-path/{nodeId}`、`/path`、`/optimizations`、`/node-stats`、`/failed-nodes`、`/iterations`。

**analysis.rs（486，21）**：`GET /executions/{id}/progress`；`GET /search?q=&types=`（统一搜索）；`GET /analysis/llm-metrics`、`/analysis/performance/compare?baseline=&compared=`；`GET /analysis/stats`、`/stats/top-workflows`、`/top-node-types`、`/agent-profiles`；错误分析族 `GET /executions/{id}/error-analysis` + `/advanced`、`/root-cause`、`/context`、`/context/{errorId}`、`/recovery/{errorId}`、`/recovery-recommendations`、`/similar`、**`/stream`（SSE）**；性能族 `GET /executions/{id}/performance` + `/summary`、`/bottlenecks`、`/iteration-comparison`。

**approvals.rs（464，11）**：`POST /approvals/request`（**无超时等待**；无 `UserInteractionHandler` 时快速失败 400 而非永久阻塞）、`/approvals/check`、`/approvals/execute-tool`（后两者策略先行：AutoApproved/Deny 不经 handler，仅 Ask 进入人工环路）；交互记录：`GET|POST /interactions`；`GET|DELETE /interactions/{id}`；`POST /interactions/{id}/respond`；`GET /interactions/by-execution/{executionId}`、`/by-status/{status}`、`/stats`。

**hooks.rs（238，1，新）**：`POST /hooks/{name}` —— **webhook 入站网关**：按 TriggerTemplate 名查注册表、校验其 `webhook_spec` metadata、per-hook token 认证、可选 execution_id 路由、fire_id 幂等，命中后走模板动作。这是 webhook 能力的唯一入口（无 hook 模板 CRUD）。

**audit.rs（152，7）**：`GET /executions/{id}/audit/summary|report|timeline|iterations|tool-calls|llm-calls|node-executions`。

**events.rs（526，16）**：`GET|DELETE /events`（删除需 `?force=true`）；`GET /events/stats`、`/search`、`/size`、`/time-range`；`GET /events/stream`（**SSE**：`MAX_SSE_CLIENTS=100` 超限 503、30s 注释帧 keepalive、初始 connected 事件）；`GET /events/timeline/{executionId}`、`/agent-timeline/{id}`、`/execution-timeline/{executionId}` + `/summary`、`/listener-stats/{executionId}`；`GET /events/agent/stats`、`/agent/{agentLoopId}` + `/turns`、`/tool-executions`。

**messages.rs（154，8）**：`GET|POST /messages`；`GET /messages/stats`、`/search`、`/by-execution/{executionId}`、`/conversation/{executionId}`；`GET|DELETE /messages/{id}`。

**query.rs（294，6）**：`POST /query`、`/query/export`、`/query/aggregate`、`/query/group-by`、`/query/evaluate`（单条 JSON 表达式）；`GET /query/distinct?field=`。

**tasks.rs（129，8）**：`GET|POST /tasks`；`GET /tasks/stats`、`/by-execution/{executionId}`；`POST /tasks/cleanup`；`GET|DELETE /tasks/{id}`；`POST /tasks/{id}/cancel`。

### 2b. agent 域（api/agent/）

**agents.rs（95）**：纯 merge 容器（profiles/loops/executions/variables/triggers）。

**profiles.rs（109，6）**：`GET|POST /agents`；`GET|PUT|DELETE /agents/{id}`；`POST /agents/validate`。

**loops.rs（613，23）**：`GET|POST /agent-loops`；`POST /agent-loops/cleanup-completed`；`GET /agent-loops/summaries`（live-first，`?status=&profile_id=`）、`/stats`；`GET|PUT|DELETE /agent-loops/{id}`；`GET|PATCH /agent-loops/{id}/status`、`POST /status/transition`；`POST /agent-loops/{id}/run`；`POST /{id}/stream`（**SSE**）；`POST /{id}/pause|resume|cancel`；`GET /{id}/summary`、`/iteration-history` + `/summary`、`/timeline`、`/variable-history/{name}`、`/context-evolution`、`/execution-path`。

**executions.rs（360，13）**：`GET /agent-executions`；`GET|DELETE /agent-executions/{id}`；`GET /agent-executions/by-definition/{defId}`、`/stats`、`/by-status/{status}`。Agent checkpoint 面（loop 与 cid 双重域校验；未知 cid NotFound → 400 Validation）：`GET|POST /agent-loops/{id}/checkpoints`；`GET /agent-loops/{id}/checkpoints/chain`；`DELETE /agent-loops/{id}/checkpoints`；`POST /agent-loops/{id}/checkpoints/{cid}/restore`、`/resume`（body 含 `mode: branch|in_place` + RunAgentLoopBody 复用）；`GET /agent-checkpoints/stats`。

**analysis.rs（230，9）**：`GET /agent-executions/{id}/errors` + `/chain`、`/root-cause`、`/statistics`、`/statistics/advanced`、`/similar/{errorId}`、`/recovery/{errorId}`；`GET /agent-loops/{id}/performance`、`/comparison`。

**graphs.rs（294，19）**：`GET /agent-loops/{id}/graph` + `/nodes`、`/edges`、`/paths`、`/paths/execution-path`、`/paths/path-stats`、`/paths/critical-path`、`/paths/steps`、`/alternatives`（+ `/iterations/{iteration}`）、`/sequences`（+ `/iterations/{iteration}`、`/types/{decisionType}`）、`/unexplored`、`/unexplored/best`、`/tool-frequency`、`/patterns`、`/efficiency`、`/probabilities`。

**llm.rs（511，26）**：`POST /llm/generate`、`/generate-batch`、`/generate-stream`（**SSE**）、`/count-tokens`；profile：`GET|POST /llm/profiles`、`GET|PUT|DELETE /llm/profiles/{id}`、`POST /llm/profiles/{id}/default`、`GET /llm/profiles/default`、`GET /llm/profiles/{id}/export`、`POST /llm/profiles/import`、`GET /llm/profiles/export-all`、`POST /llm/profiles/import-all`、`POST /llm/profiles/validate`、`POST /llm/profiles/from-template`；模板：`GET|POST|DELETE /llm/profile-templates`（DELETE 带 `?name=`）、`GET /llm/profile-templates/{name}`；**provider（新）**：`GET|POST|DELETE /llm/providers`、`GET /llm/providers/{id}`、`GET /llm/providers/{id}/models`。

**skills.rs（205，14）**：`GET /skills`、`/prompt`、`/query`；`POST /skills/scan`、`/reload`；`GET /skills/enabled`、`/disabled`；`POST /skills/cache/clear` + `/clear/{name}`；`GET /skills/{name}`、`/{name}/content`、`/resources?resource_type=`；`POST /skills/{name}/enable|disable`。

**triggers.rs（152，4）**：`GET /agent-triggers/history?execution_id=&trigger_name=`（旧 `/agent-triggers` CRUD 已随实体模型删除，模板 CRUD 在 `/templates/trigger*`）；`GET /agent-loops/{id}/interactions`；`GET /agent-interactions/{id}`；`POST /agent-interactions/{id}/respond`。

**variables.rs（198，11）**：`GET /agent-loops/{id}/messages` + `/search`、`/stats`；`POST /messages/dedupe`；`GET /agent-loops/{id}/conversation`；`GET /agent-loops/{id}/variables` + `/stats`、`/export`；`GET|PUT|DELETE /agent-loops/{id}/variables/{name}`。

### 2c. resource 域（api/resource/）

**health.rs（214，8，挂根路径）**：`GET /`（服务索引 + 端点地图）、`GET /health`（ready/persistence/storage 操作计数）、`GET /api/v1/info`、`GET /api/v1/storage/diagnose|health|stats`、`GET /system/diagnostics`、`/system/event-health`。

**entities.rs（131）**：纯 merge 容器（tasks/triggers/variables/messages/skills）。

**contract.rs**：`GET /api/v1/contract` —— 机器可读路由清单，由生成器派生并随契约一起发布。

**metrics.rs（139，6）**：`GET /api/v1/metrics/workflow?workflow_id=`、`/node-templates?top_n=`、`/agents?profile_id=`、`/report`、`/export?format=json|prometheus`、`/collectors`；顶层另有 `GET /metrics`（src/metrics.rs，Prometheus 文本 + 自监控块；输出为 retention 窗口【默认 1h】快照而非累计值）。

**scripts.rs（214，10）**：`POST /scripts/execute`、`/validate`；`GET|POST /scripts`；`GET /scripts/search?q=`；`GET|PUT|DELETE /scripts/{id}`（删除带 `?force=` 引用检查）；`POST /scripts/{id}/enable|disable`。

**tools.rs（190，11）**：`GET /tools`、`/search`；`POST /tools/execute`、`/validate-params`；`GET /tools/{id}`；`POST /tools/{id}/enable|disable`；`GET|POST /tool-registry`；`DELETE /tool-registry/{id}?force=`；`GET /tool-registry/stats`。

**templates.rs（315，14）**：节点 `GET|POST /templates/node`、`GET|PUT|DELETE /templates/node/{id}`、`GET /templates/node/{id}/export`、`POST /templates/node/import`；触发器同构一组 `/templates/trigger*`（save/update 走双写 save）。**无 hook 模板端点**（实体已删除）。

**template_queries.rs（104，6）**：`GET /templates/agent` + `/summaries`、`/featured`、`/popular`；`GET /templates/agent-trigger` + `/summaries`。

**template_library.rs（234，13）**：`GET /templates/library`、`/featured`、`/popular`；`POST /templates/library/{id}/usage`、`/clone`；`GET|POST /templates/library/workflows` + `GET|DELETE /{id}`；`GET|POST /templates/library/agents` + `GET|DELETE /{id}`。

**triggers.rs（186，9）**：`GET|POST /trigger-executions`；`POST /trigger-executions/cleanup`；`GET /trigger-executions/stats`、`/by-execution/{executionId}`、`/by-trigger/{name}`、`/by-workflow/{id}`；`GET|DELETE /trigger-executions/{id}`。（旧 `/triggers*` 实体 CRUD 端点已删除。）

**variables.rs（287，12）**：`GET|POST /variables`；`POST /variables/batch`、`/import`；`GET /variables/scopes/{executionId}`、`/scope/{scope}`、`/by-node/{executionId}/{nodeId}`、`/stats`、`/history?name=`、`/export/{executionId}`；`GET|DELETE /variables/{name}?scope=&execution_id=`。

## 3. 与 wf-api 的关系

- 服务器状态即 `Arc<wf_api::ApiContext>`；handler 多为 5–15 行 wf-api 薄适配器（例外：SSE/WS 帧、指标聚合、少量视图整形）。
- **checkpoint 统一域 API 接线**：执行级端点 `wf_api::ensure_execution_domain`，checkpoint 级 `wf_api::checkpoint::ensure_checkpoint_domain`（域不匹配 400、未知 404，未知 cid 在 agent resume 端点改映射 400）；不直接依赖 wf-checkpoint crate。
- 直接 path 依赖 10 个：wf-api、wf-runtime、wf-types、wf-storage、wf-tools、wf-config、wf-metrics、wf-core、wf-common、wf-execution-shared。

## 4. 信封 / SSE / WS / 中间件

**信封（envelope.rs，210）**——所有 JSON handler 经 `ok()`/`err()`/`error_response()`：

```json
{ "success": true,  "data": {...},  "error": null }
{ "success": false, "data": null,   "error": { "code": "...", "message": "..." } }
```

`ApiError` → HTTP：NotFound/ExecutionNotFound → 404 `NOT_FOUND`；Validation → 400 `INVALID_PARAMS`；AlreadyExists → 409 `ALREADY_EXISTS`；Conflict → 409 `CONFLICT`；Timeout → **504** `TIMEOUT`；Storage → 500 `STORAGE_ERROR`；Execution → 500 `INTERNAL_ERROR`。另有 `unauthorized`(401)/`forbidden`(403)/`rate_limited`(429 + `Retry-After`)/`service_unavailable`(503，SSE 限额)。

**SSE（sse.rs，27）**：仅剩 `sse_response(stream)`（200 + `text/event-stream` + `no-cache`），帧流由调用方自建。**SSE 端点 5 个**：`/events/stream`、`/workflows/{id}/execute/stream`、`/executions/{id}/error-analysis/stream`、`/agent-loops/{id}/stream`、`/llm/generate-stream`。

**WebSocket（ws.rs）**：`GET /api/v1/ws`。客户端 `subscribe`/`unsubscribe`（执行、回路、工作流、全局、通知维度）/`ping`；服务端推送执行与通知事件并带重连游标。每订阅 spawn 转发任务，终态自动收束；断连 abort 全部。认证接受 `x-api-key` 头或 `api_key` 查询参数（浏览器用后者），失败 close code **4001**，同时受全局鉴权与限流约束。

**中间件（middleware.rs，846）**——层序（外→内）：请求日志 → CORS → API-key 认证 → 每 IP 限流；`with_request_metrics` 为 route_layer（仅路由内可见 MatchedPath，只记录模板路径）。

- 日志：按状态类分级 tracing。
- CORS：OPTIONS 预检 200；来源白名单默认 `["*"]`；仅文件层配置（`configs/server/cors.toml`）。
- 认证：`x-api-key` 头（可配 header_name）或 `api_key` 查询参数（可配开关）；默认排除 `/health`、`/api/v1/info`、`/`；**缺 key 401、错 key 403**；env `AUTH_ENABLED`/`API_KEYS`（keys 仅 env）+ `configs/server/auth.toml`，默认关闭。流式路径同样受全局鉴权约束。
- 限流：固定窗口、按 IP（`x-forwarded-for` 首值否则 "unknown"）、全局 `OnceLock<Mutex<HashMap>>`、条目 >10000 按过期回收；默认 60s/100 请求（env `RATE_LIMIT_*` + `configs/server/rate-limit.toml` 含 excluded_paths）；429 + `x-ratelimit-limit/remaining/reset`。

**启动（server.rs，74）**：`serve_with_router` bind + `axum::serve` + oneshot 优雅关停，返回 `ServerHandle{addr, shutdown, task}`；与 wf-api/wf-metrics 解耦。

## 5. 小结

- 447 条路由（方法 × 路径，以契约清单为准），纯传输层：错误映射、信封、帧协议是唯一职责。
- 相对上一版文档的结构变化：api/ 三分目录；文件级 checkpoint（provenance/approval）与 webhook 入站网关为新增面；`/triggers`、`/agent-triggers` 实体 CRUD 与 hook 模板端点已删除；`/llm/providers*` 新增；审批端点从"30s 超时"改为"无 handler 快速失败、有 handler 无界等待"。
- 流式能力三件套：SSE（5 端点）、WebSocket（事件订阅）、Prometheus `/metrics`。
