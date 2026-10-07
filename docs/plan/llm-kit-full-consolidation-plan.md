# wf-llm 全量整合进 llm-kit 方案

## 背景与目标

`llm-tool-call` 整合完成后,`wf-llm` 仍保留约 11000 行实现。经逐模块分析,
其中绝大部分属于通用 LLM 基础设施,与具体 agent 引擎无关,对任何
AI agent 与 RAG 项目都有复用价值,且在本仓库内已被多个上层 crate
复用。本文给出剩余模块的整合方案:新建四个共享 crate,
`wf-llm` 最终收敛为薄门面(网关编排 + 插件适配 + 兼容重导出)。

## 现状分析

### 模块耦合与复用现状

| 模块 | 规模 | 外部依赖 | 已有复用方 | 结论 |
|---|---|---|---|---|
| `token/estimation` | 343 行 | 零依赖(纯文本数学) | engine/app 多处 sizing、压缩、ledger | 可直接移入共享包,价值最高 |
| `token/count` | 267 行 | `wf-types` 消息/请求类型 | `wf-execution-shared`、`wf-workflow`、`wf-agent`、`wf-tools`、`wf-runtime` 广泛调用 | 随 estimation 一起移入 |
| `messaging/message_builder`、`history_converter`、`history_text`、`helper`、`boundary` | 约 480 行 | `wf-types` 消息类型,纯函数 | `wf-execution-shared` 的 chat、会话、prompt 组装 | 纯消息变换,移入共享包 |
| `dead_loop_detector` | 362 行 | 仅 `wf-types` 检测配置,纯字符串分析 | `wf-workflow` 的 llm 配置引用 | 流式护栏,随 stream  transport 一起移入 |
| `error` | 220 行 | reqwest、serde_json、`wf-types` 的 `LlmFormat`(一个变体) | `wf-workflow`、`wf-execution-shared` 做错误分类与重试判断 | 可复用的分类逻辑(is_retryable、context-length 识别),移入 |
| `generation`(resolve/parse/emit/validate) | 约 1350 行 | `wf-types` profile/request/generation 类型,纯变换 | `wf-api`、`wf-workflow`、`wf-execution-shared`、`wf-config` 引用参数模型 | 类型化参数到各 provider 请求体的映射,任何项目都需重做,移入 |
| `codecs`(4 内置协议 + shared + helpers) | 约 3300 行 | error、generation、tool 协议、history_converter、`wf-types` | 当前仅 `wf-llm` 内部使用,但这是跨项目复用价值最高的部分(多 provider 对接) | 移入,与生成参数、tool-call 解析同包,内部引用转为包内依赖 |
| `codecs/plugin` | 227 行 | `wf-plugin-sdk` | `wf-runtime` 的插件桥 | 绑定本项目插件体系,永久保留在 `wf-llm` |
| `registry` | 189 行 | codecs、error、`wf-types` 的 `LlmFormat` | 内部使用 | 随 codecs 一起移入 |
| `messaging/stream` | 530 行 | codec trait、dead_loop、error、`wf-types` 流事件 | `wf-api` 使用 `MessageStream` trait | SSE 累加 + 取消 + 护栏,与 codec 同层,移入 |
| `token/stream` | 176 行 | `wf-metrics` 的具体采集器 | 内部使用 | 需先解耦采集器(见关键决策),再移入 |
| `client` | 484 行 | codecs、stream、`wf-common` 重试/超时、`token::count` 回退 | 广泛用于测试与单发调用 | transport 层,移入;重试改用共享实现(见关键决策) |
| `mock`、`http_mock` | 约 690 行 | client trait、`wf-types`；http_mock 零项目依赖(裸 TCP) | 全仓库 engine/app 测试大量使用 | 随 client 移入并保持 `mock` 特性门控 |
| `config`(profile/provider/catalog) | 约 830 行 | dashmap、`wf-types`；catalog 持有 reqwest 做模型发现 | `ProfileManager` 被 `wf-workflow` 校验引用,`ModelCatalog` 被 `wf-api` 引用 | 纯注册表 + 模型发现,移入 |
| `gateway`(+merge) | 约 730 行 | client、config、registry、mock、`wf-metrics` | `wf-workflow`、`wf-execution-shared`、`wf-agent` 的统一入口 | 编码本项目主张(强制 profile、无回退分支、指标接线、mock 注入),保留为门面 |

### 核心发现

1. `wf-llm` 内部不存在 engine/app 层依赖,是干净的 infra 层,不存在自举难题。
2. 重力井是 `wf_types::llm` 与 `wf_types::message` 类型(Profile、Request、
结果、流事件、生成参数、provider 定义、富消息模型)。这是决定整合
方式的关键约束,不是各模块的业务逻辑。
3. `codecs` 已深度依赖待抽离的模块(generation、tool 协议、history 转换),
四者必须同包或按依赖方向分包,不能拆散到无依赖关系的包中。
4. 两个非 `wf-types` 耦合点必须先解开:`wf-metrics` 具体采集器
(`gateway`、`token/stream`)与 `wf-common` 重试/超时(`client`)。

## 新 crate 设计

在 `crates/infra/llm-kit/` 下新增四个 crate,延续已有命名与分组约束:

```
llm-types (leaf, 已有)
  │
  ├── llm-token ────── 纯 sizing: estimator + message/request/tool/image 计数
  ├── llm-message ──── 纯消息变换: builder + history 转换/文本化 + helper + boundary 常量
  ├── llm-codec ────── 协议层: error + LlmCodec trait + 4 内置 codec + shared/helpers
  │                     + generation + registry + (dead_loop 备选,见下)
  ├── llm-client ───── transport 层: LlmClient trait + 实现 + stream 累加 + dead_loop
  │                     + 用量记录(泛型 sink)+ mock/http_mock(特性门控)
  └── llm-config ───── 配置层: profile/provider/catalog 注册表 + merge_request
```

依赖关系:

```
llm-token (leaf, 仅 wf-types,estimation 部分零依赖)
llm-message (仅 wf-types)
llm-codec ──→ llm-tool-call(xml) + llm-token + llm-message + wf-types
llm-client ──→ llm-codec + llm-token + reqwest/tokio + (重试见决策)
llm-config ──→ wf-types (+ reqwest,模型发现)
wf-llm ──→ 全部 kit 包 + wf-metrics + wf-plugin-sdk(门面层)
```

`dead_loop_detector` 归属二选一:随 stream 累加的消费者进 `llm-client`
(推荐,检测器只被累加器使用);或进 `llm-codec`。实施时按消费者位置确定,
不单独成包。

## 关键决策

### 错误类型

`LlmError`(transport/协议细节:HTTP、超时、认证、profile 缺失、codec 缺失、
上下文超限分类)与 `llm-types::Error`(跨项目边界:无效请求、provider 错误、
解码失败、取消)并存,职责不同,不强行合并。`LlmError` 随 `llm-codec`
移入,由 `wf-llm` 重导出;两者在跨项目边界处的映射关系留待独立仓库阶段收敛。

### 指标解耦

`gateway` 与 `token/stream` 对 `TokenMetricsCollector` 的具体依赖改为
`llm-client` 内定义的用量记录 trait(方法接收 prompt/completion token 数、
费用与模型名)。`wf-metrics` 在自身侧提供适配实现,`wf-llm` 门面负责接线。
如此 `llm-kit` 全组不再依赖 `wf-metrics`,保持叶子可迁移性。

### 重试解耦

`client` 当前使用 `wf-common` 的重试与超时执行器。`llm-chat-basic` 已有
精简重试策略,建议 `llm-client` 复用它,使 transport 层的重试收敛到一处。
两者能力差(观测钩子、抖动)作为后续补齐项,不阻塞搬迁;搬迁期间允许
暂时保留 `wf-common` 依赖并在收敛后移除。

### 类型策略

分两步走。第一步(本方案范围):搬迁保持 `wf-types` 依赖,与 `llm-tool-call`
的 xml 特性先例一致,只解决代码归属问题。第二步(独立仓库阶段):
把 codec/client 真正需要的类型(`LlmProfile`、`LlmRequest`、结果、流事件、
生成参数、provider 定义、富消息模型)上移到 `llm-types`。注意 `llm-types`
现有 `Message` 是最小模型,承载不了 codec 需要的富内容(思考块、工具调用、
图片引用)与信封字段,上移意味着显著扩展 `llm-types`,需单独设计,不在本
方案内展开。

### `wf-llm` 的终态

保留三样东西:一是 `LlmGateway`(强制 profile、无回退、mock 路由、指标接线
等本项目主张);二是 `codecs/plugin`(绑定 `wf-plugin-sdk`);三是全部重导出,
保证现有调用方零改动。不新增 `llm-gateway` 包:网关是项目级门面,其他项目
可直接组合 kit 包自建门面,这与设计文档中 engine 层保留的原则一致。

## 实施阶段

### 第一期:纯叶子(无耦合争议)

新建 `llm-token`(estimator + count)与 `llm-message`(builder、history 转换与
文本化、helper、boundary),`wf-llm` 对应模块改为重导出。离线单测随源码一起
搬迁,在新包内运行。

### 第二期:协议层

新建 `llm-codec`,移入 error、codec trait、四个内置 codec、shared/helpers、
generation 全组、registry。codec 内部的 `crate::tool` 与 `crate::generation`
引用分别转为对 `llm-tool-call` 与包内模块的直接依赖。`wf-llm` 的 codec 相关
重导出保持不变,外部调用方无感知。

### 第三期:transport 与配置层

新建 `llm-client`(client、stream 累加、dead_loop、泛型用量记录、mock 特性)
与 `llm-config`(profile、provider、catalog、merge_request),先完成指标与重试
两处解耦,再搬迁。`wf-llm` 收敛为网关门面加插件适配加兼容重导出。

每期独立可验证,不跨期预埋。

## 约束

- 遵循仓库 AGENTS.md:严格 DAG(kit 包只依赖 kit 内包与 foundation 层)、
workspace 集中依赖、禁用 `mod.rs`、代码注释全英文、版本不单独升级。
- 搬迁只改模块归属,不顺手重构行为;行为变更单独立项。
- 对外路径(`wf_llm::` 下全部现有路径)每期保持可用,调用方零改动。

## 验证

- 每期一次全量编译检查:`cargo clippy --all-targets --all-features`。
- 每期一次 scoped 测试:`cargo test -p <新建包> -p wf-llm --all-features`,
搬迁的单测在新包内通过,网关与 mock 端到端测试在 `wf-llm` 内通过。
- 第三期完成后追加一次 engine/app 层调用方回归(`wf-agent`、`wf-workflow`、
`wf-execution-shared` 相关用例),确认门面行为无变化。
- 验证合并到最少轮次,不逐模块单独编译。
