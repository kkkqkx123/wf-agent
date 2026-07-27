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

### Rust Crate 依赖 DAG（实际）

```
wf-types  ←  wf-common
     ↓           ↓
wf-storage ← wf-config
     ↓           ↓
     └────→ wf-core ←────┬────→ wf-llm
                          │       ↓
                          ├────→ wf-tools
                          │       ↓
                          ├────→ wf-sandbox （未实现）
                          │       ↓
                          └────→ wf-executor → wf-checkpoint
                                         ↓
                                      wf-api （未实现）
                                         ↓
                               ┌────┬────┬────┐
                             wf-cli wf-server wf-runtime
```

---

## 二、Phase 0: 基础设施准备 ✅ 已完成

### 已完成内容

- [x] Cargo workspace 骨架，Cargo.toml + rust-toolchain.toml
- [x] `wf-types` — 30+ 模块，覆盖所有领域类型（workflow, agent, checkpoint, config, events, execution, hook, llm, message, node, script, storage, tool, trigger 等）
- [x] `wf-common` — Error、Result、时间、UUID 工具
- [x] `wf-storage` — Store/BatchStore/Maintainable trait + Memory/SQLite/Postgres 三后端 + EntityStore + 14 adapter traits + CachingStore/InstrumentedStore decorator + 压缩/哈希/错误体系
- [x] `wf-core` — EventBus（`tokio::sync::broadcast`）+ Registry（泛型 `DashMap`）+ NodeState/WorkflowState 状态机（30 tests）

### 验收状态

```bash
cargo build --workspace         # ✅ 通过（27 warnings，均为 async_fn_in_trait  cosmetic）
cargo test -p wf-types           # ✅ 通过
cargo test -p wf-storage         # ✅ 通过（11 tests）
cargo test -p wf-core            # ✅ 通过（30 tests）
```

---

## 三、Phase 1: 存储层补完 ✅ 已完成

### 已完成内容

- [x] `QueryFilter` — 替换 `MetadataFilter`，新增 `fields: HashMap<String, String>` 支持任意 metadata 字段过滤
- [x] Memory/SQLite/Postgres 三后端均支持 `fields` 下推（JSON path → SQL 条件）
- [x] `From<WorkflowListOptions> for QueryFilter` 等 6 个转换实现
- [x] 14 个 adapter trait 全部有具体 struct 实现（Memory/SQLite/Postgres × 14）
- [x] 6 个新增 adapter trait（Trigger, Tool, Script, NodeTemplate, HookTemplate, AgentProfile）
- [x] `wf-runtime::StorageManager` 升级为 14 字段 StorageBackend 枚举
- [x] 集成测试覆盖各 adapter CRUD + 领域方法

### 验收状态

```bash
cargo test -p wf-storage         # ✅ 通过（11 unit tests + integration tests）
cargo clippy --all-targets       # ✅ 通过
```

---

## 四、Phase 2: 核心基础设施 ✅ 已完成

### 2.1 wf-core ✅

**已实现模块**：

| 来源 TS 模块 | Rust 实现 | 技术选型 |
|-------------|----------|---------|
| `EventRegistry` | `EventBus` | `tokio::sync::broadcast` |
| `ToolRegistry` | `ToolRegistry` | `DashMap<String, Arc<dyn Tool>>` |
| `TaskRegistry` | `TaskRegistry` | `DashMap` |
| `NodeRegistry` / `NodeTypeRegistry` | `NodeTypeRegistry` | `DashMap` |
| `Registry` (通用) | `Registry<T>` | 泛型 DashMap 封装 |
| `ExecutionState` / 状态机 | `NodeState` enum + 转换逻辑 | 状态机模式 |

### 2.2 wf-checkpoint ✅

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

### 2.3 wf-config ✅

**已实现模块**：

| 来源 TS 模块 | Rust 实现 | 模块 |
|-------------|----------|------|
| `ConfigProcessor` | `processor/` | skill, mcp, preset, config-index 等 12 种处理器 |
| `Orchestrator` | `loader.rs` | 配置编排器 |
| 验证 | `validator.rs` | 配置验证 |
| 索引 | `index.rs` | 配置索引注册表 |
| 环境 | `env.rs` | 环境变量解析 |

### 2.4 wf-runtime ✅

**已实现模块**：

| 模块 | 功能 |
|------|------|
| `bootstrap` | 运行时引导 |
| `lifecycle` | 生命周期管理 |
| `mode` | 运行模式检测 |
| `logger` | 日志配置 |
| `storage_manager` | StorageManager（14 adapter 字段） |
| `recovery/orchestrator` | 恢复编排器 |
| `recovery/scanner` | 恢复扫描器 |

### 验收状态

```bash
cargo test -p wf-core            # ✅ 通过（30 tests）
cargo test -p wf-checkpoint      # ✅ 通过（78 tests）
cargo test -p wf-config          # ✅ 通过（88 tests）
cargo test -p wf-runtime         # ✅ 通过（35 tests）
```

---

## 五、Phase 3: 执行引擎 ✅ 已完成

### 3.1 wf-tools（Phase 4.2 提前完成）✅

**已实现模块**：

| 来源 TS 模块 | Rust 实现 | 模块 |
|-------------|----------|------|
| `ToolCallExecutor` | `executor/` | base, builtin, mcp, rest, stateful, stateless |
| MCP Client | `mcp/` | client, connection, transport (stdio/SSE/StreamableHTTP) |
| 审批引擎 | `approval.rs` | 工具调用审批 |
| 失败保护 | `failure_protection.rs` | 超时/重试保护 |
| 注册表 | `registry.rs` | 工具注册与查找 |
| 回调 | `callback.rs` | 工具调用回调 |

### 3.2 wf-executor ✅

**已实现模块**：

| 来源 TS 模块 | Rust 实现 | 功能 |
|-------------|----------|------|
| `AgentLoopExecutor` | `AgentLoopExecutor` | Agent 循环执行（LLM + Tool 交替） |
| `execute_agent_loop` | 完整实现 | 消息流 → LLM 调用 → 工具执行 → 结果返回 |
| `execute_workflow` | 完整实现 | Workflow 级执行编排 |
| `query_execution_status` | 完整实现 | 执行状态查询 |
| `cancel_execution` | 完整实现 | 执行取消 |

**依赖集成**：wf-executor 正确引用 wf-tools（工具注册表）和 wf-llm（LLM 客户端）。

### 验收状态

```bash
cargo test -p wf-tools           # ✅ 通过（23 tests）
cargo test -p wf-executor        # ✅ 编译通过（0 unit tests，逻辑待补充测试）
```

---

## 六、Phase 4: Services 层（进行中）

### 4.1 wf-llm ✅

**已实现模块**：

| 来源 TS 模块 | Rust 实现 | 模块 |
|-------------|----------|------|
| LLM Client | `client.rs` | 统一 LLM 客户端接口 |
| 多 Provider | `client_factory.rs` | 动态 Provider 创建 |
| Profile 管理 | `profile_manager.rs` | LLM Profile 配置管理 |
| 消息流 | `message_stream.rs` | SSE 流式解析 |
| 格式化器 | `formatters.rs` | OpenAI 格式化器 |
| 封装 | `wrapper.rs` | LLM 调用封装 |

### 4.2 wf-tools ✅

（见 Phase 3.1，已完成并验证 23 tests）

### 4.3 wf-sandbox ❌ 未实现（1 周）

**待实现内容**：
- `Sandbox` trait + 具体实现（JS/Python/Lua/Shell）
- OS hooks（seccomp/proot/job-object）
- VFS（VirtualFileSystem）

### 验收状态

```bash
cargo test -p wf-llm            # ✅ 编译通过（0 tests，逻辑已实现待测试）
cargo test -p wf-tools           # ✅ 通过（23 tests）
```

---

## 七、Phase 5: API 层 ❌ 未开始（3 周）

### 5.1 wf-api（2 周）

**待实现内容**：

| 来源 TS 模块 | Rust 实现 |
|-------------|----------|
| WorkflowBuilder / NodeBuilder | `WorkflowBuilder` / `NodeBuilder` |
| API Commands（execute/pause/resume/checkpoint） | `Command` trait + 具体实现 |
| Resources（registry/execution/message） | `Resource` trait |
| Config Processors（20+） | 具体实现 |

### 5.2 SDK 生命周期集成（1 周）

在 `wf-api` 中组合 `wf-runtime` 的 `Runtime` 与 SDK 核心逻辑：

```rust
pub struct AppRuntime {
    runtime: wf_runtime::Runtime,
    sdk: wf_api::SDK,
}

impl AppRuntime {
    pub async fn bootstrap(config: AppConfig) -> Result<Self> {
        let runtime = wf_runtime::Runtime::bootstrap(config.runtime).await?;
        let adapters = runtime.storage().collect_adapters();
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

## 八、Phase 6: 应用层 ❌ 未开始（3 周）

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

### 7.1 全量测试补充

- wf-llm、wf-executor 等 crate 补充单元测试
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

### 新增 Rust Crates（已引入）

| Crate | 用途 | 引入 Phase |
|-------|------|-----------|
| `dashmap` | 并发 HashMap | Phase 0 |
| `bincode` | 二进制序列化 | Phase 0 |
| `moka` | LRU 缓存 | Phase 0 |
| `reqwest` | HTTP 客户端 | Phase 4 (wf-llm) |
| `eventsource-stream` | SSE 解析 | Phase 4 (wf-llm) |
| `futures` | 异步工具 | Phase 4 |

### 待引入 Crates

| Crate | 用途 | 引入 Phase |
|-------|------|-----------|
| `axum` | HTTP 框架 | Phase 6 |
| `tokio-tungstenite` | WebSocket | Phase 6 |
| `clap` | CLI 解析 | Phase 6 |
| `portable-pty` | PTY 支持 | Phase 6 |

### 已有依赖（无需新增）

serde, serde_json, chrono, thiserror, uuid, tokio, tracing, tracing-subscriber, sqlx, async-trait, sha2, flate2, tokio-util, url, bytes, once_cell, regex, glob, toml, tempfile

---

## 十二、时间线

```
Week  1-4   │ Phase 0: 基础设施 — 全部完成
             │ ├── wf-types / wf-common ✅
             │ ├── wf-storage (Store + 3 backend + 14 adapters) ✅
             │ ├── wf-core (EventBus + Registry + State) ✅
             │ └── wf-checkpoint (Coordinator + Branch + Version + ...) ✅
             │ └── wf-runtime (Bootstrap + Lifecycle + StorageManager) ✅
             │ └── wf-config (Processor + Orchestrator + Validator) ✅
             │         [注：wf-config 原本在 Phase 2，实际随 Phase 0-1 并行完成]
────────────┼────────────────────────────────────
Week  5-6   │ Phase 1: 存储层补完 ✅
             │ ├── QueryFilter 查询层 ✅
             │ ├── 14 adapter concrete struct (3 后端) ✅
             │ └── StorageManager 14 字段适配器集成 ✅
             │         [注：已完成，含 Phase 2 原计划任务]
────────────┼────────────────────────────────────
Week  7-9   │ Phase 3: 执行引擎 ✅
             │ ├── wf-tools (MCP + 内置工具 + 审批) ✅
             │ └── wf-executor (AgentLoopExecutor + WorkflowCoordinator) ✅
             │         [注：Phase 3 与 Phase 4 部分任务并行完成]
             │         [注：wf-config 在 Phase 0-1 期间已完成]
             │         [注：wf-llm 在 Phase 3 期间已完成]
             │         [注：wf-tools 在 Phase 3 期间已完成]
────────────┼────────────────────────────────────
Week 10     │ Phase 4: Services 层 — 剩余 wf-sandbox
             │ └── wf-sandbox ❌（1 周）
             │         [注：wf-llm + wf-tools 已完成]
─── 分割 ───┼────────────────────────────────────
Week 11-13  │ Phase 5: API 层 ❌
             │ ├── wf-api
             │ └── AppRuntime SDK 生命周期集成
─── 分割 ───┼────────────────────────────────────
Week 14-16  │ Phase 6: 应用层 ❌
             │ ├── wf-cli
             │ ├── wf-server
             │ └── (wf-vscode)
─── 分割 ───┼────────────────────────────────────
Week 17-18  │ Phase 7: 测试与验证
─── 分割 ───┼────────────────────────────────────
Week 19     │ Phase 8: 收尾清理
─────────────┴────────────────────────────────────
```

---

## 十三、实际完成度统计

| Crate | 状态 | Tests | 说明 |
|-------|------|-------|------|
| wf-types | ✅ | 0（纯类型定义） | 30+ 模块，130+ 文件 |
| wf-common | ✅ | 0（纯工具） | Error/ID/Time |
| wf-storage | ✅ | 11 | 3 后端 + 14 adapters + EntityStore |
| wf-core | ✅ | 30 | EventBus + Registry + StateMachine |
| wf-checkpoint | ✅ | 78 | 完整实现 |
| wf-config | ✅ | 88 | ConfigProcessor + Orchestrator + Validator |
| wf-runtime | ✅ | 35 | Bootstrap + Lifecycle + StorageManager |
| wf-llm | ✅ | 0 | LLM Client + Profile + Formatter |
| wf-tools | ✅ | 23 | MCP + Builtin + Approval |
| wf-executor | ✅ | 0 | AgentLoopExecutor |
| wf-sandbox | ❌ | — | 未实现 |
| wf-api | ❌ | — | 未实现 |
| wf-cli | ❌ | — | 未实现 |
| wf-server | ❌ | — | 未实现 |

**当前总计：10/14 crate 完成，265 tests 通过，全量编译通过，clippy 无错误。**

---

## 十四、与原方案的关键差异

| 维度 | 原方案（napi-rs 混合） | 本方案（纯 Rust） |
|------|----------------------|------------------|
| 目标架构 | Rust 核心 + TS 外壳 | 纯 Rust，无 TS 运行时 |
| 桥接层 | napi-rs 绑定（2 周） | 不存在 |
| 存储模型 | 简化 JSON blob | BLOB + 元数据分离（继承 TS 设计） |
| 缓存 | 未规划 | moka LRU（对标 TS 实现） |
| wf-config | 原 Phase 2.3 | 实际随 Phase 0-1 并行完成 |
| wf-llm | 原 Phase 4.1 | 实际随 Phase 3 提前完成 |
| wf-tools | 原 Phase 4.2 | 实际随 Phase 3 提前完成 |
| wf-executor | 原 Phase 3 | 已完成 |
| 数据兼容 | 双写验证 | 单向读取兼容（Rust 可读 TS 数据） |
| 收尾 | 移除 TS 回退代码 | 移除全部 TS 源码 |

---

## 十五、风险与缓解

| 风险 | 影响 | 概率 | 缓解 |
|------|------|------|------|
| wf-sandbox 实现复杂度 | 进度延迟 | 中 | 延迟到 Phase 5+ 或使用 OS 原生沙箱 |
| wf-api 设计复杂度 | 进度延迟 | 中 | 参考 TS SDK 接口设计 |
| wf-executor 缺少测试 | 功能 bug | 中 | Phase 7 补充执行引擎测试 |
| MCP Rust 生态不足 | 功能缺失 | 低 | 自行实现 JSON-RPC 2.0；stdio 已完成 |
| Checkpoint 数据不兼容 | 数据丢失 | 低 | 保留 JSON 反序列化路径；充分测试 |
