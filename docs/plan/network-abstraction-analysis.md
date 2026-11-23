# network 能力抽象清单与必要性分析

## 目标与范围

梳理当前代码中 `network` 能力涉及的全部抽象，逐项判断其是否有真实消费者、是否值得保留。范围包括 `web_search` / `web_fetch` 两个工具的契约、后端实现、注册表与聚合、配置形态、宿主装配与插件桥接。判断标准是"是否有真实调用点"与"是否符合本仓库 AGENTS.md 的架构约定"，而非理论上的完整性。

本文只做事实陈述与必要性评估，不含改造代码。所有结论均附 `file:line` 引用，便于逐条复核。

## 抽象全景（按分层）

网络能力横跨四层，依赖方向为 `wf-tools ← wf-resource ← wf-runtime`，`wf-config` 提供输入类型，符合 AGENTS.md 声明的严格 DAG（AGENTS.md:40-54）。

### 契约层：`crates/engine/wf-tools`

| 抽象 | 位置 | 职责 | 真实消费者 |
| --- | --- | --- | --- |
| `SearchBackend` trait | `predefined/web/web_search.rs:105` | 域无关的后端身份（id/kind） | 仅被 `WebSearchBackend` 继承，及测试桩实现 |
| `WebSearchBackend` trait | `web_search.rs:188` | 搜索能力：`search` / `endpoints` / `catalog` | 内置、声明式、插件、harness 共 4 类实现 |
| `WebFetchBackend` trait | `predefined/web/web_fetch.rs:128` | 抓取能力：`fetch` 与 id/kind | 内置、插件、harness 共 3 类实现 |
| `NetworkDomainPolicy` | `network.rs:66` | 域名 allow/deny 规则与逐跳判定 | 传输层与审批层共用 |
| `NetworkProxy` | `network.rs:53` | route-only 代理 + no_proxy | 配置装配与 client 构建 |
| `WebToolConfig` | `predefined/web.rs:30` | 持有注入的两个 dyn 后端与默认结果数 | 两个 handler |
| 共享纯函数 | `network.rs:118/141/157` | `normalize_host` / `domain_matches` / `redact_transport_error` | 传输与审批（`approval/network_policy.rs:8`） |
| 数据契约 | `web_search.rs:122/136/144/164`、`web_fetch.rs:63/69/78/102` | Query/Result/Response/Error/Config | handler、后端、聚合器 |

### 实现层：`crates/engine/wf-resource/src/network`

| 抽象 | 位置 | 职责 |
| --- | --- | --- |
| `SearchBackendClient` | `backend.rs:14` | 每后端一个 reqwest client（超时/UA/代理/域名策略）的静态工厂 |
| `DuckDuckGoBackend` | `duckduckgo.rs:35` | 内置 DDG HTML 后端 |
| `HttpSearchBackend` | `http_backend.rs:24` | 配置驱动的声明式 HTTP 搜索后端 |
| `HttpFetchBackend` | `fetch.rs:125` | 内置 HTTP 抓取后端（流式体积上限） |
| `NetworkRegistry` | `registry.rs:35` | 搜索后端注册表：owner/priority/snapshot |
| `WebSearchAggregator` | `registry.rs:155` | 聚合器（自身也是 `WebSearchBackend`，Composite 模式） |
| `SearchAggregationOptions` | `registry.rs:146` | 重试/缓存/限流/指标等运行时参数 |
| `FetchBackendRegistry` | `fetch.rs:22` | 抓取后端注册表（owner 生命周期） |
| `WebFetchRouter` | `fetch.rs:89` | 按固定 id 选取一个抓取后端（自身也是 `WebFetchBackend`） |
| 共享函数/常量 | `backend.rs:10/46/52/66` | 体积上限、端点校验、URL 规范化、去重 |
| 装配函数 | `network.rs:30/95` | 配置 → 后端实例注册与聚合器构造 |

### 配置层：`crates/infra/wf-config/src/processor/network.rs`

`NetworkConfig`（:271）由 `WebSearchConfig`（:192）与 `WebFetchConfig`（:240）组成；后端形态用带 tag 的 `WebSearchBackendConfig` 枚举（:137）区分 `duckduckgo_html` / `http`；请求与响应形状由 `HttpRequestConfig`（:100）、`HttpResponseConfig`（:123）、`ResultFieldMap`（:115）描述；`AggregationMode`（:67）、`HttpMethod`（:75）、`HttpResponseFormat`（:83）为策略枚举；`ProxyConfig`（:91）只接受 route-only 地址。全部结构启用 `deny_unknown_fields`，并带上限校验（如 `max_redirects`，:463）。

### 装配与桥接层：`crates/app/wf-runtime/src`

- `bootstrap/network.rs`：`build_network_domain_policy`（:21）、`resolve_proxy`（:41）、`build_web_search_backend`（:72）、`build_web_fetch_backend`（:105）、`build_web_fetch_config`（:119）。
- `plugin_bridge/network.rs`：`PluginWebSearchBackend`（:24）、`PluginWebFetchBackend`（:86）、tag 常量（:21-22）、`sync_network`（:163）、`unsync_network`（:238）、native + network 权限校验（:144）。

## 必要性逐项评估

### 结论为"必要，保留"

- **`WebSearchBackend` / `WebFetchBackend` 两个能力 trait**：这是整套设计的支点。搜索侧有 `DuckDuckGoBackend`、`HttpSearchBackend`、`PluginWebSearchBackend` 三个生产实现，抓取侧有 `HttpFetchBackend`、`PluginWebFetchBackend` 两个；handler 只依赖 trait（`web_search.rs:208`、`web_fetch.rs:138`）。它们让"内置 / 配置声明 / 插件"三种来源对工具层不可见，替换传输无需改工具定义。必要。
- **`NetworkDomainPolicy` / `NetworkProxy` 及共享纯函数**：审批模块的主机匹配直接复用 `crate::network` 的 `normalize_host` / `domain_matches`（`approval/network_policy.rs:8`、`approval.rs:291/304`），传输侧复用同一规则的 `allows_url` / `allows_host`（`network.rs:91/97`）。这消除了"审批判定"与"实际请求"两处规则漂移，正是 hardening 文档所述目标（`network-tools-hardening.md:60-63`）。必要。
- **`SearchBackendClient`**：集中 client 构造，被 DDG（`duckduckgo.rs:52`）、声明式 HTTP（`http_backend.rs:41`）、抓取（`fetch.rs:133`）三处复用，保证超时、代理、域名策略一致。必要（实现形态可商榷，见下）。
- **`NetworkRegistry` 的 owner / snapshot 设计**：owner 支撑插件按 id 对称注销（`registry.rs:97`、`plugin_bridge/network.rs:238`）；snapshot 让注册表在调用时读取，插件运行时激活/停用立即生效（`registry.rs:32-33/126`）。有真实消费者。必要。
- **配置层全部类型**：`deny_unknown_fields` + 启动期上限校验（超时、体积、跳数、后端 id 冲突），把错误前移到启动阶段，符合 hardening 文档方向（:64-66）。必要。
- **`AggregationMode` 双模式**：`all` 与 `priority_first_success` 在 `search_inner`（`registry.rs:396-416`）有真实分支，且分别有测试覆盖（:618/:664）。必要。

### 结论为"设计合理，成本可控"

- **`SearchBackend` 作为域无关身份父 trait**：设计文档明确其意图是"统一抽象只覆盖后端身份"，为未来 RSS 等域预留（`network-tool-group-design.md:28-30`）。当前只有 web 一个实现域，属于前瞻性抽象；仅两个方法，成本低。合理但需接受它当前只有一个子类。
- **`WebSearchAggregator` 实现 `WebSearchBackend`（Composite）**：聚合器注入 `WebToolConfig.backend`（`bootstrap/network.rs:93-101`），内部又管理一组 `WebSearchBackend`。这是标准组合模式，让 handler 无需感知单/多后端。成本是 `endpoints`（`registry.rs:316`）与 `catalog`（:328）在聚合层要再做一次归并；调用面很窄——`catalog` 仅被发现工具调用（`web_search.rs:300`），`endpoints` 仅被装配期调用一次（`bootstrap/tool_registry.rs:32`）。合理。
- **`FetchToolConfig`（wf-tools）与 `WebFetchConfig`（wf-config）字段近似**：前者是运行时有效配置（`web_fetch.rs:78`），后者是配置输入（`network.rs:240`），bootstrap 逐字段转换（`bootstrap/network.rs:130-137`）。这是"配置 schema 不依赖工具层"的解耦代价，可接受。
- **动态派发（`dyn`）**：全模块约 15 处 `Arc<dyn WebSearchBackend>` / `Arc<dyn WebFetchBackend>`，集中在本质需要"异构后端统一聚合 + 插件热插拔"的位置（注册表、Router、WebToolConfig）。用 enum 无法自然表达运行时注册的插件后端，因此 `dyn` 在此有实质必要性。

### 结论为"存在缺口或冗余，建议处理"

- **`max_redirects` 是未生效的死配置**：该字段从配置定义（`network.rs:248`）、上限校验（:463）、bootstrap 传值（`bootstrap/network.rs:134`）到 `FetchToolConfig`（`web_fetch.rs:82`）完整存在，但终点即止——`HttpFetchBackend::new`（`fetch.rs:132-144`）与 `SearchBackendClient::build`（`backend.rs:17-41`）均未读取它；自定义重定向闭包（`backend.rs:27-34`）只逐跳做域名校验，不含跳数上限。hardening 文档声称已支持"重定向上限"（`network-tools-hardening.md:105`），与实现不符。**【已落地，见文末落地记录】**
- **`dyn` 未按项目规范文档化**：AGENTS.md:85 要求"所有动态派发记录在 `docs/archive/dynamic.md`"，但该文件不存在，`dyn` 使用也未登记。抽象本身合理，但违反了本仓库的显式约定。**【已落地，见文末落地记录】**
- **身份 trait 两处定义不统一**：`SearchBackend` 抽出了 id/kind（`web_search.rs:105`），但 `WebFetchBackend` 自己重复声明 id/kind（`web_fetch.rs:131-134`）而非继承 `SearchBackend`。**【已决策维持独立】**：search 返回带排序的 JSON 对象、fetch 返回网页正文文本，二者返回结构不同，id/kind 不构成统一理由；不为统一而造父 trait。
- **`FetchBackendRegistry` + `WebFetchRouter` 价值偏薄**：Router 只按构造期固定的 `selected`（`fetch.rs:95`；`bootstrap/network.rs:114` 来自配置）从注册表取一个后端转发（:105-113），`selected` 不可运行时切换。**【已落地】**：selected 移入 Registry，Router 在调用时读取，支持运行时切换与 owner 删除回退。
- **`DuckDuckGoBackend` 与声明式后端能力重叠**：`HttpSearchBackend` 已支持 `HttpResponseFormat::DuckduckgoHtml` 并复用同一解析器。**【已落地】**：删除独立 `DuckDuckGoBackend`，DDG 预设翻译为声明式 `HttpRequestConfig`，默认后端零配置仍可用。
- **`SearchBackendClient` 以 unit struct 承载单一静态方法**：`build`（`backend.rs:17`）无状态，用类型包裹仅是命名空间。可简化为自由函数，收益有限，不紧急。**【保留】**
- **`WebSearchAggregator::new` 与 `new_with_options` 双重构造**：`new` 仅转发默认参数，主要服务测试构造。轻微冗余。**【保留】**

## 开放点处置

1. `max_redirects`：**已补齐逐跳计数**，`SearchBackendClient::build` 的 redirect 闭包按 `attempt.previous().len()` 计数，超限返回 `io::ErrorKind::Other`。
2. `dyn` 文档化：**已新建 `docs/archive/dynamic.md`** 并登记网络后端与扩展点派发，确认保留动态派发。
3. 身份 trait：**维持独立**，不为统一 id/kind 造父 trait（见上）。
4. 抓取后端可替换性：**已实现运行时可变**，selected 由 Registry 持有，Router 调用时读取；owner 删除时回退到内置 `http`。
5. DDG 默认后端：**已改为声明式配置**，保留 `duckduckgo_html` 配置预设作为简洁入口，内部由 `HttpSearchBackend` 承载。

## 落地记录

本次改造在同一依赖链上完成以下修改（行号以改造后为准）：

- `max_redirects`：`SearchBackendClient::build` 新增 `max_redirects` 参数并在 redirect 闭包逐跳计数；`WebSearchBackendConfig` 两个变体新增该字段，config 校验加上限；搜索与抓取共用同一 client 构造。
- DDG 声明式化：删除 `DuckDuckGoBackend`；`build_web_backends` 用 `duckduckgo_request` / `duckduckgo_response` 把预设翻译为声明式配置；`resolve_string` 新增 `$offset` 占位符承载分页偏移；`parse_duckduckgo_results` 保留为共享解析器。
- 抓取运行时可变：`FetchBackendRegistry` 持有 `selected: Arc<RwLock<String>>`，新增 `selected()` / `set_selected()`；`WebFetchRouter` 调用时读取当前 selected；`unregister_by_owner` 在被删后端为当前选中时回退到 `http`。
- 扩展点：`WebToolConfig` 新增 `fetch_text_converter` / `fetch_post_processor` 两个可选闭包；`web_fetch_handler` 优先使用注入实现，否则回退默认 `html_to_text`。
- 动态派发：`docs/archive/dynamic.md` 登记网络后端与扩展点派发及必要性。

## 验证

- 结论均基于源码静态引用，行号对应当前 HEAD（`ba572920`）。
- 若后续改造，应对 `wf-tools` / `wf-resource` / `wf-runtime` 相关 crate 执行 `cargo clippy --all-targets --all-features`，并运行 `bash scripts/sync-deploy.sh --check`。
