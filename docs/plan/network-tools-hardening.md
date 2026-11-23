# 网络工具问题清单与加固方案

## 背景与范围

本文梳理 `wf-tools` / `wf-resource` 中 web、network 相关工具的现状问题，并给出修改方案与落地状态。范围包括 `web_search`、`web_fetch`、搜索后端注册与聚合、`tools.toml [network]` 配置、代理凭据、审批域名规则以及测试覆盖。REST 执行器和 MCP 属于通用工具执行通道，不纳入本次网络工具组改造。

## 现状功能清单

- `web_search`：模型侧参数为 `query`、`max_results`、`backend`；返回标题、URL、摘要，并附带参与聚合的后端与部分失败信息。
- 搜索后端：内置 DuckDuckGo HTML 解析、声明式 HTTP 搜索（GET/POST、占位符、JSON 字段映射）、native 插件贡献的后端。
- 聚合：注册表按 owner 管理生命周期；支持并发全量与优先级首成功、URL 去重、整次超时。
- `web_fetch`：按 URL 抓取，支持 `text` / `html` 输出，结果走统一落盘策略。
- 配置：`[network.web_search]` 管理后端、模式、代理；凭据在运行时从 credential store 解析，传输错误脱敏。
- 治理：两个工具均标记为 `Network` 风险，受审批策略约束；temporary 会话可用，ephemeral 会话禁用。

## 问题清单

### 抓取工具与 network 能力模型不一致

`web_fetch` 的客户端、超时、UA、体积上限和代理长期硬编码在 `wf-tools` 的 handler 中（`predefined/web/web_fetch.rs`），运行时只注入搜索后端（`bootstrap/tool_registry.rs`）。结果是搜索后端可以按路由配代理，抓取却不能；在当前需要代理访问外网的部署形态下，两者可用性不一致。插件和声明式后端也只能扩展搜索，无法替换抓取传输。

### markdown 输出名不副实

`WEB_FETCH` 默认并推荐 `markdown`，但 handler 只在 `format == "text"` 时剥离标签；默认路径实际把 HTML 原样返回（`predefined/web/web_fetch.rs`）。工具 schema、提示语和实现语义相互矛盾。

### 体积上限发生在完整缓冲之后

`web_fetch` 与声明式搜索后端都先等待完整响应体，再检查 `max_content_bytes` / 2MB 上限。请求总超时只能限制时长，无法限制短时峰值内存；代码注释把它描述为下载防线并不准确。

### 域名审批与真实网络行为错位

审批只在 `extract_domain()` 从参数中取到域名时执行 allow/deny（`approval/network_policy.rs`、`approval.rs`）：

- `web_search` 参数没有域名键，域名列表对它完全不生效；
- `web_fetch` 只校验调用参数的 `url`，reqwest 默认跟随重定向，允许域名可跳转到未允许域名；
- 搜索引擎端点与被搜索站点是两类域名，简单套用同一份 allowlist 会误伤搜索后端。

### default_backends 不约束显式选择

`NetworkRegistry::selected()` 对显式 `backend` 直接查全注册表（`network/registry.rs`），不经过 `default_backends`。若配置被视为后端白名单，模型显式传任意已注册后端即可绕过。

### 插件执行器绑定不明确

插件桥接从插件贡献的所有 tool-type 中取最后一个 executor（`plugin_bridge/network.rs`），而不是与被标记工具显式绑定。一个插件贡献多个 tool-type 时，搜索后端使用哪一个取决于遍历顺序。

### 后端 metadata 可覆盖结果包络

`web_search` handler 把后端 metadata 直接展开到输出根对象（`predefined/web/web_search.rs`）。错误后端可以把 `query`、`results`、`total` 等字段替换掉，破坏工具输出的稳定结构。

### 搜索能力与健壮性缺口

DDG 解析器依赖逐行字符串匹配，页面改版可能静默返回空结果。搜索没有重试、缓存、限流、语言、地区、时间范围和分页参数；`backend` 是自由字符串且没有发现接口。按后端维度的成功率与延迟也没有独立指标。

### 测试缺口

搜索解析和声明式后端有固件与本地 mock server 覆盖，抓取没有等价测试；`--tier=network` 只跑 `web_search`。超大响应、重定向域名变化、非 UTF-8 页面、HTML 实体等路径缺少回归测试。

## 修改方案

### 统一共享网络原语

在 `wf-tools` 增加公开的网络模块，集中承载代理路由、域名规则、主机归一化和传输错误脱敏。审批模块与具体传输共用同一套匹配逻辑，避免两处规则漂移。

### 抓取配置进入 tools.toml

新增 `[network.web_fetch]`，包含超时、流式体积上限、UA、最大重定向次数与 route-only 代理。运行时复用搜索后端的凭据解析流程，把有效配置注入 `WebToolConfig`。`max_content_bytes`、`max_redirects`、代理格式与搜索后端共用校验规则，启动阶段失败而不是运行时降级。

### 修正抓取语义与传输

- 删除名不副实的 `markdown`，只保留 `text` 与 `html`；`text` 移除脚本、样式和标签，解码常见命名与数值实体，并在块级标签处补空格。
- 用响应流逐块累加并提前拒绝超限响应；返回 `final_url`，让重定向结果可见。
- 用自定义重定向策略逐跳检查域名和跳数，初始 URL 也要先校验。
- 搜索后端的 HTTP 客户端同样应用逐跳域名策略和流式体积上限。

### 域名策略分层

审批配置中的 `denied_domains` 始终下推到内置传输；`allowed_domains` 仅在 `always_allow_network = true` 时约束抓取目标，否则仍由人工审批判断。搜索后端只接收拒绝规则，因为搜索引擎端点不是被搜索站点。插件自行承担网络实现，仍以 native 后端与 `network` 权限为硬性前提。

### 收紧后端选择与插件契约

`default_backends` 明确为允许列表：省略 `backend` 时按列表顺序选择，显式选择也不得超出列表。插件贡献搜索后端时要求恰好一个 tool-type executor，歧义配置直接让激活失败。后端 metadata 不再覆盖 `query`、`results`、`total` 等保留字段。

### 抓取后端契约化与插件化

在 `wf-tools` 定义 `WebFetchBackend`，handler 只做文本整形和落盘。声明式配置构造内置 HTTP 后端，放入独立的 `FetchBackendRegistry`；`[network.web_fetch].backend` 选择内置 `http` 或 native 插件后端。`fetch-backend:web` 标签把插件 executor 适配为抓取后端，生命周期与搜索后端一致。代理、域名、体积与重定向规则作用于内置 HTTP 后端；插件自行负责传输安全，仍受 native 和 `network` 权限门槛约束。

### 搜索审批域名解析

工具定义通过 `network_domains` 元数据携带运行时解析出的搜索后端 endpoint。审批请求新增 `resolved_domains`，由 agent 与 workflow 两条审批路径从注册表读取并透传。审批协调器把它与参数中提取的 URL 合并：拒绝规则命中任一域名就拒绝，允许列表存在时要求所有域名都在列表内，无法解析任何域名仍转人工。传输层逐跳校验继续兜底。

### 搜索能力扩展

搜索工具新增 `language`、`region`、`freshness` 与 1 基 `page` 参数；声明式 HTTP 后端增加对应占位符，DuckDuckGo 后端映射到 `kl`、`df` 与结果偏移。聚合器增加单后端重试、结果缓存与调用间隔下限，全部通过 `[network.web_search]` 配置并在启动阶段校验上限。新增只读工具 `web_search_backends` 暴露后端 id、实现类别与能力，让模型能够发现可选项。

### 按后端指标

在共享指标注册器中增加以 `backend` 为标签的调用次数、耗时和错误计数。聚合器每次后端尝试都记录一次结果，包括失败；低基数标签只包含已注册后端 id。工具输出中的 `backend_errors` 继续保留，作为单调用的细粒度诊断信息。

## 已落地修改

- 新增 `wf-tools` 网络原语模块；审批主机匹配改为复用该模块。
- `web_fetch` 参数与实现收敛为 `text` / `html`；文本转换补齐脚本、样式、实体和空白处理。
- 抓取与搜索响应体均改为流式读取并在接收阶段执行体积上限。
- 抓取支持 `[network.web_fetch]` 代理、超时、UA 与重定向上限；代理凭据沿用运行时解析与错误脱敏。
- 抓取与搜索 HTTP 客户端均逐跳校验域名；搜索后端只取拒绝规则。
- `default_backends` 对显式 `backend` 生效；插件搜索后端要求唯一 executor；结果包络字段受到保护。
- 同步更新配置示例、部署模板、配置文档、工具文档与 schema 快照。

## 保留项

- DDG HTML 解析仍保留为默认内置后端。它依赖页面结构，存在改版风险；固件门禁保留，生产部署可通过声明式 HTTP 后端或插件后端替换。是否将其从默认配置移除属于产品决策，不在本次技术修补范围内。
- 告警投递仍由抓取方负责，进程内只暴露指标，不引入通知通道依赖。
- 搜索插件的 endpoint 声明仍为空：插件 executor 自行发起网络请求，宿主无法在不破坏插件边界的前提下枚举其目标域名。插件的域名合规依赖 native 权限门槛与插件自身实现。

## 验证

- 对相关 crate 执行带测试目标的编译检查。
- 对网络工具相关 crate 运行单元测试，覆盖域名策略、抓取文本转换、配置校验、后端选择与聚合。
- 检查改动文件的 Rust 格式，并执行部署模板漂移检查。
