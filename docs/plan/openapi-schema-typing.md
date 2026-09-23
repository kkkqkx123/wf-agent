# OpenAPI 响应体类型化方案（`serde_json::Value` → `ToSchema`）

配套：`openapi-utoipa-migration.md`、`openapi-utoipa-progress.md`。本文回答“是否要把泛型
`serde_json::Value` 响应体逐步换成领域类型 `ToSchema`”，若值得则给出分阶段落地方案。

## 结论（先说要不要做）

**值得做，但必须分阶段、以 wf-server 本地类型为主、领域叶子类型按前端需求驱动。**
不做“全量给 240+ 领域类型 derive”的一刀切替换。

理由：信封 + 分页外壳的类型化收益高、成本与风险低（完全不触碰 foundation）；
而把 `utoipa` 依赖下沉进 `wf-types` 会污染整个 DAG、且手写 437 处 `body` 的维护漂移大，
收益边际递减，不符合项目“正向编程、最小合理架构”的取向。

## 现状事实（源码核对）

- 所有 `#[utoipa::path]` 成功响应体当前统一为 `serde_json::Value`（自由格式）。
  `openapi-typescript` 会把自由格式 Object 生成为 `unknown` → 前端拿不到 `data` 类型，
  迁移的核心收益（类型安全调用）无法兑现。这是“做”的主要动因。
- 实际 HTTP body 一律是**信封**：`ApiEnvelope { success, data, error }`（`envelope.rs`）；
  列表再套 `PageView`/`CappedView`（`paged.rs`）。当前文档把 200 body 直接写成 `Value`，
  其实连信封都没描述。→ 任何“类型化”都要**先建模信封与分页外壳**，否则文档与线上格式不符。
- 领域视图类型集中在 `wf-api`（约 172 个 `Serialize`），但字段大量内嵌 `wf-types` 枚举
  （`ExecutionStatus`、`EventType`、`VariableSource`…，定义于 foundation）与 `Arc<…>`。
  → 给这些类型 `derive(ToSchema)` 会把 `utoipa` 拉进 **foundation 层**（最大成本/风险点）。
- 无 `chrono`/`uuid`：时间戳是 `String`（ISO）。→ 省掉 utoipa 相关 feature。
- 响应类别异质：普通 data / paged / capped / 临时 `json!` map / 布尔或字符串 / SSE / 文件下载。
  无法用单一 `body` 统一描述。
- 两处既有文档不准（与类型化正交，但应一并修正）：
  - SSE 端点被标成 200 JSON，实为 `text/event-stream`（仅手工标注的 `agent/loops.rs` 用了正确
    content_type，脚本批量生成的其余流式端点未处理）。
  - `download(...)` 端点被标成 JSON，实为 `text/csv` / `text/markdown`。
  - `security` 声明的是 `bearer_auth`（HTTP bearer），而真实鉴权是 **API key**
    （默认头 `x-api-key`，或查询参数 `api_key`；由 `AUTH_ENABLED`/`API_KEYS` 控制）。

## 分阶段方案

### 阶段 0 — 准确性修正（先做，独立于类型化，纯注解文本改动）
- **鉴权对齐**：`openapi.rs` 的 `SecurityAddon`/`security` 从 `bearer_auth` 改为
  `apiKey` + `in: header`（`name: x-api-key`），与 `middleware.rs` 的真实行为一致；
  前端才会按正确方式携带凭据。
- **SSE 端点**：`responses` 用 `content((text/event-stream = String))`（或事件负载 schema），
  不再写 `body = serde_json::Value` + 200 JSON。
- **下载端点**：`content_type` 标注为 `text/csv`/`text/markdown`，`body = String`。
- 收益：文档可信；成本：改现有 30+ 端点的注解（可由脚本按 handler 内的
  `sse_response`/`download` 调用点识别，或人工修）。

### 阶段 A — 信封 + 分页外壳类型化（最高价值 / 低风险，wf-server 本地）
- 给 `ApiEnvelope<T>`、`PageView<T>`、`CappedView<T>` 在 **wf-server** 内 `derive(ToSchema)`
  （三者已是本地类型，DAG 无影响）；`ApiErrorBody` 已实现。
- 用 utoipa 泛型实例化机制在 `components(schemas(...))` 注册用到的具体组合。
- 成功响应体由 `serde_json::Value` 改为 `crate::envelope::ApiEnvelope<serde_json::Value>`
  （先定型外壳，`data` 暂留自由格式）。→ 前端立刻区分 `success/data/error`，
  分页字段（`items/limit/offset/has_more`）也变类型化。
- **风险验证（先做 spike）**：嵌套泛型展开（`ApiEnvelope<PageView<Value>>`）在 utoipa 5 的支持度。
  若不支持，退化为“`data = ApiEnvelope`，列表外壳另设命名别名”。
- 可自动化：脚本把 `body = serde_json::Value` 批量替换为信封形态；类型名仍为 `Value`。

### 阶段 B — wf-server 本地视图类型（零 DAG 成本，机会式）
- 对返回**本模块内 `pub(crate) struct *View/*Response`**（如 `TransformWorkflowView`）的端点，
  本地 `derive(ToSchema)` 作为 `data` 的具体类型。
- 判定：`ok(SomeLocalStruct { .. })` / `ok_page(some_local_vec)`。需人工/AST 识别，不适合正则。

### 阶段 C — 领域类型（前端按需；可选下沉；谨慎）
仅当前端确需某个 `data` 的精确模型时才处理，二选一：
- **C1（默认推荐）镜像 DTO**：在 wf-server 内为该端点定义**仅含 `ToSchema`** 的轻量响应 DTO，
  不引入 foundation 依赖。代价是与 `wf-api` 形状可能漂移——用 `serde` 往返断言或集成测试兜底。
- **C2 下沉 derive（受限）**：给 `wf-api`/`wf-types` 加 **feature-gated 可选 `utoipa`**
  （新 `api-schema` feature，仅被 `openapi-docs` 传递开启），直接 `derive(ToSchema)`。
  优点是单一事实源；代价是把 `utoipa` 引入 foundation（编译期成本、DAG 约束、
  `Arc<…>`/枚举需要逐点 `#[schema(value_type=…)]` 处理）。**只有在较多端点确需时才启动，
  且务必 feature 门控**，保证生产/非文档构建不付出该依赖成本。
- 其余端点保留 `ApiEnvelope<Value>`。

## 落地顺序
1. 阶段 0（鉴权 + 内容类型修正）。
2. 阶段 A spike：单端点验证泛型信封 + 刷新快照 + `openapi-typescript` 产物是否符合预期；
   通过再批量。
3. 阶段 B 按模块推进（返回本地结构体的端点优先）。
4. 阶段 C 由 web-app 具体页面的需求触发（见 `web-app-integration.md`），默认走 C1。
5. 每阶段后：`cargo check -p wf-server --features openapi-docs`、刷新 `openapi.json` 快照、
   `openapi-typescript` + `svelte-check`。

## 验收标准
- `openapi.json`：`securitySchemes` 与真实鉴权一致；SSE/下载端点 `content-type` 正确；
  成功响应体为 `ApiEnvelope<…>`（至少外壳定型）。
- 现有 `openapi::tests` 冒烟测试相应更新（信封出现、操作数不变）。
- 前端样例页能类型安全读取 `res.data` 与分页字段。

## 边界（明确不做）
- 不做全量 240+ 领域类型 `derive`。
- 请求体 `request_body` 类型化与响应同机制，但优先级更低（写操作返回值多为 id/bool），
  留待响应稳定后统一处理。
