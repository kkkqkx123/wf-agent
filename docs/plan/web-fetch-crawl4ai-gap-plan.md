# web_fetch 与 crawl4ai 差异收敛设计

## 背景与目标

对照 `ref/crawl4ai` 的实现，`web_fetch` 的基础抓取已经完备，但内容筛选、渲染能力契约、结果结构和传输治理仍有明显差距。本设计把差异拆成可落地的改造项，保持当前分层与依赖方向不变：

- `wf-tools`：工具契约、HTML 内容管线、输出包络。
- `wf-resource`：HTTP 传输、后端注册与路由。
- `wf-config` / `wf-runtime`：配置、装配、插件桥接与指标。

改造遵循现有约束：不引入浏览器运行时，不新增“版本 2”模块，不保留旧字段别名，不把 crawl4ai 的全量结果对象直接暴露给模型。

## 差异结论

| 维度 | crawl4ai | 改造前现状 | 判断 |
|---|---|---|---|
| 抓取编排 | `AsyncWebCrawler.arun` 承担缓存、重试、代理、反爬与后处理 | 单页 handler + 可替换后端 | 保持单页定位，补齐局部治理 |
| HTML 清洗 | `LXMLWebScrapingStrategy` 支持排除标签/选择器、属性清理、媒体与表格抽取 | 固定跳过标签 + 单一渲染器 | 需增加可配置筛选与结构化摘要 |
| Markdown | raw/citations/references/fit 多种输出 | 单一内联链接 Markdown | 需增加引用模式 |
| 内容过滤 | Pruning / BM25 / LLM 三类 Filter | 仅 `selector` 与 `max_chars` 截断 | 需增加轻量密度与焦点筛选，不引入 LLM |
| 抽取/分块 | extraction/chunking 策略族 | fetch 内没有；项目另有 `rag-core` | 保持在 fetch 之外，需要时桥接 RAG |
| 结果对象 | `CrawlResult` 字段完整但体积大 | 紧凑包络，缺诊断字段 | 只补充低基数诊断与结构化摘要 |
| 渲染 | Playwright 与 HTTP 两种策略 | 仅 HTTP，render 由插件承诺但无能力校验 | 不内置浏览器，改能力发现与契约校验 |
| 缓存/robots | 持久化缓存、标准 robots、代理轮换、反爬重试 | 进程内条件缓存、简化 robots、固定限速 | 补齐 robots 语义、重试、抖动、压缩与指标 |

现有实现中应当保留的优势：流式体积上限、逐跳域名校验、插件后端 owner 生命周期、确定性编码链、搜索摘要与抓取正文共用 DOM 规则、结果包络防覆盖。

## 已实施改造

### 内容管线

在 `crates/engine/wf-tools/src/web_text.rs` 中扩展同一套 HTML5 DOM 管线：

- 遍历改为文档顺序，修复 selector 命中和链接列表兄弟节点逆序的问题。
- 在清洗前提取 `<base href>`，并以它优先解析相对链接。
- 新增 `link_mode`：内联链接、编号引用、纯文本。引用模式生成紧凑 References，避免重复 URL 占用上下文。
- 新增 `exclude_selector`、`focus`、`min_words`：按文本密度、链接密度、标签权重、class/id 正负信号筛掉低价值块；显式 `focus` 无命中时直接丢弃该块。
- 新增重复块去重与隐藏样式识别，覆盖 `display: none`、`visibility: hidden`。
- 新增 `clean_html` 渲染：输出保留结构属性、移除脚本与隐藏节点的 HTML。
- 抽取结构化表格摘要，包含表头、行数、前若干行与 caption。
- 元数据补齐 og/twitter 标题回退；链接按 URL 去重。

### 工具契约

在 `web_fetch` 定义与 handler 中：

- `format` 扩展为 `markdown`、`text`、`clean_html`、`html`。
- 新增 `backend`、`exclude_selector`、`focus`、`min_words`、`link_mode`、`refresh`。
- 输出包络新增 `backend`、`backend_kind`、`cache_status`、`elapsed_ms`、`response_headers`、`tables`。
- 保留字段清单同步扩展，宿主后处理器不能覆盖这些诊断字段。
- 新增 `web_fetch_backends` 发现工具，暴露后端 id、kind 和能力列表。

### 后端与插件契约

- `WebFetchQuery.backend` 由路由器解析；`allowed_backends` 作为每次调用的后端白名单，启动时校验配置后端 id。
- `WebFetchBackend.catalog()` 统一暴露能力；内置 HTTP 后端列出其实际支持的筛选与缓存能力，路由器汇总注册表中所有允许后端。
- 插件桥接不再无条件声明 `options_applied = true`：插件必须通过布尔值或逐项对象声明实际处理过的 `render`、`wait_for_ms`、`selector`、`exclude_selector`、`focus`、`min_words`、`refresh`；请求了但未声明的项直接失败。
- 插件可返回 `title`、`metadata`、`links`、`tables`、`response_headers`、`cache_status` 与 `elapsed_ms`，handler 统一合并到包络。
- 浏览器渲染仍由插件承担，核心不依赖 Playwright。

### 传输与治理

- reqwest 启用 gzip/brotli；体积上限继续按解压后的字节流执行。
- 共享 HTTP 客户端补充 `Accept`、`Accept-Language` 默认头与独立 connect timeout。
- 内置后端增加有界重试：网络错误、429 与 5xx 才重试，尊重 `Retry-After`，使用指数退避与抖动。
- 增加封禁页特征检测，命中后返回结构化 `Blocked` 错误，避免把验证页当成正文。
- robots 解析改为标准分组语义：UA 组、Allow/Disallow 最长匹配、通配符、`$` 结束锚点、Crawl-delay；限速取配置间隔与 Crawl-delay 的较大值。
- 缓存增加 `refresh` 绕过、`Vary` 保守校验、`hit_validated` / `hit_fallback` 状态。
- 按后端记录调用次数、耗时与成功/失败的指标，与搜索后端指标口径一致。

## 明确不做

- 不在核心引入 Chromium/Playwright、session、截图、PDF、MHTML、console/network 日志。
- 不在 `web_fetch` 内实现 LLM 内容过滤、LLM 抽取或结构化 schema 抽取。
- 不在单页工具内实现批量调度、深爬、URL seeding 或自适应遍历。
- 不引入持久化数据库缓存；缓存仍为进程内、有容量上限的临时状态。
- 不直接暴露 crawl4ai 风格的完整结果字段，继续走长内容落盘与紧凑包络。

## 涉及文件

- `crates/engine/wf-tools/src/web_text.rs`：DOM 筛选、链接模式、clean HTML、表格摘要。
- `crates/engine/wf-tools/src/predefined/web/web_fetch.rs`：契约、handler、发现工具。
- `crates/engine/wf-tools/src/predefined/web.rs`、`crates/engine/wf-tools/src/lib.rs`：导出与工具清单。
- `crates/engine/wf-resource/src/network/fetch.rs`：路由、重试、robots、缓存、指标、封禁检测。
- `crates/engine/wf-resource/src/network/backend.rs`、`crates/engine/wf-resource/src/network.rs`：共享客户端与装配。
- `crates/infra/wf-config/src/processor/network.rs`：新增配置与校验。
- `crates/app/wf-runtime/src/bootstrap/`：指标注入与后端选择。
- `crates/app/wf-runtime/src/plugin_bridge/network.rs`：插件能力声明与结果字段。
- `Cargo.toml`：reqwest 压缩特性。
- `config-examples/infrastructure/tools.toml`、`docs/config/infrastructure.md`、schema 快照与夹具：同步契约。

## 验证

- 对 `wf-tools`、`wf-resource`、`wf-config`、`wf-runtime` 执行带测试目标的编译检查。
- 单元测试覆盖：base href、文档顺序、focus/exclude 剪枝、引用模式、clean HTML、表格摘要、robots Allow/最长匹配/Crawl-delay、封禁特征、条件缓存状态、后端选择与白名单、插件选项声明。
- 夹具更新 `web_fetch` 的 clean_html 用例与 `web_fetch_backends` 发现用例，并重新生成 schema 快照。
- 执行部署模板漂移检查，确认 `deploy/templates/` 与配置源一致。

## 后续迭代

- 若真实站点仍大量触发封禁，可在插件侧增加 UA 池、代理轮换与 sticky session；核心只消费能力声明，不内嵌策略。
- 若需要把抓取结果沉淀为知识库，使用单独的 RAG 设计方案，在宿主层桥接 `rag-core`，不在 fetch 内复制分块与索引逻辑。
- SSRF 的 DNS rebinding 防护需要 resolve-and-pin 出网代理，属于部署层能力，单独评估。
