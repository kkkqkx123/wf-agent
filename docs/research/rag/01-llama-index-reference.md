# llama_index 参考设计与文件位置

> 快照：`ref/llama_index/llama-index-core/llama_index/core`。行号为定位辅助，重点在设计取舍而非逐行实现。

## 数据模型：文档、节点、带分结果

- 基座在 `schema.py`：`BaseComponent`（`81` 行，统一类名注入）、`TransformComponent`（`191` 行，管线变换抽象）、`NodeRelationship`（`207` 行，来源与前后关系枚举）、`BaseNode`（`264` 行）、`TextNode`（`765` 行）、`IndexNode`（`948` 行，父块索引）、`NodeWithScore`（`1035` 行，召回结果）、`Document`（`1097` 行，写入侧载体）、`QueryBundle`（`1449` 行，查询侧载体）。
- 取舍：写入与检索共用节点族，父子关系显式建模，带分结果独立成类型。该分层可直接借鉴为本文档模型划分的依据。

## 切分：节点解析器抽象

- 抽象在 `node_parser/interface.py`：`NodeParser`（`50` 行，元数据开关、前后关系开关、标识函数可配）、`TextSplitter`（`210` 行）、`MetadataAwareTextSplitter`（`233` 行）。
- 实现在 `node_parser/` 子目录：`text/` 按长度切分，`file/` 按格式切分，`relational/` 处理结构化与层级。
- 取舍：切分是文档到节点的纯变换，不耦合向量化与存储，标识函数可注入保证幂等。该解耦方式值得照搬。

## 存储：向量存储协议与查询规约

- 协议在 `vector_stores/types.py`：`VectorStoreQueryResult`（`37` 行）、`VectorStoreQueryMode`（`45` 行，稠密、稀疏、混合）、`FilterOperator`（`63` 行）、`FilterCondition`（`85` 行）、`MetadataFilter`（`94` 行）、`MetadataFilters`（`142` 行）、`VectorStoreQuery`（`240` 行）、`VectorStore`（`269` 行，协议）。
- 简单实现在 `vector_stores/simple.py`，向量索引入口在 `indices/vector_store/`。
- 取舍：查询规约独立于存储实现，过滤是查询的一等参数。该形状对应本方案存储协议的设计。

## 检索：从基础到融合与路由

- 基座在 `indices/base_retriever.py` 与 `base.py`。
- 特色检索在 `retrievers/`：融合检索（`fusion_retriever.py`）、自动合并（`auto_merging_retriever.py`）、递归（`recursive_retriever.py`）、路由（`router_retriever.py`）、变换（`transform_retriever.py`）。
- 取舍：小块召回大块合成依赖父子节点关系；融合检索把多路结果合并为一路。该思想对应本方案关系回填与混合融合的设计。

## 后处理：重排器族

- 在 `postprocessor/`：基础抽象 `types.py`（`BaseNodePostprocessor`），实现含大模型重排（`llm_rerank.py`）、语句向量重排（`sbert_rerank.py`）、排序模型重排（`rankGPT_rerank.py`）、结构化重排（`structured_llm_rerank.py`）、元数据替换（`metadata_replacement.py`）、时效性（`node_recency.py`）、长上下文优化（`optimizer.py`）。
- 取舍：重排与召回解耦，候选映射收敛一处。该分工对应本方案复用现有重排内核的决策。

## 摄入与合成：管线与查询引擎

- 摄入管线在 `ingestion/pipeline.py`，变换在 `ingestion/transformations.py`，缓存与去重在 `ingestion/cache.py`。
- 查询引擎在 `query_engine/`（`retriever_query_engine.py` 为主入口，`citation_query_engine.py` 处理引用，`router_query_engine.py` 处理路由），响应合成在 `response_synthesizers/`（`refine.py`、`tree_summarize.py`、`compact_and_refine.py` 等），对话形态在 `chat_engine/`。
- 取舍：写路径强调幂等与缓存，读路径强调检索加合成两段式，引用是合成的一等输出。该划分对应本方案摄入与查询双流程的设计。

## 对当前项目的直接借鉴

- 文档模型分三层：文档、切片、带分结果。
- 切分接口纯变换，不碰向量化与存储。
- 存储查询规约独立成类型，过滤为一等参数。
- 融合与关系回填以前后、父子关系为前提，需在切分时保留关系。
