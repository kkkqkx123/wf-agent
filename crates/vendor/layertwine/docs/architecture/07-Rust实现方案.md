# 7. Rust 实现方案

## 7.1 技术栈

| 层面 | 选型 | 说明 |
|------|------|------|
| 语言 | Rust 1.88 | 内置 `async fn in trait`，无需 `async-trait` crate |
| 异步运行时 | `tokio` | 业界标准 async runtime |
| 存储引擎 | `rusqlite` | SQLite 嵌入式数据库，单文件、零配置 |
| 序列化 | `rkyv` + `serde_json` | 零拷贝反序列化、高性能（替代已停止维护的 `bincode`） |
| 哈希 | `blake3` | 内容寻址哈希，极速 |
| Diff 引擎 | `similar` | 纯 Rust 行级 diff/merge |
| CLI 框架 | `clap` v4 | 命令行解析 |
| 日志 | `tracing` | 结构化日志 + span 追踪 |
| 错误处理 | `thiserror` + `anyhow` | 分层错误处理 |
| 日期时间 | `chrono` | 时间戳管理 |
| UUID | `uuid` v7 | 时间排序 UUID（可选） |

## 7.2 项目结构

```
layertwine/
├── Cargo.toml
├── src/
│   ├── main.rs                    # CLI 入口
│   ├── lib.rs                     # 库入口
│   │
│   ├── core/                      # 核心数据类型
│   │   ├── mod.rs
│   │   ├── file_node.rs           # FileNode
│   │   ├── delta.rs               # Delta + LineDiff
│   │   ├── snapshot.rs            # Snapshot（核心不可变实体）
│   │   ├── partition.rs           # Partition（可变指针）
│   │   ├── layer.rs               # Layer + LayerType
│   │   └── types.rs               # 公共类型（ID 类型等）
│   │
│   ├── state_machine/             # 分层状态机
│   │   ├── mod.rs
│   │   ├── manual.rs              # manual_edit 层操作
│   │   ├── agent.rs               # agent_edit 层操作
│   │   ├── approval.rs            # approval 层操作
│   │   ├── staged.rs              # staged 层操作
│   │   └── transition.rs          # 层间流转逻辑
│   │
│   ├── backup/                    # 独立快照备份模块
│   │   ├── mod.rs
│   │   ├── backup_repo.rs         # 备份仓库
│   │   └── backup_snapshot.rs     # 备份快照类型
│   │
│   ├── checkpoint/                # 自研检查点仓库
│   │   ├── mod.rs
│   │   ├── checkpoint.rs          # Checkpoint
│   │   ├── branch.rs              # Branch
│   │   ├── repo.rs                # CheckpointRepo
│   │   └── dag.rs                 # 有向无环图
│   │
│   ├── git_sync/                  # Git 双向同步
│   │   ├── mod.rs
│   │   ├── git_bridge.rs          # Git 桥接
│   │   └── gc.rs                  # 冗余检查点 GC
│   │
│   ├── storage/                   # 持久化层
│   │   ├── mod.rs
│   │   ├── sqlite_storage.rs      # SQLite 存储实现
│   │   ├── migrations.rs          # 数据库迁移
│   │   └── repository.rs          # 仓库 trait 定义
│   │
│   ├── cli/                       # 命令行接口
│   │   ├── mod.rs
│   │   ├── commands.rs            # 子命令定义
│   │   └── output.rs              # 格式化输出
│   │
│   └── error.rs                   # 全局错误类型
```

## 7.3 Rust 1.88 特性利用

### 内置 async trait（无需 `async-trait` crate）

```rust
// Rust 1.88 原生支持
trait SnapshotStore: Send + Sync {
    async fn store_snapshot(&self, snapshot: Snapshot) -> Result<SnapshotId>;
    async fn get_snapshot(&self, id: SnapshotId) -> Result<Snapshot>;
    async fn find_snapshots(&self, filter: SnapshotFilter) -> Result<Vec<Snapshot>>;
}

trait CheckpointRepo: Send + Sync {
    async fn commit(&mut self, message: &str) -> Result<CheckpointId>;
    async fn create_branch(&mut self, name: &str) -> Result<()>;
    async fn merge_branches(&mut self, source: &str) -> Result<CheckpointId>;
}
```

### impl Trait 位置语法

```rust
// 返回复杂异步 trait
async fn create_repo(path: &Path) -> Result<impl SnapshotStore + CheckpointRepo> {
    SqliteRepo::new(path).await
}
```

## 7.4 关键 trait 设计

### 存储层接口

```rust
/// 文件节点存储
#[async_trait]
trait FileNodeStore {
    async fn store(&self, node: &FileNode) -> Result<()>;
    async fn get(&self, path: &Path) -> Result<Option<FileNode>>;
}

/// Delta 存储
#[async_trait]
trait DeltaStore {
    async fn store(&self, delta: &Delta) -> Result<DeltaId>;
    async fn get(&self, id: DeltaId) -> Result<Delta>;
    async fn get_by_snapshot(&self, snapshot_id: SnapshotId) -> Result<Vec<Delta>>;
}

/// Snapshot 存储
#[async_trait]
trait SnapshotStore {
    async fn store(&self, snapshot: &Snapshot) -> Result<SnapshotId>;
    async fn get(&self, id: SnapshotId) -> Result<Snapshot>;
    async fn get_parents(&self, id: SnapshotId) -> Result<Vec<Snapshot>>;
}
```

### 状态机接口

```rust
#[async_trait]
trait StateMachine {
    /// 获取指定层的当前快照
    async fn current_snapshot(&self, layer: LayerType) -> Result<Option<SnapshotId>>;

    /// 推送修改到指定层
    async fn push_edit(&mut self, layer: LayerType, deltas: Vec<Delta>) -> Result<SnapshotId>;

    /// 正向流转
    async fn forward(&mut self, from: LayerType, to: LayerType) -> Result<SnapshotId>;

    /// 逆向回退
    async fn rollback(&mut self, target: LayerType) -> Result<SnapshotId>;

    /// 合并回溯到具体父快照
    async fn rollback_to_parent(&mut self, target: LayerType, parent_id: SnapshotId) -> Result<()>;
}
```

## 7.5 错误处理

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("存储错误: {0}")]
    Storage(#[from] StorageError),

    #[error("快照未找到: {0}")]
    SnapshotNotFound(SnapshotId),

    #[error("分区未找到: {0}")]
    PartitionNotFound(PartitionId),

    #[error("无历史可回退")]
    NoHistory,

    #[error("无效的层间流转: {from} → {to}")]
    InvalidTransition { from: LayerType, to: LayerType },

    #[error("Git 同步错误: {0}")]
    GitSync(String),

    #[error("检查点冲突: {0}")]
    CheckpointConflict(String),

    #[error("序列化错误: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
```

## 7.6 SQLite Schema 设计

```sql
-- 不可变实体（只增）
CREATE TABLE file_nodes (
    path TEXT PRIMARY KEY,
    base_hash BLOB NOT NULL
);

CREATE TABLE deltas (
    id BLOB PRIMARY KEY,
    file_path TEXT NOT NULL REFERENCES file_nodes(path),
    diff BLOB NOT NULL,           -- rkyv 序列化的 LineDiff
    source_type INTEGER NOT NULL, -- 枚举值
    source_id TEXT,               -- AgentInstanceId（可选）
    timestamp INTEGER NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch('now'))
);

CREATE TABLE snapshots (
    id BLOB PRIMARY KEY,
    file_path TEXT NOT NULL REFERENCES file_nodes(path),
    delta_ids BLOB NOT NULL,      -- rkyv: Vec<DeltaId>
    parent_ids BLOB NOT NULL,     -- rkyv: Vec<SnapshotId>
    partition_type INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

-- 可变实体（可更新指针）
CREATE TABLE partitions (
    id BLOB PRIMARY KEY,
    name TEXT NOT NULL,
    layer_type INTEGER NOT NULL,
    current_snapshot BLOB REFERENCES snapshots(id),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE partition_history (
    partition_id BLOB NOT NULL REFERENCES partitions(id),
    snapshot_id BLOB NOT NULL REFERENCES snapshots(id),
    seq INTEGER NOT NULL,
    PRIMARY KEY (partition_id, seq)
);

CREATE TABLE layers (
    layer_type INTEGER PRIMARY KEY,
    partition_ids BLOB NOT NULL  -- rkyv: Vec<PartitionId>
);

-- 检查点仓库
CREATE TABLE checkpoints (
    id BLOB PRIMARY KEY,
    parent_ids BLOB NOT NULL,
    baseline_snapshot BLOB NOT NULL REFERENCES snapshots(id),
    author TEXT NOT NULL,
    message TEXT NOT NULL,
    git_anchor TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE branches (
    name TEXT PRIMARY KEY,
    head BLOB NOT NULL REFERENCES checkpoints(id),
    created_at INTEGER NOT NULL
);
```

## 7.7 内容寻址方案

```rust
/// 使用 Blake3 哈希作为内容 ID
#[derive(Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
struct ContentId([u8; 32]);

impl ContentId {
    fn from_content(data: &[u8]) -> Self {
        Self(blake3::hash(data).into())
    }
}

type SnapshotId = ContentId;
type DeltaId = ContentId;
type CheckpointId = ContentId;

/// 快照 ID 由其内容决定
impl Snapshot {
    fn compute_id(&self) -> SnapshotId {
        let data = serde_json::to_vec(self).unwrap();
        ContentId::from_content(&data)
    }
}
```
