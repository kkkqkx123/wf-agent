# Layertwine

[![Crates.io](https://img.shields.io/crates/v/layertwine.svg)](https://crates.io/crates/layertwine)
[![Documentation](https://docs.rs/layertwine/badge.svg)](https://docs.rs/layertwine)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Layertwine** —— 专为多 Agent 协同编辑 + 人工审核工作流设计的轻量级文件编辑历史存储层。

[English Documentation](README.md)

## 特性

- **分层状态机**：隔离的编辑层（`manual_edit`、`agent_edit`、`approval`、`staged`）与受控流转
- **不可变快照**：所有编辑创建不可变快照，以行级增量（Delta）存储
- **检查点仓库**：Git 风格的提交历史，支持分支、合并和 DAG 血缘关系
- **Agent 协作**：专用 Agent 编辑流程，包含人工审核工作流
- **Git 同步**：Layertwine 检查点与 Git 提交之间的双向同步
- **快照备份**：物理隔离的备份系统，用于关键恢复点
- **多传输接口**：CLI、HTTP REST 和 gRPC 接口共享相同核心逻辑

## 为什么选择 Layertwine？

传统版本控制（Git）无法处理来自多个来源的未提交更改。Layertwine 解决了这一问题：

1. **跟踪未提交编辑**：记录在进入 Git 之前的变化
2. **来源追溯**：区分人工编辑与代理生成的更改
3. **人工审核网关**：代理更改需经人工审核后才能集成
4. **安全回滚**：完整的审计追踪支持时间点恢复

## 快速入门

### 安装

```bash
# 带 CLI 支持构建（默认）
cargo install layertwine

# 或作为库使用
cargo add layertwine
```

### 初始化仓库

```bash
# 从当前目录初始化
layertwine init

# 或从现有 Git 仓库初始化
layertwine --git-repo /path/to/repo init --git-ref HEAD
```

### 编辑并提交

```bash
# 手动编辑
layertwine edit src/main.rs -c "fn main() { println!(\"Hello\"); }"

# 提交检查点
layertwine commit -m "初始提交" -a "developer"

# 查看历史
layertwine log
```

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

## 传输层

### CLI

```bash
# 所有命令支持 --json 输出模式
layertwine --help
layertwine status
layertwine branch list
layertwine checkpoint rollback <ID>
```

### HTTP API

```bash
# 启动服务器
LAYERTWINE_MODE=http cargo run --features http

# 初始化
curl -X POST http://127.0.0.1:8080/api/v1/init \
  -H 'Content-Type: application/json' -d '{}'

# 编辑文件
curl -X POST http://127.0.0.1:8080/api/v1/edit \
  -H 'Content-Type: application/json' \
  -d '{"file":"src/main.rs","content":"fn main() {}"}'
```

### gRPC API

```protobuf
// 连接到 localhost:50051
rpc Edit(EditRequest) returns (EditResponse);
rpc Commit(CommitRequest) returns (CommitResponse);
rpc Log(LogRequest) returns (LogResponse);
// ... 和 22 个其他 RPC 方法
```

## 多 Agent 工作流示例

```bash
# Agent A 做出更改并提交审核
layertwine agent agent-a edit src/auth.rs -c "pub fn login() {}"
layertwine agent agent-a submit

# Agent B 做出更改并提交审核
layertwine agent agent-b edit src/db.rs -c "pub fn connect() {}"
layertwine agent agent-b submit

# 查看待审核项
layertwine approval list

# 审批两个 Agent
layertwine approval approve agent-a
layertwine approval approve agent-b

# 合并审批并提交
layertwine approval merge-to-unified
layertwine approval merge-to-staged
layertwine commit -m "合并 auth 和 db 模块"
```

## 功能标志

| 功能 | 描述 |
|------|------|
| `cli` | 命令行界面（默认） |
| `http` | 通过 Axum 实现的 HTTP REST API |
| `grpc` | 通过 Tonic 实现的 gRPC API |
| `cli-http` | CLI + HTTP 组合 |
| `cli-grpc` | CLI + gRPC 组合 |
| `all` | 所有传输层 |

```bash
# 使用特定功能构建
cargo build --features http,grpc
```

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

所有实体 ID 均为它们规范 JSON 表示的 Blake3 哈希：
```rust
let id = blake3::hash(serde_json::to_vec(&entity).unwrap());
```

## 存储

- **数据库**：SQLite（嵌入式、单文件、事务性）
- **压缩**：大 Delta 链使用 Zstd 压缩
- **维护**：内置 GC、VACUUM 支持、WAL 检查点

## Git 集成

```bash
# 将 Layertwine 检查点提交到本地 Git 分支
layertwine --git-repo /path/to/repo git-commit -m "同步检查点"

# 从 Git 拉取提交到 Layertwine
layertwine --git-repo /path/to/repo pull --remote origin --git-ref main
```

注意：Git 同步是可选的，不会干扰活动编辑工作流。

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

Layertwine 提供结构化错误类型和退出码：

| 退出码 | 含义 |
|--------|------|
| 0 | 成功 |
| 1 | 一般错误（未找到、内部、存储） |
| 2 | 使用错误（无效参数、缺少参数） |

所有错误均包含可操作的解决建议。

## 项目结构

```
src/
├── core/           # 不可变数据类型（FileNode、Delta、Snapshot）
├── storage/        # SQLite 持久化（SqliteStorage、迁移）
├── engine/         # Diff/merge/inverse 操作
├── state_machine/  # 层转换逻辑
├── layered/        # 层实现（manual、agent、approval...）
├── checkpoint/     # 检查点仓库（branch、dag、repo）
├── backup/         # 快照备份模块
├── git_sync/       # Git 同步与 GC
├── api/            # 共享 API 服务与类型定义
├── cli/            # CLI 传输（基于 clap）
├── config/         # 配置管理
├── runtime/        # 运行时工具
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

- [CLI 使用指南](docs/user-guide/01-CLI 使用指南.md)
- [HTTP API 使用指南](docs/user-guide/02-HTTP-API 使用指南.md)
- [gRPC API 参考](docs/user-guide/03-gRPC-API 参考.md)
- [架构总览](docs/architecture/01-架构总览.md)

---

用 ❤️ 使用 Rust 构建
