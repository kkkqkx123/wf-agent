# RAG 数据源抽象设计

## 0. 结论速览

* 当前 RAG 已经把“存到哪里”抽象成 `VectorStorage` 与 `KeywordIndex`，但没有把“从哪里来”抽象成可枚举、可复用的数据源契约；`source` 只是 payload 里的一个字符串标签。
* 借鉴 llama_index 的文档、节点、带分结果三层模型与幂等摄取思路，但不引入它的组件运行时和索引体系。工作流图继续承担编排职责。
* 设计目标是补一层 `source-contracts`：描述数据源描述符、版本号、读取会话和归一化转换；`rag-core` 仍只负责清洗、切分、嵌入、索引与召回，不直接访问网络或文件系统。
* source 状态成为权威记录，向量索引与关键词索引都是从 source 派生的可重建结果。状态持久化到 `wf-storage`，替换当前进程内状态。
* 宿主负责打开向量后端、选择 embedding provider、注册数据源读取器并注入 RAG 句柄。运行时装配缺失是当前最大的功能缺口，必须与抽象设计一起解决。

## 1. 参考与现状

### llama_index 可借鉴点

| 参考位置 | 设计要点 | 本项目取舍 |
|---|---|---|
| `ref/llama_index/llama-index-core/llama_index/core/schema.py` 的 `Document`、`TextNode`、`IndexNode`、`NodeWithScore`、`QueryBundle` | 写入载体、可检索节点、父节点与带分结果分层；父子、前后关系显式建模 | 直接借鉴分层与关系字段；本项目已有 `RagDocument`、`RagChunk`、`RetrievedChunk`，不重建模型族 |
| `ref/llama_index/.../node_parser/interface.py` 的 `NodeParser` | 切分是纯变换，标识函数可注入，保证幂等 | 本项目 `RecursiveChunker` 继续纯变换；稳定 id 由 source 标识加序号派生 |
| `ref/llama_index/.../vector_stores/types.py` 的 `VectorStoreQuery` 与过滤类型 | 查询规约独立于存储实现，过滤是一等参数 | 与现有 `VectorQuery`、`SearchFilter` 同向；不合并向量与关键词为统一超 trait |
| `ref/llama_index/.../ingestion/pipeline.py` 与 `ingestion/cache.py` | 通过文档哈希做幂等摄取，支持 upsert 与删除过期文档 | 借鉴“状态先于索引”；本项目用 `sync.rs` 的哈希与 HTTP 校验计划，不引入 Python 式缓存组件 |
| `ref/llama_index/.../retrievers/fusion_retriever.py`、`auto_merging_retriever.py`、`router_retriever.py` | 多路融合、父子回填、按查询分流 | 已有融合、auto-merge 与路由内核；此设计只消费它们，不改召回算法 |
| `ref/llama_index/.../postprocessor/types.py` 与 `query_engine/` | 重排、合成、引用是独立阶段 | 维持现有 `rerank_adapter`、`synth`、`citation` 边界 |

### 当前缺口

* `rag-core/src/converter.rs` 定义了 `DocumentConverter`，但管线、工具和工作流都没有调用它；文件、PDF、结构化网页没有统一入口。
* `rag-core/src/sync.rs` 提供变更判定纯函数，但已索引 source 列表由调用方维护；`RagCapabilityState` 只用内存 `DashMap` 记录少量同步状态，重启即丢失。
* `RagHandles` 需要宿主注入 store、embedder 与可选 keyword，但仓库内生产入口没有调用 `with_rag_handles` 或 `attach_rag_capabilities`；HTTP 诊断路由和工具因此处于不可用状态。
* 关键词索引已有 `KeywordIndexLocation` 与配置解析；向量侧没有对应配置，simvec 与 Qdrant 实现停留在库级 feature，宿主无法配置切换。
* `wf-server` 只有 stats、preview、diagnose 三个只读端点，没有 source 列举、摄取、删除或重建入口。

## 2. 目标与不做事项

### 目标

* 提供统一的 source 描述，覆盖本地文件、URL、调用方提供的文档数组以及插件提供的读取器。
* 让增量摄取可复现：同一 source 在同一 revision 下重复运行不产生重复写入，也不覆盖更新的 revision。
* 让索引可重建：任何 source 都能按 namespace 与 collection 精确删除、重嵌和恢复。
* 保留现有管线与插件语义：召回、融合、重排、引用和九种 advisory 扩展不改职责。
* 提供运行时装配闭环，使 HTTP 路由、工具和工作流节点真正可用。

### 不做事项

* 不引入 llama_index 的组件执行器、回调系统、索引类型树或 chat engine。
* 不在本篇实现稀疏向量、多向量、具名向量与图谱；这些属于长期扩展，另见 `docs/plan/long-term/sparse-writes.md`、`docs/plan/long-term/rag-vector-kind-extension-design.md` 与 `docs/plan/long-term/knowledge-graph.md`。
* 不做分布式抓取、任务队列或后台 worker；首版 source 读取由调用方发起，后续需要调度时复用工作流图。
* 不做旧状态兼容。开发期直接替换内存状态与松散 converter 路径，已有索引通过显式 reindex 重建。

## 3. 目标模型

### 三层文档模型

* `SourceDescriptor`：描述“读什么”。包含 collection、namespace、稳定 source id、kind、uri、可选 reader 名、converter 名、刷新策略与只读标签。易变的 URL 查询参数不进入 source id，展示标题放 metadata。
* `SourceDocument`：描述“读到了什么”。包含 id、规范化文本、metadata 与 `SourceRevision`。revision 至少包含内容哈希；HTTP 读取器可附加 ETag 与 Last-Modified。
* `RetrievedChunk`：描述“召回到了什么”。继续复用现有命中结构，score 始终 higher-is-better；新增可选的 source revision 与 reader 名用于追溯，不改变已有字段语义。

`RagDocument` 与 `RagChunk` 保持现有职责：前者是进入切分管线的归一化文档，后者是可索引的检索单元。source id 继续作为 payload 中的 `source`，chunk id 继续由 source id 与序号稳定派生。

### 查询载体

现有 `RagQueryConfig` 已经承担查询规约角色。设计上把它的输入语义明确为：原始问题、改写变体、namespace、结构化条件、候选数与召回域开关。宿主组装它，管线不读取插件注册表或配置源码。

## 4. 契约与依赖方向

### 新增 source-contracts

在 `crates/infra/source-contracts` 建立叶子契约包，职责只有四类：

* source 描述符与 revision 值类型；
* 分页读取结果与不透明游标；
* `SourceReader` 读取端口，声明支持的 descriptor kind、打开会话、批量读取、可选 revision 校验与可选列举；
* `DocumentConverter` 端口，声明文件名或 MIME 匹配、字节到规范化文本的转换。

契约只描述输入输出与错误，不访问网络、文件系统、向量库、关键词库或插件注册表。读取器返回文档与 revision，绝不直接写入索引。

### 依赖方向

`source-contracts` 依赖最小基础类型，不依赖 `rag-core`。`rag-core` 依赖它，并在摄取管线内编排读取结果、清洗、切分、嵌入与双写。`wf-api` 依赖 `rag-core`、`wf-tools` 与 `wf-storage`，负责实现受权限约束的 URL 与文件读取器、持有 source 状态并装配工具。`wf-runtime` 负责打开后端并注入句柄，不改变现有 crate DAG 方向。

`converter.rs` 中的 trait 与值类型迁入 `source-contracts`；`rag-core` 保留 RAG 专属的 converter 注册表、选择策略与适配器，不保留重复定义。

### SourceReader 语义

读取器按会话工作。会话持有已解析的 descriptor，返回带 revision 的文档批次；分页读取器使用不透明游标并保证同一次读取中顺序稳定。能力声明区分三类：完整读取、revision 校验、分页列举。未声明的能力调用时返回明确错误，不用空结果伪装成功。

读取失败、权限拒绝、解析失败都通过契约错误上抛。读取器不做重试策略之外的业务决策，不修改 source 状态。

## 5. 摄取流程

### 单次 source run

一次摄取按固定顺序执行：

1. 解析 descriptor 并校验 collection、namespace、reader 与 converter 名称；未知名称直接失败。
2. 打开 reader 会话，读取文档批次与 revision。
3. 用 converter 把字节或原始记录归一化为文本与 metadata；显式 converter 名优先，未配置时才按扩展名或 MIME 选择，多个 converter 争用同一类型时配置期失败。
4. 清洗、插件 metadata extractor、切分；切分器可走内置或插件，关系字段继续保留 prev、next、parent。
5. 用 `sync.rs` 决策 Insert、Update 或 Skip。HTTP 校验只有 validator 明确匹配时才 Skip。
6. Update 先删除旧 source scope，再写入新点；Skip 不触碰索引。
7. 向量与关键词双写完成后，最后提交 source 状态。
8. 统计写入文档数、chunk 数、点数和跳过数，附 revision 一并返回。

### 状态存储

source 状态落在 `wf-storage` 的独立域，主键由 collection、namespace、source id 组成。记录字段包含 revision、converter 名、reader 名、文档数、chunk 数、最近成功时间、最近失败信息与写入中标记。元数据字段建立索引以支持按 collection、namespace、status 列举。

`wf-storage` 的批量写能力用于一次 run 的状态提交；批量失败时整批不提交。状态是 source 枚举与诊断的权威来源，扫描向量 payload 只用于核对，不作为发现新 source 的手段。

### 失败与补偿

* 写入前校验 embedding 维度、集合距离与 converter 输出；任何前置校验失败都不产生索引写入。
* 关键词写入失败时删除本次写入的向量 id，并把 source 状态标记为 failed；不把半写入状态暴露给查询。
* 删除 source 时向量与关键词两侧都尝试执行，首个错误上抛；状态在两侧成功后删除。
* revision 校验不匹配、读取中断或写入中断都返回显式错误，不回退到旧 revision 静默跳过。

## 6. 查询与可观测

查询路径不感知 source reader。已有 dense、multi-variant、hybrid、路由与合成入口保持不变；命中 payload 中的 source id、revision 与 reader 名用于引用和诊断。

诊断 API 从 source 状态读取已索引 source 列表，再用 `source_counts` 对比向量侧与关键词侧点数。近似 count 只作为趋势提示，不用于判定一致性；需要精确核对时走过滤后的 count 或分页 scroll。

## 7. 运行时装配与配置

### 向量后端配置

在 `wf-types` 增加向量索引配置，形状与关键词配置对齐：

* backend 选择 memory、simvec 或 qdrant；默认 memory 用于开发与测试。
* simvec 配置目录，qdrant 配置 base URL、API key、超时、batch、exact count 与版本策略。
* collection 默认距离、HNSW 与量化策略；未配置时沿用当前 `CollectionSpec` 默认值。
* embedding provider 复用既有 LLM provider 配置，不建立第二套凭证来源。

`wf-api` 提供 `VectorLocation` 解析，环境变量优先级高于配置文件；显式路径或 URL 高于派生路径。`wf-runtime` 启动时打开向量、关键词和 embedding provider，失败即 fail fast。

### 句柄注入

`Runtime` 完成以下步骤后视为 RAG 就绪：

1. 打开向量后端与关键词索引；
2. 创建 embedding provider；
3. 构造 `RagHandles` 并注入 `ApiContext`；
4. 创建 source reader registry、converter registry 与持久化 source 状态域；
5. 按 capability 配置调用 `attach_rag_capabilities`，注册工具与资源描述；
6. 注册 RAG_QUERY 与 RAG_INGEST 节点处理器；读取器未配置时相关工具仍可显示，但调用返回明确的服务不可用错误。

### 对外入口

保留现有 stats、preview、diagnose 路由。新增 source 列举、摄取、删除与重建路由，以及同名的模型工具：source 摄取是网络或文件风险操作，必须沿用域名、路径范围与审批策略。工作流 RAG_INGEST 节点增加可选 source descriptor 变量；已有 documents 变量路径继续作为纯文档入口，与 source 路径共用同一摄取管线。

## 8. 插件边界

现有九种检索扩展保持 advisory 语义，不因为 source 抽象改变。新增的 source reader 属于 provider 型贡献，不是 advisory：

* 仅在原生 C ABI 暴露注册口，Lua 与 Wasm 不提供该通道；
* 需要独立权限位，网络与文件访问由宿主按 manifest 授权和限制；
* 插件只返回规范化文档与 revision，不能直接写索引或改 source 状态；
* 同名 reader 按插件归属做冲突检查，卸载插件时同步注销；
* 配置引用未知 reader 时失败，读取执行失败时整次 source run 失败，不做内置读取器回退。

document converter 可按同样模式开放原生贡献，但排序由宿主配置显式决定；不允许插件隐式抢占已声明的文件类型。

## 9. 迁移与验证

开发期不做兼容层。删除 `RagCapabilityState` 的内存 source 表，改用持久化状态；删除 `rag-core` 内重复的 converter 定义；已有索引通过显式 reindex 重新生成，不设计自动迁移。

验证范围覆盖 reader 与 converter 契约一致性、稳定 source id、分页完整性、哈希与 HTTP 增量判定、失败补偿、重启后状态恢复、URL 与本地文件适配器、HTTP 路由、工具和节点装配。feature 关闭时 opener 给出可读错误，不静默降级。

## 10. 实施顺序

* 第一步：落地 `source-contracts` 与 source 状态域，迁移 converter 端口。
* 第二步：补向量配置解析、embedding 注入与 `RagHandles` 装配，打通现有 HTTP 路由、工具和节点。
* 第三步：实现 URL 与本地文件 reader，接入 source 摄取、列举、删除和重建入口。
* 第四步：接入原生 reader 与 converter 插件贡献，补一致性核对与诊断。
