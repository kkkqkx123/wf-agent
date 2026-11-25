# 关键词检索契约抽取

## 背景

RAG 混合检索中的关键词侧已经具备可替换后端：SQLite FTS5、Tantivy 与 Elasticsearch 都实现同一个 `KeywordIndex`，运行期通过 `Arc<dyn KeywordIndex>` 注入，查询与摄取管线只依赖抽象。Tantivy 已经作为可选本地倒排索引落地，并非待引入能力。

问题不在后端可替换性，而在契约边界：原 `KeywordIndex` 定义在 `rag-core` 内，`search` 返回 RAG 专属的 `RetrievedChunk`，写入直接接收带向量的 `VectorPoint`，关键词后端必须理解 `content`、`chunk_id` 等 RAG payload 约定。这与 `vector-contracts` 的中立边界不对等，也会让未来非 RAG 的全文检索被迫依赖整条 RAG 管线。

## 结论

保留后端实现在 `rag-core`，把操作契约与通用值类型上移为独立的 `keyword-contracts`。契约层只描述“按文本写入、按查询取回带分数的 payload 点”，不描述 RAG chunk、父子块或引用语义。RAG 侧负责在管线与后端之间做双向适配。

不建立覆盖向量检索与关键词检索的统一超trait。两类检索的排序语义不可互换，分数融合属于 RAG 管线的职责。

## 依赖方向

`vector-contracts` 保持叶子包。`keyword-contracts` 依赖 `vector-contracts`，复用 `SearchFilter` 与 `ScoredPoint`，确保混合检索两侧使用同一过滤语义和同一打分点类型。`rag-core` 同时依赖两者，上层应用依赖 `keyword-contracts` 持有检索句柄，依赖 `rag-core` 调用管线与打开后端。

`keyword-contracts` 依赖 `wf-types` 中已有的关键词配置枚举与格式策略。这样做避免为同一组序列化配置再建一套镜像类型；后端打开时仍由宿主配置解析格式戳，契约本身不访问插件注册表。

## 契约内容

契约包包含错误类型、文本文档类型、查询对象、格式戳与操作 trait。

文本文档显式携带 id、可索引文本和 payload，关键词后端不再从 payload 中猜测正文字段。查询对象携带文本、可选共享过滤条件和返回上限，后续增加阈值、片段或 explain 能力时不需要改 trait 方法签名。搜索结果直接复用 `ScoredPoint`，由 `rag-core` 的适配器转成 `RetrievedChunk`。

trait 保持对象安全，方法集合与原实现一致：写入、搜索、按 id 删除、按来源删除、计数。`backend_name` 继续用于日志诊断。

## 适配层职责

`rag-core` 内保留 `keyword_index` 模块，但职责改为适配：

- 从 `VectorPoint` 读取正文 payload，构造 `KeywordDocument`；无正文的点直接跳过。
- 把关键词搜索返回的 `ScoredPoint` 转成 `RetrievedChunk`；缺少 RAG 必填 payload 的命中沿用原有忽略行为。
- 摄取管线在写入向量后构造关键词文档再写关键词索引。
- 混合检索在调用关键词侧时构造 `KeywordQuery`，融合前把命中转成 RAG chunk。

分词器、插件 analyzer、format mismatch 策略和 rebuild 流程继续留在 `rag-core`，它们依赖 RAG 运行时语义，不属于存储契约。

## 后端策略

SQLite FTS5 继续作为默认本地后端，Tantivy 继续作为可选的本地高性能倒排索引，Elasticsearch 继续作为远程全文后端。三者只实现契约，不各自定义结果类型。

不把后端实现搬进契约包。当前 SQLite 与 Tantivy 的写入依赖 RAG payload 适配和插件 analyzer，Elasticsearch 依赖 RAG 配置结构；搬入契约包会把业务语义带到叶子层。后续出现与 RAG 无关的全文检索消费者时，再按后端拆分实现包，并在那里复用同一契约。

## 错误边界

关键词后端返回 `KeywordError`，包括非法输入、后端失败和格式戳不匹配。RAG 错误类型通过 `From` 接收 `KeywordError`，管线中的 `?` 不需要逐处转换。格式戳不匹配仍保留拒绝与只读两种策略，行为不变。

## 迁移方式

直接移动契约与更新调用点，不保留旧路径别名。上层 crate 增加对 `keyword-contracts` 的直接依赖，并使用 `keyword_contracts::KeywordIndex`。`rag-core` 不再重新导出关键词 trait，减少两条路径并存带来的歧义。

后端模块内部统一改为返回关键词契约的结果类型；`rag-core` 自身仍使用 `RagError` 表达文档、嵌入、重排与管线错误。

## 验证范围

保留并调整现有 SQLite 与 Tantivy 的往返、过滤、计数、中文分词、并发与跨后端对齐测试。测试仍通过 RAG 点构造辅助文档，但后端断言改为检查 `ScoredPoint` 的 id 和 payload 语义。Elasticsearch 的在线测试保持环境变量门控。

编译检查覆盖工作区全部目标与特性组合中的关键词路径，重点确认 feature 关闭时的 opener 仍能给出可读错误，且上层不再通过 `rag-core` 间接取得关键词 trait。
