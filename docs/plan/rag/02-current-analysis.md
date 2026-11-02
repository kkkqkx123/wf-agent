# 现状分析：各层已有能力与差距

> 本文件盘点与 RAG 相关的现有模块，结论是内核分散但可复用度高，缺口集中在文档、切分、存储检索三处。
>
> 更新：CCE 已转向 Apache-2.0，跨项目代码参考的许可障碍解除；`simvec` 在本项目为正式 submodule（`crates/infra/simvec`，`path = crates/infra/simvec`），与 `llm-suite` 同一复用方式；向量契约包 `crates/infra/vector-contracts` 已落地（类型、过滤、存储契约、内存实现、按标识取回、simvec 适配、Qdrant REST 客户端）；RAG 包 `crates/infra/rag-core` 已落地（文档、切分、管线、融合、重排适配、引用、转换端口）。

## 向量化与重排：基本就绪

向量化能力位于独立套件。提供方抽象与批量结果定义在 `crates/infra/llm-suite/crates/llm-suite-embedding/src/provider.rs:10` 的 `EmbeddingResult` 与同文件 `54` 行的 `EmbeddingProvider`，兼容 OpenAI 接口的实现在 `openai_compatible.rs`，问句预处理在 `preprocessor.rs:11` 的 `PreprocessorConfig` 与 `23` 行的 `PreprocessorImpl`（空处理、前缀、模板三种形态）。服务层做批量调度（`service.rs`）。该能力可直接作为摄入向量化与查询向量化的统一入口，无需另起向量化通道。缺口仅在于缺少按分词预算批量切包的辅助（上游有意将该逻辑排除在叶子层之外，应由检索内核按需实现）。

重排能力同样位于独立套件。候选与结果类型在 `crates/infra/llm-suite/crates/llm-suite-rerank/src/provider.rs:18`（`RerankCandidate`）、`37` 行（`RerankedCandidate`）、`55` 行（`RerankResult`），运行时参数在 `84` 行（`RerankRuntimeConfig`，含候选上限、温度、是否保留推理过程、融合策略、超时），融合策略在 `fusion.rs:13`。兼容服务的请求体组装与响应解析在 `cohere.rs`，生成式重排在 `generative.rs`，测试替身在 `mock.rs`。RAG 侧只需实现切片与候选之间的映射，不改重排内核。

## 工具机制：适合承载只读检索

工具注册表在 `crates/engine/wf-tools/src/registry.rs:27`（`ToolRegistry`），暴露决议在 `crates/engine/wf-tools/src/tool_exposure.rs:91`（`resolve_tool_exposure`），输入与决议类型在同文件 `33` 行（`ExposureInput`）与 `60` 行（`ExposureResolution`）。代理侧的单轮装配收敛在 `crates/engine/wf-agent/src/tool_router.rs:29`（`ToolRouter`）与 `57` 行（`build_tool_router`），可见集、发现集、门控集、隐藏集四态划分已稳定。

这意味着检索工具不需要新的执行器类型：注册一个只读处理器，参与现有白名单与门控计算即可。敏感知识库可降为发现式，仅注入元数据，待正式激活后才可见完整调用形态。现有门控语义保证运行时拦截不改变请求前缀形态，对缓存友好。

## 图执行：可复用为管线运行时

`wf-workflow` 已具备图遍历、条件路由、变量存储、消息上下文、检查点与执行回调，依赖面覆盖工具、存储、资源与代理（`crates/engine/wf-workflow/Cargo.toml`）。摄入与查询流程均可表达为节点类型加边连接，无需另起 Haystack 式管线运行时。复用的关键是把 RAG 节点的副作用收敛到检索内核调用，执行语义仍由现有协调器负责。

## 提示词装配：只需新增片段模板

资源装配与模板渲染在 `crates/engine/wf-resource/src/resource_assembler.rs` 与 `crates/engine/wf-resource/src/template.rs:77`（`render_template` 系列）。检索上下文注入本质上是一种片段模板加渲染选项，不新增装配路径。引用渲染同样归为模板职责，由调用方决定展示形态还是模型可见形态。

## 存储：有实体存储，无向量检索

`wf-storage` 提供实体存储、内存与关系型后端、过滤与跨表操作（`crates/infra/wf-storage/src/domain/store.rs:19` 的 `FilterOp`、`45` 行的 `QueryFilter`、`232` 行的 `FilterCondition`、`345` 行的 `Store`、`386` 行的 `StoreExt`；`store/entity_store.rs:16` 的 `EntityStore`）。该层解决的是结构化实体存取，不解决向量相似度查询、混合检索与切片关系维护。直接在该层塞向量逻辑会污染现有实体语义，因此向量索引应由新内核持有，持久化细节再委托给现有后端形态。

## 差距汇总

| 维度 | 现状 | 差距 |
|---|---|---|
| 文档模型 | 无统一文档与切片类型 | 需新建，含标识、内容、元数据、分数、向量槽位、来源与前后关系 |
| 切分 | 无 | 需新建递归切分器，其余切分器复用其管线 |
| 向量存储 | 仅结构化存储 | 需新建协议与内存实现，SQLite 随后 |
| 检索器 | 无 | 需新建稠密、过滤、混合、多查询四种形态 |
| 写入幂等 | 无去重策略 | 需引入等价于 `DuplicatePolicy` 的语义 |
| 合成引用 | 无引用约定 | 需新增模板与答案组装约定 |
| 评估排障 | 仅通用度量 | 需补充检索专用的命中预览与延迟信号 |

总体判断：新建代码集中在一个内核 crate 内，对现有 crate 的改动均为加法（新增工具处理器、新增节点类型、新增模板），不触动现有执行主干。
