# Layertwine

[![Crates.io](https://img.shields.io/crates/v/layertwine.svg)](https://crates.io/crates/layertwine)
[![Documentation](https://docs.rs/layertwine/badge.svg)](https://docs.rs/layertwine)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Layertwine** —— 专为多 Agent 协同编辑 + 人工审核工作流设计的轻量级文件编辑历史存储层。

Layertwine 是纯 infra 库 crate：以进程内方式嵌入使用（经由 `wf-checkpoint`），不提供 CLI、HTTP/gRPC 服务与独立二进制。

[English Documentation](README.md)

## 特性

- **分层状态机**：隔离的编辑层（`manual_edit`、`agent_edit`、`approval`、`staged`）与受控流转
- **不可变快照**：所有编辑创建不可变快照，以行级增量（Delta）存储
- **检查点仓库**：Git 风格的提交历史，支持分支、合并和 DAG 血缘关系
- **Agent 协作**：专用 Agent 编辑流程，包含人工审核工作流
- **Git 同步**：Layertwine 检查点与 Git 提交之间的双向同步
- **快照备份**：物理隔离的备份系统，用于关键恢复点
- **进程内接口**：存储引擎之上的单一 `ApiService` 门面，供同进程调用方使用

## 为什么选择 Layertwine？

传统版本控制（Git）无法处理来自多个来源的未提交更改。Layertwine 解决了这一问题：

1. **跟踪未提交编辑**：记录在进入 Git 之前的变化
2. **来源追溯**：区分人工编辑与代理生成的更改
3. **人工审核网关**：代理更改需经人工审核后才能集成
4. **安全回滚**：完整的审计追踪支持时间点恢复

## 快速入门

将本 crate 作为依赖引入，打开进程内服务：

```rust
use layertwine::api::{ApiService, CommitRequest, EditRequest, ServiceConfig};

let service = ApiService::open(ServiceConfig {
    db_path: ".layertwine/layertwine.db".into(),
    workspace_key: None,
})?;
```

在 `wf-agent` 工作区中，生产调用方经由 `wf-checkpoint`
（`FileCheckpointManager`）使用本引擎，由其负责归因、采样与合并策略。
直接使用 `ApiService` 仅面向测试与工具场景。

## 架构概览

```
┌─────────────────────────────────────────────────────────────┐
│               检查点仓库                                    │
│   （分支、提交、DAG 历史）                                 │
└───────────────┬───────────────────────────────┬─────────────┘
                │                               │
                ▼                               ▼
┌─────────────────────────┐     ┌──────────────────────────────┐
│  分层状态机             │     │     快照备份                 │
│  ┌───────────────────┐  │     │  （物理隔离）              │
│  │ manual_edit       │  │     └──────────────────────────────┘
│  │ agent_edit        │──┼────────► approval ◄── Agent 流程
│  │ staged            │  │
│  └───────────────────┘  │
└─────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Git 仓库                                 │
│   （长期持久化，周期性同步）                               │
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
打开服务、应用分层编辑、持久化检查点。

```rust
let edit = service.edit(EditRequest {
    file: "src/main.rs".into(),
    content: Some("fn main() {}\n".into()),
})?;
let commit = service.commit(CommitRequest {
    message: "initial commit".into(),
    author: Some("dev-1".into()),
})?;
```

没有网络传输层，也没有功能标志：当 Layertwine 收敛为纯 infra
库 crate 时，原 CLI、HTTP 与 gRPC 层已被移除。

## 多 Agent 工作流

每个 Agent 在各自隔离分区中编辑并提交审核，人工审批后合并到
staged 并提交——生命周期与之前相同，只是经由 `ApiService`
（或 `wf-checkpoint`）驱动，而不再使用 shell 命令。

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

Git 同步（检查点导出/导入）与 GC 由嵌入方经存储引擎驱动。
Git 同步是可选的，不会干扰活动编辑工作流。

## 测试

```bash
# 单元测试
cargo test --lib

# 所有测试（单元 + 集成 + e2e）
cargo test

# 仅 e2e 测试
cargo test --test e2e_tests
```

## 性能基准

详见 [benches/PERFORMANCE_ANALYSIS.md](benches/PERFORMANCE_ANALYSIS.md) 中的详细性能分析。

```bash
# 运行基准测试
cargo bench
```

## 错误处理

Layertwine 提供结构化错误类型（`LayertwineError`、`StorageError`），
覆盖存储、引擎、检查点、恢复、完整性、Git 同步与 GC 失败。
所有错误均携带可操作的详情信息。

## 项目结构

```
src/
├── core/           # 不可变数据类型（FileNode、Delta、Snapshot）
├── storage/        # SQLite 持久化（SqliteStorage、迁移）
├── engine/         # Diff/merge/词级 diff 操作
├── layered/        # 层实现（manual、agent、approval...）
├── checkpoint/     # 检查点仓库（branch、dag、repo）
├── backup/         # 快照备份模块
├── git_sync/       # Git 同步与 GC
├── api/            # 进程内服务门面与类型定义
├── config/         # 配置管理
└── error.rs        # 错误类型定义

tests/
├── common/         # 测试配置和辅助工具
├── e2e/            # 端到端测试场景
└── ...
```

## 贡献

1. Fork 仓库
2. 创建功能分支
3. 编写测试代码
4. 确保所有测试通过：`cargo test`
5. 提交拉取请求

## 许可证

MIT 许可证 - 详见 [LICENSE](LICENSE)

## 文档

- [架构总览](docs/architecture/01-架构总览.md)

---

用 ❤️ 使用 Rust 构建
