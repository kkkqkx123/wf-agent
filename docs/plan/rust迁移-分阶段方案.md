# Rust 重写分阶段实施方案

## 一、策略总览

### 核心原则

1. **纯 Rust 重写**：目标是用 Rust 完全替代 TypeScript/Node.js 运行时，不保留任何 TS 生产代码
2. **设计继承**：TS 版的设计经过生产验证，Rust 重写时直接借鉴其数据模型和架构决策，不重复试错
3. **BLOB 优先**：存储层采用 TS 版验证的 BLOB + 元数据分离模型，而非简化的 JSON blob
4. **自底向上**：先基础设施，再执行引擎，最后应用层
5. **可独立交付**：每个阶段产出可编译、可测试的完整模块

### 架构目标

```
┌──────────────────────────────────────────────────────────────┐
│                      Application Layer                        │
│  wf-cli (CLI) │ wf-server (HTTP/WS) │ wf-vscode (Extension)  │
├──────────────────────────────────────────────────────────────┤
│                      Public API Layer                         │
│  wf-api — Workflow/Agent builders, commands, resources        │
├──────────────────────────────────────────────────────────────┤
│                      Core Engine Layer                        │
│  wf-core — EventBus, Registry, Graph, State Machine           │
│  wf-executor — Workflow/Node coordinators, Tool executor      │
│  wf-checkpoint — Branch/Version/Strategy managers             │
├──────────────────────────────────────────────────────────────┤
│                      Services Layer                           │
│  wf-llm — Multi-provider LLM client + formatters              │
│  wf-tools — MCP client + built-in tools + approval            │
│  wf-sandbox — Script execution (JS/Python/Lua/Shell)          │
├──────────────────────────────────────────────────────────────┤
│                      Infrastructure Layer                     │
│  wf-storage — StorageAdapter + SQLite/Postgres/Memory backends │
│  wf-config — Config loaders + orchestrator                    │
│  wf-runtime — Bootstrap, lifecycle, storage manager           │
├──────────────────────────────────────────────────────────────┤
│                      Foundation Layer                         │
│  wf-types — Type definitions (serde)                          │
│  wf-common — Error, Result, time, ID                         │
└──────────────────────────────────────────────────────────────┘
```

### Rust Crate 依赖 DAG

```
wf-types  ←  wf-common
    ↓           ↓
wf-storage ← wf-config
    ↓           ↓
    └────→ wf-core ←────┬────→ wf-llm
                         │       ↓
                         ├────→ wf-tools
                         │       ↓
                         ├────→ wf-sandbox
                         │       ↓
                         └────→ wf-executor → wf-checkpoint
                                        ↓
                                     wf-api
                                        ↓
                              ┌────┬────┬────┐
                            wf-cli wf-server wf-runtime
```

---

## 二、Phase 0: 基础设施准备 ✅ 已完成

### 已完成内容

- [x] Cargo workspace 骨架，3 个 crate 可独立编译
- [x] `wf-types` — 20 种节点类型 + workflow/agent/checkpoint 类型定义
- [x] `wf-common` — Error、Result、时间、UUID 工具
- [x] `wf-storage` — trait 定义 + 三后端骨架（待补完）

### 验收状态

```bash
cargo build --workspace  # ✅ 通过
cargo test -p wf-types   # ✅ 通过
```

---

## 三、Phase 1: 存储层补完（3 周）

> 详见 [storage-补完方案.md](migration/storage-补完方案.md)

### 目标

将 `crates/wf-storage` 从骨架状态补完为生产级实现，完全对标 TS `packages/storage` 的设计。

### 核心设计决策

- **数据模型**：BLOB + 元数据列分离（沿用 TS 版验证方案）
- **缓存**：moka 并发 LRU（TTL + 容量限制 + 读写穿透）
- **完整性**：SHA-256 采样哈希 + 读写验证
- **维护**：VACUUM / ANALYZE / WAL checkpoint / 碎片监控
- **错误**：5 类错误体系对齐 TS

### 任务概要

| 子阶段 | 内容 | 工作量 |
|--------|------|--------|
| A | 数据模型重构 + 批量操作 + Checkpoint 补全 + 完整性校验 + 错误体系 + 工作流版本 | 1.5 周 |
| B | LRU 缓存 + 指标收集 + SQLite 维护 + Postgres 连接池 + 压缩 + Memory 增强 | 1.5 周 |

---

## 四、Phase 2: 核心基础设施（4 周）

### 目标

构建 wf-core（事件系统 + 注册系统 + 状态机）和 wf-checkpoint（分支/版本管理），以及 wf-config 和 wf-runtime。

### 2.1 wf-core（2 周）

**迁移内容**：

| 来源 TS 模块 | Rust 实现 | 技术选型 |
|-------------|----------|---------|
| `EventRegistry` | `EventBus` | `tokio::sync::broadcast` |
| `ToolRegistry` | `ToolRegistry` | `DashMap<String, Arc<dyn Tool>>` |
| `TaskRegistry` | `TaskRegistry` | `DashMap` |
| `NodeRegistry` / `NodeTypeRegistry` | `NodeTypeRegistry` | `DashMap` |
| `Registry` (通用) | `Registry<T>` | 泛型 DashMap 封装 |
| `ExecutionState` / 状态机 | `NodeState` enum + 转换逻辑 | 状态机模式 |

**EventBus 设计**：
```rust
pub struct EventBus {
    sender: broadcast::Sender<Event>,
}

impl EventBus {
    pub fn subscribe(&self) -> Receiver<Event> { self.sender.subscribe() }
    pub async fn publish(&self, event: Event) -> Result<usize, EventError> {
        Ok(self.sender.send(event)?)
    }
}
```

**Registry 设计**：
```rust
pub struct Registry<T> {
    items: DashMap<String, Arc<T>>,
}

impl<T> Registry<T> {
    pub fn register(&self, key: String, item: Arc<T>) -> Result<(), RegistryError> { ... }
    pub fn get(&self, key: &str) -> Option<Arc<T>> { ... }
    pub fn list(&self) -> Vec<String> { ... }
    pub fn unregister(&self, key: &str) -> bool { ... }
}
```

### 2.2 wf-checkpoint（1.5 周）

**迁移内容**：

| 来源 TS 模块 | Rust 实现 |
|-------------|----------|
| `CheckpointCoordinator` | `CheckpointCoordinator` |
| `BranchManager` | `BranchManager` |
| `CheckpointVersionManager` | `VersionManager` |
| `CheckpointStrategy` | `CheckpointStrategy` trait |

**关键技术决策**：
- 序列化：bincode（新数据）+ JSON（兼容读取）
- 并发：`RwLock<DashMap<String, Branch>>`
- 版本管理：增量快照 + 引用计数

### 2.3 wf-config（0.5 周，可与 2.1 并行）

**迁移内容**：
- `ConfigProcessor` — 配置文件加载器（skill, mcp, preset, config-index）
- `Orchestrator` — 配置编排器
- 继承 TS 的配置验证和合并逻辑

### 2.4 wf-runtime（0.5 周）

**迁移内容**：
- `Bootstrap` — 应用启动流程
- `StorageManager` — 存储生命周期管理
- `Lifecycle` — 优雅关闭
- `ModeDetector` — 运行模式检测（CLI/Server/Extension）

### 交付物

- [ ] wf-core: EventBus + Registry 通过并发压力测试
- [ ] wf-checkpoint: CheckpointCoordinator 通过 E2E 测试
- [ ] wf-config: 配置加载 + 验证
- [ ] wf-runtime: 启动/关闭生命周期

---

## 五、Phase 3: 执行引擎（6 周）

### 目标

迁移核心执行引擎：Workflow/Node 协调器、Tool 执行器。

### 3.1 wf-executor — 工具执行器（1.5 周）

**迁移内容**：

| 来源 TS 模块 | Rust 实现 |
|-------------|----------|
| `ToolCallExecutor` | `ToolCallExecutor` |
| `ApprovalEngine` | `ApprovalEngine` |

### 3.2 wf-executor — Workflow/Node 协调器（3 周）

**迁移内容**：

| 来源 TS 模块 | Rust 实现 |
|-------------|----------|
| `WorkflowExecutionCoordinator` | `WorkflowCoordinator` |
| `NodeExecutionCoordinator` | `NodeCoordinator` |
| `GraphBuilder` | `GraphBuilder` |
| `StateManager` | `StateManager` |
| 15 种节点执行逻辑 | `NodeHandler` trait + 具体实现 |

**状态机设计**：
```rust
pub enum NodeState {
    Pending,
    Running,
    Completed(NodeOutput),
    Failed(NodeError),
    Skipped,
    Paused,
}
```

**Fork/Join 并行执行**：
```rust
async fn execute_fork_node(&self, node_id: &str) -> Result<NodeOutput, NodeError> {
    let branches = self.graph.get_fork_branches(node_id)?;
    let handles: Vec<_> = branches.into_iter().map(|branch| {
        let coordinator = self.clone();
        tokio::spawn(async move { coordinator.execute_branch(branch).await })
    }).collect();
    let results = join_all(handles).await;
    NodeOutput::merge(results)
}
```

### 3.3 wf-checkpoint 集成（1.5 周）

- 将 checkpoint 系统集成到 executor
- 执行过程中自动创建 checkpoint
- 失败恢复机制

### 交付物

- [ ] wf-executor: 工具执行 + 审批引擎
- [ ] wf-executor: Workflow/Node 全类型支持
- [ ] Fork/Join 并行执行正确
- [ ] Checkpoint 集成到执行流程

---

## 六、Phase 4: Services 层（5 周）

### 目标

迁移 LLM 服务、MCP 工具系统、沙箱执行。

### 4.1 wf-llm（2 周）

**迁移内容**：

| 来源 TS 模块 | Rust 实现 |
|-------------|----------|
| LLM Client（OpenAI/Gemini/Anthropic） | `LlmProvider` trait + 具体实现 |
| `MessageStream` | SSE 流式解析 + `tokio_stream::Stream` |
| Tool Call 格式化器 | `ToolFormatter` trait |
| Token 计数器 | `tiktoken-rs` |

**关键技术决策**：
- SSE 解析：`eventsource-stream` 或自定义
- 多 Provider：trait 对象 + 工厂模式
- 流式工具调用：部分 JSON 拼接

### 4.2 wf-tools（2 周）

**迁移内容**：

| 来源 TS 模块 | Rust 实现 |
|-------------|----------|
| MCP Client（stdio/SSE/StreamableHTTP） | `McpClient` + `McpTransport` trait |
| 内置工具（filesystem, shell, workflow） | `BuiltinTool` trait + 具体实现 |
| 审批引擎 | `ApprovalEngine` |
| 使用分析 | `UsageAnalytics` |

**MCP 传输层**：
```rust
#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn connect(&mut self) -> Result<(), McpError>;
    async fn list_tools(&self) -> Result<Vec<McpTool>, McpError>;
    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, McpError>;
    async fn close(&mut self) -> Result<(), McpError>;
}
```

### 4.3 wf-sandbox（1 周）

**迁移内容**：

| 来源 TS 模块 | Rust 实现 |
|-------------|----------|
| JS/Python/Lua/Shell 沙箱 | `Sandbox` trait + 具体实现 |
| OS hooks（seccomp/proot/job-object） | 平台特定实现 |
| VFS | `VirtualFileSystem` |

### 交付物

- [ ] wf-llm: 多 Provider LLM 调用 + 流式
- [ ] wf-tools: MCP 协议兼容 + 内置工具
- [ ] wf-sandbox: 脚本安全执行

---

## 七、Phase 5: API 层（3 周）

### 目标

迁移 `packages/sdk` 的公开 API 和 `packages/sdk-kit` 的高级封装。

### 5.1 wf-api（2 周）

**迁移内容**：

| 来源 TS 模块 | Rust 实现 |
|-------------|----------|
| WorkflowBuilder / NodeBuilder | `WorkflowBuilder` / `NodeBuilder` |
| API Commands（execute/pause/resume/checkpoint） | `Command` trait + 具体实现 |
| Resources（registry/execution/message） | `Resource` trait |
| Config Processors（20+） | 具体实现 |

### 5.2 wf-sdk-kit 等价封装（1 周）

- `WorkflowManager` / `AgentManager`
- `Executor` / `Converter` 等高级 API

### 交付物

- [ ] wf-api: 完整公开 API 表面
- [ ] SDK 兼容性：与原 sdk-kit API 语义对齐

---

## 八、Phase 6: 应用层（3 周）

### 目标

构建 Rust 原生应用替代 TS apps。

### 6.1 wf-cli（1 周）

**来源**: `apps/cli-app`
- CLI 参数解析：`clap`
- 终端交互：复用 TS 的 commander 逻辑
- PTY 支持：`portable-pty`

### 6.2 wf-server（1.5 周）

**来源**: `apps/server`
- HTTP 框架：`axum`
- WebSocket：`tokio-tungstenite`
- SSE：`axum` 原生支持

### 6.3 wf-vscode（0.5 周，可选）

**来源**: `apps/vscode-app`
- VSCode Extension 需保持 TS（DAP/Extension API 限制）
- 核心逻辑通过 WASM 或子进程调用 Rust

### 交付物

- [ ] wf-cli: CLI 应用功能完整
- [ ] wf-server: HTTP/WS 服务功能完整
- [ ] E2E 测试通过

---

## 九、Phase 7: 测试与验证（2 周）

### 9.1 全量测试迁移

- 将 TS 测试用例翻译为 Rust 测试
- E2E 测试覆盖核心 workflow 场景

### 9.2 性能基准

| 指标 | 目标 |
|------|------|
| 100 节点 workflow 执行 | < TS 版本的 1/5 |
| 内存占用 | < TS 版本的 50% |
| 事件广播 | > 10K events/sec |
| 存储读取（缓存命中） | < 1ms P99 |

### 9.3 兼容性验证

- Checkpoint 数据兼容：Rust 可读取 TS 生成的 checkpoint
- 配置文件格式兼容

---

## 十、Phase 8: 收尾（1 周）

### 任务

- [ ] 移除 pnpm workspace / package.json / tsconfig
- [ ] 移除全部 TypeScript 源码（apps + packages）
- [ ] 更新 README 和文档
- [ ] CI 流水线简化（移除 Node.js 构建步骤）
- [ ] 最终性能基准报告

---

## 十一、依赖清单

### 新增 Rust Crates

| Crate | 用途 | 引入 Phase |
|-------|------|-----------|
| `moka` | 并发 LRU 缓存 | Phase 1 |
| `sha2` + `digest` | SHA-256 哈希 | Phase 1 |
| `flate2` | zlib 压缩 | Phase 1 |
| `dashmap` | 并发 HashMap | Phase 2 |
| `eventsource-stream` | SSE 解析 | Phase 4 |
| `tiktoken-rs` | Token 计数 | Phase 4 |
| `reqwest` | HTTP 客户端 | Phase 4 |
| `axum` | HTTP 框架 | Phase 6 |
| `tokio-tungstenite` | WebSocket | Phase 6 |
| `clap` | CLI 解析 | Phase 6 |
| `portable-pty` | PTY 支持 | Phase 6 |

### 已有依赖（无需新增）

serde, serde_json, chrono, thiserror, uuid, tokio, tracing, sqlx

---

## 十二、时间线

```
Week  1-3   │ Phase 1: 存储层补完
            │ ├── 数据模型 + 批量操作 + Checkpoint 补全
            │ └── 缓存 + 指标 + 维护 + 连接池 + 压缩
────────────┼────────────────────────────────────
Week  4-7   │ Phase 2: 核心基础设施
            │ ├── wf-core (事件 + 注册)
            │ ├── wf-checkpoint (分支 + 版本)
            │ ├── wf-config
            │ └── wf-runtime
────────────┼────────────────────────────────────
Week  8-13  │ Phase 3: 执行引擎
            │ ├── 工具执行器
            │ ├── Workflow/Node 协调器
            │ └── Checkpoint 集成
────────────┼────────────────────────────────────
Week 14-18  │ Phase 4: Services 层
            │ ├── wf-llm (多 Provider)
            │ ├── wf-tools (MCP + 内置工具)
            │ └── wf-sandbox
────────────┼────────────────────────────────────
Week 19-21  │ Phase 5: API 层
            │ ├── wf-api
            │ └── SDK Kit 封装
────────────┼────────────────────────────────────
Week 22-24  │ Phase 6: 应用层
            │ ├── wf-cli
            │ ├── wf-server
            │ └── (wf-vscode)
────────────┼────────────────────────────────────
Week 25-26  │ Phase 7: 测试与验证
────────────┼────────────────────────────────────
Week 27     │ Phase 8: 收尾清理
────────────┴────────────────────────────────────
```

**总计约 27 周（约 7 个月）**，建议 2-3 名 Rust 开发人员全职投入。

---

## 十三、与原方案的关键差异

| 维度 | 原方案（napi-rs 混合） | 本方案（纯 Rust） |
|------|----------------------|------------------|
| 目标架构 | Rust 核心 + TS 外壳 | 纯 Rust，无 TS 运行时 |
| 桥接层 | napi-rs 绑定（2 周） | 不存在 |
| 存储模型 | 简化 JSON blob | BLOB + 元数据分离（继承 TS 设计） |
| 缓存 | 未规划 | moka LRU（对标 TS 实现） |
| 数据兼容 | 双写验证 | 单向读取兼容（Rust 可读 TS 数据） |
| 迁移验证 | 对比 TS vs Rust | 功能正确性 + 性能基准 |
| 收尾 | 移除 TS 回退代码 | 移除全部 TS 源码 |

---

## 十四、风险与缓解

| 风险 | 影响 | 概率 | 缓解 |
|------|------|------|------|
| 执行引擎复杂度高 | 进度延迟 | 高 | 分节点类型逐步迁移；优先核心 5 种节点 |
| MCP Rust 生态不足 | 功能缺失 | 高 | 自行实现 JSON-RPC 2.0；优先 stdio |
| Checkpoint 数据不兼容 | 数据丢失 | 低 | 保留 JSON 反序列化路径；充分测试 |
| 异步运行时竞态 | 死锁/数据竞争 | 中 | tokio-console 调试；并发压力测试 |
| 团队 Rust 经验 | 进度延迟 | 中 | 代码 Review；遵循 TS 已验证的设计 |
