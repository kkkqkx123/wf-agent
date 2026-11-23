# 动态派发登记

按 `AGENTS.md` 的编码约定，所有动态派发（`dyn`）必须登记于此。本文覆盖 network 能力相关的动态派发；其他模块的既有派发由各自 crate 负责，不在本次范围。

## 为什么 network 需要动态派发

network 能力刻意保留了两类可插拔抽象，因此动态派发是必要而非偷懒：

- 搜索源需要统一到 `WebSearchBackend`：内置声明式后端（DuckDuckGo、Tavily 一类 HTTP API）与插件贡献的后端实现类型不同，却在运行时一起参与聚合、按 id 路由与 owner 生命周期管理。聚合器在调用时读取注册表快照，插件激活/停用必须立即生效，只有统一 trait 对象能满足。
- 抓取后端需要统一到 `WebFetchBackend`：内置 HTTP 后端与插件后端由 `FetchBackendRegistry` 按 owner 管理，路由器在调用时读取当前选中 id，运行时可切换。
- 文本转换与输出后处理是明确留给宿主的扩展点，用轻量闭包注入，避免为单一行为引入带 id/kind 的 trait。

没有动态派发就无法实现"插件在运行时贡献网络后端并即时生效"，也就必须回退到编译期枚举——那会让插件体系失效。因此这里的 `dyn` 有实质必要性。

## 后端契约的动态派发

| 位置 | 派发类型 | 必要性 |
| --- | --- | --- |
| `wf-tools/src/predefined/web/web_search.rs` | `WebToolConfig.backend: Option<Arc<dyn WebSearchBackend>>` | handler 不感知后端具体实现 |
| `wf-tools/src/predefined/web/web_fetch.rs` | `WebToolConfig.fetch_backend: Option<Arc<dyn WebFetchBackend>>` | handler 不感知抓取传输实现 |
| `wf-resource/src/network/registry.rs` | `Arc<dyn WebSearchBackend>` 作为注册表条目、聚合入参、快照元素 | 异构搜索源统一聚合与 owner 注销 |
| `wf-resource/src/network/fetch.rs` | `Arc<dyn WebFetchBackend>` 作为注册表条目、路由目标 | 内置/插件抓取后端统一路由与运行时切换 |
| `wf-resource/src/network.rs` | `Vec<Box<dyn WebSearchBackend>>` 构建期收集后转为 `Arc` | 配置驱动的多种后端统一构造 |
| `wf-runtime/src/bootstrap/network.rs` | `RuntimeResult<Arc<dyn WebSearchBackend>>` | 装配层向工具层返回统一后端 |

具体生产者（无需动态派发即可静态识别）：`HttpSearchBackend`（声明式，承载 DDG 与所有 HTTP 搜索源）、`PluginWebSearchBackend`、`HttpFetchBackend`、`PluginWebFetchBackend`、`WebSearchAggregator`、`WebFetchRouter`、`HarnessSearchBackend` / `HarnessFetchBackend`（测试注入）。

## 扩展点闭包的动态派发

| 位置 | 类型别名 | 用途 |
| --- | --- | --- |
| `wf-tools/src/predefined/web.rs` | `FetchTextConverter = Arc<dyn Fn(&str) -> String + Send + Sync>` | 替换 `web_fetch` 的 HTML 转文本策略 |
| `wf-tools/src/predefined/web.rs` | `FetchPostProcessor = Arc<dyn Fn(Value) -> Value + Send + Sync>` | 对 `web_fetch` 输出包络做后处理 |

二者为可选；未注入时 handler 使用默认 `html_to_text` 且不做后处理。选择闭包而非 trait，是为了不为单一行为引入带 id/kind 的类型抽象——这与"不为统一 id/kind 造 trait"的约定一致。

## 对照：未采用的动态派发

- 未定义统一的"后端身份" trait 让 search 与 fetch 共享。二者返回结构不同（web 搜索结果是带排序的 JSON 对象，抓取是网页正文文本），id/kind 不足以构成统一理由，因此 `WebFetchBackend` 独立声明 id/kind，不继承 `SearchBackend`。
- 未用 `dyn` 表达配置形态。配置是数据（`WebSearchBackendConfig` 枚举），校验与默认值在 `wf-config` 静态完成。

## 压缩引擎动态派发

压缩引擎是需要运行时替换的能力：内建引擎与第三方原生引擎实现同一套决策与规划语义，由配置名在装配期选中一个并绑定到本次执行，运行期不可更换。只有统一 trait 对象能让宿主在不知道具体引擎的情况下驱动三种触发入口，也才能让原生插件在启动时贡献新引擎。

| 位置 | 派发类型 | 必要性 |
| --- | --- | --- |
| `wf-execution-shared/src/compaction/engine.rs` | `Arc<dyn CompactionEngine>` 作为工厂产物与引擎契约 | 宿主按统一契约调用，不感知内建或第三方实现 |
| `wf-execution-shared/src/compaction/engine.rs` | `dyn CompactionEngineFactory`、`&dyn ProgressSink` | 工厂按名构造异构引擎；进度上报不感知消费方 |
| `wf-execution-shared/src/compaction/registry.rs` | `DashMap<String, Arc<dyn CompactionEngineFactory>>` | 进程级按名索引异构引擎工厂，重名即拒绝 |
| `wf-agent/src/coordinator/iteration.rs` | `Arc<dyn CompactionEngine>` 实例字段 | 每次执行解析一次并冻结，杜绝多套压缩逻辑并行 |

具体生产者（无需动态派发即可静态识别）：`BuiltinEngine`（内建，包装原生决策与规划）、插件经原生边界实现的工厂与引擎。`Arc<dyn CompactionEngine>` 只在注册表与实例字段边界出现；引擎返回的写回意图是强类型 `CompactionOutcome`，写回由宿主执行。
