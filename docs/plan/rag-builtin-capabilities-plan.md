# RAG 内置增强能力设计

## 背景与范围

本文只做设计，不在本阶段实施。目标是梳理当前 RAG 层已有的纯逻辑能力，给出一组可选、可组合、默认关闭的内置增强项，避免把抓取、Embedding、向量库或模型供应商硬编码进 `rag-core`。

当前事实基线：

- `rag-core` 已具备文档模型、清洗、递归分块、ingest/query 管线、混合检索、RRF/加权融合、rerank 适配、引用生成、上下文重排与裁剪、查询改写、路由、增量同步决策与离线评估等纯逻辑。
- `rag-core` 明确不使用具体向量后端、Embedding 与 Completion 实现；这些由宿主通过 `VectorStorage`、`KeywordIndex`、provider trait 注入。
- 工具层只有只读的 `rag_search`，通过 `RagSearchBackend` 委托给宿主；没有 URL 入库、索引状态查询或离线评估工具。
- `web_fetch` 已具备 HTML5 DOM 清洗、焦点筛选、clean HTML 与结构化表格摘要，但抓取结果与 RAG 摄取之间没有内置桥接。

因此“内置增强”指的是：在现有 crate 内提供默认实现与编排选项，而不是新增供应商绑定或复制已有算法。

## 设计原则

1. 保持 `rag-core` 的供应商中立：内置的是算法与编排，不是模型或数据库。
2. 能力默认关闭，宿主按场景开启；配置名表达行为，不表达实现代际。
3. 复用现有模块：分块用 `RecursiveChunker`，同步决策用 `sync`，引用用 `citation`，评估用 `eval`，改写用 `rewrite`/`transform`。
4. 网页入库的传输与内容清洗复用 `web_fetch` 管线；RAG 层不重复实现 DOM 解析或robots/代理策略。
5. 可选能力不得改变 `rag_search` 的默认只读语义；新工具按权限和风险单独声明。

## 可选内置功能

### 网页摄取桥接

在宿主层增加一个可选 ingest 入口：输入 URL，调用 `web_fetch` 得到 Markdown 或 clean HTML，再交给 `rag-core::ingest_documents`。摄取内容保留 `source` URL、标题、最终 URL、ETag、Last-Modified 与抓取时间作为元数据。

该能力具备以下约束：

- 工具或节点必须标记为网络风险，继承审批与 `NetworkDomainPolicy`。
- 继续受抓取体积上限、落盘预算和域名规则约束。
- 支持 `focus`、`selector` 与 `min_words`，让入库内容在分块前先去除导航和广告。
- 若目标站点禁止索引，应尊重 robots 与页面元数据；判定归抓取层，RAG 层只消费结果。
- 不新增 `web_fetch` → `rag-core` 的直接 crate 依赖，桥接落在 `wf-runtime` 或应用层。

### 内容感知分块

以现有 `ChunkerConfig` 为基础提供命名预设：

- Markdown 结构优先：按标题层级与列表边界切分，保留标题路径作为块元数据。
- 句子/段落优先：适合自然语言问答，配置 overlap 与最小块长。
- 代码优先：扩大代码块边界，避免把函数定义切开。
- 表格优先：把抓取得到的结构化表摘要与 Markdown 表格一起纳入同一块，避免表头与数据行分离。

这些预设只影响分块参数与前后处理顺序，不新增独立分块模块。插件仍可通过已注册的 chunk splitter 覆盖默认行为。

### 增量同步与去重

复用 `sync::content_hash`、`plan_sync`、`decisions_for_documents` 与 stale/pending 决策，提供两种同步依据：

- 内容哈希：适合本地文件和静态页面。
- HTTP 验证元数据：ETag/Last-Modified 未变化时跳过重新 embedding，变化时只删除旧块再写入新块。

同时支持按 `source` 删除、按 namespace 重索引和重复策略配置。所有决策保持纯函数，实际删除/写入仍由宿主管线执行。

### 检索增强管线

把已有检索能力组合成可选模式，而不是新增万能算法：

- 密集与关键词混合召回，支持 RRF 与加权融合两种模式。
- 查询改写：无 LLM 时使用启发式改写；配置 provider 后启用 HyDE、多问题分解或反馈式改写。
- rerank：在候选合并后可选调用 rerank provider，并对 top_n 与截断长度做显式配置。
- 上下文整理：父块提升、窗口扩展、按预算裁剪、按相关度重排，输出给模型前统一走 `postprocess`。
- 路由：按 namespace 或查询规则选择索引集合，规则命中失败时退回默认集合。

每种模式都应可被单独开启/关闭，并保留 `rag_search` 的简洁默认输出，避免把调试信息塞给模型。

### 引用与可追溯性

基于 `citation` 模块提供统一引用格式：

- 每个命中块带 source、locator、score 与简短摘录。
- `check_completeness` 用于发现“引用存在但正文缺失”或“正文存在但没有引用”的情况。
- 网页来源携带抓取时的 final_url 与抓取时间；本地来源携带路径与内容哈希。
- 对无命中场景使用固定模板，避免模型编造答案。

引用渲染保持为纯文本/结构化数据，不绑定特定前端或提示词模板。

### 评估与回归

把 `eval` 模块接入可选评估流程：

- 离线样例集：问题、期望 source/locator、最小命中要求。
- 用 `score_case` 与 `summarize` 汇总召回命中率与排序质量。
- 同一套样例既可用于本地回归，也可用于配置变更前后的对照。
- 评估不调用外部服务时只做确定性打分；需要模型裁判时通过宿主注入 judge。

### 索引状态与诊断工具

可选增加只读工具或 API：

- 列出 namespace、文档数、块数、最近同步时间与过期来源。
- 查询某个 source 的块分布与内容哈希。
- 暴露最近一次 ingest/query 的统计：耗时、候选数、融合后数量、rerank 用量、上下文字符数。

这些字段属于诊断，不应混入 `rag_search` 的默认结果包络。

## 分层与依赖

- `rag-core` 继续只依赖向量契约、关键词契约与 provider trait；新增能力都放在已有模块或现有管线参数中。
- `wf-tools` 只定义工具契约，不选择向量库或模型。
- `wf-runtime` 负责把 `web_fetch` 结果、审批策略、Embedding/Rerank/Vector 后端与 `rag-core` 管线装配起来。
- 插件提供的 analyzer、advisor、postprocessor、judge 继续是建议性扩展，不绕过宿主的资源选择与安全边界。

## 配置方向

所有增强以能力开关和参数存在，默认关闭：

- ingest：允许的来源类型、URL/namespace 映射、元数据字段、最大文档数与单次摄取体积。
- chunking：预设名、目标长度、overlap、是否 hierarchical。
- sync：依据类型（hash 或 HTTP 验证元数据）、TTL、删除策略。
- retrieval：融合模式、候选数、rerank 开关与 top_n、窗口半径、上下文预算。
- citation：摘录长度、是否包含 score、无命中模板。
- eval：样例集路径、 judge 是否启用、阈值。

具体字段在实现时随所在配置段确定，不为不同阶段保留别名。

## 安全与资源约束

- URL 入库属于网络操作，必须走审批、域名策略与代理；不能因为进入 RAG 就绕过抓取治理。
- 单文档体积、总文档数、chunk 数与查询候选数都要有上限，防止索引膨胀和上下文爆炸。
- 外部内容只作为数据，不直接作为指令执行；提示词模板对来源内容做明确分隔。
- 同步删除按 source 精确执行，避免误删整个 namespace。
- 本地或离线模式下允许仅启用关键词检索与启发式分块，不强制要求 Embedding 服务。

## 本阶段非目标

- 不实现网页入库工具、索引状态工具或评估工具。
- 不引入本地 Embedding 模型、向量库实现或rerank服务。
- 不修改 `rag_search` 默认输出结构。
- 不在 `web_fetch` 内复制分块、融合、rerank 或引用逻辑。
- 不实现自动定时爬取或站点级同步调度。

## 后续落地建议

按依赖顺序推进：先做宿主层的 URL 摄取桥接与同步元数据；再开放分块预设与检索增强开关；最后接入引用校验与离线评估。每一步都复用现有 `rag-core` 纯函数，并通过独立工具或配置项暴露，保持核心 RAG 契约稳定。
