# web-app（SvelteKit）OpenAPI 集成方案

配套：`openapi-utoipa-migration.md`、`openapi-schema-typing.md`。目标：让
`apps/web-app`（SvelteKit 2 + Svelte 5）基于 `wf-server` 生成的 OpenAPI 文档做
**类型安全的 API 调用**，并明确流式/WebSocket 的类型化边界。

## 现状事实

- `apps/web-app`：SvelteKit 2 + Svelte 5 + TS + Vite，node>=22；目前只有一个占位
  `src/routes/+page.svelte`，**无任何 API 客户端代码**。`typecheck` 脚本被置空（`true`），
  真实类型检查走 `svelte-check`（`npm run check`）。
- 后端：REST 全部在 `/api/v1` 下；文档在 `/api-docs/openapi.json`（dev/debug 或
  `openapi-docs` feature），自包含 Swagger UI 在 `/api-docs/swagger`。
  响应体统一信封 `ApiEnvelope{success,data,error}`，列表套 `PageView`/`CappedView`。
- 鉴权真实为 **API key**：默认请求头 `x-api-key`，亦支持 `api_key` 查询参数；
  由 `AUTH_ENABLED` 开关、`API_KEYS` 供密钥（`middleware.rs`）。CORS 走
  `cors.toml`（`allow_origin`/`allow_headers`），**CORS 无环境变量**（部署期决定）。
- `openapi-schema-typing.md` 阶段 0 会把 `securitySchemes` 从 `bearer_auth` 修正为 `apiKey/header`。

## 设计目标

- 类型来源唯一：接口类型全部由后端 OpenAPI 生成，禁止手写重复的响应模型。
- 调用点少样板：一个 client + 薄封装，处理信封拆包、错误归一、鉴权头注入。
- 流式与 WebSocket 有明确的、类型化的接入方式（不与 REST 混为一谈）。

## 1. 类型生成管线（codegen）

- 引入 `openapi-typescript`（生成 `paths`/`components` 的 `.d.ts`）。
- 产物：`src/lib/api/schema.d.ts`。
- **spec 来源用“提交到仓库的快照”，而不是运行时拉取**：
  - 后端提供一个导出命令/开关，把 `ApiDoc::openapi()` 序列化为 `openapi.json`，
    由仓库持有（如 `apps/web-app/openapi.json`）。
  - codegen 读本地快照 → 前端构建/类型检查**不依赖后端在跑**，且类型随快照显式更新
    （避免“悄悄漂移”）。
  - `package.json` 增加 `gen:api` 脚本（`openapi-typescript ./openapi.json -o src/lib/api/schema.d.ts`）。
- 后端侧建议新增一个 feature-gated 的导出动作（`--dump-openapi <path>` 或
  `cargo run ... > openapi.json`），与文档端点共用同一 `ApiDoc`；这是唯一需要补的后端小改。
- CI/本地可加一步“重新导出快照并 `git diff --exit-code`”来发现文档漂移。

## 2. 客户端封装

- 采用 `openapi-fetch`（以生成的 `paths` 泛型为唯一事实源，运行时只做 fetch，零手写类型）。
- `src/lib/api/client.ts`：
  - `createClient<paths>({ baseUrl })`；`baseUrl` 默认 `/api/v1`。
  - 鉴权：通过 `client.use({ onRequest })` 注入 `x-api-key`（key 来源见 §5）。
  - 与 `securitySchemes` 对齐（依赖 schema-typing 阶段 0 完成 `apiKey` 修正）。
- **信封拆包**（核心，且是 schema-typing 阶段 A 的前端动机）：
  - 每个 REST 调用拿到的是 `ApiEnvelope`；封装 `call()` 辅助：`success=false` 或
    HTTP 非 2xx → 抛/返回类型化错误；否则返回 `data`。
  - 阶段 A 前 `data` 是 `unknown` → `unwrap` 只能给 `unknown`；
    阶段 A（响应体改 `ApiEnvelope<Value>`）后可安全返回 `unknown` 且 `error` 有类型；
    阶段 B/C 后 `data` 变具体类型 → `unwrap` 返回值随之精确。
  - 分页：列表端点 `data` 为 `PageView`/`CappedView`，封装 `callPage()` 返回 `{items, hasMore}`。
- 错误归一：把 `ApiErrorBody{code,message}` 映射为前端可展示/可分支的错误对象
  （`NOT_FOUND`/`INVALID_PARAMS`/`UNAUTHORIZED`/`RATE_LIMITED` 等，见 `envelope.rs`）。

## 3. 分层用法（Svelte）

- `src/lib/api/` 只放 client + 生成类型 + 拆包封装。
- 领域薄封装（可选）`src/lib/api/<domain>.ts`：把常见调用聚合成带业务语义的函数
  （如 `listAgentLoops(query)`），仍复用生成的 query 参数类型，不重声明响应类型。
- `+page.ts`/`+layout.ts` 的 `load` 里调用；组件消费拆包后的数据。

## 4. 流式 / WebSocket（类型化边界，重点）

- **REST + OpenAPI 覆盖不到流式**：
  - SSE（`text/event-stream`，如 loops 运行流、events stream）：`openapi-typescript`
    只给到“响应是流”，无法描述每帧结构。方案：
    - schema-typing 阶段 0 先把 SSE 端点 `content_type` 修正，避免 `openapi-fetch` 误当 JSON。
    - SSE 客户端用原生 `EventSource`/`fetch`+`ReadableStream` 单列 `src/lib/api/sse.ts`；
      事件帧的 TS 类型**镜像后端事件枚举**（后端已有 `content((text/event-stream = String))` 或
      事件 schema 时，从中取；否则在 `src/lib/api/events.d.ts` 手写并加契约测试）。
  - WebSocket（`ws::routes()`，按设计排除在 OpenAPI 外）：单列 `src/lib/api/ws.ts`，
    消息类型在 `wf-server`/`ws` 侧有稳定契约后再手写或另行生成。
  - 原则：`schema.d.ts` 是 REST 的唯一来源；流式类型是**独立、显式维护**的一层，
    不假装由 OpenAPI 生成。
- **导出快照的时机**：只要 handler 注解或事件契约变化，重导 `openapi.json` + 跑 `gen:api`；
  流式事件契约变化时同步更新 `sse`/`ws` 的 TS 类型。

## 5. 配置与鉴权来源

- `baseUrl`：走 `VITE_API_BASE`，默认 `/api/v1`（同源部署时相对路径即可）。
- API key 来源（前端不可信，按部署形态择一，**默认不硬编码**）：
  1. 同源反代：由网关/后端在 `--static-dir` 同源托管前端，前端不碰密钥（推荐生产）。
  2. 登录换取：若后续加 login/token 端点，前端存短期凭据（需先在后端落地）。
  3. 开发本地：`import.meta.env.DEV` 下读 `.env.local` 的 demo key。
- 明确记录：当前后端无 login/token 端点；§1 之外不需要新后端端点，鉴权方式保持 API key。

## 6. 部署形态

- 后端已支持 `--static-dir` 的 SPA fallback 托管前端构建产物（`static_files.rs`）：
  前端 `npm run build` → `apps/web-app/build`（`adapter-static`）→ 由 `wf-server` 指过去，
  同源，免 CORS。
- 需把 `svelte.config.js` 从 `adapter-auto` 明确为 `@sveltejs/adapter-static`（SPA：
  `paths.base=''`、fallback 指向 `index.html`），并在 `web-app` 装该适配器。
- 独立部署时：CORS 用 `cors.toml` 配置 `allow_origin`/`allow_headers`（含 `x-api-key`）。

## 7. 落地顺序（建议）

1. schema-typing 阶段 0（鉴权 `apiKey`、SSE/下载 `content-type`）→ 让生成类型语义正确。
2. schema-typing 阶段 A（信封/分页外壳类型化）→ 前端 `call`/`callPage` 才有稳定 `data`。
3. web-app：装 `openapi-typescript`+`openapi-fetch`+`adapter-static`；加 `gen:api`；
   先提交 `openapi.json` 快照并生成 `schema.d.ts`；建 `client.ts` + 拆包封装。
4. 用一个真实只读页面（如 agent-loops 列表，走 `callPage`）打通端到端。
5. 再按页面需要引入阶段 B/C 的具体 `data` 类型（前端需求驱动，默认镜像 DTO）。
6. 流式页面（运行流/事件流）接入 `sse.ts`/`ws.ts`。

## 8. 验收标准

- `npm run check`（`svelte-check`）通过，且 `gen:api` 能从快照稳定生成、无 `any` 泄漏。
- 至少一个 REST 列表页 + 一个详情页：请求参数、`data`、`error` 全类型化，运行时无手写响应模型。
- SSE 运行流页面：逐帧消费、类型化事件、错误/关闭有处理。
- 生产构建由后端 `--static-dir` 同源托管，`/api/v1/*` 与前端路由均可用。
- 文档端点 `/api-docs/openapi.json` 与仓库快照一致（CI diff 校验）。

## 9. 风险与取舍

- 信封 `data` 若长期停在 `Value` → 前端等于手写类型（违背目标）。故**阶段 A 是本集成方案的前置
  依赖**，应先于 §7-3 完成。
- `openapi-typescript` 对 SSE/WS 无能为力是既定事实，不要为此扭曲 REST 文档；单独维护流式契约层。
- 快照机制以“显式更新”换取“离线可构建 + 漂移可见”，代价是需一条 CI 校验保证快照与后端同步。
