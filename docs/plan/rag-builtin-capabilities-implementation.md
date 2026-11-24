# RAG 内置增强能力实施方案

## 实施边界

本方案把设计文档中的能力落成四组可独立启用的代码改动。保持 `rag-core` 供应商中立；不引入向量库、Embedding 或模型供应商实现；不新增 `web_fetch` 到 `rag-core` 的依赖；不修改 `rag_search` 默认输出。

所有宿主侧新工具默认不注册 handler：只有宿主注入后端时才开放，因此不会改变现有只读检索语义。

## 阶段一：核心纯逻辑补齐（`rag-core`）

- 配置化分块预设：在 `ChunkerConfigData` 增加预设名，`RagIndexConfig::chunker_config` 按预设生成运行时参数，显式 target/overlap/token_budget 仍覆盖预设值。
- 补齐表格预设与 Markdown 标题路径元数据：`RecursiveChunker` 支持 `table()` 预设，Markdown 预设把标题层级写入 `heading_path` / `heading` 块元数据；插件 splitter 仍然优先。
- 同步依据扩展：新增 `SyncBasis`、`HttpValidator` 与 `plan_http_sync`。内容哈希继续走现有 `plan_sync`；HTTP 验证元数据在 ETag 或 Last-Modified 命中时判为 Skip，未提供可比验证子时判为 Update，避免误跳过。
- 引用能力补齐：`Citation` 增加 locator；新增可配置 excerpt、score 显示和固定无命中模板的渲染函数；新增答案引用一致性检查，识别正文无引用与引用编号越界。
- 离线评估补齐：`EvalCase` 支持期望 locator；`score_case` 同时考核 source 与 locator；新增 `run_eval_cases` 统一执行样例集并汇总。

## 阶段二：工具契约（`wf-tools`）

- 新增 `rag_ingest_url`：网络风险工具，参数为 URL、集合/命名空间、抓取裁剪参数、同步依据与去重策略；只在宿主注入 `RagIngestBackend` 时注册。
- 新增 `rag_index_status`：只读诊断工具，返回来源、块数、哈希/验证元数据与最近一次摄取统计，不污染 `rag_search` 输出。
- 新增 `rag_evaluate`：只读评估工具，输入样例集与召回参数，返回逐条结果与汇总。
- 三个工具的后端 trait 都放在 `wf-tools`，handler 只做参数校验和结果序列化；URL 工具把宿主解析出的域名写入工具元数据供审批使用。

## 阶段三：宿主桥接（`wf-api` + `wf-tools`）

- 把 `web_fetch` 的 HTML 清洗/渲染逻辑抽成公开渲染函数，`web_fetch` handler 与 URL 摄取桥接共用，避免 RAG 侧重复 DOM 解析。
- 在 `wf-api` 增加 `RagCapabilityService`：持有 `RagHandles`、`WebFetchBackend` 与能力配置，实现阶段二的三个后端 trait。
- URL 摄取流程：抓取 → 复用页面渲染 → 生成带 source/标题/final_url/ETag/Last-Modified/抓取时间的文档 → 按哈希或 HTTP 验证元数据做同步决策 → 调用 `rag_core::pipeline::ingest_documents`。
- 同步状态使用宿主可注入、可复用的内存状态表 `RagCapabilityState`；来源删除按精确 source 执行，Stale 清单只进入状态诊断而不触发自动删除。

## 阶段四：配置、注册与验证

- `wf-types` 增加 RAG 能力配置段，表达 ingest/chunking/sync/retrieval/citation/eval 开关与上限；默认关闭。
- `wf-runtime` 暴露配置字段并通过 `Runtime::attach_rag_capabilities` 提供唯一装配入口：校验配置、构造 `RagCapabilityService`、复用 bootstrap 的 `web_fetch` 后端、注册工具与工具描述。默认不调用该入口时不改变启动行为。
- 增强工具定义与描述默认不包含在 predefined 目录中；只有宿主完成装配后才注册，避免模型看到不可用工具。
- 为每阶段补充单元测试，统一执行一次编译检查与测试验证。

## 明确不做

- 不新增 URL 入库以外的自动爬取或定时同步。
- 不新增索引状态写入工具；诊断保持只读。
- 不在工具结果中返回向量或模型提示词模板。
- 不把 judge、rerank、Embedding、向量后端的具体实现放进 `rag-core`。

## 实施结果

- 阶段一已完成：`rag-core` 新增分块预设配置、表格预设、标题路径元数据、HTTP 验证同步决策、引用样式与一致性检查、locator 评估与批量评估执行；同时修正了新建 tantivy 文件索引被误判为格式不匹配的问题。
- 阶段二已完成：`rag_ingest_url`、`rag_index_status`、`rag_evaluate` 三个工具契约与 handler 落地，分别按 Network/ReadOnly 风险声明；未注入后端时不注册 handler。
- 阶段三已完成：`web_fetch` 渲染逻辑抽出为共享函数；`wf-api` 的 `RagCapabilityService` 实现抓取、清洗、同步决策、摄取、状态、评估和共享检索后端。`RagCapabilityState` 由宿主构造并注入。
- 阶段四已完成：`wf-types` 增加默认关闭的 RAG 能力配置段；`wf-runtime::RuntimeConfig` 暴露配置，`Runtime::attach_rag_capabilities` 完成校验与装配；`RagCapabilitySettings::from_config` 映射预设、同步、检索、引用、摄取上限和评估上限。
- 验证：受影响 crate 的全 target、全特性 `cargo check` 通过；`rag-core` 126 个单测、`wf-tools` 435 个库单测及 3 个 golden 测试、`wf-api` 568 个库单测及相关集成测试通过。
