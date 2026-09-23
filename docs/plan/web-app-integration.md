# web-app（SvelteKit）OpenAPI 集成方案

配套：`openapi-utoipa-migration.md`、`openapi-schema-typing.md`、`docs/api/09-openapi-文档生成.md`。目标：让
`apps/web-app`（SvelteKit 2 + Svelte 5）基于 `wf-server` 的 **OpenAPI 离线快照**做
类型安全的 API 调用，并明确流式/WebSocket 的类型化边界。

## 现状事实

- `apps/web-app`：SvelteKit 2 + Svelte 5 + TS 7 + Vite，node>=22；`typecheck` 脚本被置空（`true`），
  真实类型检查走 `svelte-check`（`npm run check`）。
- 后端：REST 业务面在 `/api/v1`；系统面（`/`、`/health`、`/system/*`）与 Prometheus `/metrics` 在根路径。
  文档调试路由仅 `/api-docs/openapi.json`（debug 或 `openapi-docs` feature），**无 Swagger UI、无 CDN 页面**。
  响应体统一信封 `ApiEnvelope{success,data,error}`，列表套 `PageView`/`CappedView`；错误统一 `ErrorResponse`。
- 快照：`apps/web-app/openapi.json` 已提交（golden-file，`cargo test -p wf-server` 校验；`WF_REFRESH_OPENAPI=1` 刷新）；
  当前约 390 paths / 452 operations / 72 component schemas，`securitySchemes: apiKey/header (x-api-key)`。
- 鉴权真实为 **API key**：默认请求头 `x-api-key`，亦支持 `api_key` 查询参数；
  由 `AUTH_ENABLED` 开关、`API_KEYS` 供密钥（`middleware.rs`）。CORS 走
  `cors.toml`（`allow_origin`/`allow_headers`），**CORS 无环境变量**（部署期决定）。
- `openapi-schema-typing.md` 阶段 0 的鉴权修正已完成：文档为 `apiKey`，非 `bearer_auth`。

## 设计目标

- 类型来源唯一：接口类型全部由后端 OpenAPI 快照生成，禁止手写重复的响应模型。
- 调用点少样板：一个 client + 薄封装，处理信封拆包、错误归一、鉴权头注入。
- 流式与 WebSocket 有明确的、类型化的接入方式（不与 REST 混为一谈）。
- codegen **按需、离线**：不依赖后端进程，不进入日常 dev/build 热路径。

## 1. 类型生成管线（codegen）

### 快照（输入）

- 仓库持有 `apps/web-app/openapi.json`，由后端测试写出/校验（不是运行时拉取）。
- 刷新时机：handler 注解、全局错误码、安全方案或路径集合变更后：
  `WF_REFRESH_OPENAPI=1 cargo test -p wf-server committed_snapshot_matches_document`。
- 漂移检测：默认跑同一测试，与仓库文件 diff 失败即红；可再加 CI `git diff --exit-code`。

### 生成器（`tools/openapi-codegen`）

- **独立 npm 小包**，不进入 `apps` workspace，与 web-app 依赖树隔离。
- 依赖：`openapi-typescript` + peer `typescript@5`。
- 职责：读入 `../../apps/web-app/openapi.json`，在**本目录**写出 `schema.d.ts`（中间产物，`.gitignore` 忽略）。
- 正式类型：将 `tools/openapi-codegen/schema.d.ts` 复制为 `apps/web-app/src/lib/api/schema.d.ts` 后提交；codegen 目录生成物不进 git 历史。
- 安装与执行仅在**需要重新生成时**进行（低频）。

**为何独立小包而非装在 web-app**

| 方案 | 结论 |
|------|------|
| web-app 内直接装 `openapi-typescript` | **否决**：peer 要求 `typescript ^5.x`，web-app 为 TS7；全部 openapi-typescript 7.x 在 TS7 下运行时失败（`ts.factory` 不存在，上游 issue 未关） |
| npm overrides / legacy-peer-deps 强行共存 | **否决**：peer 解析不稳定，属兜底，易在锁文件更新时回归 |
| `tools/openapi-codegen` 独立包（TS5 仅存在于该目录） | **采用**：生成是按需一次性操作，目录隔离即结构隔离；前端只消费 JSON + `.d.ts`，日常 check/build 不装 codegen 链 |

TS7 programmatic API 官方亦未稳定到可用于此类工具；等待上游支持不可控，隔离是确定解。

### 生成命令

- 在 `tools/openapi-codegen` 下 `npm install` 后执行 `npm run gen`，得到本目录 `schema.d.ts`。
- `cp schema.d.ts ../../apps/web-app/src/lib/api/schema.d.ts` 作为正式文件提交。
- 依赖只声明在 codegen 包；web-app 不持有 `openapi-typescript` / `gen:api`（已移除，避免双源）。
- 详细步骤见 `tools/openapi-codegen/README.md`。

### 产物

- `tools/openapi-codegen/schema.d.ts`：中间产物，**不提交**（该目录 `.gitignore`）。
- `apps/web-app/src/lib/api/schema.d.ts`：复制后的正式类型，提交入库。
- 前端 tsconfig 通过包含该文件或 `/// <reference` 消费（实现阶段确定一种，保持单一方式）。

## 2. 客户端封装

- 采用 `openapi-fetch`（以生成的 `paths` 泛型为唯一事实源，运行时只做 fetch，零手写类型）。
- `src/lib/api/client.ts`：
  - `createClient<paths>({ baseUrl })`；`baseUrl` 默认 `/api/v1`（根级系统端点若使用需在调用处写全路径）。
  - 鉴权：`client.use({ onRequest })` 注入 `x-api-key`（来源见 §5），与 `securitySchemes` 对齐。
- **信封拆包**：
  - `call()`：`success=false` 或 HTTP 非 2xx → 类型化错误；否则返回 `data`。
  - `data` 精度随 schema-typing 阶段推进：阶段 A 后外壳与分页字段类型化；C1 镜像后具体 DTO 替换 `unknown`。
  - 分页：`callPage()` 返回 `{ items, hasMore }` 等，对应 `PageView`/`CappedView`。
- 错误归一：`ErrorResponse` / `ApiErrorBody{code,message}` 映射为可分支的前端错误
  （`NOT_FOUND`/`INVALID_PARAMS`/`UNAUTHORIZED`/`RATE_LIMITED` 等，见 `envelope.rs`）。

## 3. 分层用法（Svelte）

- `src/lib/api/`：client + 生成类型 + 拆包封装。
- 领域薄封装（可选）`src/lib/api/<domain>.ts`：聚合常见调用，复用生成 query 类型，不重声明响应类型。
- `+page.ts`/`+layout.ts` 的 `load` 调用；组件消费拆包后的数据。

## 4. 流式 / WebSocket（类型化边界）

- **REST + OpenAPI 不覆盖流式**：
  - SSE：生成物只表达 content-type，无法描述每帧。原生 `EventSource`/`fetch`+`ReadableStream` 单列 `src/lib/api/sse.ts`；
    事件帧类型镜像后端事件枚举（后端有 schema 则引用，否则 `events.d.ts` 手写 + 契约测试）。
  - WebSocket：按设计排除在 OpenAPI 外；`src/lib/api/ws.ts` 手写或另行生成，不假装来自快照。
- 原则：`schema.d.ts` 是 REST 唯一来源；流式类型独立、显式维护。
- 快照 + `.d.ts` 重生成时机：注解或全局文档结构变更；流式契约变更只改 `sse`/`ws` 层。

## 5. 配置与鉴权来源

- `baseUrl`：`VITE_API_BASE`，默认 `/api/v1`（同源相对路径即可）。
- API key：默认不硬编码。择一：
  1. 同源反代 / `--static-dir` 托管前端（推荐生产，前端不碰密钥）。
  2. 登录换取（若后续有 login/token 端点）。
  3. 开发本地：`import.meta.env.DEV` 下读 `.env.local` demo key。
- 当前后端无 login/token；鉴权保持 API key。

## 6. 部署形态

- `--static-dir` SPA fallback 托管 `apps/web-app/build`（`adapter-static`），同源免 CORS。
- `svelte.config.js` 明确 `@sveltejs/adapter-static`（`paths.base=''`，fallback `index.html`）。
- 独立部署：CORS 用 `cors.toml`（含 `x-api-key`）。

## 7. 落地顺序

1. schema-typing 阶段 0（鉴权 apiKey、SSE/下载 content-type）→ **已完成**。
2. schema-typing 阶段 A（信封/分页外壳类型化）→ **后端已完成**；前端依赖快照与 `.d.ts`。
3. 离线快照 golden-file → **已完成**（`apps/web-app/openapi.json`）。
4. `tools/openapi-codegen`：独立包、TS5、本目录生成后复制到 web-app → **已完成**。
5. web-app：`openapi-fetch` + `client.ts` 拆包；一个真实只读页打通。
6. 按页面需要引入 C1 镜像 DTO，替换 `data: unknown`。
7. 流式页面接 `sse.ts`/`ws.ts`。

## 8. 验收标准

- `tools/openapi-codegen` 无需后端进程即可从快照生成；同一 JSON 重复生成稳定；中间文件不进 git，复制到 web-app 的 `.d.ts` 为唯一正式产物。
- `npm run check`（svelte-check）通过；`schema.d.ts` 无 `any` 泄漏。
- 至少一个 REST 列表页 + 一个详情页：参数、`data`、`error` 全类型化，无手写响应模型。
- SSE 页面：逐帧消费、类型化事件、错误/关闭有处理。
- 生产构建由 `--static-dir` 同源托管。
- 快照与后端 `ApiDoc` 一致（`cargo test`；可选 CI diff）。

## 9. 风险与取舍

- `data` 长期停在通用 JSON → 前端等于手写类型；需按页面推进 C1。
- `openapi-typescript` 对 SSE/WS 无能为力是既定事实；不为此扭曲 REST 文档。
- 快照 + `.d.ts` 以显式更新换离线可构建与漂移可见；codegen 工具升级需单独在 `tools/` 验证，不牵动前端锁文件。
- TypeScript 7 与 openapi-typescript 的兼容以上游为准；在上游修复并声明 peer 前，codegen 包保持 TS5，不与 web-app 合并。
