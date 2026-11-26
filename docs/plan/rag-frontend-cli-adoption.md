# RAG 前端与 CLI 接入设计

## 0. 结论速览

* 后端已经具备检索、摄取、源管理、诊断、工具和工作流节点，但前端与 CLI 没有对应的领域入口：前端只有生成出来的 OpenAPI 类型，CLI 只能通过 `tool execute` 间接调用。
* 首要问题不是新增检索算法，而是把现有 HTTP 契约变成两端可消费的类型化接口，并让“能力是否可用”有一致的判定方式。
* 前端按“服务层 → fixture 预览 → 管理页面 → 工具引用展示”的顺序补齐；CLI 按“客户端请求能力 → `wf rag` 命令 → 命中结果渲染”的顺序补齐。
* 两端只依赖 `wf-server` 的 HTTP 契约，不复制分块、嵌入、融合或数据源读取逻辑。
* 开发期不做兼容层：旧的内存状态、手工复制的生成文件和通用 `unknown` 页面不作为长期方案保留。

## 1. 现状盘点

| 层面 | 已有能力 | 缺口 |
|---|---|---|
| `wf-rag` / `wf-api` | 混合检索、分块、重排、引用、source 状态持久化 | 无面向 UI 的能力总览 |
| `wf-tools` | `rag_search` 基础工具；摄取、源管理、状态、评估工具按 capability 注入 | 只读检索无法单独开启：宿主装配要求摄取或评估至少一项启用 |
| `wf-server` | 5 条 RAG 路由，受 `rag-vector` feature 控制 | 成功响应多为通用 JSON；域文档只列了 3 条只读路由 |
| 前端 | `schema.d.ts` 已包含 RAG 路径；通用 `executeTool` 可调用工具 | 无 `services/rag.ts`、页面、fixture、引用解析；工具分类把 `rag_search` 归为网络调用 |
| CLI | 通用 `wf tool list/show/execute` | 无 `wf rag` 子命令；无命中结果格式化；无法读写 source 管理端点 |

## 2. 目标与不做事项

### 目标

* 让 stats、preview、diagnose、source 列举/摄取/重建/删除在两端都有稳定入口。
* 让前端在预览模式下也能演示完整流程，不依赖真实后端。
* 让模型工具输出的检索命中在对话与 CLI 中可读：来源、分数、chunk 标识、摘录。
* 让只读检索、摄取、评估可以独立配置；能力关闭时返回明确的服务不可用语义。

### 不做事项

* 不在前端或 CLI 实现 RAG 管线，不直接访问向量库或文件系统。
* 不改变召回、融合、重排、引用和 source 指纹算法。
* 不做后台摄取任务队列、进度轮询与取消；当前摄取保持同步 HTTP 语义。
* 不做对话级 RAG 开关的持久化协议；本轮只在工具结果呈现层识别检索命中。

## 3. 契约层设计

### 响应模型

在 `wf-server` 的 RAG 域中定义 UI 需要的响应 DTO，并把现有 `serde_json::Value` 响应注解替换为这些 DTO。运行时 JSON 字段保持不变，DTO 只负责生成 OpenAPI schema 与两端类型。

| DTO | 字段要点 |
|---|---|
| 集合统计 | collection、namespace、point_count、keyword_point_count（可空） |
| 检索预览命中 | chunk_id、source、excerpt、score |
| 无命中诊断 | likely_causes 字符串数组；可选 namespace/collection 提示由服务端追加 |
| source 状态条目 | state（含 collection、namespace、source、kind、uri、状态、内容哈希、计数、错误信息与可选revision 字段）、point_count、keyword_point_count |
| 摄取结果 | source、action（insert/update/skip）、documents、chunks、points、skipped、content_hash、revision、metadata |

`RagSourceWrite` 继续作为统一请求体：source、uri、kind、collection、namespace 为必填，converter、options、force 可选。删除请求复用 source、collection、namespace 三元组。

### 生成链路

1. 修改 handler 的 `utoipa::path` 注解，声明具体响应 DTO。
2. 刷新 `crates/app/wf-server/openapi.json` 快照。
3. 由 `tools/openapi-codegen` 生成 `apps/web-app/src/lib/api/schema.d.ts`；`wf-client` 在编译时从同一快照生成 Rust 客户端。
4. 生成产物不手工编辑；域文档 `docs/app/server/16-rag.md` 与路由表保持一致。

### 错误语义

* 未注入 RAG handles：stats/preview 返回服务不可用；诊断仍可基于查询形状工作。
* 未注入 source service：source 路由返回服务不可用。
* 参数非法：统一为校验错误。前端把服务不可用渲染成“能力未配置”，而不是空列表；CLI 输出稳定错误码与提示。

## 4. 能力门禁设计

`RagCapabilitiesConfig::any_enabled` 当前要求摄取或评估至少一项开启，导致只配置只读检索时 `attach_rag_capabilities` 不执行。本轮把主开关直接作为装配条件；检索 backend 由主开关控制，摄取、源管理、状态、评估 backend 继续保持各自子开关。这样工具可见性与实际可执行性一致，且不需要新增“版本 2”配置。

## 5. 前端设计

### 服务层

新增 `apps/web-app/src/lib/services/rag.ts`，定义与 DTO 对应的 TypeScript 类型和五个领域函数：读取统计、预览检索、诊断、列举 source、写入/重建/删除 source。统一走现有 `client` + `call` + `requireData` 约定，不在组件内散落路径字符串。

### 预览模式

新增 RAG fixture 与 mock 路由并注册到 mock registry，覆盖有数据、空数据和能力不可用三种状态。预览模式下的页面与真实后端使用同一服务层，不通过组件内分支伪造数据。

### 管理页面

新增知识库页面：顶部选择 collection/namespace，分区展示索引统计、source 列表、检索预览与诊断。source 操作包含添加、强制重建、删除；每个操作有进行中、成功和失败状态。页面入口加入侧边导航与中英文文案。

### 工具引用展示

`utils/toolcalls.ts` 先把 `rag_*` 归入 `knowledge`，再执行通用搜索/网络规则；同时提供命中数组解析函数，识别 `chunk_id`、`source`、`score`、`content`。`ToolCallCard` 在工具卡片内以紧凑列表展示引用，完整 JSON 仍保留在详情中。

## 6. CLI 设计

### 客户端能力

为 `WfClient` 增加带 JSON body 的 DELETE 请求方法，因为 source 删除请求体携带 collection、namespace、source。其余端点复用现有 typed 调用或 JSON escape hatch。

### 命令结构

新增 `wf rag` 命令组：

* `stats`：输出集合统计。
* `preview`：按 query、top_k、excerpt_chars 输出命中。
* `diagnose`：输出可能原因。
* `sources list`：按范围列举 source 状态。
* `sources add`：从 JSON 文件或独立参数构造 source 描述符并摄取。
* `sources reindex`：强制重建。
* `sources delete`：按三元组删除。

参数命名沿用后端 snake_case，命令输出沿用 `OutputEnvelope`，文本模式显示摘要，结构化模式保留原始 DTO。

### 命中渲染

`wf rag preview` 与 `wf tool execute rag_search` 共享命中格式化函数，输出分数、chunk 标识、来源与摘录；空结果显式提示。

## 7. 实施阶段与验收

1. 契约阶段：DTO、OpenAPI 快照、TS schema、域文档一致；生成产物由工具产生。
2. 门禁阶段：只读检索可单独启用；增强工具仍受子开关控制。
3. 前端接入阶段：服务层、fixture、页面、导航和文案可用；预览模式能演示全流程。
4. CLI 接入阶段：`wf rag` 全部子命令可执行，错误码可读。
5. 呈现阶段：前端与 CLI 均能展示检索命中的引用信息。

验收以一次工作区编译检查和一次前端检查为准，避免在开发过程中反复触发全量构建。

## 8. 验收记录

* `devs` 上执行 `cargo check --locked` 通过；`wf-server` OpenAPI 快照刷新测试通过。
* `wf-types`、`wf-cli-shared` 的新增单元测试在 `devs` 上通过。
* `web-app` 的 `check`、`check:preview` 与 `vitest run` 均通过，预览模式覆盖新增 RAG 路由。
