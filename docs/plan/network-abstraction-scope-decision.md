# 搜索抽象边界与数据源路线决策

## 目标与范围

回答三个问题：工具层现有抽象是否多余；扩展第三方搜索源（Tavily 一类）应落在哪条路线；是否应移除 `network` 抽象、只保留搜索抽象。

范围限定在 `web_search` / `web_fetch` / `rag_search` 三条能力及其契约、实现、配置与装配。判断依据是源码事实与既有设计文档，不预设"抽象越少越好"或"越统一越好"。

## 事实基线

以下事实决定后续全部结论，均可按 `file:line` 复核。

- 搜索契约：`WebSearchBackend`（`wf-tools/src/predefined/web/web_search.rs:188`）继承自仅有 id/kind 的 `SearchBackend`（:105）；生产实现有 `DuckDuckGoBackend`、`HttpSearchBackend`、`PluginWebSearchBackend`。
- 抓取契约：`WebFetchBackend`（`web_fetch.rs:128`）独立声明 id/kind，未继承 `SearchBackend`；配套 `FetchBackendRegistry`（`wf-resource/src/network/fetch.rs:22`）与 'WebFetchRouter'（:89），Router 的 `selected` 在构造期固定（`bootstrap/network.rs:114`）。
- 检索契约：工具层已存在独立的 `RagSearchBackend`（`wf-tools/src/predefined/rag.rs:97`），返回带 score 与 citation locator 的 `RagHit`（:81）；它**没有**继承 `SearchBackend`。
- 传输原语：`NetworkDomainPolicy` 与 `normalize_host` / `domain_matches` 位于 `wf-tools/src/network.rs:66/118/141`，被审批模块直接复用（`approval/network_policy.rs:8`、`approval.rs:291/304`）。
- 声明式搜索：`HttpSearchBackend`（`wf-resource/src/network/http_backend.rs:24`）支持 GET/POST、请求头、JSON body、`items_path` 点路径与字段映射（`wf-config/src/processor/network.rs:100/123/115`）。
- 密钥注入：配置加载阶段对整份文件做 `${VAR}` 环境展开（`wf-config/src/env.rs:130`，入口 `layered.rs:82`），声明式后端 header 可用环境变量承载 API key（`config-examples/infrastructure/tools.toml:88-96` 有示例）。
- RAG 摄入能力：`rag-core` 的 converter 只做 Plain/Markdown/JSON 的字符串转换（`converter.rs`），`reqwest` 仅用于 Elastic 索引（`elastic_index.rs`），**没有任何在线网页抓取入口**；网页正文由宿主页面抽取管线转换后交给 RAG，`RagDocument.source` 只是标签（`document.rs`）。
- 无桥接：`rag-core` 不引用 `web_search`，两个 rag 工具模块也不引用 `web_search`（检索为空）。

## "network 抽象"的三层拆解

"移除 network 抽象"这个动作必须先拆开，否则会把三种性质完全不同的东西一起删掉。

**第一层：传输安全原语（`NetworkProxy` / `NetworkDomainPolicy` / 共享纯函数）。**
这不是"network 工具组的编排抽象"，而是**审批与传输共用的安全基座**。审批模块直接 `use crate::network::{domain_matches, normalize_host}`（`approval/network_policy.rs:8`），传输侧用同一套规则逐跳校验（`network.rs:91/97`）。移除它等于让"审批判定"与"真实请求"两处规则各自实现，正是 hardening 文档要消除的漂移（`network-tools-hardening.md:60-63`）。**必须保留，且不应算作"额外抽象"。**

**第二层：搜索与抓取的能力契约（`WebSearchBackend` / `WebFetchBackend`）。**
`WebSearchBackend` 就是用户所说的"搜索本身的抽象"，是扩展 Tavily 的正确入口（见下节）。`WebFetchBackend` 服务的是"读网页全文"这一独立工具能力，与"发现搜索源"不是同一件事。

**第三层：对称性基础设施（`SearchBackend` 父 trait、`FetchBackendRegistry` + `WebFetchRouter`、多后端聚合器）。**
这才是真正可质疑的部分：父 trait 只有 id/kind 且只有 `WebSearchBackend` 一个子类；抓取侧的注册表+路由是为"与搜索侧对称"而建，但抓取没有聚合需求、`selected` 也不可运行时切换。

## 判断一：扩展 Tavily 不需要任何新抽象，也不需要 RAG

Tavily 是标准 JSON HTTP 搜索 API（POST + header 鉴权 + `results[].{title,url,content}`）。现有声明式 `HttpSearchBackend` 的能力刚好覆盖：方法、URL、header、JSON body、`items_path`、字段映射，且 header 密钥可用 `${WF_TAVILY_API_KEY}` 在配置加载期展开。**结论：接入 Tavily 是纯配置工作，落在 `[network.web_search.backends]` 的 `kind = "http"` 分支，零代码、零新 trait。**

这直接回答了"工具层实现额外抽象是否多余"：**为接入第三方搜索源而新增抽象是多余的**，现有 `WebSearchBackend` + 声明式后端已经足够。真正多余的抽象不在这里，而在第三层（父 trait 与抓取对称设施）。

## 判断二："额外数据源走 RAG"只对一半场景成立

必须区分两类"数据源"：

- **在线发现型（Tavily / Brave / SerpAPI）**：给定 query 返回"指向外部网页的链接与摘要"，无索引、无 embedding、无 chunk。这类属于**搜索**，应走 `WebSearchBackend`。
- **入库检索型（内部文档、抓取后沉淀的语料）**：需要 ingest → chunk → embed → 索引 → 检索，并要 score 与 citation。这类属于 **RAG**，走 `RagSearchBackend`。

二者的输出语义不同：`WebSearchResult` 是 `{title,url,snippet}`（`web_search.rs:136`），`RagHit` 是 `{chunk_id,document_id,source,content,score}`（`rag.rs:81`）。把 Tavily 塞进 RAG，会强迫一个"发现"接口产出"已索引片段与相关性分数"，语义错位，且当前 `rag-core` 没有在线抓取入口来实现这条管道。

因此"额外数据源走 RAG 更合适"只在**目标是把外部内容沉淀为可检索知识**时成立；若目标只是"多一个实时搜索来源"，走 RAG 反而更重。既有设计文档也明确反对单一万能 trait、主张按域拆分（`network-tool-group-design.md:28-30`），这一判断在代码里已被 `RagSearchBackend` 独立于 `SearchBackend` 的事实印证。

## 判断三："仅保留搜索抽象"的可行边界

用户的目标（只留搜索、便于加源、去掉 network 层）在**契约层完全可达成**，但不能删到传输原语。可行的收敛边界是：

- **保留**：`WebSearchBackend`（搜索契约）、声明式 `HttpSearchBackend`（加源入口）、`NetworkProxy` / `NetworkDomainPolicy` 与共享纯函数（安全与审批基座）、`[network.web_search]` 配置。
- **可合并**：`SearchBackend` 父 trait 并入 `WebSearchBackend`——它唯一的设计目的是"域无关身份"，但唯一被设想复用的 `RagSearchBackend` 并未继承它（`rag.rs:97`），前瞻抽象未兑现。
- **可大幅简化**：`FetchBackendRegistry` + `WebFetchRouter`。若抓取不需要插件热替换，装配期直接注入一个 `Arc<dyn WebFetchBackend>` 即可，去掉注册表与路由两层。
- **可收敛**：若只保留单搜索源，`WebSearchAggregator` 的多后端聚合与 `default_backends` 白名单可退化为"单后端直连"；但一旦要同时挂 Tavily 与 DDG，聚合器就有实际价值，**不建议为简化而删**。

需要指出：把 `network` 作为**命名/配置段/能力组**的概念保留是低成本的，它只是命名；真正值得砍的是第三层设施，而不是这个名词。贸然删除 `wf-tools::network` 模块会连带拆掉审批依赖的安全原语。

## 开放点处置

1. `web_fetch` 的定位：**保留插件可替换抓取后端，并实现运行时可变**。selected 由 `FetchBackendRegistry` 持有，Router 调用时读取，`set_selected` 可在运行时切换，owner 删除时回退到内置 `http`。
2. `SearchBackend` 父 trait：**维持独立抽象，不合并**。`WebFetchBackend` 不继承 `SearchBackend`；用户确认 fetch 与 search 返回结构不同（网页正文文本 vs 带排序 JSON 对象），id/kind 不构成统一理由，其他工具组也不为 id/kind 造 trait。
3. 聚合器去留：**保留 `WebSearchAggregator`**，多源聚合（DDG + Tavily 一类）是其明确价值，不为简化而退化直连。
4. RAG 与搜索的边界：**不新建沉淀管道**；Tavily 一类在线搜索源走声明式 `WebSearchBackend`，RAG 仅承载已入库语料检索。二者语义不同，保持正交。
5. `network` 命名：**保留**，并补充 `docs/archive/dynamic.md` 说明动态派发必要性，避免被误读为工具组编排层而遭误删。

## 验证

- 全部结论基于当前 HEAD `ba572920` 的静态引用。
- 若按上述收敛，改动应限于 `wf-tools`（契约合并）、`wf-resource`（去 Router/Registry 或保留）、`wf-runtime`（装配简化），并需跑 `cargo clippy --all-targets --all-features` 与 `bash scripts/sync-deploy.sh --check`。
- Tavily 接入验证可用现有 `--tier=network` E2E（`e2e/fixtures/network-tools.toml`）扩展一条声明式后端用例，无需新增 Rust 代码。
