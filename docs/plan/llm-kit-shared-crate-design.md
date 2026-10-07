# LLM / Embedding 跨项目共享包设计方案

## 背景与动机

对当前项目 wf-agent、`~/code/code-context-engine`、`~/code/linkrs` 三个项目的 LLM 与 embedding 实现进行了对比分析,结论如下:

| 能力 | wf-agent | code-context-engine | linkrs |
|---|---|---|---|
| Chat 完成度 | 4 协议 codec + 流式 + tool-call | OpenAI-compat 基础 chat | 无 |
| Tool-call 协议 | 深度支持(流式 XML 解析 + partial JSON) | 无 | 无 |
| Embedding | 无 | OpenAI-compat provider | OpenAI/Gemini/Azure/Ollama(含预处理器) |
| Rerank | 无 | Cohere + Generative(chat 模拟) | 无 |

核心结论:

1. **embedding 重复**:cce 与 linkrs 各有一份 OpenAI-compat HTTP `/embeddings` 调用,linkrs 是功能超集(多 provider 分支 + Preprocessor)。
2. **chat 在不同层,不应共用**:wf-llm 的 chat 是多协议 codec + 流式 + tool-call 的 **agent 引擎层**;cce 的是纯 OpenAI-compat 请求/响应的**基础 HTTP 层**,二者不在同一抽象级别,不应合并。各项目按需使用各自的层,仅 `llm-types` 对齐数据模型。
3. **rerank 是 cce 独有的新增能力**:两份实现——Cohere 专用 API 调用 + Generative(通过 chat prompt 模拟 rerank),wf-agent 和 linkrs 目前无 rerank 需求。

## 共享包目录结构

共享包一律落在 `crates/infra/llm-kit/` 下(与 `infra/checkpoint/` 分组一致),内部 crate 不带 `wf-` 前缀,第三期上移独立共享仓库时保留名称不变。

```
crates/infra/llm-kit/
├── llm-types          # 共享数据模型(叶子包,零运行时依赖)
├── llm-embedding      # embedding provider trait + OpenAI-compat HTTP 实现
├── llm-chat-basic     # OpenAI-compat 基础 chat HTTP 客户端(无协议 codec)
├── llm-tool-call      # 流式 tool-call 协议解析(条件编译)
└── llm-rerank         # rerank: Cohere + Generative provider + 分数融合策略
```

各 crate 的依赖关系:

```
llm-types (leaf)
  │
  ├── llm-embedding ──── reqwest + serde
  ├── llm-chat-basic ─── reqwest + serde + (streaming feature → tokio/eventsource)
  ├── llm-tool-call ──── serde_json + [xml feature → wf-types/wf-common (phase1→3 过渡)]
  └── llm-rerank ─────── reqwest + serde + [cohere feature → reqwest]
```

## 各 crate 的详细说明

### llm-types (已实现 ✅)

- 内容:`Message`/`MessageContent`/`MessageRole`/`ToolCall`/`ToolCallDelta`、统一 `Error`
- 设计原则:纯数据模型,无外部依赖(仅 serde/thiserror)
- 作业:leaf crate,全项目共用

### llm-embedding (已实现 ✅,第二期已补齐 ✅)

- 内容:`EmbeddingProvider` trait、`OpenAICompatibleProvider`、`EmbeddingService`(批量分块)、`EmbeddingConfig`/`EmbeddingResult`、`PreprocessorConfig`/`PreprocessorImpl`(Nomic/Stella 前缀)
- **无条件编译**:embedding 的 HTTP 调用 + provider 分支复杂度有限,不值得 feature gate
- 已从 linkrs 补齐:`PreprocessorImpl`(Nomic 4 种 task-type + Stella 4 种 task-type)、可选 api_key(适配 Ollama 等本地服务)、`dimension` 启动校验、index 顺序重组与维度校验、OpenAI/Azure/Gemini/Ollama endpoint 预设
- linkrs 并无传输层 provider 分支(单一 OpenAI-compat 实现覆盖四方),故共享包同样以单一传输 + endpoint 预设实现多 provider 支持;token 预算分块未移植(需要 tokenizer,会耦合叶子包)

### llm-chat-basic (已实现 ✅)

**对比结论**:cce-llm-client 的 chat handler(~100 行)是一个简单的 OpenAI-compat HTTP `/chat/completions` 调用,不带流式、不带多协议 codec、不带 tool-call 协议。wf-llm 的 chat(codec 层)远比它复杂,二者不在同一抽象级。

第一期未创建,第二期已落地,内容如下:

- 从 cce-llm-client 移植:`services/chat/handler.rs` → `client.rs`(`BasicChatClient` + `LlmChat` trait)、`services/request_builder.rs` → `request_builder.rs`、`core/config.rs`(ChatConfig 部分) → `config.rs`
- 消息类型复用 `llm-types::Message`(含 Tool 角色映射);`ChatResult` 保留 prompt/completion/total 三段 token 口径
- feature 门控:
  - `streaming` → SSE 增量解析(`stream.rs`,tokio + eventsource-stream;多协议 codec 流式仍保留在 wf-llm)
  - `retry` → 精简指数退避 `RetryPolicy`(暂无 jitter/观测钩子)
  - `ratelimit` → 精简 token-bucket `RateLimiter`(仅主动限流,被动 429 归 retry)
- default 全关,仅基础 chat

### llm-tool-call (已实现 ✅)

- **feature 条件编译**:

| feature | 内容 | 源文件来源 |
|---|---|---|
| `json-partial` | 流式 partial JSON 参数恢复 | wf-llm `partial_json_parser.rs` |
| `xml` | XML tool-call 解析 + 协议渲染 | wf-llm `tool/parser.rs` + `tool/protocol.rs` |

- default 全关,调用方按需启用
- 第一期:已从 wf-llm 复制源文件并以 workspace 依赖引用 wf-types/wf-common,第三期上移独立仓库时解耦

### llm-rerank (已实现 ✅)

**Rerank 的实现特征分析**:

cce 的 rerank 有两类实现,传递方式不同:

| Provider | 传输 | 依赖 |
|---|---|---|
| `CohereRerankProvider` | HTTP POST → 专用 `/rerank` 端点 | reqwest |
| `GenerativeRerankProvider` | 通过 OpenAI-compat chat 发送 prompt → 解析 JSON | reqwest + chat 基础类型 |

rerank 涉及的概念:

- `RerankRequest`(query + candidates + top_k)
- `RerankResult`(reranked candidates)
- `RerankFusionStrategy`:LinearWeighted / Multiplicative / RRF / RerankOnly
- `RerankRuntimeConfig`:max_candidates / temperature / return_reasoning / timeout

**推荐方案:独立 `llm-rerank` crate**,不与其他 crate 合并,理由:

1. 传输方式多样——专用 API 调用(Cohere)与 chat 模拟(Generative)走不同路径,不能在同一个 transport trait 下统一
2. 类型体系独立——RerankRequest/Result/FusionStrategy 不与 chat 或 embedding 共享
3. 不需要条件编译细分:provider 分支很浅(目前就两个),不值得 feature gate;future 新增 provider(如 VoyageAI)只需加模块

结构设计:

```
llm-rerank/src/
├── lib.rs
├── config.rs        # RerankConfig(base_url / api_key / model / timeout)
├── error.rs         # RerankError
├── provider.rs      # RerankProvider trait + types
├── cohere.rs        # CohereRerankProvider
├── generative.rs    # GenerativeRerankProvider
└── fusion.rs        # RerankFusionStrategy + score 计算
```

 依赖:serde、serde_json、reqwest、tracing、tokio、`async-trait`(trait 定义与 `llm-embedding` 保持一致)。`cohere` 与 `generative` 不拆分 feature——两个 provider 都在包里,运行时选择而非编译时。`llm-types` 暂未引入:rerank 类型体系独立,无需复用 Message。

第二期已落地:请求校验/候选截断/超时收敛到共享 helper(`provider.rs`),generative 的 chat 调用为精简内联 POST(不依赖 `llm-chat-basic`,保持叶子包独立可迁移)。

**不放入 `llm-chat-basic`**:因为 rerank 的传输路径(Cohere 非 chat)导致 ts 不在 chat 逻辑内;合并反而不清晰。
**不放入 `llm-extend` 杂项包**:杂项包边界模糊,长期难以维护。宁可创建专名 crate(`llm-rerank`)也不搞 `llm-extend`。

## 约束

- 遵循本仓库 AGENTS.md:严格 DAG、workspace 集中依赖、禁用 `mod.rs`(扁平 `<module_name>.rs`)、代码/注释全英文。
- 各包为 infra 层叶子包,不依赖任何 engine/app 层 crate。
- version 随 workspace 统一。第三期上移独立共享仓库后确认具有独立性。

## 实施阶段

### 第一期(已完成 ✅)

在 `crates/infra/llm-kit/` 下落地:
- `llm-types` — 叶子包,零外部依赖
- `llm-embedding` — EmbeddingProvider + OpenAICompatibleProvider + EmbeddingService
- `llm-tool-call` — 条件编译(feature: json-partial / xml),源文件从 wf-llm 复制

特征:暂不替换 wf-llm 内部实现;已验证 `cargo clippy --all-features --all-targets` 通过。

### 第二期(已完成 ✅)

从 code-context-engine / linkrs 补齐:
- `llm-chat-basic` — 已移植 cce-llm-client 的 chat handler;streaming/ratelimit/retry feature gate
- `llm-rerank` — Cohere + Generative provider + 分数融合策略
- `llm-embedding` 补齐——已从 linkrs 移植 Preprocessor + 可选 api_key + dimension 校验 + endpoint 预设
- 离线单测已覆盖与 source 项目一致的核心行为(请求体形状、解析/校验、融合分数、预处理器输出)

### 第三期(待实施 ⏳)

- 将成熟 crate 上移到独立共享仓库
- wf-agent / cce / linkrs 三项目改用 git 依赖统一引用
- wf-llm 内部的类型/transport 层迁移到共享 crate(保留 agent 引擎层不变)