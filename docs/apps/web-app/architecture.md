# Web 应用架构设计

> 范围：`apps/web-app`。状态：与当前仓库实现对齐。后端为 Rust `wf-server`，Web 应用是**纯前端**，不存在 Node 中间层，也不再拆分 frontend/backend 双包。
> 功能边界：`docs/plan/web/frontend-feature-list.md`；集成与类型管线：`docs/plan/web-app-integration.md`；API 契约：`docs/api/`；样式与组件：`docs/spec/web/`。

## 1. 定位与边界

- `apps/web-app` 是 Modular Agent Framework 的 Web 前端，包名 `@wf-agent/web-app-frontend`，基于 SvelteKit 2 + Svelte 5 + TypeScript。
- 所有能力来自 Rust 后端：`wf-server`（axum HTTP 传输层）← `wf-api`（应用门面）← `wf-runtime`（运行时装配）。前端**直连** `wf-server`，不经过任何自建服务端。
- 后端驱动功能只以服务端现有路由为边界；清单见 `docs/plan/web/frontend-feature-list.md`，缺口分析（已落地）见 `docs/plan/web/server-gaps.md`。

## 2. 整体架构

```
┌──────────────────────────────────────────────────────────┐
│ apps/web-app（SvelteKit 前端）                            │
│  ┌────────────┐  ┌────────────┐  ┌────────────────────┐  │
│  │  装配层     │  │  业务组件   │  │  原子组件           │  │
│  │ routes/load │  │ 时间线/差异 │  │ 按钮/表格/弹窗     │  │
│  └─────┬──────┘  └─────┬──────┘  └────────────────────┘  │
│        └───────────────┼────────────────────────────────┘  │
│  ┌─────────────────────▼────────────────────────────────┐│
│  │ src/lib/api/                                          ││
│  │  client.ts（openapi-fetch + 信封拆包 + x-api-key）     ││
│  │  schema.d.ts（OpenAPI 快照生成，REST 唯一类型来源）     ││
│  │  sse.ts / ws.ts（流式与订阅，类型独立维护）             ││
│  └─────────────────────┬────────────────────────────────┘│
│  stores（跨组件共享状态）  │                               │
└──────────────────────────┼───────────────────────────────┘
                           │ HTTP /api/v1 · SSE · WebSocket
┌──────────────────────────▼───────────────────────────────┐
│ crates/app/wf-server（axum）                              │
│  路由组装 · 鉴权/CORS/限流 · ApiEnvelope · SSE/WS · 静态托管│
├──────────────────────────────────────────────────────────┤
│ crates/app/wf-api（传输无关门面，ApiContext）               │
├──────────────────────────────────────────────────────────┤
│ wf-runtime → engine/infra（wf-agent、wf-workflow、存储等）  │
└──────────────────────────────────────────────────────────┘
```

依赖方向：前端只依赖 HTTP 契约；`wf-server → wf-api` 与 `wf-server → wf-runtime`；`wf-api` 不含任何 HTTP 类型。

## 3. 技术栈

### 3.1 现状（已存在）

| 项 | 选型 |
|---|---|
| 框架 | Svelte 5 + SvelteKit 2，Vite 构建 |
| 运行环境 | Node.js >= 22（仅构建/工具链） |
| 质量工具 | `svelte-check`、`tsc --noEmit`、ESLint、Prettier、Vitest |
| API 类型 | `apps/web-app/openapi.json` → `tools/openapi-codegen` → `src/lib/api/schema.d.ts` |
| API 客户端 | 规划为 `openapi-fetch`（以生成的 `paths` 泛型为事实源，见集成方案） |

### 3.2 目标补充（随实施阶段引入）

- 样式：TailwindCSS，主题与 Token 按 `docs/spec/web/style-guide.md`。
- 组件分层与数据规则：按 `docs/spec/web/component-guide.md`（原子 / 业务 / 装配三层）。
- 工作流与回路图渲染：满足组件规范的图组件要求，渲染库在实现阶段选型，不预设。

## 4. 后端接口约定

### 4.1 路径与信封

- 业务面统一前缀 `/api/v1`；系统面（`/`、`/health`、`/system/*`、`/metrics`）在根路径。
- 响应统一信封 `ApiEnvelope{success, data, error}`；错误统一 `ErrorResponse`，错误码可分支处理（`NOT_FOUND` / `INVALID_PARAMS` / `UNAUTHORIZED` / `FORBIDDEN` / `RATE_LIMITED` 等）。
- 客户端封装职责：注入鉴权头、拆信封、非 2xx 与 `success=false` 归一为类型化错误。详见集成方案第 2 节。

### 4.2 分页

- 过滤型列表统一游标包络 `PageView{items, limit, offset, has_more}`，默认 50、上限 500，**无总数**。
- 链与时间线类走封顶视图（截断标记），不套 `PageView`。
- 前端按游标翻页；裸数组仅限后端注明的有界小枚举。

### 4.3 鉴权与跨域

- API key 鉴权：请求头 `x-api-key`，或查询参数 `api_key`（浏览器 WebSocket 场景）。
- 开关与密钥：`AUTH_ENABLED`、`API_KEYS`（仅环境变量）；默认关闭，排除路径为 `/`、`/health`、`/api/v1/info`。
- CORS 由 `configs/server/cors.toml` 配置，**无环境变量**；默认允许 `*` 与 `x-api-key` 头。
- 密钥不入库不入仓：生产推荐同源托管（前端不接触密钥）；开发期可用本地 `.env.local`。

### 4.4 限流

- 服务端按 IP 滑动窗口限流，响应头 `x-ratelimit-*`，超限 429 + `Retry-After`。前端错误归一需覆盖 429。

## 5. 类型与契约管线

```
wf-server utoipa 注解
   │  WF_REFRESH_OPENAPI=1 cargo test -p wf-server committed_snapshot_matches_document
   ▼
apps/web-app/openapi.json          （golden-file，提交入库，漂移即测试红）
   │  cd tools/openapi-codegen && npm run gen
   ▼
tools/openapi-codegen/schema.d.ts  （中间产物，不提交）
   │  cp schema.d.ts ../../apps/web-app/src/lib/api/schema.d.ts
   ▼
apps/web-app/src/lib/api/schema.d.ts（唯一正式 REST 类型，提交入库）
```

- 类型单一来源：接口类型全部由快照生成，禁止手写重复响应模型。
- codegen 是独立 TS5 小包（不进 `apps` workspace），按需离线运行，不进入日常 dev/build 热路径。
- **SSE/WS 帧类型不在此管线内**：OpenAPI 只能表达 content-type，无法描述每帧；流式类型在 `sse.ts` / `ws.ts`（或 `events.d.ts`）独立显式维护。
- 日常校验：`cargo test -p wf-server committed_snapshot_matches_document`。

## 6. 实时通道

### 6.1 SSE 端点（5 个）

| 端点 | 用途 |
|---|---|
| `GET /api/v1/events/stream` | 全局事件流，支持 `executionId`/`agentLoopId`/`workflowId`/`since` 过滤；首帧 `connected`，30s keepalive 注释帧；客户端上限 100 |
| `POST /api/v1/workflows/{id}/execute/stream` | 工作流流式执行；唯一使用命名事件的端点（首帧 `event: metadata`） |
| `POST /api/v1/agent-loops/{id}/stream` | Agent 回路流式运行 |
| `POST /api/v1/llm/generate-stream` | 大模型流式生成试算 |
| `GET /api/v1/executions/{id}/error-analysis/stream` | 错误链分析流 |

- 执行流帧协议 `ExecutionStreamEvent`（`type` 标签，snake_case）：`engine`、`iteration_start`、`llm_delta`、`tool_start`、`tool_end`、`iteration_end`、`interrupted`、`completed`、`failed`、`reasoning_delta`、`usage`、`sub_agent_started`、`sub_agent_ended`。
- 事件总线帧为 `BaseEvent`，类型名为 SCREAMING_SNAKE_CASE；`since` 游标为不透明 hex，原样回传即可断点续拉。

### 6.2 WebSocket

- 端点 `GET /api/v1/ws`（刻意排除在 OpenAPI 之外）。
- 客户端帧：`subscribe` / `unsubscribe` / `ping`；订阅维度 `executionId`、`agentLoopId`、`workflowId`、`global`、`notifications`。
- 服务端帧：`connection`、`execution_event`、`agent_loop_event`、`workflow_event`、`global_event`、`notification`、`subscribed`、`unsubscribed`、`pong`、`error`；负载为元数据，详情按需 REST 补拉。
- 心跳 30s；鉴权与 REST 相同（头或查询参数），拒绝时关闭码 4001。

### 6.3 选用规则

- 简单日志与时间线 → SSE；多执行并行订阅 → WS。
- POST 流式通道无法用原生 `EventSource` 直连，统一用可取消的流读取器封装（处理鉴权、限流、重连、断点续拉）。
- 流式渲染限速合并，禁止逐帧全量重渲染。

## 7. 前端分层与目录结构

组件三层（详见 `docs/spec/web/component-guide.md`）：原子层不感知业务域；业务层可感知单域模型；装配层负责路由、数据拉取与 store 装配。

```
apps/web-app/
├── openapi.json                     # 后端 OpenAPI 快照（golden-file）
├── src/
│   ├── app.html
│   ├── lib/
│   │   ├── api/
│   │   │   ├── client.ts            # openapi-fetch 客户端 + 信封拆包（规划）
│   │   │   ├── schema.d.ts          # 生成的 REST 类型（已提交）
│   │   │   ├── sse.ts               # SSE 接入（规划）
│   │   │   ├── ws.ts                # WS 接入（规划）
│   │   │   └── <domain>.ts          # 领域薄封装（可选，不重声明响应类型）
│   │   ├── components/              # 原子层 + 业务层
│   │   ├── stores/                  # 跨组件共享状态
│   │   └── utils/
│   └── routes/                      # SvelteKit 文件路由（装配层）
├── package.json                     # @wf-agent/web-app-frontend
├── svelte.config.js
├── vite.config.ts
├── tsconfig.json / tsconfig.test.json
├── vitest.config.ts
└── eslint.config.js / .prettierrc
```

数据流：

- REST：路由 `load` → `client.call()`/`callPage()` → 拆包数据 → store 或组件状态。
- SSE/WS：`sse.ts`/`ws.ts` 建连 → 限速合并写 store → 组件响应式消费；组件不直接持有流读取器。

## 8. 配置、运行与部署

### 8.1 后端侧

| 项 | 来源 | 默认 |
|---|---|---|
| 监听地址 | CLI `--addr` > `WF_SERVER_BIND_ADDR` > `configs/server/server.toml` | `127.0.0.1:3000` |
| 静态目录 | CLI `--static-dir` > `WF_SERVER_STATIC_DIR` > `server.toml` | 未配置 = 纯 API |
| 鉴权 | `AUTH_ENABLED` / `API_KEYS` + `auth.toml` | 关闭 |
| 限流 | `RATE_LIMIT_*` + `rate-limit.toml` | 见配置文件 |
| CORS | `cors.toml`（仅文件） | `allowed_origins = ["*"]` |

### 8.2 前端侧

- `baseUrl`：`VITE_API_BASE`，默认 `/api/v1`（同源相对路径）。
- 开发期接入方式在实施阶段**二选一并固化**：Vite 代理转发 `/api`（保持 `baseUrl` 不变），或显式 `VITE_API_BASE` 指向后端地址（依赖 CORS）。不并存两套。
- 类型检查：`npm run check`（svelte-check）与 `npm run typecheck`（tsc，含测试工程）。

### 8.3 部署形态

- **生产（推荐，同源免 CORS）**：前端构建为静态产物，由 `wf-server --static-dir` 托管并做 SPA 回退；`/api/*` 未知路径保持 JSON 404，不回落到 index.html。适配器目标为 `@sveltejs/adapter-static`（`fallback: index.html`）。
- **独立部署**：静态资源与 API 分离，CORS 用 `cors.toml` 放行来源并包含 `x-api-key` 头。

## 9. 与 CLI / TUI 应用的对比

| 维度 | CLI（wf-headless / wf-mini / wf-tui） | Web（apps/web-app） |
|---|---|---|
| 界面 | 终端 | 浏览器 |
| 后端调用 | 进程内直连 SDK/门面 | HTTP `/api/v1` + SSE/WS |
| 实时反馈 | 同步输出 + pacing | 流式推送 + 限速合并渲染 |
| 可视化 | 文本图 | 图组件、差异查看、统计图表 |
| 部署 | 本地安装 | 静态托管（同源优先） |
| 状态 | 进程内存 | 前端 store + 服务端持久化 |

## 10. 关键设计决策

1. **无 Node 中间层**：`wf-api` 已是传输无关门面，Web 直连 `wf-server`；任何“后端适配层”都属于服务端职责，前端不再自建。
2. **类型单一来源**：REST 类型只来自 OpenAPI 快照与生成的 `schema.d.ts`；流式类型独立维护，不假装来自快照。
3. **实时通道不扭曲 REST 文档**：SSE/WS 的帧协议以代码与专门类型为准，不为生成器妥协。
4. **同源部署优先**：密钥不进前端，CORS 只在独立部署时启用。
5. **功能以服务端路由为边界**：无后端通道的功能（文件编辑、触发器定时启停、技能安装、模板评分、通知收件箱、登录/RBAC）不做前端预设。

## 11. 关联文档

| 文档 | 职责 |
|---|---|
| `docs/plan/web/frontend-feature-list.md` | 功能清单权威版（后端路由为边界） |
| `docs/plan/web-app-integration.md` | OpenAPI 集成与 codegen 管线 |
| `docs/plan/web/server-gaps.md` | 服务端缺口（已落地） |
| `docs/plan/web/web-frontend-borrow-analysis.md` | UI 借鉴方向（zcode / deeix） |
| `docs/spec/web/style-guide.md` | 主题、Token、排版、动效、状态呈现 |
| `docs/spec/web/component-guide.md` | 组件分层与数据规则 |
| `docs/api/08-wf-server-HTTP层.md` | HTTP 层细节 |
| `docs/api/09-openapi-文档生成.md` | OpenAPI 生成机制 |
| `docs/apps/web-app/web-app-feature-list.md` | 页面导航与实施阶段映射 |
| `docs/apps/web-app/implementation-phase-1..4.md` | 分阶段实施计划 |
