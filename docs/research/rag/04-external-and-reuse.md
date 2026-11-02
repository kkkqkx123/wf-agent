# 外部服务与复用决策：解析、工具、向量代码

> 背景：CCE 已转向 Apache-2.0，`simvec` 在两边均为正式 submodule，`wf-agent` 的 `ref/rust-client` 为官方 Qdrant Rust 客户端源码快照。本文记录三项决策的依据与落点。

## 外部解析服务：内置只做轻量，重格式走 converter 端口

`cce-document` 的现状是依据：内置只覆盖可确定性解析的格式（`crates/parser/cce-document/src/lib.rs` 按格式分管线：纯文本、Markdown、JSON、TOML、YAML、XML），复杂输入收敛到 `plugin.rs` 的 `PluginDocumentPipeline`，把外部解析结果归一为标准切片后再进同一套切分与存储。

`wf-agent` 照搬该分层：

- 内置转换只做纯文本、Markdown、JSON，无网络、无重依赖、可单测。
- 重解析定义为外部 `DocumentConverter` 端口：输入为字节加文件名，输出为归一化文档加可选 sidecar（公式、表格、版式单独存放，不污染正文向量）。minerU、unstructured、云 OCR 各实现该端口的 HTTP、MCP 或插件适配器之一。
- 路由按文件类型与复杂度配置：默认走内置，命中重格式或体积阈值转外部，外部失败按策略降级（报错或回退纯文本抽取，不静默丢弃）。
- 横切能力与索引管线同级：异步调用、超时、重试、熔断、结果缓存（按内容哈希）、隐私开关（本地优先还是允许远端）。

## 工具集成：内置最小集，丰富形态走外部扩展

预定义工具是编译期静态 schema（`crates/engine/wf-tools/src/predefined/schema.rs` 的 `ToolDefinition`），适合稳定原语，不适合检索这种参数多变的场景。检索的实际参数面至少包括返回数、分数下限、元数据过滤、命名空间、混合开关、融合权重、重排开关，以及后端特有项（参考 CCE 的 `DenseSearchQuery{limit/score_threshold/hnsw_ef/filter}` 与 `SearchFilter{raw_filter}` 逃生舱）。

- 内置只留一个最小只读检索工具：问句、返回数、过滤、知识库标识。只读、可门控、可发现式，走现有 `ToolRegistry` 与 `scope_gate`。
- 丰富形态走现有扩展机制：MCP 服务器（复杂检索、多后端、鉴权由 server 侧收敛，`McpExecutor` 直接执行）、`wf-plugin`（Lua 与 Native 插件贡献工具）、skill（检索流程的提示词封装）。CCE 侧已有 `cce-mcp` 与 `cce-transport` 作为先例。
- 内核侧只保证一件事：外部工具的出参与内置工具同构（内容、分数、来源、引用定位），提示词模板与引用渲染不用为每种扩展重写。

## 向量代码复用：只共享契约，不共享后端

CCE 的复用现状（`.gitmodules`）：已抽为 submodule 的是 `simvec`、`tantivy`、`llm-suite`、`encoding-guess` 这类边界清晰、无业务耦合的独立引擎；`cce-storage-vector-local`、`cce-storage-vector-qdrant`、`cce-storage-bm25`、`cce-storage-common` 仍在 `crates/infra` 树内，因为它们依赖业务语义。`LocalVectorStore`（`cce-storage-vector-local/src/store.rs`）本质是 `LocalVectorEngine` 配集合名、维度校验、距离映射的薄适配；真正可共享的是 `VectorStorage` 契约（`cce-storage-common/src/lib.rs` 的 `VectorStorage`）。

结合 `wf-agent` 的约束：

- 工作区排斥：两边根 `Cargo.toml` 都把 `llm-suite` 排除在成员之外、用 path 依赖，避免双工作区抢包名。新共享包沿用同一做法。
- 薄封装不值得复用：local 适配各项目自写，`wf-agent` 不引入 CCE 的 local 实现。
- 后端语义过重不值得复用：CCE 的 Qdrant 实现约六千行，含代际、分组隔离、调度与度量；`wf-agent` 的 RAG 语义只需要命名空间加元数据过滤。共享后端等于把业务模型强加过来。

落点（已执行一半）：两个独立包，中立命名以便抽仓。

- 包 A `crates/infra/vector-contracts`（已落地）：类型、过滤、操作契约、内存实现、simvec 适配（`simvec` 特性）、Qdrant REST 薄客户端（`qdrant` 特性）。从 `cce-storage-common` 摘向量半区，`raw_filter` 这类后端逃生舱不进契约。
- 包 B 独立 RAG 包（后续）：文档、切分、管线、重排适配、引用。依赖包 A 与向量化、重排 trait，不依赖具体后端。

## Qdrant 客户端选型：REST 薄客户端，不引入官方 gRPC 客户端

事实：CCE 的 Qdrant 后端没有用官方客户端，自研了基于 `reqwest` 的 REST 客户端（`cce-storage-vector-qdrant/src/client.rs`），刻意避开 `tonic`；`ref/rust-client` 的官方客户端是 gRPC 路线（`tonic 0.14 + prost`），`wf-agent` 当前没有 `tonic` 依赖。

- 官方客户端的好处是协议覆盖全、gRPC 性能好；代价是引入整套 `tonic/prost` 编译负担，且版本与服务端耦合。
- REST 薄客户端的好处是零新增重依赖（复用现有 `reqwest 0.13` 栈）、只实现 RAG 所需子集、出问题可直接修；代价是放弃 gRPC 与高级特性。

`wf-agent` 沿用 CCE 的选择。`ref/rust-client` 继续当协议参考，不进依赖表。若压测证明 REST 是瓶颈，再以 feature 门的形式引入官方客户端做 gRPC 后端，契约保持不变。
