# Haystack 参考设计与文件位置

> 快照：`ref/haystack/haystack`。Haystack 以组件加管线为组织方式，每个组件职责单一，索引与查询流程由管线显式连接。

## 文档与存储协议

- 文档在 `dataclasses/document.py`：`Document` 含标识、文本、可选二进制、元数据、分数、稠密向量、稀疏向量，标识缺省时按内容派生。
- 存储协议在 `document_stores/types/protocol.py:11`：`DocumentStore` 要求实现写入、过滤、计数、序列化。去重策略在 `document_stores/types/policy.py:8`：`DuplicatePolicy` 含默认、跳过、覆盖、报错四种语义。过滤合并策略在 `document_stores/types/filter_policy.py`（初始化过滤与运行时过滤的合并规则）。
- 内存实现在 `document_stores/in_memory/document_store.py:83`：`InMemoryDocumentStore`，写入在 `496` 行（`write_documents`），关键词检索在 `772` 行（`bm25_retrieval`），向量检索在 `841` 行（`embedding_retrieval`）。
- 取舍：协议只约操作，过滤语义独立成章，去重策略显式化。该形状可直接映射为本方案存储协议的设计。

## 切分：预处理器族

- 入口在 `components/preprocessors/`：
  - 通用切分 `document_splitter.py:28`（`DocumentSplitter`，按词、行、段落等，参数为切分单位、长度、重叠）。
  - 递归切分 `recursive_splitter.py:20`（`RecursiveDocumentSplitter`，分隔符列表逐级降级）。
  - 层级切分 `hierarchical_document_splitter.py:14`（`HierarchicalDocumentSplitter`）。
  - 按标题切分 `markdown_header_splitter.py:17`（`MarkdownHeaderSplitter`）。
  - 按句切分 `sentence_tokenizer.py:121`（`SentenceSplitter`）。
  - 向量化感知切分 `embedding_based_document_splitter.py:25`。
  - 表格与代码专用 `csv_document_splitter.py:21`（`CSVDocumentSplitter`）、`python_code_splitter.py:34`（`PythonCodeSplitter`）。
  - 清洗 `document_cleaner.py:19`（`DocumentCleaner`）、`text_cleaner.py`。
- 取舍：切分器正交罗列而非层层继承，参数口径统一。该罗列方式适合作为本方案切分模块的演进路线。

## 向量化与写入解耦

- 向量化在 `components/embedders/`：文本与文档向量化分离（如 `openai_text_embedder.py` 与 `openai_document_embedder.py`），测试替身在 `mock_text_embedder.py`、`mock_document_embedder.py`。
- 写入在 `components/writers/document_writer.py:13`：`DocumentWriter`，构造参数为存储与去重策略（`31` 行），执行入口在 `81` 行（`run`），支持运行时覆盖策略。
- 取舍：向量化只产向量，写入只管持久化与去重。该解耦对应本方案摄入流程分步的设计。

## 检索：组件族与合并器

- 检索器在 `components/retrievers/`：
  - 文本向量检索 `text_embedding_retriever.py:16`（`TextEmbeddingRetriever`，先向量化问句再委派）。
  - 过滤检索 `filter_retriever.py:13`（`FilterRetriever`）。
  - 多查询 `multi_query_text_retriever.py:18`、`multi_query_embedding_retriever.py:19`。
  - 多路 `multi_retriever.py:21`（`MultiRetriever`）。
  - 句子窗口 `sentence_window_retriever.py:15`（`SentenceWindowRetriever`）。
  - 自动合并 `auto_merging_retriever.py:14`（`AutoMergingRetriever`）。
- 合并在 `components/joiners/document_joiner.py:46`（`DocumentJoiner`），合并模式在同文件 `JoinMode`：拼接、合并、倒数排名融合、分布融合。
- 路由在 `components/routers/`：条件路由、文档长度路由、文档类型路由、文件类型路由、元数据路由。
- 取舍：单路检索器只做一路，多路合并收敛到合并器，路由独立成组件。该分工对应本方案检索模块四种实现加融合的划分。

## 重排与构建器

- 重排在 `components/rankers/`：大模型重排 `llm_ranker.py:85`（`LLMRanker`）、中间丢失 `lost_in_the_middle.py:11`、元字段分组 `meta_field_grouping_ranker.py:14`、元字段 `meta_field.py:19`。
- 构建在 `components/builders/`：提示词构建 `prompt_builder.py`、对话提示词构建 `chat_prompt_builder.py`、答案构建 `answer_builder.py`。
- 取舍：重排只动顺序与裁剪，构建只管模板渲染与答案组装。该边界对应本方案重排复用与模板渲染收敛的决策。

## 管线：显式连接

- 管线在 `core/pipeline/`，组件基座在 `core/component/`。索引管线典型连接为转换、清洗、切分、向量化、写入；查询管线典型连接为向量化、检索、合并、重排、构建。
- 取舍：流程可声明、可序列化、可复跑。本方案以 `wf-workflow` 图承担该角色，不另起运行时。

## 对当前项目的直接借鉴

- 文档模型字段口径照搬，稀疏槽位预留。
- 切分参数口径照搬：切分单位、长度、重叠。
- 写入策略照搬 `DuplicatePolicy` 四态。
- 合并模式照搬拼接、合并、倒数排名融合三态起步。
- 问句向量化与向量检索解耦，检索器可组合。
