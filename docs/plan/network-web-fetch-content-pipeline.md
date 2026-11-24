# web_fetch 内容管线改进设计

## 背景

当前 `web_fetch` 由 `wf-tools` 定义工具与文本整形，`wf-resource` 提供可替换的 HTTP/插件抓取后端。传输层已经具备流式体积上限、代理、逐跳域名校验与重定向上限，但内容层仍使用正则剥离标签，输出是丢失结构的单段文本；响应包络没有状态码、页面标题、链接和元信息，也不支持正文筛选、条件请求缓存或动态页面插件参数。

参考 `ref/crawl4ai` 的分层：抓取 → HTML 清洗与结构化抽取 → Markdown 生成 → 内容过滤 → 结果对象。本设计借鉴该管线，但不引入 Python/Playwright 依赖，也不把 crawl4ai 的全部结果字段直接暴露给模型。

## 目标

- 用 HTML5 DOM 解析替换正则文本抽取，保留标题、列表、代码、表格、链接和图片的语义。
- 工具默认输出 Markdown，并保留 `text`、`html` 两种格式；`text` 保留行结构但不含 Markdown 标记。
- 返回紧凑且可追溯的结果包络：原始 URL、最终 URL、状态码、标题、正文、元信息、受限链接列表、截断信息。
- 支持 `selector`、`max_chars`、`render`、`wait_for_ms` 参数；内置 HTTP 后端显式拒绝渲染参数，浏览器渲染交给已有插件后端。
- 传输层按 HTTP charset、HTML meta charset、BOM、UTF-8 的顺序解码；保留少量必要响应头。
- 增加按 URL 的条件请求缓存、可选 robots.txt 校验与按域名最小请求间隔。
- 保持 `wf-tools` / `wf-resource` / `wf-runtime` 的既有依赖方向，不新建替代模块，不保留旧字段或旧格式别名。

## 非目标

- 不内置 Chromium/Playwright，也不实现通用 CSS 选择器引擎；`selector` 只支持标签、`#id`、`.class` 及其简单组合。
- 不把完整 HTML、全部链接或全部元数据塞进模型上下文；长内容继续走统一落盘。
- 不实现批量调度、LLM 抽取策略和站点登录态。
- 不引入数据库持久化缓存；缓存是进程内、有容量上限的临时状态。

## 实现任务

### 任务一：DOM 页面抽取

在现有 `wf-tools/src/web_text.rs` 中引入 HTML5 tokenizer/tree builder，以节点槽位构成 DOM。新增页面抽取结果，包含标题、meta 元信息、绝对化链接列表、Markdown 正文与纯文本正文。renderer 负责：

- 跳过 script、style、noscript、svg、nav、footer、header、aside、form 等样板节点；
- 将标题、列表、引用、代码块、表格渲染为可读 Markdown，纯文本模式去掉标记但保留换行；
- 链接与图片基于最终 URL 解析为绝对地址，忽略 data URL；
- 支持选择器限定正文范围与 `max_chars` 预算裁剪。
- `html_to_text` 继续服务搜索摘要，改为基于同一 DOM 的紧凑文本，避免两套解析规则漂移。

### 任务二：工具契约

更新 `web_fetch` 定义与 handler：

- `format` 取 `markdown`（默认）、`text`、`html`；
- 新增可选 `selector`、`max_chars`、`render`、`wait_for_ms`；
- `WebFetchQuery` 携带这些调用选项，`WebFetchResponse` 携带状态码与筛选后的响应头；
- 输出包络保留字段由 handler 最后写回，宿主后处理器不得覆盖；
- 新增不支持选项、非法选择器等错误类型，避免参数被静默忽略。

### 任务三：传输与治理

改造内置 HTTP 后端：

- 按响应头 charset、HTML meta charset、BOM、UTF-8 顺序解码，不再无条件使用有损 UTF-8 转换；
- 将非 2xx、媒体类型不支持等失败落成结构化错误；
- 以 URL 为键保存少量响应头、实体标签与最后修改时间，在有效期内发送条件请求，收到 304 时复用缓存正文；
- 按域名限制最小请求间隔；可选读取并匹配 robots.txt 的 Disallow 规则；
- 所有新开关通过 `[network.web_fetch]` 配置并在启动时校验。

### 任务四：装配与周边

同步更新 `wf-config` 配置结构、运行时 bootstrap、插件桥接参数、部署模板、配置文档、工具 schema 快照与网络夹具。插件桥接把调用选项传给插件工具，并从插件结果读取状态码与响应头；内置后端不支持的选项直接失败。

### 任务五：验证

以本地 mock server 与单元测试覆盖：多编码页面、malformed HTML、实体、表格/列表/相对链接、样板剪枝、选择器与字符预算、条件请求缓存、robots.txt、域名限速、重定向域名拒绝、体积上限、超长内容落盘。编译与测试合并为少量验证批次。
