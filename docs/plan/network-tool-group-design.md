# network 工具组与可插拔搜索后端设计方案

## 目标与范围

为 agent 补充 `network` 工具组，第一个工具为 `web_search`。默认搜索后端为 DuckDuckGo HTML 端点（`https://html.duckduckgo.com/html/`），实现放在 `wf-resource`。同一工具需要同时支持四类后端来源：

1. `wf-resource` 中新增的 Rust 内置实现；
2. 通过配置声明的 HTTP 搜索后端（请求头、请求体、响应字段映射）；
3. 插件贡献的搜索后端；
4. 后续的多后端聚合（先做 web search，未来扩展 RSS 等搜索域）。

`web_fetch` 保持现有行为，后续按同一模式接入网络工具组；本次不改写它的传输实现。

## 现状与约束

- `web_search` / `web_fetch` 的定义和处理器目前在 `wf-tools::predefined::web*`，DDG 的 HTTP 请求与 HTML 解析也在该 crate。它没有搜索后端抽象，也没有代理配置。
- 工具定义与 LLM 描述由 `wf-tools::predefined::ToolDefinition` 单一来源生成；`wf-resource` 负责注册定义，`wf-runtime` 通过 `BuiltinHandlersConfig.web` 注入处理器配置。
- 依赖方向为 `wf-tools ← wf-resource ← wf-runtime`。`wf-resource` 不依赖 `wf-plugin`，插件桥接位于 `wf-runtime`。新增后端时不应反转这些依赖。
- 代理设施已存在：`llm-suite-proxy` 提供代理 URL 校验、`no_proxy` 和 reqwest client 构造；配置文件只保存路由，凭据保存在 credential store，通过路由派生的 key 读取并临时组合。
- `tools.toml` 的未知段会启动失败；`deploy/templates/` 是生成物，配置示例修改后需要重新同步部署模板。

## 架构决策

### 命名分工

`network` 是能力组、配置和实现层名称（`tools.toml [network]`、`wf-resource::network`、后端注册表）；`web` 保留为工具自身的语义类别（`web_search` / `web_fetch`）。这样模型看到的分类仍然精确，RSS 等新工具可以用自己的类别，而不需要把所有联网工具都塞进一个过宽的 `network` 类别。需要显式分组展示时用 tags 或 metadata.custom_fields，不复用 `category`。

### 分层而不是单一万能 trait

统一抽象只覆盖"后端身份与生命周期"：id、kind、能力声明。搜索语义按域拆分为 `WebSearchBackend`，未来 RSS 增加 `RssSearchBackend`。这样聚合器也按域拆分：`WebSearchAggregator` 只处理 web 结果，RSS 聚合器只处理条目/时间/订阅源字段，避免把不同结果形状塞进 `serde_json::Value` 或强迫后端实现不需要的方法。

### 契约在 wf-tools，实现在 wf-resource

- `wf-tools` 保留工具定义、参数 schema、handler 和后端 trait/结果类型。handler 只依赖 trait；没有注入后端时明确报告"搜索服务未配置"，而不是内联一个 DDG fallback。
- `wf-resource` 新增 `network` 模块，承载 DDG 实现、声明式 HTTP 后端、后端注册表、web 聚合器以及内置后端清单。新增内置实现只需在该模块增加一个实现和构建分支。
- `wf-config` 负责配置解析、校验与默认值；`wf-runtime` 负责代理凭据解析、client 构造、后端装配和插件桥接。

### 插件复用现有 contribution

第一版不新增插件 SDK 的 contribution 类型或 ABI。插件继续用 `tool` + `tool-type` 贡献：声明一个带 `search-backend:web` 标记的工具及其 executor。运行时桥接把该 executor 包装成 `WebSearchBackend` 注册进后端注册表，停用时按插件 id 注销。联网插件限定 native 后端并要求 `network` permission；其代理由插件自行处理，宿主不向插件传递凭据。

### 声明式后端有边界

支持配置化的请求方法、URL、查询参数、请求头、JSON 请求体、响应 JSON 点路径与字段映射；占位符只支持 `$query` 和 `$max_results`，并做类型化替换。复杂签名、OAuth、非标准分页或解析交给插件。密钥只通过环境变量占位符进入配置，日志与错误不输出请求头和代理凭据。

## 模块划分

- `crates/engine/wf-tools/src/predefined/web/web_search.rs`：后端 trait、查询/结果/错误类型、通用 handler、`backend` 参数。
- `crates/engine/wf-tools/src/predefined/web.rs`：`WebToolConfig` 改为持有注入的后端。
- `crates/engine/wf-resource/src/network.rs` 与 `network/`：DDG 后端、HTTP 声明式后端、注册表、聚合器、构建入口。
- `crates/engine/wf-resource/src/registry.rs`：`ResourceRegistries` 持有共享的后端注册表，插件桥接可直接追加后端。
- `crates/infra/wf-config/src/processor/network.rs`：`tools.toml [network]` 的输入类型、校验和转换；`ToolConfigs` 增加 `network` 段。
- `crates/app/wf-runtime/src/bootstrap/network.rs`：从 credential store 解析每个后端的代理，调用 `wf-resource` 构建后端并注入 handler 配置。
- `crates/app/wf-runtime/src/plugin_bridge/network.rs`：插件搜索后端的注册与注销。

## 后端注册表与聚合

注册表以 id 为键，记录 owner 与 priority。内置后端 owner 为 `builtin`，插件后端 owner 为插件 id。id 冲突直接失败；插件停用只删除自己名下的后端。

web 聚合器从注册表快照读取后端，不缓存固定列表，因此插件在运行时激活/停用能立即生效。参数 `backend` 指定 id 时只调用该后端；未指定时按配置的策略执行：

- `all`：调用全部后端，按 URL 去重，按优先级和 id 稳定排序，部分失败记录到输出元数据，不拖垮成功结果；
- `priority_first_success`：按优先级顺序调用，返回第一个非空成功结果。

聚合器设置总超时与总结果上限；单个后端自己持有按后端配置的超时。默认配置只注册 DDG 一个后端，保持当前输出形状与行为。

## 代理与凭据

每个后端配置独立的 `proxy.url` 和 `proxy.no_proxy`。配置只允许 route-only URL，内嵌 userinfo 在配置校验期失败。运行时用 `credential_key(route)` 从 `AuthRegistry` 的 credential store 读取代理凭据，用 `compose_proxy_url` 得到仅存在于内存的URL，再由 `wf-resource` 用 `llm-suite-proxy` 构造该后端专属的 reqwest client。未配置代理时沿用 reqwest 的默认环境变量行为。凭据不进入注册表、工具参数或日志。

## 配置形态

`tools.toml` 新增 `[network]` 段，包含 web search 的默认后端列表、聚合模式与后端数组。后端通过 `kind` 区分：

- `duckduckgo_html`：端点、UA、超时、结果上限、代理；
- `http`：请求方法与 URL、请求头、查询参数或 JSON 请求体、响应格式（`json` 或 `duckduckgo_html`）、JSON 条目路径与字段映射、代理与超时。

未提供 `[network]` 时生成单一 `duckduckgo` 后端的默认配置。未知 kind、重复 id、非法代理路由、缺失响应映射等错误在配置阶段失败。部署文档和 `config-examples/infrastructure/tools.toml` 同步补充说明，部署模板由同步脚本生成。

## 实施顺序

1. `wf-tools`：抽出后端契约与通用 handler，移除 DDG 请求/解析实现；harness 注入本地 mock 后端；更新 schema 快照。
2. `wf-resource`：实现 DDG 后端、声明式 HTTP 后端、注册表与聚合器、配置到后端的构建入口。
3. `wf-config`：新增 network 处理器、校验、默认配置与 `ToolConfigs` 字段。
4. `wf-runtime`：调整 bootstrap 顺序以取得 credential store；装配后端并注入 `WebToolConfig`；插件桥接注册/注销插件后端。
5. 文档与配置示例：更新工具文档、配置文档、示例和部署模板；补充 fixture 与单元测试。

## 验证

- DDG 解析使用迁移后的 fixture 单测；
- 声明式后端用本地 mock HTTP server 覆盖 GET/POST、查询参数、JSON 请求体与响应映射；
- 聚合器用 fake backend 覆盖去重、部分失败、指定后端、优先级模式和超时；
- 配置测试覆盖未知 kind、重复 id、非法代理路由和默认值；
- 插件桥接测试覆盖激活注册、停用注销和错误结果拒绝；
- 最后统一执行一次完整编译检查与部署模板漂移检查。
