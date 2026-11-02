# 对比与借鉴点：两框架对照及落点映射

> 本文把两框架的同类设计并排对照，并映射到当前项目的改动点，避免实现时重复造轮子。

## 总体哲学对照

| 维度 | llama_index | Haystack | 本方案取舍 |
|---|---|---|---|
| 组织方式 | 索引、检索器、查询引擎分层 | 组件加管线，流程显式连接 | 以 `wf-workflow` 图承担管线角色，不新增运行时 |
| 文档观 | 文档、节点、带分结果三层 | 单一文档加分数与向量槽位 | 三层，写入侧用文档，检索侧用切片，召回用带分结果 |
| 切分观 | 节点解析器纯变换 | 预处理器正交罗列 | 接口纯变换，实现先递归后扩展 |
| 存储观 | 向量存储协议加查询规约 | 文档存储协议加过滤策略 | 协议加查询规约，过滤为一等参数 |
| 检索观 | 融合、合并、路由检索器 | 多检索器加文档合并器 | 四种检索形态加默认倒数排名融合 |
| 重排观 | 后处理器族 | 重排器族 | 复用现有重排内核，只做映射 |
| 合成观 | 响应合成器 | 提示词构建器加答案构建器 | 模板走资源层，调用走网关，内核只产变量 |

## 关键文件对照表

| 能力 | llama_index 位置 | Haystack 位置 | 当前项目落点 |
|---|---|---|---|
| 文档模型 | `ref/llama_index/llama-index-core/llama_index/core/schema.py`（`Document`、`TextNode`、`NodeWithScore`） | `ref/haystack/haystack/dataclasses/document.py`（`Document`） | 新建 `wf-rag` 文档模块，字段口径向两者看齐，关系字段向 llama 看齐 |
| 切分抽象 | `ref/llama_index/llama-index-core/llama_index/core/node_parser/interface.py`（`NodeParser`） | `ref/haystack/haystack/components/preprocessors/document_splitter.py`、`recursive_splitter.py` | 新建切分接口与递归实现，参数口径向 Haystack 看齐 |
| 存储协议 | `ref/llama_index/llama-index-core/llama_index/core/vector_stores/types.py`（`VectorStore`、`VectorStoreQuery`） | `ref/haystack/haystack/document_stores/types/protocol.py`（`DocumentStore`） | 新建存储协议，查询规约向 llama 看齐，去重向 Haystack 看齐 |
| 内存实现 | `ref/llama_index/llama-index-core/llama_index/core/vector_stores/simple.py` | `ref/haystack/haystack/document_stores/in_memory/document_store.py` | 新建内存实现，检索语义向两者看齐 |
| 融合检索 | `ref/llama_index/llama-index-core/llama_index/core/retrievers/fusion_retriever.py` | `ref/haystack/haystack/components/joiners/document_joiner.py` | 新建混合检索，融合模式向 Haystack 看齐 |
| 上下文增强 | `ref/llama_index/.../retrievers/auto_merging_retriever.py` | `ref/haystack/haystack/components/retrievers/sentence_window_retriever.py`、`auto_merging_retriever.py` | 新建关系回填，依赖切分保留的关系字段 |
| 查询改写 | `ref/llama_index/.../retrievers/router_retriever.py`、`transform_retriever.py` | `ref/haystack/haystack/components/retrievers/multi_query_text_retriever.py` | 复用 `LlmGateway` 实现改写，不新增调用路径 |
| 重排 | `ref/llama_index/.../core/postprocessor/` | `ref/haystack/haystack/components/rankers/` | 复用 `llm-suite-rerank`（候选类型在 `crates/infra/llm-suite/crates/llm-suite-rerank/src/provider.rs`），只做映射 |
| 向量化 | `ref/llama_index/.../core/embeddings/` | `ref/haystack/haystack/components/embedders/` | 复用 `llm-suite-embedding`（提供方在 `crates/infra/llm-suite/crates/llm-suite-embedding/src/provider.rs`，预处理在 `preprocessor.rs`） |
| 合成 | `ref/llama_index/.../core/response_synthesizers/`、`query_engine/` | `ref/haystack/haystack/components/builders/` | 渲染走 `wf-resource`（`crates/engine/wf-resource/src/template.rs`），调用走网关 |
| 编排 | `ref/llama_index/.../core/workflow/`、`query_engine/` | `ref/haystack/haystack/core/pipeline/` | 复用 `wf-workflow` 图，不新增运行时 |
| 工具化 | `ref/llama_index/.../core/tools/` | `ref/haystack/haystack/components/agents/`、`tools/` | 复用 `wf-tools` 暴露机制（`crates/engine/wf-tools/src/tool_exposure.rs`），新增只读处理器 |

## 现有可复用位置一览

- 向量化：`crates/infra/llm-suite/crates/llm-suite-embedding/src/provider.rs`（结果与提供方抽象）、`preprocessor.rs`（问句预处理）、`openai_compatible.rs`（兼容实现）、`service.rs`（批量调度）。
- 重排：`crates/infra/llm-suite/crates/llm-suite-rerank/src/provider.rs`（候选、结果、运行时参数）、`fusion.rs`（融合策略）、`cohere.rs`（兼容服务）、`generative.rs`（生成式）、`mock.rs`（测试替身）。
- 工具：`crates/engine/wf-tools/src/registry.rs`（`ToolRegistry`）、`tool_exposure.rs`（暴露决议）、`scope_gate.rs`（作用域门控）。
- 代理：`crates/engine/wf-agent/src/tool_router.rs`（`ToolRouter` 单轮装配）。
- 资源：`crates/engine/wf-resource/src/template.rs`（渲染）、`resource_assembler.rs`（装配）。
- 存储参考：`crates/infra/wf-storage/src/domain/store.rs`（过滤、`Store`、`StoreExt`），注意该层语义为实体存取，向量查询不直接塞入，由新内核持有。

## 不借鉴的部分

- llama_index 的多索引类型（树、图、结构化）首版不引入，只取向量索引子集。
- Haystack 的全量转换器与抓取器首版不引入，只取纯文本、Markdown、JSON。
- 两框架的评估框架首版不引入，只记录可复跑评测集与命中情况。
- 两框架的分布式与云端存储实现首版不引入，只做内存与 SQLite。
