# Rust 迁移分阶段实施方案

## 一、迁移策略总览

### 核心原则

1. **渐进式迁移**：通过 napi-rs 实现 Rust 核心 + TS 外壳的混合架构，用户无感知
2. **自底向上**：先迁移基础设施，再迁移执行引擎，最后迁移 API 绑定层
3. **保持兼容**：sdk-kit 层维持 TS API 不变，底层逐步替换为 Rust 实现
4. **充分验证**：每个阶段完成后运行完整 E2E 测试套件

### 架构目标

```
┌─────────────────────────────────────────────────┐
│              TypeScript 外壳层                    │
│  sdk-kit / runtime / apps (保持不变)              │
├─────────────────────────────────────────────────┤
│              napi-rs 绑定层                       │
│  wf-sdk (自动生成的 TS 类型包装)                   │
├─────────────────────────────────────────────────┤
│              Rust 核心层                          │
│  wf-core / wf-executor / wf-checkpoint / wf-llm  │
│  wf-tools / wf-agent / wf-storage / wf-types     │
└─────────────────────────────────────────────────┘
```

---

## 二、Phase 0: 基础设施准备（1-2 周）

### 目标

建立 Rust workspace 基础结构，配置构建工具链，验证 napi-rs 桥接可行性。

### 任务清单

#### 0.1 创建 Cargo Workspace

```
crates/
├── Cargo.toml          # workspace 定义
├── wf-types/           # 类型定义
├── wf-common/          # 公共工具
├── wf-storage/         # 存储层
├── wf-core/            # 核心基础设施
├── wf-checkpoint/      # Checkpoint 系统
├── wf-executor/        # 执行引擎
├── wf-llm/             # LLM 服务
├── wf-tools/           # 工具执行
├── wf-agent/           # Agent 循环
└── wf-sdk/             # napi-rs 绑定
```

#### 0.2 配置工具链

- `rust-toolchain.toml`：指定 stable channel + targets（跨平台）
- `.cargo/config.toml`：构建优化配置（LTO, codegen-units）
- `clippy.toml`：统一 lint 规则
- CI 集成：在现有 GitHub Actions 中增加 Rust 构建/测试 job

#### 0.3 验证 napi-rs 桥接

- 编写最小可行性示例：TS 调用 Rust 函数并返回结构化数据
- 验证 napi-rs 生成的 `.node` 文件与现有 pnpm workspace 的兼容性
- 测试跨平台构建（Windows/macOS/Linux）

#### 0.4 共享类型定义约定

- 确定 Rust 类型与 TS 类型的映射规则（serde → napi-rs 自动生成）
- 制定错误类型统一规范（`thiserror` → napi-rs 错误转换）
- 制定异步接口规范（`async-trait` + `tokio` → napi-rs `napi::bindgen_prelude::Promise`）

### 交付物

- [ ] Cargo workspace 骨架，所有 crate 可独立编译
- [ ] napi-rs 最小示例通过 TS 调用成功
- [ ] CI 配置更新，Rust 构建/测试纳入流水线
- [ ] 类型映射规范文档

### 验收标准

```bash
cd crates && cargo build --workspace  # 全量构建通过
cd crates/wf-sdk && npm test          # napi-rs 桥接测试通过
```

---

## 三、Phase 1: 基础类型与存储层（2-3 周）

### 目标

迁移最底层、耦合度最低的模块，验证 Rust 与 TS 的互操作模式。

### 任务清单

#### 1.1 wf-types（1 周）

**迁移内容**：
- 所有 Zod schema → serde `Serialize/Deserialize` derive
- 枚举类型 → Rust enum + serde `rename_all`
- 联合类型 → Rust enum + serde `tag` 属性
- 15 种节点类型定义 → Rust enum + 配置 struct

**关键技术决策**：
```rust
// 节点类型映射示例
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NodeType {
    Start,
    End,
    Llm,
    Fork,
    Join,
    // ...
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LlmNodeConfig {
    pub model: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub tools: Vec<ToolReference>,
}
```

**挑战与对策**：
- Zod 的 `refine`/`transform` → Rust 的 `TryFrom` 或自定义 deserializer
- Zod 的 `discriminatedUnion` → serde 的 `tag = "type"` 属性
- 保持序列化后的 JSON 结构完全一致，确保与现有 TS 代码互操作

#### 1.2 wf-storage（1-2 周）

**迁移内容**：
- 存储 trait 定义（StorageAdapter trait）
- SQLite 实现（sqlx + rusqlite 混合）
- PostgreSQL 实现（sqlx + tokio-postgres）
- 连接池管理（sqlx::Pool）
- 实体 CRUD 操作

**关键技术决策**：
```rust
#[async_trait]
pub trait CheckpointStorage: Send + Sync {
    async fn save(&self, checkpoint: &Checkpoint) -> Result<(), StorageError>;
    async fn load(&self, id: &str) -> Result<Option<Checkpoint>, StorageError>;
    async fn list_by_session(&self, session_id: &str) -> Result<Vec<Checkpoint>, StorageError>;
    async fn delete(&self, id: &str) -> Result<bool, StorageError>;
}
```

**挑战与对策**：
- better-sqlite3 是同步的 → Rust 端使用 `tokio::task::spawn_blocking` 包装 rusqlite
- 或完全异步化，使用 sqlx 的 SQLite 驱动
- 迁移期间保持双写策略：TS 和 Rust 存储层同时写入，验证一致性

#### 1.3 wf-common（并行进行）

**迁移内容**：
- Result 类型 → Rust `Result<T, E>`
- 错误处理工具 → `thiserror` derive
- 日志接口 → `tracing` crate
- 哈希工具（xxhash, SHA256）→ `xxhash-rust` + `sha2`
- 压缩工具 → `zstd` crate

### 交付物

- [ ] wf-types: 所有类型定义迁移完成，serde 序列化与 TS Zod 输出一致
- [ ] wf-storage: SQLite + PostgreSQL 实现通过现有集成测试
- [ ] wf-common: 工具函数迁移完成，tracing 日志集成
- [ ] 双写验证报告：TS 和 Rust 存储层数据一致性 100%

### 验收标准

```bash
cd crates/wf-types && cargo test
cd crates/wf-storage && cargo test
pnpm --filter @wf-agent/storage test  # 原有 TS 测试仍通过
# 新增：Rust 存储层通过 TS 侧的集成测试
```

---

## 四、Phase 2: 核心基础设施（3-4 周）

### 目标

迁移事件系统、注册系统、Checkpoint 核心，构建 Rust 执行引擎的基础。

### 任务清单

#### 2.1 wf-core（2 周）

**迁移内容**：

| TS 模块 | Rust 实现 | 技术选型 |
|---------|----------|---------|
| EventRegistry | EventBus | `tokio::sync::broadcast` + `flume` |
| ToolRegistry | ToolRegistry | `DashMap` + `Arc<dyn Tool>` |
| TaskRegistry | TaskRegistry | `DashMap` |
| CheckpointCore | CheckpointCore | trait + 状态机 |

**EventBus 设计**：
```rust
pub struct EventBus {
    sender: broadcast::Sender<Event>,
    // 订阅者管理
}

impl EventBus {
    pub fn subscribe(&self) -> Receiver<Event> {
        self.sender.subscribe()
    }
    
    pub async fn publish(&self, event: Event) -> Result<usize, EventError> {
        Ok(self.sender.send(event)?)
    }
}
```

**Registry 设计**：
```rust
pub struct ToolRegistry {
    tools: DashMap<String, Arc<dyn Tool>>,
    categories: DashMap<String, Vec<String>>,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn execute(&self, input: Value) -> Result<Value, ToolError>;
}
```

#### 2.2 wf-checkpoint（1-2 周）

**迁移内容**：
- CheckpointCoordinator（当前 59KB 大文件）
- BranchManager
- CheckpointVersionManager
- CheckpointStrategy

**序列化方案**：
```rust
// 使用 bincode 替代 JSON，性能提升 3-5x
#[derive(Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub session_id: String,
    pub state: WorkflowState,
    pub created_at: DateTime<Utc>,
    pub metadata: HashMap<String, Value>,
}

impl Checkpoint {
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }
    
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}
```

**挑战与对策**：
- Checkpoint 版本迁移：保留 JSON 反序列化路径，新数据用 bincode
- Branch 并发访问：`RwLock<DashMap<String, Branch>>`
- 大状态序列化：考虑使用 `rkyv` 实现零拷贝反序列化

### 交付物

- [ ] wf-core: EventBus + Registry 通过并发压力测试
- [ ] wf-checkpoint: CheckpointCoordinator 通过现有 E2E 测试
- [ ] 性能基准报告：Rust 实现 vs TS 实现的延迟/吞吐对比

### 验收标准

```bash
cd crates/wf-core && cargo test
cd crates/wf-checkpoint && cargo test
# 事件系统：10K 事件/秒广播无丢失
# Checkpoint：序列化速度提升 3x 以上
```

---

## 五、Phase 3: 执行引擎（6-8 周）

### 目标

迁移核心执行引擎，包括 Workflow/Node/LLM 协调器、Tool 执行器。

### 任务清单

#### 3.1 wf-executor — 工具执行器（2 周）

**迁移内容**：
- ToolCallExecutor
- ScriptExecutor（保持 TS 沙箱，Rust 仅做调度）
- 审批引擎（ApprovalEngine）

```rust
pub struct ToolCallExecutor {
    registry: Arc<ToolRegistry>,
    event_bus: Arc<EventBus>,
    approval_engine: Arc<ApprovalEngine>,
}

impl ToolCallExecutor {
    pub async fn execute(&self, call: ToolCall) -> Result<ToolResult, ExecutorError> {
        let tool = self.registry.get(&call.tool_name)?;
        
        if tool.requires_approval() {
            self.approval_engine.request(&call).await?;
        }
        
        let result = tool.execute(call.input).await?;
        
        self.event_bus.publish(Event::ToolExecuted {
            call_id: call.id,
            result: result.clone(),
        })?;
        
        Ok(result)
    }
}
```

#### 3.2 wf-executor — Workflow/Node 协调器（3-4 周）

**迁移内容**：
- WorkflowExecutionCoordinator
- NodeExecutionCoordinator
- GraphBuilder
- StateManager
- 15 种节点的执行逻辑

**状态机设计**：
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum NodeState {
    Pending,
    Running,
    Completed(NodeOutput),
    Failed(NodeError),
    Skipped,
    Paused,
}

pub struct NodeExecutionCoordinator {
    state: Arc<RwLock<HashMap<String, NodeState>>>,
    graph: Arc<WorkflowGraph>,
    executor: Arc<ToolCallExecutor>,
}

impl NodeExecutionCoordinator {
    pub async fn execute_node(&self, node_id: &str) -> Result<NodeOutput, NodeError> {
        let node = self.graph.get_node(node_id)?;
        
        self.set_state(node_id, NodeState::Running).await;
        
        let result = match &node.config {
            NodeConfig::Llm(config) => self.execute_llm_node(node_id, config).await,
            NodeConfig::Tool(config) => self.execute_tool_node(node_id, config).await,
            NodeConfig::Script(config) => self.execute_script_node(node_id, config).await,
            NodeConfig::Fork => self.execute_fork_node(node_id).await,
            NodeConfig::Route(config) => self.execute_route_node(node_id, config).await,
            // ... 其他节点类型
        };
        
        match &result {
            Ok(output) => self.set_state(node_id, NodeState::Completed(output.clone())).await,
            Err(err) => self.set_state(node_id, NodeState::Failed(err.clone())).await,
        }
        
        result
    }
}
```

**Fork/Join 并行执行**：
```rust
async fn execute_fork_node(&self, node_id: &str) -> Result<NodeOutput, NodeError> {
    let branches = self.graph.get_fork_branches(node_id)?;
    
    let handles: Vec<_> = branches.into_iter().map(|branch| {
        let coordinator = self.clone();
        tokio::spawn(async move {
            coordinator.execute_branch(branch).await
        })
    }).collect();
    
    let results: Vec<Result<NodeOutput, NodeError>> = join_all(handles).await
        .into_iter()
        .map(|r| r.unwrap_or_else(|e| Err(NodeError::JoinError(e.to_string()))))
        .collect();
    
    NodeOutput::merge(results)
}
```

#### 3.3 wf-llm（2-3 周）

**迁移内容**：
- LLM Client（OpenAI/Gemini/Anthropic）
- MessageStream（SSE 解析 + 流式处理）
- Tool Call 格式化器
- Token 计数器（tiktoken-rs）

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError>;
    async fn stream_chat(&self, request: ChatRequest) -> Pin<Box<dyn Stream<Item = Result<ChatChunk, LlmError>> + Send>>;
}

pub struct OpenAiClient {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

#[async_trait]
impl LlmProvider for OpenAiClient {
    async fn stream_chat(&self, request: ChatRequest) -> Pin<Box<dyn Stream<Item = Result<ChatChunk, LlmError>> + Send>> {
        let response = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;
        
        let stream = response.bytes_stream()
            .map(|chunk| self.parse_sse_chunk(chunk));
        
        Box::pin(stream)
    }
}
```

**挑战与对策**：
- SSE 解析：使用 `eventsource-stream` 或自定义解析器
- 流式工具调用：部分 JSON 拼接处理
- 多 Provider 适配：trait 对象 + 工厂模式

### 交付物

- [ ] wf-executor: ToolCallExecutor 通过所有工具相关测试
- [ ] wf-executor: Workflow/Node Coordinators 通过现有 workflow E2E 测试
- [ ] wf-llm: LLM Client 通过 formatter 测试（OpenAI/Gemini/Anthropic）
- [ ] 性能基准：workflow 执行速度提升 5x 以上

### 验收标准

```bash
cd crates/wf-executor && cargo test
cd crates/wf-llm && cargo test
# 端到端：运行 apps/cli-app 的核心工作流场景，功能正常
# 性能：100 节点 workflow 执行时间 < TS 版本的 1/5
```

---

## 六、Phase 4: 工具链与 Agent（4-5 周）

### 目标

迁移 MCP 工具系统、Agent 循环，完善 Rust 执行引擎的全部功能。

### 任务清单

#### 4.1 wf-tools（2-3 周）

**迁移内容**：
- MCP Client（stdio/SSE/StreamableHTTP 传输）
- 内置工具（filesystem, shell, workflow, agent）
- 工具审批引擎
- 使用分析

```rust
pub struct McpClient {
    transport: Box<dyn McpTransport>,
    tools: DashMap<String, McpTool>,
    approval_engine: Arc<ApprovalEngine>,
}

#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn connect(&mut self) -> Result<(), McpError>;
    async fn list_tools(&self) -> Result<Vec<McpTool>, McpError>;
    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, McpError>;
    async fn close(&mut self) -> Result<(), McpError>;
}

pub struct StdioTransport {
    child: tokio::process::Child,
    stdin: tokio::io::BufWriter<tokio::process::ChildStdout>,
    stdout: tokio::io::BufReader<tokio::process::ChildStdin>,
}

pub struct SseTransport {
    client: reqwest::Client,
    base_url: String,
    event_source: Option<EventSource>,
}
```

**挑战与对策**：
- MCP Rust 生态不完善：需自行实现 JSON-RPC 2.0 协议
- stdio 进程管理：`tokio::process` + 行级 JSON-RPC
- 连接池与重试：`tower` crate 的 retry layer

#### 4.2 wf-agent（2 周）

**迁移内容**：
- AgentLoopCoordinator
- AgentLoopStateTransitor
- ConversationSession 管理

```rust
pub struct AgentLoopCoordinator {
    llm: Arc<dyn LlmProvider>,
    tool_executor: Arc<ToolCallExecutor>,
    checkpoint_coordinator: Arc<CheckpointCoordinator>,
    max_iterations: usize,
}

impl AgentLoopCoordinator {
    pub async fn run(&self, session: &mut Session, initial_message: Message) -> Result<AgentResult, AgentError> {
        let mut messages = session.history.clone();
        messages.push(initial_message);
        
        for i in 0..self.max_iterations {
            let response = self.llm.chat(ChatRequest {
                messages: messages.clone(),
                tools: self.tool_executor.available_tools(),
            }).await?;
            
            messages.push(response.message.clone());
            
            if response.is_final() {
                return Ok(AgentResult::new(messages, response));
            }
            
            for tool_call in response.tool_calls {
                let result = self.tool_executor.execute(tool_call).await?;
                messages.push(Message::tool_result(result));
            }
            
            self.checkpoint_coordinator.save(Checkpoint::new(&messages)).await?;
        }
        
        Err(AgentError::MaxIterationsReached)
    }
}
```

### 交付物

- [ ] wf-tools: MCP Client 通过 MCP 协议兼容性测试
- [ ] wf-tools: 内置工具通过现有工具执行测试
- [ ] wf-agent: AgentLoopCoordinator 通过 Agent E2E 测试
- [ ] 完整工作流验证：从 trigger 到 agent 响应的全链路

### 验收标准

```bash
cd crates/wf-tools && cargo test
cd crates/wf-agent && cargo test
# MCP：连接主流 MCP 服务器（filesystem, github, slack）功能正常
# Agent：cli-app 中 agent 模式运行正常
```

---

## 七、Phase 5: napi-rs 绑定与集成（3-4 周）

### 目标

构建 Rust 到 TypeScript 的绑定层，实现混合架构的最终集成。

### 任务清单

#### 5.1 wf-sdk napi-rs 绑定层（2 周）

**迁移内容**：
- 自动生成 TS 类型定义
- 错误类型映射
- Promise/Future 转换
- Stream 转换为 AsyncIterator

```rust
#[napi]
pub struct WorkflowExecutor {
    inner: Arc<WorkflowExecutionCoordinator>,
}

#[napi]
impl WorkflowExecutor {
    #[napi(constructor)]
    pub fn new(config: WorkflowConfig) -> Result<Self> {
        let coordinator = WorkflowExecutionCoordinator::new(config)?;
        Ok(Self { inner: Arc::new(coordinator) })
    }
    
    #[napi]
    pub async fn execute(&self, input: Value) -> Result<WorkflowOutput> {
        self.inner.execute(input).await
    }
    
    #[napi]
    pub fn on_event(&self, callback: JsFunction) -> Result<()> {
        let ts_receiver = callback.threadsafe_function()?;
        let mut rx = self.inner.event_bus.subscribe();
        
        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                let _ = ts_receiver.call(Ok(event), napi::threadsafe_function::ThreadsafeFunctionMode::NonBlocking);
            }
        });
        
        Ok(())
    }
}
```

#### 5.2 混合架构集成（1-2 周）

**集成策略**：

```
┌──────────────────────────────────────────────┐
│                apps/cli-app                   │
│                  (TypeScript)                 │
├──────────────────────────────────────────────┤
│              @wf-agent/sdk-kit                │
│        (TypeScript API 保持不变)               │
├──────────────────────────────────────────────┤
│              @wf-agent/sdk                    │
│         (napi-rs 生成的 .node)                 │
├──────────────────────────────────────────────┤
│          Rust Core (wf-*)                     │
│    wf-core / wf-executor / wf-checkpoint     │
│    wf-llm / wf-tools / wf-agent / wf-storage │
└──────────────────────────────────────────────┘
```

**回退机制**：
- 每个 Rust 调用封装 try-catch，失败时回退到 TS 实现
- 特性开关控制：`USE_RUST_CORE=true/false`
- 灰度发布：按用户/会话比例逐步切换

#### 5.3 性能优化与监控

- 内存 profiling：`dhat` 或 `valgrind` 检测内存泄漏
- CPU profiling：`perf` / `flamegraph` 定位热点
- 延迟追踪：`tracing` + OpenTelemetry 导出
- 基准测试：criterion 持续跟踪关键路径性能

### 交付物

- [ ] wf-sdk: 完整的 napi-rs 绑定，覆盖所有公开 API
- [ ] 混合架构运行正常，特性开关工作正常
- [ ] 性能监控 Dashboard（对比 TS vs Rust）
- [ ] 回归测试通过率 100%

### 验收标准

```bash
cd crates/wf-sdk && npm test           # napi-rs 绑定测试
pnpm --filter @wf-agent/sdk test       # SDK 集成测试
pnpm --filter @wf-agent/cli-app test   # CLI E2E 测试
# 全量回归：所有 apps 和 packages 的测试通过
# 性能目标：内存占用降低 50%，执行速度提升 5x
```

---

## 八、Phase 6: 清理与优化（2 周）

### 目标

移除 TS 旧实现，清理过渡代码，完成迁移收尾。

### 任务清单

- [ ] 移除 TS 端的 Rust 回退实现
- [ ] 删除特性开关和条件分支
- [ ] 清理双写逻辑
- [ ] 更新文档和架构图
- [ ] 移除不再需要的 TS 依赖（如 better-sqlite3 的本地编译依赖）
- [ ] 最终性能基准测试与报告

---

## 九、风险与缓解措施

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|---------|
| napi-rs 跨平台构建失败 | 阻塞发布 | 中 | CI 多平台构建验证；准备预编译二进制分发 |
| Checkpoint 数据不兼容 | 数据丢失 | 低 | 双写验证；保留 JSON 反序列化路径 |
| MCP Rust 生态不足 | 功能缺失 | 高 | 自行实现 JSON-RPC；优先支持 stdio 传输 |
| 性能未达预期 | 用户无感知收益 | 中 | 每个 Phase 设定性能门禁；及时回退 |
| 异步运行时差异 | 死锁/竞态 | 中 | 充分并发测试；使用 tokio-console 调试 |
| 团队 Rust 经验不足 | 进度延迟 | 高 | 前期培训；结对编程；代码 Review |

---

## 十、时间线与里程碑

```
Week  1-2  │ Phase 0: 基础设施准备
           │ ├── Cargo workspace 搭建
           │ └── napi-rs 可行性验证
───────────┼────────────────────────────────────
Week  3-5  │ Phase 1: 基础类型与存储层
           │ ├── wf-types 迁移完成
           │ └── wf-storage 迁移完成
───────────┼────────────────────────────────────
Week  6-9  │ Phase 2: 核心基础设施
           │ ├── wf-core 迁移完成
           │ └── wf-checkpoint 迁移完成
───────────┼────────────────────────────────────
Week 10-17 │ Phase 3: 执行引擎
           │ ├── ToolCallExecutor 迁移完成
           │ ├── Workflow/Node Coordinator 迁移完成
           │ └── wf-llm 迁移完成
───────────┼────────────────────────────────────
Week 18-22 │ Phase 4: 工具链与 Agent
           │ ├── wf-tools (MCP) 迁移完成
           │ └── wf-agent 迁移完成
───────────┼────────────────────────────────────
Week 23-26 │ Phase 5: napi-rs 绑定与集成
           │ ├── 绑定层完成
           │ └── 混合架构联调通过
───────────┼────────────────────────────────────
Week 27-28 │ Phase 6: 清理与优化
           │ └── 全量 TS 旧代码移除
───────────┴────────────────────────────────────
```

**总计约 28 周（7 个月）**，建议 2-3 名 Rust 开发人员全职投入。

---

## 十一、团队与资源建议

### 人员配置

| 角色 | 数量 | 职责 |
|------|------|------|
| Rust 核心开发 | 2 | 编写 Rust crate、性能优化 |
| TS/Rust 桥接开发 | 1 | napi-rs 绑定、混合架构集成 |
| QA/测试 | 1 | 跨语言集成测试、E2E 验证 |

### 技术储备

- Rust 异步编程（tokio, async-trait）
- napi-rs 绑定开发经验
- 序列化协议（serde, bincode, JSON）
- 状态机与并发模式
- 现有 TS 代码架构深度理解
