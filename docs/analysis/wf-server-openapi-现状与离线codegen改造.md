# wf-server OpenAPI 现状分析与离线 codegen 改造

> 范围：`crates/app/wf-server`、`crates/app/wf-api`、`apps/web-app`
> 目标用法：OpenAPI 以**离线快照**消费、以 **TypeScript 代码生成**为主要用途
> 配套：`docs/api/09-openapi-文档生成.md`、`docs/plan/openapi-schema-typing.md`、`docs/plan/web-app-integration.md`

## 1. 集成现状

技术选型为单一依赖 `utoipa 5`（解析为 5.5.0），无 `utoipa-axum`/`aide` 等桥接库。文档在编译期由三处手工同步装配：

1. 每个 handler 上的 `#[utoipa::path(...)]` 注解（改造前 437 处）；
2. `openapi.rs` 中 `ApiDoc` 的 `paths(...)` 按**函数引用**登记；
3. 各模块 `routes()` 中的 axum 路径字符串。

响应信封 `ApiEnvelope<T>`、分页壳 `PageView`/`CappedView`、错误体 `ApiErrorBody` 均已在 wf-server 内 `ToSchema`；`utoipa` 不进入 foundation/engine 层。文档路由在 debug 或 `openapi-docs` feature 下挂载于 `/api-docs/*`。

## 2. 一致性核查结论（穷举比对）

| 维度 | 结论 |
|------|------|
| paths 登记 ↔ 注解 | 改造前 437 = 437，零差异 |
| 注解路径 ↔ axum 路由 | 0 幽灵路由、0 路径不匹配 |
| 已注册未文档化 | health/storage/metrics/Prometheus `/metrics`/WS 共 16 项 |
| 查询参数 | 约 23 个 handler 与注解漂移（漏字段或幻影 `limit`/`offset`） |
| 状态码 | 401/403/429/503/504 均未在操作级声明 |
| `ops == 437` 测试 | 只能约束「注解已登记」，**不能**发现「新增路由未写注解」 |

## 3. 问题清单（按影响排序）

1. **无离线快照导出**：唯一出口是 debug-only HTTP 路由，前端 codegen 必须起服务——与离线用法直接冲突。
2. **`data: unknown`**：改造前 329+ 操作的 `data` 为 `serde_json::Value`，`openapi-typescript` 生成 `unknown`。
3. **请求体 100% 为 `Value`**：91 处 `request_body = serde_json::Value`，其中多处本地类型已 derive `ToSchema` 却未引用。
4. **查询/路径参数手工三重维护**：结构体、注解元组、路由各写一份；已实测漂移。
5. **全局错误码缺失**：中间件真实产生 401/403/429/503，`ApiError::Timeout` 产生 504，文档零声明。
6. **`ErrorResponse` 与 `ApiEnvelope<ApiErrorBody>` 形状重复**：文档标前者，运行时发后者。
7. **Swagger UI 依赖 unpkg CDN**：部署环境不直连外网时页面必挂，且与代码注释「无需出网」矛盾。
8. **health/metrics 整面未文档化**；`system` 标签描述声称覆盖 health/metrics 但 paths 未登记。
9. **`openapi-docs` 挡板过浅**：仅挡路由挂载，注解与 `ApiDoc` 始终编译。
10. **`utoipa` 未进 `workspace.dependencies`**，版本仅由 wf-server 单点声明。

## 4. wf-api 无 `ToSchema` 的问题与解法

### 现状

- `wf-api` 约 172 个 `Serialize` 类型、0 个 `ToSchema`；真实 DTO 大量内嵌 `wf-types` 枚举/结构（`ExecutionStatus`、`BaseEvent`、`LlmRequestSummary` 等）甚至 engine 类型（`ToolCallRecord`）。
- 若在 wf-api/wf-types 直接 derive，`utoipa` 将级联进入 foundation/engine/checkpoint——违反分层，且 536 个 `Serialize` + 123 个枚举的属性处理成本高。
- serde 属性组合（`flatten`×11、`deny_unknown_fields`×3、`untagged`、同名双 `ExecutionStatus`）进一步抬高直接 derive 成本。

### 方案对比

| 方案 | 做法 | 结论 |
|------|------|------|
| **C1 镜像 DTO（默认）** | 在 wf-server 为端点定义仅含 `ToSchema` 的轻量视图 | **推荐起点**：零 DAG 污染，已有 6 例先例；按前端页面需求逐端点补；用 serde 往返测试防形状漂移 |
| **C2 feature 门控 derive** | `api-schema` feature 下在 wf-api/wf-types derive，仅由 `openapi-docs` 传递开启 | **升级路径**：当镜像维护成本明显超过 cfg 噪声时启用；须先上收 `utoipa` 到 workspace |
| 无门控直接 derive | wf-api/wf-types 无条件 `ToSchema` | **否决**：foundation 每次构建都付依赖成本 |
| `schemars` | 第二套 schema 体系 | **否决**：产出 JSON Schema 而非 OpenAPI，需合并转换；同样级联；对本仓库 serde 属性组合相对 utoipa 无增益 |

### 升级判据

- 默认走 C1，仅对**前端真实页面需要**的响应/请求类型建镜像；
- 当需精确类型的端点数量显著上升（镜像字段清单开始系统性复制 wf-api 结构）时，再启动 C2，且必须 feature 门控。

## 5. 离线 + TS codegen 为主的适配性评估

### 结论：骨架不需重构，缺的是导出能力与内容深度

**无需改造的部分：**

- 编译期 `ApiDoc` 聚合、函数引用登记、OpenAPI 3.1.0 输出，均适配 codegen；
- utoipa 5.5 与 `openapi-typescript` 无格式兼容问题（3.1 的 type 数组/null 处理正确）；`Value` → `unknown` 是**内容**问题；
- 分层干净（utoipa 仅在传输层）；
- 注解常驻编译（不 feature 化）反而有利于快照测试在任意 profile 下运行。

**必须改造（按优先级）：**

1. **快照导出机制**（阻断项）：golden-file 测试写出/校验 `apps/web-app/openapi.json`，`WF_REFRESH_OPENAPI=1` 刷新；不采用 build.rs、不以 HTTP 为 codegen 源。
2. **参数单一事实源**：全部 Query/Path 结构 `IntoParams`，注解 `params(Type)` 由 extractor 类型驱动，消灭元组手工重复与漂移类。
3. **请求体引用真实 `Json<T>` 类型**：本地 `*Body` 补 `ToSchema` 并写入 `request_body`；领域类型按 C1/C2 分期，未类型化前保持 `Value`。
4. **全局错误响应**：单一 `Modify` 向每个操作注入 401/403/429/503/504，避免 437 处手写。
5. **统一错误 schema**：`ErrorResponse` 成为运行时唯一错误信封（成功仍是 `ApiEnvelope<T>`），删除双轨。
6. **删除 CDN Swagger UI**：保留 `/api-docs/openapi.json` 供运行中调试；人类浏览用本地查看器打开快照。
7. **补全 health/metrics 注解**；`servers` 调整为根路径并给路径加 `/api/v1` 前缀，使根级路由可被同一文档描述。
8. **route↔annotation 一致性测试**：解析源码路由表与注解路径比对，堵住 `ops == 437` 的单向盲区。

**明确不做：**

- 不从 axum router 反推整份文档（无成熟集成、引入第二事实源）；
- 不把注解 feature-gate 掉（快照测试与 release 文档导出都会失效）；
- 不用 build.rs 生成（多余重编译，违背资源约束）；
- 不预置 vendor Swagger UI 静态资源（当前目标不需要）。

### web-app 侧配套

- 提交 `apps/web-app/openapi.json` 快照 + `src/lib/api/schema.d.ts` 产物；
- **codegen 独立小包 `tools/openapi-codegen`**：仅在该目录声明 `openapi-typescript` + `typescript@5`，
  按需读入快照 JSON、在本目录写出 `schema.d.ts`（gitignore 中间产物）；复制到
  `apps/web-app/src/lib/api/schema.d.ts` 才是正式文件。不进 `apps` workspace，web-app 日常依赖不含 codegen 链。
  理由：openapi-typescript 全部 7.x 的 peer 为 `typescript ^5.x` 且在 TS7 下运行时失败（`ts.factory`），
  而 web-app 为 TS7；生成是低频一次性操作，目录隔离优于 overrides 兜底。详见 `web-app-integration.md` §1。
- 后续接 `openapi-fetch` + 信封拆包（见 `web-app-integration.md`）；
- 流式（SSE/WS）类型独立维护，不伪装成 OpenAPI 生成物。

## 6. 改造路线与验收

| 阶段 | 内容 | 验收 |
|------|------|------|
| 结构修正 | 删 CDN UI；全局错误码 Modify；`ErrorResponse` 运行时统一 | `cargo test -p wf-server`；快照中出现 401/403/429/503/504 |
| 参数/请求体 | `IntoParams` + `params(Type)`（utoipa 5 无 `With<>`）；本地请求体 `ToSchema` | 查询参数与结构体字段一致；请求体 schema 引用真实类型 |
| 覆盖补全 | health/metrics 注解；servers/路径前缀；route↔annotation 测试 | 业务路由除显式排除（WS/docs）外均有文档 |
| 离线 codegen | golden-file 快照；codegen 本目录写 `.d.ts` 后复制到 web-app | 无服务进程可生成类型；快照漂移使 `cargo test` 失败 |
| 按需类型化 | C1 镜像 → 必要时 C2 | 前端页面 `data` 不再是 `unknown` |

每阶段后统一跑：`cargo fmt` → 合并的 `cargo clippy -p wf-server --all-targets` / `cargo test -p wf-server` → 快照刷新 → 在 `tools/openapi-codegen` 执行生成。

### 落地状态（本分析文档配套改造）

- 结构修正、参数/请求体、health/metrics 覆盖、servers 根路径、golden-file 快照：**已完成**（操作数 452，快照 390 paths）。
- `tools/openapi-codegen` 实装与 `schema.d.ts` 提交：**已完成**（TS5 隔离，`npm run gen`）。
- route↔annotation 一致性测试：**已完成**（`routes_match_utoipa_annotations`，排除 WS/docs）。
