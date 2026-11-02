# 需要支持的功能清单

> 口径：取 `ref/llama_index` 与 `ref/haystack` 的交集作为必备项，两者各自的特色作为增强项。描述以自然语言为主，不贴完整实现。

## 功能总览

一条完整闭环包含七段：加载、清洗、切分、向量化、存储、检索、合成，外加贯穿始终的编排、评估与可观测。每段在两框架中都有明确对应物，整理如下。

## 文档模型

两框架对文档的定义高度一致，是整个设计的基石：

- Haystack 用 `Document` 承载文本、可选二进制、元数据、分数、稠密向量与稀疏向量（`ref/haystack/haystack/dataclasses/document.py`）。标识在未显式指定时按内容生成，支持与字典及 JSON 互转。
- llama_index 用 `Document / TextNode / NodeWithScore` 分层表达原始文档、切片与带分结果（`ref/llama_index/llama-index-core/llama_index/core/schema.py` 内 `Document`、`TextNode`、`NodeWithScore` 类）。节点间通过 `NodeRelationship` 记录来源与前后相邻关系。

本项目需要的新模型应至少包含：稳定标识、文本内容、元数据映射、分数、稠密向量槽位、来源与前后关系。稀疏向量槽位可先预留字段而不实现检索。

元数据过滤是检索的前提而非附属。Haystack 在协议层定义了比较与逻辑两类过滤（`ref/haystack/haystack/document_stores/types/protocol.py` 的 `filter_documents` 约定，比较算子含等于、不等、大小比较与集合包含，逻辑算子含与或非），llama_index 在向量存储层定义了等价的 `MetadataFilter / MetadataFilters` 与 `FilterOperator / FilterCondition`（`ref/llama_index/llama-index-core/llama_index/core/vector_stores/types.py`）。本项目已有结构化过滤雏形（`crates/infra/wf-storage/src/domain/store.rs:19` 的 `FilterOp`、`QueryFilter` 与 `FilterCondition`），RAG 侧应复用同一语义并扩展到切片维度。

## 摄入侧

摄入指从原始来源到可检索状态的写流程，两框架都拆成四步：

- 加载转换：llama_index 的 `readers/` 与 Haystack 的 `converters/` 负责把多格式输入归一为文档。首版只需覆盖纯文本、Markdown、JSON，其余格式后续按需扩展。
- 清洗：Haystack 的 `DocumentCleaner` 与 `TextCleaner`（`ref/haystack/haystack/components/preprocessors/document_cleaner.py`、`text_cleaner.py`）负责空白、重复、特殊字符处理。llama_index 把同类逻辑放在节点解析管线的预处理环节。清洗规则应可开关、可配置。
- 切分：这是摄入的核心。Haystack 提供 `DocumentSplitter`（按词、按行、按段落等，`ref/haystack/haystack/components/preprocessors/document_splitter.py`）、`RecursiveDocumentSplitter`（按分隔符列表递归降级，`recursive_splitter.py`）、`HierarchicalDocumentSplitter`、`MarkdownHeaderSplitter`、`SentenceSplitter`；llama_index 提供 `NodeParser` 抽象与 `TextSplitter / MetadataAwareTextSplitter`（`ref/llama_index/llama-index-core/llama_index/core/node_parser/interface.py`）。交集是递归切分加长度与重叠参数，附带是否携带元数据、前后关系是否保留两个开关。
- 向量化写入：Haystack 把向量化（`embedders/`）与写入（`writers/document_writer.py` 的 `DocumentWriter`）解耦，写入策略用 `DuplicatePolicy`（`ref/haystack/haystack/document_stores/types/policy.py`，含跳过、覆盖、报错语义）控制幂等；llama_index 把同类职责放在 `ingestion/` 管线。写入前必须向量化，批量调用，失败可重试。

增量更新属于摄入的延伸：按来源标识与内容哈希判断新增、变更与删除，孤儿切片随来源删除而清理。两框架均未把该逻辑固化为单一组件，但 `DocumentWriter` 的去重策略与存储层的按标识覆盖是其基础。

## 存储侧

存储指向量的持久化与查询能力，两框架都强调协议与实现分离：

- Haystack 定义 `DocumentStore` 协议，要求实现计数、过滤、写入、序列化（`ref/haystack/haystack/document_stores/types/protocol.py` 的 `DocumentStore`），内存实现同时提供关键词检索与向量检索（`ref/haystack/haystack/document_stores/in_memory/document_store.py` 的 `InMemoryDocumentStore`，含 `bm25_retrieval` 与 `embedding_retrieval`）。
- llama_index 定义 `VectorStore` 协议与查询规约（`ref/llama_index/llama-index-core/llama_index/core/vector_stores/types.py` 的 `VectorStore`、`VectorStoreQuery`、`VectorStoreQueryResult`、`VectorStoreQueryMode`），支持稠密、稀疏、混合查询模式。

本项目存储侧的必备查询能力为：按向量相似度取前若干结果、按元数据过滤限定候选、返回分数与来源。混合查询模式与稀疏索引为增强项。

## 检索侧

检索指查询时的读流程，两框架的组件族几乎一一对应：

- 基础检索：Haystack 的 `TextEmbeddingRetriever`（先向量化问句再委派给向量检索，`ref/haystack/haystack/components/retrievers/text_embedding_retriever.py`）与 llama_index 的 `BaseRetriever`（`ref/llama_index/llama-index-core/llama_index/core/indices/base_retriever.py`）是同一思想：问句向量化与候选召回解耦。
- 过滤检索：Haystack 的 `FilterRetriever` 与 llama_index 的元数据过滤查询对应纯条件召回场景。
- 多路召回：Haystack 的 `MultiRetriever` 与 `DocumentJoiner`（`ref/haystack/haystack/components/joiners/document_joiner.py`，支持拼接、合并、倒数排名融合、分布融合）对应 llama_index 的 `FusionRetriever`。融合策略至少支持倒数排名融合，其余策略后续补充。
- 上下文增强检索：Haystack 的 `SentenceWindowRetriever` 与 `AutoMergingRetriever` 对应 llama_index 同名检索器（`auto_merging_retriever.py`、`recursive_retriever.py`），思想都是小块召回、大块合成，父子切片关系是前提。
- 查询改写：Haystack 的 `MultiQueryTextRetriever` 与 `MultiQueryEmbeddingRetriever` 把一问改写为多问再合并，对应 llama_index 查询改写与路由检索器。改写调用复用现有大模型网关即可。

检索参数的交集为：返回数量、分数下限、元数据过滤、是否启用混合、融合权重。问句预处理（如任务前缀）已有雏形（`crates/infra/llm-suite/crates/llm-suite-embedding/src/preprocessor.rs:11` 的 `PreprocessorConfig`），检索侧应复用。

## 重排与后处理

重排指对召回候选的二次排序与裁剪：

- Haystack 提供大模型重排、中间丢失重排、元字段分组重排（`ref/haystack/haystack/components/rankers/` 下 `llm_ranker.py`、`lost_in_the_middle.py`、`meta_field_grouping_ranker.py`）。
- llama_index 提供大模型重排、语句向量重排、结构化重排、元数据替换、时效性等一系列后处理器（`ref/llama_index/llama-index-core/llama_index/core/postprocessor/`）。

本项目已有重排内核（`crates/infra/llm-suite/crates/llm-suite-rerank/src/provider.rs:18` 的 `RerankCandidate`、`RerankedCandidate`、`RerankResult`，`fusion.rs:13` 的融合策略，`RerankRuntimeConfig` 的候选上限与超时控制），RAG 侧只需做好候选映射（切片到候选的标识、内容截断、初始分保留）与结果回填（名次变化、融合分），无需另起重排服务。

## 合成侧

合成指把检索结果组装为最终答案：

- Haystack 用提示词构建器加答案构建器分工（`ref/haystack/haystack/components/builders/` 下 `prompt_builder.py`、`chat_prompt_builder.py`、`answer_builder.py`），前者负责模板渲染，后者负责答案与引用组装。
- llama_index 用响应合成器承担同类职责，强调引用与多候选综合。

本项目合成侧应复用现有两条路径：模板渲染走 `wf-resource`（`crates/engine/wf-resource/src/template.rs:77` 的 `render_template` 系列），大模型调用走 `LlmGateway`。新增的只是检索上下文片段模板与引用渲染约定，不新增调用路径。引用至少包含来源标识与切片定位，渲染形态由模板决定。

## 编排、评估与可观测

- 编排：Haystack 以 `Pipeline` 为一等概念（`ref/haystack/haystack/core/pipeline`），节点间以显式连接组装索引与查询流程；llama_index 以查询引擎与工作流引擎分层。两者共同点是流程可声明、可序列化、可复跑。本项目以 `wf-workflow` 图承担该角色即可。
- 代理调用：两框架都支持把检索器封装为代理工具，代理按需调用而非每轮必调。本项目对应现有四态工具暴露机制（`crates/engine/wf-tools/src/tool_exposure.rs:91` 的 `resolve_tool_exposure`）。
- 评估：llama_index 的 `evaluation/` 与 Haystack 的 `evaluation/` 均围绕召回命中与答案忠实度展开。首版只需记录可复跑的评测集与命中情况，不引入完整评估框架。
- 可观测：检索延迟、候选规模、重排前后名次、最终引用来源四组信号是排障最小集，复用现有度量收集器即可。
