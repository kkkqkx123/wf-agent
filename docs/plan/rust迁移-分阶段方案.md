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
│  wf-storage — Store + EntityStore + domain adapters           │
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

- [x] Cargo workspace 骨架，Cargo.toml + rust-toolchain.toml
- [x] `wf-types` — 20 种节点类型 + workflow/agent/checkpoint/execution 类型定义 + 12 storage 元数据类型
- [x] `wf-common` — Error、Result、时间、UUID 工具
- [x] `wf-storage` — Store/BatchStore/Maintainable trait + Memory/SQLite/Postgres 三后端 + EntityStore + 8 adapter traits + CachingStore/InstrumentedStore decorator + 压缩/哈希/错误体系
- [x] `wf-storage` 查询层 — `QueryFilter`（支持任意 metadata 字段过滤）+ `From<*ListOptions>` 转换 + 三后端 SQL 下推
- [x] `wf-core` — EventBus（`tokio::sync::broadcast`）+ Registry（泛型 `DashMap`）+ NodeState/WorkflowState 状态机（30 tests）
- [x] `wf-checkpoint` — CheckpointCoordinator + BranchManager + VersionManager + Delta/Diff + RestoreRegistry + Serializer + Strategy + Cleanup + Layertwine 集成（78 tests）
- [x] `wf-runtime` — bootstrap、lifecycle、mode、logger、storage_manager、error（完整实现，35 tests）

### 验收状态

```bash
cargo build --workspace         # ✅ 通过
cargo clippy --all-targets       # ✅ 通过
cargo test -p wf-types           # ✅ 通过
cargo test -p wf-storage         # ✅ 11 passed
cargo test -p wf-core            # ✅ 30 passed
cargo test -p wf-checkpoint      # ✅ 78 passed
cargo test -p wf-runtime         # ✅ 35 passed
```

---

## 三、Phase 1: 存储层补完（当前阶段）

### 目标

为 `wf-storage` 实现所有领域 storage adapter 的具体 struct（当前只有 trait 定义，无具体实现），使上层 crate 可以通过具体类型或 trait 引用完成数据持久化。

### 已完成

- [x] `QueryFilter` — 替换 `MetadataFilter`，新增 `fields: HashMap<String, String>` 支持任意 metadata 字段过滤
- [x] Memory/SQLite/Postgres 三后端均支持 `fields` 下推（JSON path → SQL 条件）  
- [x] `From<WorkflowListOptions> for QueryFilter` 等 6 个转换实现
- [x] `wf-checkpoint::StorageBackedStateManager.list_by_entity` 从内存过滤升级为 `QueryFilter` 下推

Phase 1 已全部完成。所有 14 个 adapter 的 trait 定义 + 3 后端具体实现 + EntityStore 绑定已完成。165 测试通过，clippy 无警告。

### 1.1 为已有的 8 个 trait 实现具体 struct ✅ 已完成

**实现方式**：通过 `make_base_adapter!` 宏在 `adapter/macro.rs` 中定义，`adapter/concrete.rs` 调用宏 + 手写领域方法。每个 adapter 为泛型 `WorkflowStorage<S: Store>`，自动生成 Memory/SQLite/Postgres 三后端类型别名。

| Trait | 实体类型 | 新增文件 |
|-------|---------|---------|
| `WorkflowStorageAdapter` | `WorkflowDefinition` | `entity_impl.rs` + `adapter/concrete.rs` |
| `WorkflowExecutionStorageAdapter` | `WorkflowExecution` | `entity_impl.rs` + `adapter/concrete.rs` |
| `CheckpointStorageAdapter` | `CheckpointStorageMetadata` | `entity_impl.rs` + `adapter/concrete.rs` |
| `TaskStorageAdapter` | `TaskStorageMetadata` | `entity_impl.rs` + `adapter/concrete.rs` |
| `AgentLoopStorageAdapter` | `AgentLoopStorageMetadata` | `entity_impl.rs` + `adapter/concrete.rs` |
| `MetricsStorageAdapter` | `MetricsDataPoint` | `adapter/concrete.rs`（独立实现，非 CRUD） |
| `FileCheckpointStorageAdapter` | `FileCheckpointStorageMetadata` | `entity_impl.rs` + `adapter/concrete.rs` |

**新增模块**：

```
src/
├── entity_impl.rs           # Entity trait 实现（6 个 domain 类型）
├── adapter/
│   ├── macro.rs             # make_base_adapter! 宏
│   └── concrete.rs          # 所有 8 个 adapter 的具体实现
```

**实现模式**：
- `make_base_adapter!` 宏生成泛型 struct + `BaseStorageAdapter` impl（CRUD 委托给 EntityStore，`list()` 通过 `From<*ListOptions> for QueryFilter` 转换）
- 领域方法（`update_status`, `list_versions`, `get_stats` 等）手写在 `concrete.rs` 的 `impl<S: Store> TraitName for Name<S>` 中
- `MetricsStorageAdapter` 不继承 `BaseStorageAdapter`，直接包装 `Store`

### 1.2 新增缺失的 adapter trait + 实现 ✅ 已完成

TS 有以下 adapter 但 Rust 中既无 trait 也无实现：

| Adapter | TS 来源 | 需要 | 领域方法 |
|---------|---------|------|---------|
| `TriggerStorageAdapter` | packages/storage | trait + impl | CRUD + list_by_event |
| `ToolStorageAdapter` | packages/storage | trait + impl | CRUD + get_stats |
| `ScriptStorageAdapter` | packages/storage | trait + impl | CRUD + list_by_type |
| `NodeTemplateStorageAdapter` | packages/storage | trait + impl | CRUD + list_by_node_type |
| `HookTemplateStorageAdapter` | packages/storage | trait + impl | CRUD + list_by_hook_type |
| `AgentProfileStorageAdapter` | packages/storage | trait + impl | CRUD + get_default |

**实现方式**：与 1.1 相同模式 — `make_base_adapter!` 宏 + 领域方法手写。每个 trait 定义在独立的 `adapter/*.rs` 文件。

**新增实体类型**（`wf-types/src/storage/`）：

| 类型 | 关键字段 | 领域方法 |
|------|---------|---------|
| `TriggerStorageMetadata` | id, name, event, enabled | `list_by_event` |
| `ToolStorageMetadata` | id, tool_id, tool_type, enabled | `get_stats` (按 type 分组) |
| `ScriptStorageMetadata` | id, name, language, enabled | `list_by_language` |
| `NodeTemplateStorageMetadata` | id, name, node_type | `list_by_node_type` |
| `HookTemplateStorageMetadata` | id, name, hook_type | `list_by_hook_type` |
| `AgentProfileStorageMetadata` | id, profile_id, name | `get_default` (返回第一个) |

**新增文件**：
- `wf-types/src/storage/trigger.rs` 等 6 文件 — 追加 Metadata struct
- `wf-storage/src/adapter/trigger.rs` 等 6 文件 — trait + ListOptions + From 转换
- `wf-storage/src/entity_impl.rs` — 追加 6 个 Entity impl
- `wf-storage/src/adapter/concrete.rs` — 追加宏调用 + 领域方法 + 18 个 type alias

### 1.3 更新 wf-runtime::StorageManager ✅ 已完成

**实现**：`StorageBackend` 枚举每个 variant 包含 14 个 adapter 字段。accessor 保持返回 `&dyn Store`（通过 adapter 的 `store()` 方法），`clear()` 遍历所有 14 个 adapter 调用 `store().clear()`。

### 任务概要

| 编号 | 内容 | 工作量 | 状态 |
|------|------|--------|------|
| 1.0 | `QueryFilter` 查询层改造（fields 下推 + From 转换 + 三后端适配） | 0.5 周 | ✅ |
| 1.1 | 为 8 个已有 trait 实现 concrete struct（Memory/SQLite/Postgres × 8） | 1.5 周 | ✅ |
| 1.2 | 新增 6 个缺失 adapter trait 定义 + 3 后端实现 | 1 周 | ✅ |
| 1.3 | 更新 wf-runtime StorageManager 以使用具体 adapter 类型 | 0.5 周 | ✅ |
| 1.4 | 集成测试：verify 每个 adapter 的 CRUD + 领域方法 | 0.5 周 | ✅ |
| 2.3 | wf-config 配置加载 + 验证（移至 Phase 1 并行） | 0.5 周 | ❌ |

---

## 四、Phase 2: 核心基础设施 ✅ 已完成（除 2.3 wf-config）

### 2.1 wf-core ✅ 已完成

**已实现模块**：

| 来源 TS 模块 | Rust 实现 | 技术选型 |
|-------------|----------|---------|
| `EventRegistry` | `EventBus` | `tokio::sync::broadcast` |
| `ToolRegistry` | `ToolRegistry` | `DashMap<String, Arc<dyn Tool>>` |
| `TaskRegistry` | `TaskRegistry` | `DashMap` |
| `NodeRegistry` / `NodeTypeRegistry` | `NodeTypeRegistry` | `DashMap` |
| `Registry` (通用) | `Registry<T>` | 泛型 DashMap 封装 |
| `ExecutionState` / 状态机 | `NodeState` enum + 转换逻辑 | 状态机模式 |

### 2.2 wf-checkpoint ✅ 已完成

**已实现模块**：

| 来源 TS 模块 | Rust 实现 | 模块 |
|-------------|----------|------|
| `CheckpointCoordinator` | `coordinator/` | Agent/Workflow 协调器 + restore |
| `BranchManager` | `branch/manager.rs` | 分支命名/解析 + 层级恢复 |
| `CheckpointVersionManager` | `version/manager.rs` | 版本管理 + 迁移 |
| `CheckpointStrategy` | `strategy/inner.rs` | 策略触发条件 |
| Delta 系统 | `delta/` | Diff 计算 + 增量恢复 |
| 序列化 | `serializer.rs` | JSON/Bincode + 自动检测 |
| 文件快照 | `file.rs` | Unified diff 生成/应用 |
| Layertwine 集成 | `layertwine.rs` | 文件编辑历史快照 |
| 缓存 | `cache.rs` | LRU checkpoint 缓存 |
| 指标 | `metrics/collector.rs` | 创建/恢复统计 |

### 2.3 wf-config ❌ 待实现（0.5 周，已从 Phase 1 移入）

**迁移内容**：
- `ConfigProcessor` — 配置文件加载器（skill, mcp, preset, config-index）
- `Orchestrator` — 配置编排器
- 继承 TS 的配置验证和合并逻辑

### 2.4 wf-runtime ✅ 已完成

**当前状态**：全部 7 个模块已实现并通过测试。

**剩余修改**（Phase 1 adapter 就绪后）：
- `StorageManager` accessor 从 `&dyn Store` 改为具体 adapter 类型
- `StorageBackend` 枚举扩展为全量 adapter 字段
- bootstrap 添加 `RuntimeHooks` 为上层预留回调点

---

## 五、Phase 3: 执行引擎（6 周）

### 目标

迁移核心执行引擎：Workflow/Node 协调器、Tool 执行器。

### 3.1 wf-executor — 工具执行器（1.5 周）

| 来源 TS 模块 | Rust 实现 |
|-------------|----------|
| `ToolCallExecutor` | `ToolCallExecutor` |
| `ApprovalEngine` | `ApprovalEngine` |

### 3.2 wf-executor — Workflow/Node 协调器（3 周）

| 来源 TS 模块 | Rust 实现 |
|-------------|----------|
| `WorkflowExecutionCoordinator` | `WorkflowCoordinator` |
| `NodeExecutionCoordinator` | `NodeCoordinator` |
| `GraphBuilder` | `GraphBuilder` |
| `StateManager` | `StateManager` |
| 15 种节点执行逻辑 | `NodeHandler` trait + 具体实现 |

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

### 4.1 wf-llm（2 周）

| 来源 TS 模块 | Rust 实现 |
|-------------|----------|
| LLM Client（OpenAI/Gemini/Anthropic） | `LlmProvider` trait + 具体实现 |
| `MessageStream` | SSE 流式解析 + `tokio_stream::Stream` |
| Tool Call 格式化器 | `ToolFormatter` trait |
| Token 计数器 | `tiktoken-rs` |

### 4.2 wf-tools（2 周）

| 来源 TS 模块 | Rust 实现 |
|-------------|----------|
| MCP Client（stdio/SSE/StreamableHTTP） | `McpClient` + `McpTransport` trait |
| 内置工具（filesystem, shell, workflow） | `BuiltinTool` trait + 具体实现 |
| 审批引擎 | `ApprovalEngine` |
| 使用分析 | `UsageAnalytics` |

### 4.3 wf-sandbox（1 周）

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

### 5.1 wf-api（2 周）

| 来源 TS 模块 | Rust 实现 |
|-------------|----------|
| WorkflowBuilder / NodeBuilder | `WorkflowBuilder` / `NodeBuilder` |
| API Commands（execute/pause/resume/checkpoint） | `Command` trait + 具体实现 |
| Resources（registry/execution/message） | `Resource` trait |
| Config Processors（20+） | 具体实现 |

### 5.2 SDK 生命周期集成

在 `wf-api` 中组合 `wf-runtime` 的 `Runtime` 与 SDK 核心逻辑：

```rust
pub struct AppRuntime {
    runtime: wf_runtime::Runtime,
    sdk: wf_api::SDK,
}

impl AppRuntime {
    pub async fn bootstrap(config: AppConfig) -> Result<Self> {
        let runtime = wf_runtime::Runtime::bootstrap(config.runtime).await?;
        let adapters = runtime.storage().collect_adapters(); // &dyn WorkflowStorageAdapter, etc.
        let sdk = wf_api::SDK::new(adapters).await?;
        sdk.wait_for_ready().await?;
        Ok(Self { runtime, sdk })
    }

    pub async fn shutdown(self) -> Result<()> {
        self.sdk.destroy().await?;
        self.runtime.shutdown().await
    }
}
```

### 交付物

- [ ] wf-api: 完整公开 API 表面
- [ ] AppRuntime: Runtime + SDK 生命周期集成

---

## 八、Phase 6: 应用层（3 周）

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

### 7.1 全量测试迁移

- 将 TS 测试用例翻译为 Rust 测试
- E2E 测试覆盖核心 workflow 场景

### 7.2 性能基准

| 指标 | 目标 |
|------|------|
| 100 节点 workflow 执行 | < TS 版本的 1/5 |
| 内存占用 | < TS 版本的 50% |
| 事件广播 | > 10K events/sec |
| 存储读取（缓存命中） | < 1ms P99 |

### 7.3 兼容性验证

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
| `dashmap` | 并发 HashMap | Phase 0 (已引入) |
| `eventsource-stream` | SSE 解析 | Phase 4 |
| `tiktoken-rs` | Token 计数 | Phase 4 |
| `reqwest` | HTTP 客户端 | Phase 4 |
| `axum` | HTTP 框架 | Phase 6 |
| `tokio-tungstenite` | WebSocket | Phase 6 |
| `clap` | CLI 解析 | Phase 6 |
| `portable-pty` | PTY 支持 | Phase 6 |

### 已有依赖（无需新增）

serde, serde_json, chrono, thiserror, uuid, tokio, tracing, tracing-subscriber, sqlx, async-trait, moka, sha2, flate2

---

## 十二、时间线

```
Week  1-4   │ Phase 0: 基础设施 — 全部完成
            │ ├── wf-types / wf-common ✅
            │ ├── wf-storage (Store + 3 backend + adapter traits) ✅
            │ ├── wf-core (EventBus + Registry + State) ✅
            │ ├── wf-checkpoint (Coordinator + Branch + Version + ...) ✅
            │ └── wf-runtime (Bootstrap + Lifecycle + ...) ✅
────────────┼────────────────────────────────────
Week  5-6   │ Phase 1: 存储层补完 ✅
            │ ├── [✅] QueryFilter 查询层
            │ ├── [✅] 8 adapter trait → 24 concrete struct
            │ ├── [✅] 6 新 adapter trait + 3 后端 (18 struct)
            │ ├── [✅] StorageManager 14 字段适配器集成
            │ ├── [✅] 11 个集成测试
            │ └── [❌] wf-config（移至 Phase 2）
────────────┼────────────────────────────────────
Week  6-10  │ Phase 3: 执行引擎
            │ ├── wf-executor (工具执行器 + 协调器)
            │ └── Checkpoint 集成
────────────┼────────────────────────────────────
Week 11-15  │ Phase 4: Services 层
            │ ├── wf-llm (多 Provider)
            │ ├── wf-tools (MCP + 内置工具)
            │ └── wf-sandbox
────────────┼────────────────────────────────────
Week 16-18  │ Phase 5: API 层
            │ ├── wf-api
            │ └── AppRuntime SDK 生命周期集成
────────────┼────────────────────────────────────
Week 19-21  │ Phase 6: 应用层
            │ ├── wf-cli
            │ ├── wf-server
            │ └── (wf-vscode)
────────────┼────────────────────────────────────
Week 22-23  │ Phase 7: 测试与验证
────────────┼────────────────────────────────────
Week 24     │ Phase 8: 收尾清理
────────────┴────────────────────────────────────
```

---

## 十三、与原方案的关键差异

| 维度 | 原方案（napi-rs 混合） | 本方案（纯 Rust） |
|------|----------------------|------------------|
| 目标架构 | Rust 核心 + TS 外壳 | 纯 Rust，无 TS 运行时 |
| 桥接层 | napi-rs 绑定（2 周） | 不存在 |
| 存储模型 | 简化 JSON blob | BLOB + 元数据分离（继承 TS 设计） |
| 缓存 | 未规划 | moka LRU（对标 TS 实现） |
| adapter 归属 | — | 具体实现在 wf-storage，trait 定义 + backend + EntityStore 统一管理 |
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
