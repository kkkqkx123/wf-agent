# wf-tools 完整性分析与 LLM 依赖评估

## 一、wf-tools 与 TS 版对比

### 1.1 覆盖度总览

| 域 | TS 文件数 | RS 已实现 | 覆盖度 |
|----|----------|----------|--------|
| Executor Trait + Base | 3 | 3 | 100% |
| StatelessExecutor | 1 | 1 (stub) | 40% |
| StatefulExecutor | 1 | 1 (基础) | 30% |
| RestExecutor | 1 | 1 (基础) | 50% |
| BuiltinExecutor | 1 | 1 | 80% |
| McpExecutor | 1 | 1 | 70% |
| ToolRegistry | 2 | 1 | 50% |
| ToolCallExecutor | 1 | 1 | 40% |
| ToolApproval | 3 | 1 | 30% |
| MCP Client | 2 | 2 | 70% |
| MCP Transports | 4 | 3 (1 stub) | 60% |
| MCP Features | 6 | 0 | 0% |
| Auto-Approval | 3 | 1 (基础) | 20% |
| Validation | 2 | 1 (基础) | 30% |
| Protection/Metrics | 2 | 0 | 0% |
| **合计** | **~35** | **~17** | **~45%** |

### 1.2 已实现模块对照

| TS 模块 | RS 实现 | 状态 |
|---------|---------|------|
| `IToolExecutor` | `ToolExecutor` trait | ✅ 含 retry/timeout 扩展 |
| `BaseExecutor` | `BaseExecutor` | ✅ 基础校验 |
| `StatelessExecutor` | `StatelessExecutor` | ⚠️ 核心逻辑 stub |
| `StatefulExecutor` | `StatefulExecutor` | ⚠️ 缺 factory + execution-scoped 实例 |
| `RestExecutor` | `RestExecutor` | ⚠️ 无 interceptor/circuit breaker |
| `BuiltinExecutor` | `BuiltinExecutor` | ✅ 4 种内置调度 |
| `McpExecutor` | `McpExecutor` | ✅ |
| `ToolRegistry` | `ToolRegistry` | ⚠️ 缺持久化、search、availability |
| `ToolCallExecutor` | `ToolCallExecutor` | ⚠️ 缺 events、failure protection |
| `ToolApprovalCoordinator` | `ToolApprovalCoordinator` | ⚠️ 缺 audit trail、event |
| `McpClient` | `McpClient` | ⚠️ 缺 listResources/readResource |
| `McpConnectionManager` | `McpConnectionManager` | ⚠️ 缺 lifecycle、health check |
| `StdioTransport` | `StdioTransport` | ✅ |
| `SseTransport` | `SseTransport` | ❌ stub |
| `StreamableHttpTransport` | `StreamableHttpTransport` | ⚠️ 基础实现 |
| `ExecutionCallback` | `ExecutionCallback` | ✅ 回调桥接 |

---

## 二、缺失功能必要性分级

### P0 — 阻塞性缺失

#### 1. SseTransport 完整实现

**必要性**: MCP SSE 传输是 MCP 规范三大传输之一，当前 stub 直接返回错误，导致所有 SSE 类型 MCP 服务器不可用。

**实现方案**:
```
SseTransport 需要实现:
├── SSE 长连接建立 (GET endpoint, 接收 event stream)
├── 服务端消息解析 (data: {...}\n\n 格式)
├── 客户端请求发送 (POST 到 message endpoint)
├── 自动重连 (exponential backoff: 1s → 2s → 4s → max 30s)
└── 状态机: Disconnected → Connecting → Connected → Reconnecting → Closing → Closed

技术选型:
- 使用 reqwest + eventsource-stream crate (已在 workspace deps)
- 或用 tokio::net 手动实现 SSE 协议
```

#### 2. 参数校验增强

**必要性**: 当前 `BaseExecutor::validate_parameters` 仅检查 required 字段和 additional_properties。TS 版有完整 Zod schema 校验（类型、格式、范围、嵌套对象）。

**实现方案**:
```rust
// 在 BaseExecutor 中增强
fn validate_parameters(tool: &Tool, parameters: &Value) -> ToolResult<()> {
    // 1. 检查 required 字段存在
    // 2. 类型校验 (string/number/boolean/array/object)
    // 3. 格式校验 (uri, email, uuid, ipv4)
    // 4. 数值范围 (minimum, maximum)
    // 5. 字符串约束 (minLength, maxLength, pattern)
    // 6. 数组约束 (minItems, maxItems)
    // 7. enum 约束
}
// 可直接基于 tool.parameters (JSON Schema) 做校验
// 或引入 jsonschema crate (但会增加依赖)
```

### P1 — 生产就绪必需

#### 3. StatefulExecutor Factory 模式

**必要性**: TS 版支持 execution-scoped 实例隔离（不同 executionId 的同一工具使用不同实例）。RS 版只有全局 DashMap，无法隔离。

**实现方案**:
```rust
pub struct StatefulExecutor {
    instances: DashMap<String, DashMap<String, Value>>, // execution_id → (tool_name → state)
}

impl StatefulExecutor {
    pub fn get_instance_state(&self, execution_id: &str, tool_name: &str) -> Option<Value> {
        self.instances.get(execution_id)?.get(tool_name).map(|v| v.clone())
    }
    pub fn cleanup_execution(&self, execution_id: &str) {
        self.instances.remove(execution_id);
    }
}
```

#### 4. MCP Lifecycle + Health Check

**必要性**: 生产环境 MCP 连接会因网络/服务器故障断开，需要自动检测和恢复。

**实现方案**:
```rust
// McpServerConfigBase 中已有 lifecycle 字段
pub enum McpLifecycleMode {
    Lazy,      // 首次调用时连接
    Eager,     // 注册后立即连接
    KeepAlive, // 保持连接 + 健康检查
}

// McpConnectionManager 增加:
pub async fn start_health_check(&self, interval: Duration) {
    // 定时对所有 KeepAlive 服务器发送 list_tools 或 ping
    // 失败 → 标记 Disconnected → 尝试重连
}

pub async fn disconnect_idle(&self, idle_timeout: Duration) {
    // 超过 idle_timeout 未使用的连接自动断开
}
```

#### 5. RestExecutor Interceptors + Circuit Breaker

**必要性**: 生产 HTTP 调用需要请求/响应拦截（日志、鉴权、重试）和熔断保护。

**实现方案**:
```rust
pub struct RestExecutor {
    client: reqwest::Client,
    request_interceptors: Vec<Arc<dyn Fn(RequestBuilder) -> RequestBuilder + Send + Sync>>,
    response_interceptors: Vec<Arc<dyn Fn(Response) -> Response + Send + Sync>>,
    circuit_breaker: Option<CircuitBreaker>,
}

pub struct CircuitBreaker {
    failure_threshold: u32,
    reset_timeout: Duration,
    state: AtomicU8, // Closed / Open / HalfOpen
    failures: AtomicU32,
    last_failure: Mutex<Option<Instant>>,
}
```

#### 6. ToolRegistry 持久化

**必要性**: 工具定义需要在服务重启后保留。

**实现方案**:
```rust
impl ToolRegistry {
    pub async fn initialize_from_storage(&self, storage: &dyn ToolStorage) -> ToolResult<()> {
        let tools = storage.load_tools().await?;
        for tool in tools {
            self.tools.insert(tool.id.clone(), tool);
        }
        Ok(())
    }
}

// ToolStorage trait 由 wf-storage 实现
#[async_trait]
pub trait ToolStorage: Send + Sync {
    async fn load_tools(&self) -> ToolResult<Vec<Tool>>;
    async fn save_tool(&self, tool: &Tool) -> ToolResult<()>;
    async fn delete_tool(&self, tool_id: &str) -> ToolResult<()>;
}
```

#### 7. AutoApprovalChecker 增强

**必要性**: 当前 `should_auto_approve` 仅基于 risk_level 字符串匹配。TS 版有完整的参数上下文提取（文件路径、命令、MCP 请求）和风险判断。

**实现方案**:
```rust
pub struct AutoApprovalChecker;

impl AutoApprovalChecker {
    pub fn check_auto_approval(&self, params: &AutoApprovalParams) -> AutoApprovalDecision {
        // 1. 提取上下文: file_path, command, mcp_request
        // 2. 按 risk_level 分级判断
        // 3. 应用 security_preset
        // 4. 返回 Approve / Deny / Ask / Timeout
    }
}
```

#### 8. ToolFailureProtection

**必要性**: LLM 可能反复调用一个失败的工具，浪费资源和 token。

**实现方案**:
```rust
pub struct ToolFailureProtectionState {
    failures: DashMap<String, Vec<Instant>>, // tool_id → 最近失败时间
    max_consecutive: u32,
    cooldown_period: Duration,
}

impl ToolFailureProtectionState {
    pub fn can_execute(&self, tool_id: &str) -> bool {
        let recent = self.failures.get(tool_id)
            .map(|ts| ts.iter().filter(|t| t.elapsed() < self.cooldown_period).count())
            .unwrap_or(0);
        recent < self.max_consecutive as usize
    }
}
```

### P2 — 功能完整性

| 功能 | 说明 | 实现要点 |
|------|------|---------|
| MCP Metadata Cache | TTL 缓存 list_tools 结果 | `DashMap<String, CachedTools>` + 定时清理 |
| MCP Idle Timeout | 空闲连接自动断开 | `last_activity: DashMap<String, Instant>` |
| ToolRegistry Search | 关键字搜索 | 遍历 name/description/tags 匹配 |
| ToolRegistry Availability | execution-scoped 可用性 | `DashMap<(tool_id, execution_id), bool>` |
| ToolCallExecutor Events | 事件发射 | `mpsc::Sender<ToolCallEvent>` |
| MCP Approval | per-server/tool 审批 | 复用 McpServerRegistry 的 disabled/always_allow |
| ToolPermissionManager | 运行时启用/禁用 | `DashMap<String, bool>` |
| Predefined Tools | read_file, write_file 等 | 独立模块实现具体逻辑 |

### P3 — 锦上添花（可延后）

| 功能 | 说明 |
|------|------|
| MCP Usage Analytics | 调用统计、热点识别 |
| Plugin Executor | 插件贡献 executor 类型 |
| ToolMetricsCollector | Prometheus 导出 |
| Dynamic Context Provider | LLM prompt 注入 |
| Tool Schema Utilities | schema 清理/格式化 |

---

## 三、Tools ↔ LLM 依赖分析

### 3.1 依赖方向结论

**Tools 不依赖 LLM。** 依赖方向是单向的 LLM → Tools。

| 方向 | TypeScript | Rust |
|------|-----------|------|
| Tools → LLM | ❌ 无 | ❌ 无 |
| LLM → Tools | ✅ LLM formatters 使用 ToolDeclarationFormatter | N/A (无 LLM crate) |
| 循环依赖 | ❌ 无 | ❌ 无 |

### 3.2 TS 实证

- `BuiltinExecutor` 不 import 任何 LLM 模块
- `ToolRegistry` 不 import 任何 LLM 模块
- 所有 executor 仅依赖 `@wf-agent/types` 和 `BaseExecutor`
- **反向依赖**: `services/llm/formatters/openai-chat.ts` → imports `ToolDeclarationFormatter`（用于将 ToolSchema 转为 provider 格式）

### 3.3 RS 实证

- `wf-tools/Cargo.toml` 仅依赖 `wf-types`, `wf-common` + 第三方 crate
- `wf-tools` 不使用 `wf_types::llm::*` 中的任何类型
- `callback.rs` 中的 `AgentLoopConfig.model` 是 `Option<String>`（模型名字符串），非 LLM 类型

### 3.4 架构含义

```
wf-types (已有 LLM 类型定义)
    ↓
wf-llm (Phase 4)  ←── 依赖 wf-types::llm::*
    │
    │ 使用 ToolSchema 格式化请求
    ↓
wf-tools (当前)   ←── 不依赖 wf-llm
```

**结论**: wf-tools 可独立实现和测试，无需等待 wf-llm。wf-llm 将在 Phase 4 实现，它依赖 wf-tools 的类型（Tool, ToolCall 等），但 wf-tools 不依赖 wf-llm。

---

## 四、wf-llm Crate 实现方案

### 4.1 定位

`wf-llm` 是 LLM 调用的统一抽象层，负责：
- 多 provider 适配（OpenAI, Anthropic, Gemini）
- 请求/响应格式转换
- 流式响应处理
- Token 统计
- 错误处理

### 4.2 依赖关系

```
wf-types ← wf-common
    ↓
wf-llm (依赖 wf-types::llm::* 类型)
```

`wf-llm` 不依赖 `wf-tools` 或 `wf-executor`。

### 4.3 已有类型（wf-types/src/llm/）

| 文件 | 已有定义 |
|------|---------|
| `state.rs` | `LlmProvider` (5 种), `LlmProviderConfig` |
| `profile.rs` | `LlmProfile` |
| `request.rs` | `LlmRequest`, `ChatRequest`, `DeadLoopDetectionConfig`, `ToolCallProtocolViolationPolicy` |
| `response.rs` | `LlmResult`, `ChatResponse`, `ChatChoice` |
| `usage.rs` | `TokenUsageStats`, `TokenUsageHistory`, `TokenUsageStatistics` |
| `client.rs` | `LlmClient` trait |
| `execution_config.rs` | `LlmExecutionConfig`, `GraphLlmExecutionConfig`, `AgentLlmExecutionConfig` |
| `tool_call_format.rs` | `ToolCallFormat`, `ToolCallFormatConfig` |
| `protocol_config.rs` | `ToolCallProtocolViolationPolicy`, `CrossBoundaryConfig` |
| `message_stream_events.rs` | `MessageStreamEvent` 枚举 |

### 4.4 需要实现的核心组件

#### 4.4.1 LlmClient Trait（已有定义，需实现）

```rust
// wf-types/src/llm/client.rs 已定义
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn generate(&self, request: &LlmRequest) -> Result<LlmResult, LlmError>;
    async fn generate_stream(&self, request: &LlmRequest) -> Result<Box<dyn MessageStream>, LlmError>;
    async fn count_tokens(&self, request: &LlmRequest) -> Result<TokenCountResult, LlmError>;
}
```

#### 4.4.2 Provider Formatter（核心适配层）

```rust
// 每个 provider 一个 formatter，负责:
// 1. 将 LlmRequest 转为 provider-specific HTTP 请求体
// 2. 将 provider 响应转为 LlmResult
// 3. 解析流式响应 chunk

#[async_trait]
pub trait LlmFormatter: Send + Sync {
    /// 构建 HTTP 请求 (URL + Headers + Body)
    fn build_request(&self, request: &LlmRequest, profile: &LlmProfile) -> Result<HttpRequest, LlmError>;
    
    /// 解析非流式响应
    fn parse_response(&self, body: &str) -> Result<LlmResult, LlmError>;
    
    /// 解析流式 chunk
    fn parse_stream_chunk(&self, data: &str) -> Result<Option<StreamChunk>, LlmError>;
    
    /// 转换工具定义格式
    fn convert_tools(&self, tools: &[Tool]) -> Result<Vec<Value>, LlmError>;
    
    /// 解析响应中的 tool_calls
    fn parse_tool_calls(&self, result: &LlmResult) -> Vec<LlmToolCall>;
}
```

**5 种 provider 实现**:

| Provider | Formatter | Endpoint |
|----------|-----------|----------|
| `OpenaiChat` | `OpenaiChatFormatter` | `/chat/completions` |
| `OpenaiResponse` | `OpenaiResponseFormatter` | `/responses` |
| `Anthropic` | `AnthropicFormatter` | `/v1/messages` |
| `GeminiNative` | `GeminiNativeFormatter` | `/models/{model}:generateContent` |
| `GeminiOpenai` | `GeminiOpenaiFormatter` | `/chat/completions` |

#### 4.4.3 LlmWrapper（统一入口）

```rust
pub struct LlmWrapper {
    profile_manager: ProfileManager,
    client_factory: ClientFactory,
}

impl LlmWrapper {
    pub async fn generate(&self, request: &LlmRequest) -> Result<LlmResult, LlmError> {
        // 1. 通过 ProfileManager 获取 profile
        // 2. 通过 ClientFactory 获取/创建 client
        // 3. 调用 client.generate()
        // 4. 包装错误和统计
    }
    
    pub async fn generate_stream(&self, request: &LlmRequest) -> Result<Box<dyn MessageStream>, LlmError> {
        // 同上，返回流式响应
    }
}
```

#### 4.4.4 ProfileManager

```rust
pub struct ProfileManager {
    profiles: DashMap<String, LlmProfile>,
    default_profile_id: Option<String>,
}

impl ProfileManager {
    pub fn register(&self, profile: LlmProfile) -> Result<(), LlmError> { ... }
    pub fn get(&self, id: &str) -> Option<LlmProfile> { ... }
    pub fn get_default(&self) -> Option<LlmProfile> { ... }
    pub fn list(&self) -> Vec<LlmProfile> { ... }
    pub fn remove(&self, id: &str) -> Option<LlmProfile> { ... }
}
```

#### 4.4.5 ClientFactory（带缓存）

```rust
pub struct ClientFactory {
    clients: DashMap<String, Arc<dyn LlmClient>>,
}

impl ClientFactory {
    pub fn get_or_create(&self, profile: &LlmProfile) -> Arc<dyn LlmClient> {
        // 1. 检查缓存
        // 2. 不存在则根据 provider 创建对应 formatter + client
        // 3. 缓存并返回
    }
    
    pub fn register_mock(&self, profile_id: String, client: Arc<dyn LlmClient>) { ... }
}
```

#### 4.4.6 MessageStream（流式响应）

```rust
pub trait MessageStream: Send {
    async fn next(&mut self) -> Option<Result<MessageStreamEvent, LlmError>>;
}

pub struct SseMessageStream {
    stream: eventsource_stream::EventSourceStream<...>,
    buffer: String,
}

impl MessageStream for SseMessageStream {
    async fn next(&mut self) -> Option<Result<MessageStreamEvent, LlmError>> {
        // 解析 SSE event → MessageStreamEvent
    }
}
```

#### 4.4.7 DeadLoopDetector

```rust
pub struct DeadLoopDetector {
    config: DeadLoopDetectionConfig,
    content_history: String,
}

impl DeadLoopDetector {
    pub fn check(&mut self, new_content: &str) -> bool {
        // 检测短序列重复、段落重复、列表重复
        // 返回 true 表示检测到死循环
    }
}
```

### 4.5 文件结构

```
crates/wf-llm/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── error.rs              # LlmError 枚举
│   ├── wrapper.rs            # LlmWrapper 统一入口
│   ├── client.rs             # LlmClientImpl 统一实现
│   ├── client_factory.rs     # ClientFactory 缓存
│   ├── profile_manager.rs    # ProfileManager
│   ├── message_stream.rs     # MessageStream 实现
│   ├── dead_loop_detector.rs # 死循环检测
│   ├── formatters/
│   │   ├── mod.rs
│   │   ├── trait_def.rs      # LlmFormatter trait
│   │   ├── openai_chat.rs
│   │   ├── openai_response.rs
│   │   ├── anthropic.rs
│   │   ├── gemini_native.rs
│   │   └── gemini_openai.rs
│   └── tool_converter.rs     # Tool → provider 格式转换
```

### 4.6 Cargo.toml 依赖

```toml
[package]
name = "wf-llm"
version = "0.1.0"
edition = "2021"

[dependencies]
wf-types = { path = "../wf-types" }
wf-common = { path = "../wf-common" }

async-trait = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", features = ["json", "stream"] }
eventsource-stream = "0.2"
tokio = { version = "1", features = ["full"] }
futures = "0.3"
dashmap = "6"
thiserror = "2"
tracing = "0.1"
```

### 4.7 实现优先级

```
Phase 1: 核心框架
├── LlmError 定义
├── ProfileManager
├── ClientFactory
├── LlmClientImpl (非流式)
└── OpenaiChatFormatter (最常用)

Phase 2: 流式 + 多 Provider
├── MessageStream (SSE)
├── AnthropicFormatter
├── GeminiNativeFormatter
└── OpenaiResponseFormatter

Phase 3: 增强功能
├── DeadLoopDetector
├── Token 统计
├── GeminiOpenaiFormatter
└── Tool 格式转换 (依赖 wf-tools 的 Tool 类型)
```

---

## 五、实现顺序建议

### 总体路线

```
当前: wf-tools 骨架完成 (Phase 3-A/3-B)
  ↓
Step 1: wf-tools P0 补全 (SseTransport + 参数校验)
  ↓
Step 2: wf-tools P1 补全 (StatefulExecutor + MCP Health + Rest CB + 持久化)
  ↓
Step 3: wf-executor 骨架 (Phase 3-C~3-F)
  ↓
Step 4: wf-llm 实现 (依赖 wf-types + wf-tools 的 Tool 类型)
  ↓
Step 5: wf-executor + wf-llm 集成 (Agent Loop)
  ↓
Step 6: wf-tools P2 + P3 补全
```

### 关键决策

1. **wf-tools 不依赖 wf-llm** — 可独立推进
2. **wf-llm 依赖 wf-tools** — 需要 Tool 类型做格式转换，因此 wf-llm 应在 wf-tools 稳定后实现
3. **wf-types::llm 已完备** — 所有 LLM 类型已定义，wf-llm 只需实现运行时逻辑
4. **ExecutionCallback 是桥接点** — wf-executor 实现此 trait 为 BuiltinExecutor 提供能力，内部调用 wf-llm

---

## 六、风险与注意事项

1. **回调桥接的线程安全**: 当前 `BuiltinExecutor` 使用 `std::thread::spawn` + `block_on` 调用 async callback，有死锁风险。应改为直接 async 调用或 `Handle::current().block_on()`。

2. **SSE 流式解析**: 需要正确处理 SSE 协议（event/id/retry/data 字段），建议使用 `eventsource-stream` crate。

3. **Provider API 差异**: Anthropic 的 tool_use 格式与 OpenAI 差异较大，formatter 需要完整处理各种边界情况。

4. **Token 计算**: 非 Anthropic provider 不支持 token count API，需要本地 tokenizer（如 `tiktoken-rs`）或估算。
