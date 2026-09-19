# Layertwine

**Layertwine** —— 专为多 Agent 协同编辑 + 人工审核工作流设计的轻量级文件编辑历史存储层。

Layertwine 是纯 infra 库 crate：以进程内方式嵌入使用（经由 `wf-checkpoint`），不提供 CLI、HTTP/gRPC 服务与独立二进制，也不再作为独立包发布。

[English Documentation](README.md)

## 特性

- **分层状态机**：隔离的编辑层（`manual_edit`、`agent_edit`、`approval`、`staged`）与受控流转
- **不可变快照**：所有编辑创建不可变快照，以行级增量（Delta）存储
- **检查点仓库**：支持分支、合并和 DAG 血缘关系的提交历史
- **Agent 协作**：专用 Agent 编辑流程，包含人工审核工作流
- **GC**：由嵌入方驱动的冗余检查点垃圾回收

## 为什么选择 Layertwine？

传统版本控制（Git）无法处理来自多个来源的未提交更改。Layertwine 解决了这一问题：

1. **跟踪未提交编辑**：记录在进入 Git 之前的变化
2. **来源追溯**：区分人工编辑与代理生成的更改
3. **人工审核网关**：代理更改需经人工审核后才能集成
4. **安全回滚**：完整的审计追踪支持时间点恢复

## 快速入门

将本 crate 作为路径依赖引入，直接使用存储引擎：

```rust
use layertwine::storage::SqliteStorage;
use std::path::Path;

let storage = SqliteStorage::new_full(Path::new(".layertwine/layertwine.db"))?;
```

在 `wf-agent` 工作区中，生产调用方经由 `wf-checkpoint`
（`FileCheckpointManager`）使用本引擎，由其负责归因、采样与合并策略。
不再提供独立服务门面：原 `ApiService`、备份仓库、TOML 配置链与 Git 桥接
已在放弃独立包方向时移除。

## 架构概览

```
┌─────────────────────────────────────────────────────────────┐
│               检查点仓库                                    │
│   （分支、提交、DAG 历史）                                 │
└───────────────────────────────┬─────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────┐
│  分层状态机                                                 │
│  ┌───────────────────┐                                      │
│  │ manual_edit       │                                      │
│  │ agent_edit        │──► approval ──► integrated ──► staged │
└─────────────────────────────────────────────────────────────┘
```

### 核心概念

| 概念 | 描述 |
|------|------|
| **层** | 编辑隔离边界（manual, agent, approval, staged） |
| **分区** | 层内的工作空间（如 `agent:agent-01`） |
| **快照** | 在特定时间点捕获的文件状态 |
| **Delta** | 行级更改描述（插入/删除/替换） |
| **检查点** | 关联一个或多个快照的命名提交 |
| **分支** | 指向检查点谱系的可移动指针 |

## 嵌入使用

Layertwine 以进程内方式运行。典型流程（由 `wf-checkpoint` 执行）是：
打开 `SqliteStorage`、经 `layered::*` 应用分层编辑、经 `CheckpointRepo`
持久化检查点。

没有网络传输层，也没有功能标志：当 Layertwine 收敛为纯 infra
库 crate 时，原 CLI、HTTP、gRPC 层，连同独立 `ApiService`、备份仓库、
TOML 配置链与 Git 桥接均已移除。

## 多 Agent 工作流

每个 Agent 在各自隔离分区中编辑并提交审核，人工审批后合并到
staged 并提交——由 `wf-checkpoint` 在本引擎之上驱动。

## 数据模型

### 不可变保证

- **快照**：仅插入，永不修改或删除
- **Delta**：仅插入，形成不可变链
- **检查点**：仅插入，通过父引用形成 DAG

### 可变状态

仅分区指针和层状态可变：
- `partitions`：当前快照引用
- `partition_history`：每个分区的 Delta 链
- `layers`：转换元数据

### 内容寻址 ID

Snapshot、Checkpoint、FileNode 的 ID 均为其规范 JSON 表示的 Blake3 哈希。Delta 的 ID 额外包含编辑时间戳与进程内单调递增的调用序号，因此即使内容完全相同，每次编辑工具调用也会生成唯一记录：
```rust
let id = blake3::hash(serde_json::to_vec(&entity).unwrap());
```

## 存储

- **数据库**：SQLite（嵌入式、单文件、事务性）
- **压缩**：大 Delta 链使用 Zstd 压缩
- **维护**：内置 GC、VACUUM 支持、WAL 检查点

## Git 集成

GC 由嵌入方经存储引擎驱动。

## 测试

```bash
# 单元测试
cargo test -p layertwine --lib

# 所有 layertwine 测试（单元 + 集成）
cargo test -p layertwine
```

## 性能基准

```bash
# 运行基准测试
cargo bench -p layertwine
```

## 错误处理

Layertwine 提供结构化错误类型（`LayertwineError`、`StorageError`），
覆盖存储、引擎、检查点、完整性与 GC 失败。
所有错误均携带可操作的详情信息。

## 项目结构

```
src/
├── core/           # 不可变数据类型（FileNode、Delta、Snapshot）
├── storage/        # SQLite 持久化（SqliteStorage、迁移）
├── engine/         # Diff/merge/词级 diff 操作
├── layered/        # 层实现（manual、agent、approval...）
├── checkpoint/     # 检查点仓库（branch、repo、types、gc）
└── error.rs        # 错误类型定义

tests/
├── engine_integration.rs
├── layered_integration.rs
└── ...
```

## 文档

- [架构总览](docs/architecture/01-架构总览.md)
