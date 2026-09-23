# OpenAPI 文档生成（后端）

对应 `crates/app/wf-server` 内的核心文件：`openapi.rs`（文档聚合、全局修改器、快照测试）、`envelope.rs`（成功/错误信封）、`paged.rs`（分页与截断视图）、`extract.rs`（共享 Path/Query 提取器与 `IntoParams`）。生成方式为编译期静态聚合：每个 handler 上的 `utoipa::path` 注解在编译时被收集，`ApiDoc::openapi()` 在运行时拼出完整文档，不依赖运行时反射或外部扫描。

**消费形态以离线快照 + TypeScript 代码生成为主**：仓库内提交 `apps/web-app/openapi.json`，前端类型由 `tools/openapi-codegen` 按需从该 JSON 一次性生成（见文末）。不提供 Swagger UI，不依赖公网 CDN。

## 1. 生成链路

链路共四环，新增端点必须走完四环文档才完整：

1. handler 注解：每个 handler 上方写 `#[utoipa::path(...)]`，声明方法、路径、tag、参数、请求体、响应与鉴权。
2. 路径注册：在 `openapi.rs` 的 `ApiDoc` 的 `paths(...)` 列表中登记该 handler 函数路径，登记即进入文档。
3. 类型注册：在 `components(schemas(...))` 中登记该 handler 引用的具体类型（信封实例、分页壳、本地 `*Body`/视图类型）；泛型每实例化一次登记一次。
4. 快照与测试：golden-file 测试读写 `apps/web-app/openapi.json`；`openapi::tests` 守住规模、全局错误码与鉴权方案。

路由挂载（`router.rs`）与业务路由相互独立：文档路由仅 `/api-docs/openapi.json`（debug 构建或 `openapi-docs` feature），业务域路由在 `/api/v1` 下，系统面（`/`、`/health`、`/system/*` 等）在根路径。文档路由在状态擦除后挂载，不经业务领域逻辑。

## 2. `ApiDoc` 的文档结构

`ApiDoc` 为空结构体，仅承载 `openapi` 属性，主要部分：

- 基本信息：标题 `wf-server API`，版本 `v1`，描述覆盖 agent 循环、工作流、checkpoint、模板与 LLM 集成。
- 路径列表：按 agent、workflow、checkpoint、trigger、template、llm、entity、observation、system、web 等域分组登记，当前 **452 个操作**（含 health/metrics/Prometheus `/metrics`）。登记的是 handler 函数，不是 URL 字符串。
- 类型组件：登记响应体与请求体引用的具体类型；当前快照 72 个 component schema。
- 标签：十个域标签，每个标签带一句话说明，供文档工具分组。
- 服务器：`url = "/"`，路径字符串自带完整前缀（业务面带 `/api/v1`，根级路由写绝对路径），因此根级健康检查与 `/api/v1/*` 可出现在同一文档中。
- 全局鉴权：声明全部操作默认使用 `api_key`；方案定义由 `SecurityAddon` 在文档生成后注入（API key，请求头 `x-api-key`，兼容查询参数 `api_key`，与 `middleware.rs` 一致）。
- 全局错误响应：`GlobalErrorResponses` 向每个操作注入 `401`/`403`/`429`/`503`/`504`，与中间件真实行为对齐，避免在 400+ 注解中手写。

## 3. 响应体与错误约定

成功响应体统一使用信封 `ApiEnvelope<T>`（字段：成功标志、数据与错误）。错误响应统一使用 `ErrorResponse`（运行时唯一错误信封；`error` 非空，`data` 在错误路径恒为 `null`）。约定按端点形态分类：

- 普通单个资源：数据为通用 JSON 或本地视图类型，注解写对应信封实例。
- 列表分页：数据为分页视图（items、limit、offset、hasMore），注解写信封套分页视图。不返回总数是刻意设计。
- 链与时间线截断：数据为截断视图，注解写信封套截断视图。
- SSE 流：成功响应声明 `text/event-stream`，不按 JSON body 声明；错误仍走信封。
- 文件下载：成功响应声明字符串与对应文件内容类型；错误仍走信封。

分页构造统一：handler 取 limit+1 作窗口，多出的一条转为 `hasMore` 并丢弃；列表默认每页 50、硬上限 500；链条目硬上限 500，时间线硬上限 5000。查询参数以 `IntoParams` 结构体为单一事实源，注解写 `params(Type)`（utoipa 5 无 `With<T>`）。

## 4. handler 注解写法

每个注解包含方法、路径、标签、参数、请求体、响应与鉴权等要素：

- 路径参数与查询参数：优先 `IntoParams` 结构体；`#[serde(flatten)]` 分页字段在注解侧展开为 `limit`/`offset`，handler 用 `resolve_page_fields` 构造。
- 请求体：本地 `Json<T>` 类型 derive `ToSchema` 并写入 `request_body = Type`；未类型化的保持通用 JSON。
- 响应：200 按第三章形态声明；400/400 级参数错误、404、500 按错误信封声明；401/403/429/503/504 由全局修改器注入，注解可不写。
- 鉴权：操作级 `security(("api_key" = []))`，与全局声明呼应。

本地视图类型（仅含返回字段的结构体）派生 `ToSchema` 后在成功响应的数据部分引用；领域类型不进 foundation/engine 的 `utoipa` derive， wf-server 侧用 `#[schema(value_type = ...)]` 标注。

## 5. 快照、文档服务与构建开关

**离线快照（codegen 唯一输入）**

- 路径：`apps/web-app/openapi.json`（已提交）。
- 刷新：`WF_REFRESH_OPENAPI=1 cargo test -p wf-server committed_snapshot_matches_document`。
- 校验：默认跑同一测试，序列化结果与仓库文件逐字节比对，漂移即失败。

**运行中调试**

- 仅挂载 `GET /api-docs/openapi.json`，返回当前进程的 `ApiDoc` 序列化结果，供开发时对照。
- **无 Swagger UI、无 `/api-docs/swagger`、无公网 CDN 依赖**；人类浏览用本地查看器打开快照文件。

**构建开关**：debug 构建或启用 `openapi-docs` feature 时挂载文档路由；不满足条件的发布构建不挂载，对外无文档面。注解与 `ApiDoc` 始终参与编译（快照测试需在任意 profile 下可运行）。

## 6. 测试守卫

`openapi::tests` 覆盖：

1. 文档为合法 OpenAPI 3.1.0，标题正确，路径规模完整。
2. 每个操作声明 200 成功响应，且操作总数为 **452**（一操作一注解 handler）。
3. 全局错误响应（401/403/429/503/504）已注入。
4. `securitySchemes` 注册为请求头位置的 API key（`x-api-key`）。
5. 提交快照与当前 `ApiDoc` 一致（可用 `WF_REFRESH_OPENAPI=1` 重写）。

操作总数断言约束「注解已登记」；`routes_match_utoipa_annotations` 解析源码 `.route` 与 `#[utoipa::path]` 做集合比对，显式排除 WS 与文档路由，补上「只加路由不加注解」的盲区。

## 7. 离线 TypeScript codegen

生成与消费分离，**按需、一次性**，不随 dev server 实时刷新：

1. 后端：注解变更后刷新 `apps/web-app/openapi.json`（见第五章）。
2. 工具：独立小包 `tools/openapi-codegen`，仅依赖 `openapi-typescript` 与其 peer `typescript@5`。
3. 命令：在该目录 `npm install` 后执行 `npm run gen`，读入快照 JSON，在**本目录**写出 `schema.d.ts`（该目录 `.gitignore` 忽略生成物）。
4. 正式类型：将 `tools/openapi-codegen/schema.d.ts` 复制为 `apps/web-app/src/lib/api/schema.d.ts` 并提交；web-app 日常 `svelte-check`/构建不依赖 codegen 工具链。

**为何独立小包**：`openapi-typescript` 的 peer 为 `typescript ^5.x`，且当前全部 7.x 版本在 TypeScript 7 下运行时失败（上游 issue 仍开放；TS7 根导出无 `ts.factory`）。web-app 使用 TS7（svelte-check）。生成是低频按需操作，用目录隔离 TS5 依赖，避免 npm overrides/legacy peer 与 apps workspace 提升冲突，也不把 TS5 混进前端依赖树。`tools/` 不在 `apps` workspace 内，独立 `package.json` + lockfile。

详细步骤与前端封装见 `docs/plan/web-app-integration.md`。

## 8. 新增端点操作清单

新增业务端点时，文档侧同步四处：

1. handler 上方的 `#[utoipa::path]` 注解。
2. `ApiDoc` 的 `paths(...)` 函数登记。
3. `components(schemas(...))` 中新响应/请求类型的登记（本地类型需 `ToSchema`）。
4. 测试中的操作总数（`ops == 452` 断言）。

随后刷新快照并按需重跑 codegen：`WF_REFRESH_OPENAPI=1` 相关测试 → `tools/openapi-codegen` 执行 `npm run gen` → 将本目录 `schema.d.ts` 复制到 `apps/web-app/src/lib/api/`。响应形态特殊（流或下载）时按第三章类别声明，错误保持 `ErrorResponse`。

## 9. 已知限制与后续

- route↔annotation 一致性测试已覆盖源码路由表；SSE/WS 帧结构不由本生成物覆盖，流式类型独立维护。
- 大量响应 `data` 仍为通用 JSON（`unknown`），待按前端需求用 C1 镜像 DTO 逐步类型化（见 `openapi-schema-typing.md`）。
- 下载端点在非下载模式下实际可能返回信封，文档按下载成功形态声明，分支差异尚未在文档中展开。
