# RAG 落地实现记录

> 本文件记录 `05-remaining-tasks.md` 中剩余任务的代码落点与验收证据。事实声明附文件引用，行号为定位辅助。

## 内核补齐（`rag-core`）

- 清洗环节：新增 `crates/infra/rag-core/src/cleaner.rs`（`CleanerConfig` 四类开关与 `clean_text`），摄入流程在切分前应用清洗（`pipeline.rs` 的 `ingest_documents`），配置经 `RagIndexConfig::cleaning` 序列化（`config.rs`）。
- 切分具名预设：`chunker.rs` 新增 `ChunkerConfig::markdown`（标题优先分隔）与 `ChunkerConfig::sentence`（句边界优先分隔），复用同一递归管线。
- 融合策略：`config.rs` 新增 `FusionMode`（倒数排名、拼接、加权）与 `FusionConfig`，`RagQueryConfig::fusion` 承载策略；`retriever.rs` 新增 `fuse_lists` 分发、`concat_fusion`、`weighted_fusion`；多变体查询改走配置策略（`pipeline.rs` 的 `search_variants`）。
- 关键词召回：`retriever.rs` 新增 `tokenize`、`keyword_score`（词符重叠度）、`keyword_search_points`（调用方提供候选点集，因存储契约无扫描调用）、`blend_keyword`；`pipeline.rs` 新增 `query_hybrid`（稠密加关键词按 `keyword_weight` 融合），`RagQueryConfig::keyword_candidates` 与 `keyword_weight` 控制开关。
- 查询改写：新增 `rewrite.rs`（`QueryRewriter` 端口、`NoopRewriter`、`rewrite_with_fallback` 失败回退原问句、`merge_variants` 去重合并）。内核仍只收变体，改写实现由工作流或代理侧注入。
- 合成补齐：`citation.rs` 新增 `NoHitContext` 与 `render_no_hits`（无命中模板变量组装）、`CitationCompleteness` 与 `check_completeness`（空列表或空白来源判为不完整，不伪造来源）。
- 增量更新：新增 `sync.rs`（`content_hash` 内容哈希、`plan_sync` 插入更新跳过决策、`pending_sources`、`remove_source`、`remove_by_ids`、`source_point_count`）；`pipeline.rs` 新增 `delete_source` 与 `delete_by_ids` 薄封装。
- 度量：新增 `metrics.rs`（`RetrievalMetrics`：延迟、请求候选数、返回候选数、保留命中数、重排位移、是否重排；`displacement_of` 与 `rank_pairs`）；`pipeline.rs` 新增 `query_with_metrics` 与 `query_with_rerank_metrics`。
- 评测集：新增 `eval.rs`（`EvalCase`、`score_case` 命中与召回、`summarize` 命中率与平均召回）；可复跑集合位于 `crates/infra/rag-core/tests/eval_set.json`，回归测试位于 `tests/eval_rerun.rs`（写入两文档、跑全部用例，要求命中率与平均召回均为一）。
- 兼容性调整：管线函数泛型界放宽为 `?Sized` 以支持 `Arc<dyn VectorStorage>` 与 `Arc<dyn EmbeddingProvider>` 注入；`RetrievedChunk`、`RankedChunk`、`Citation` 补 `Serialize`/`Deserialize` 以便进入工作流变量与工具输出。

## 集成（上层调用方）

- 检索工具（`wf-tools`）：新增 `crates/engine/wf-tools/src/predefined/rag.rs`。最小只读工具 `rag_search`（问句必填，集合、命名空间、返回数、分数下限、元数据过滤可选），`RagSearchBackend` 端口解耦后端，`RagService` 泛型适配器 over 任意向量存储与向量化提供方，`RagToolConfig` 承载后端与默认集合命名空间，未配置后端时显式报错而非静默降级。已接入定义聚合（`predefined.rs`）与默认处理器注册（`handlers.rs` 的 `BuiltinHandlersConfig::rag`）。验收：处理器返回后端命中、空问句拒绝、暴露决议保持直接可见。
- 代理接线（`wf-agent`）：无需改执行主干。`tool_router.rs` 新增用例确认 `rag_search` 经共享暴露决议参与单轮装配；工具路由本身委派 `wf_tools::resolve_tool_exposure`，形态不会漂移。
- 提示词片段（`wf-resource`）：新增 `crates/engine/wf-resource/src/predefined/rag.rs`。四模板（模型上下文、展示上下文、引用行、无命中），变量表由命中列表统一组装（`build_context_variables`），引用行缺模板时回退内核排版，截断预算由调用方传入。已接入 `register_all`。模板分类新增 `rag`（`wf-types/src/template.rs` 的 `TEMPLATE_CATEGORIES`，仅模板校验引用该表，无其他分支依赖）。
- 工作流节点（`wf-workflow`）：新增 `crates/engine/wf-workflow/src/handler/rag.rs`。`Custom("RAG_QUERY")` 跑读路径（问题来自内联或变量，命中数组与摘要写入变量，检查点只记命中标识与来源）；`Custom("RAG_INGEST")` 跑写路径（文档数组来自变量，统计写入变量，去重策略按名解析）。复用 `HandlerRegistry`，无需改动封闭节点枚举。`register_rag_handlers` 一次注册两处理器。
- 管理排障接口（`wf-api`）：新增 `crates/app/wf-api/src/rag.rs`。只读三件套：`collection_stats` 与 `source_stats`（计数）、`preview_search`（摘要与分数，不外传向量）、`score_histogram`（分数分布）；`diagnose_no_hits` 按查询形态提示 likely 原因（空问句、零返回数、过严过滤、过高阈值、切分过粗、维度错配），调用方叠加命名空间计数后再展示。

## 规模（持久化、验证、度量）

- simvec 持久化：`simvec_adapter.rs` 新增重开验证（写入后释放再打开，索引仍可用）。`--features simvec` 全绿。
- Qdrant 远端：已有纯函数单测（过滤形状、入参前置校验）保持；新增 `QDRANT_URL` 门控的契约往返测试（建集合、写入、查询、按标识取回、计数、按来源删除），默认运行跳过，需远端服务时置变量即跑。
- 度量排障：`RetrievalMetrics` 三组信号（延迟、候选规模、重排位移）随查询返回，调用方写入既有收集器；`diagnose_no_hits` 覆盖常见无命中原因。

## 验收对照

| 05 任务 | 状态 | 证据 |
|---|---|---|
| 检索工具 | 完成 | `wf-tools/src/predefined/rag.rs` 单测（命中、空问句、暴露） |
| 提示词模板 | 完成 | `wf-resource/src/predefined/rag.rs` 单测（四模板注册、双模式渲染、调用方截断） |
| 工作流节点 | 完成 | `wf-workflow/src/handler/rag.rs` 单测（类型注册、处理器注册） |
| 代理接线 | 完成（无主干改动） | `wf-agent/src/tool_router.rs` 新增用例 |
| 管理排障接口 | 完成 | `wf-api/src/rag.rs` 单测（直方图、诊断） |
| 清洗 | 完成 | `cleaner.rs` 单测、`pipeline.rs` 清洗集成用例 |
| 切分预设 | 完成 | `chunker.rs` 新增用例 |
| 关键词混合 | 完成（调用方提供语料） | `retriever.rs` 新增用例、`pipeline.rs` 混合用例 |
| 查询改写 | 完成（端口加回退） | `rewrite.rs` 单测 |
| 合成模板 | 完成 | `citation.rs` 新增用例 |
| 增量更新 | 完成 | `sync.rs` 单测 |
| 持久化验证 | 完成 | simvec 重开用例、Qdrant 门控往返用例 |
| 度量排障 | 完成 | `metrics.rs` 单测、`query_with_metrics` 用例 |
| 评测集 | 完成 | `tests/eval_rerun.rs`（命中率一、平均召回一） |

## 运行方式

- 内核回归：`cargo test -p rag-core`（含可复跑评测集）。
- 契约回归：`cargo test -p vector-contracts --features simvec,qdrant`；远端往返需置 `QDRANT_URL`。
- 集成回归：`cargo test -p wf-tools predefined::rag`、`cargo test -p wf-resource predefined::rag`、`cargo test -p wf-api rag`、`cargo test -p wf-agent tool_router`、`cargo test -p wf-workflow handler::rag`。
- 编译门：`cargo check -p rag-core -p wf-tools -p wf-resource -p wf-workflow -p wf-api -p wf-agent`；格式化：`cargo fmt`（仅提交本任务文件）。

## 已知缺口与后续

- 全量关键词索引仍为轻量词频实现，大规模场景需原生倒排或外部检索引擎，未引入。
- SQLite 文档切片元数据形态与 Postgres 接口仍为预留，未在本步实现。
- 丰富检索参数形态仍走 MCP 与插件扩展，内置工具保持最小只读定义。
- Qdrant 远端往返测试需真实服务，默认跳过；压测若证 REST 为瓶颈，再以特性门引入 gRPC 后端，契约不变。
