# Storage 层补完方案

## 一、设计原则

### 1.1 核心理念

**迁移成本远低于后期重构代价**。TS 版 storage 经过生产验证，Rust 重写时直接继承其数据模型和架构决策。同时利用 Rust 类型系统和生态优势修复 TS 版的设计缺陷。

### 1.2 关键设计继承

| TS 设计 | Rust 实现策略 |
|---------|--------------|
| BLOB + 元数据分离 | 直接沿用：`data BLOB` + `metadata JSON` 双列存储 |
| LRU 读写穿透缓存 | `moka` crate 实现，语义对齐 TS |
| SHA-256 完整性校验 | `sha2` crate，采样策略对齐 TS |
| 5 类错误体系 | `StorageError` 枚举扩展，语义一一对应 |
| 操作指标收集 | `StorageMetrics` struct + `AtomicU64` 原子计数 |
| PRAGMA 集中配置 | 配置 struct + 初始化时批量执行 |
| 连接池增强 | 应用层 PoolManager（指标 + 健康检查） |

### 1.3 不直接移植的 TS 模式（Rust 惯用替代）

| TS 模式 | 问题 | Rust 替代 |
|---------|------|-----------|
| 4 层继承链（StorageAdapterBase → KeyValueStorageBase → BaseSqliteStorage → 具体类） | Rust 无继承，模板方法模式不自然 | `EntityStore<I, T>` 泛型组合 + `Entity` trait |
| 模板方法 `save()` 调用 `doSave()` | 抽象方法爆炸 | 装饰器模式：`CachingStore<S>` 包裹任意存储 |
| 回调式缓存包装 `loadFromCache(id, loadFn)` | 闭包生命周期复杂 | 结构体字段 `cache: EntityCache` |
| 全局可变单例 `getGlobalPool()` | Rust 禁止隐式全局可变 | 显式依赖注入或 `once_cell::Lazy` |
| `any` 类型 DB 行转换 | 运行时类型不安全 | `sqlx::query_as::<_, (String,)>` 编译期检查 |
| `Uint8Array` 防御性复制 | 每个 save/load 都复制 | 借用 `&[u8]` + 所有权，零拷贝 |
| 字符串拼接构造 metrics key | 运行时错误、无类型安全 | `enum Operation` + `match` + 结构化 struct |

### 1.4 不采纳的 TS 特性

| TS 特性 | 不采纳原因 |
|---------|-----------|
| 延迟/错误模拟 | 纯测试特性，Rust 用 `mockall` 或集成测试替代 |
| Statement Cache | sqlx 内部已做 prepared statement cache，无需应用层重复 |
| 外部连接注入 | Rust 可通过依赖注入实现，不需要 `setExternalDb` 模式 |

---

## 二、TS 设计缺陷及修复

以下 TS 实现中的已知问题，在 Rust 重写时一并修复：

### 2.1 N+1 查询问题

**TS 问题**（`storage-adapter-base.ts:244-249`）：`loadBatch` 默认实现对每个 id 单独查询。`list_by_entity`、`get_latest_by_entity`、`delete_by_entity` 均为"全表扫描 + 内存过滤"模式。

**Rust 修复**：
- `list_by_entity` → `WHERE metadata->>'entityId' = $1`
- `get_latest_by_entity` → `WHERE ... ORDER BY created_at DESC LIMIT $1`
- `delete_by_entity` → `DELETE FROM ... WHERE ...`
- `load_batch` → `WHERE id IN (...)` 或 `= ANY($1)`

### 2.2 批量操作串行化

**TS 问题**（`storage-adapter-base.ts:236-242`）：`saveBatch` 默认实现为 sequential `for await`。Postgres 子类虽优化为 `WHERE id = ANY`，但仍是逐条 `saveToClient`。

**Rust 修复**：
```rust
// SQLite: 单事务多 INSERT
// INSERT INTO table (id, data, metadata) VALUES (?1, ?2, ?3), (?4, ?5, ?6), ...
// PostgreSQL: UNNEST 批量
// INSERT INTO table SELECT * FROM UNNEST($1::text[], $2::bytea[], $3::jsonb[])
```

### 2.3 非线程安全计数器

**TS 问题**（`base-sqlite-storage.ts:318-326`）：`this.loadCounter++` 非原子操作；metrics 中 `updateMetric` 使用字符串拼接构造 key（`${operation}Count`），`as keyof` 强制转换隐藏类型错误。

**Rust 修复**：
- 所有计数器使用 `AtomicU64`
- Metrics 使用结构化 struct，字段访问编译期检查

```rust
pub struct StorageMetrics {
    pub save: OperationMetrics,
    pub load: OperationMetrics,
    pub delete: OperationMetrics,
    pub list: OperationMetrics,
}

pub struct OperationMetrics {
    pub count: AtomicU64,
    pub total_time_ms: AtomicU64,
    pub total_bytes: AtomicU64,
}
```

### 2.4 MemoryStore close() 清除数据

**TS 问题**（`base-memory-storage.ts:151-155`）：`close()` 调用 `store.clear()` 删除全部数据，语义错误。

**Rust 修复**：`close()` 仅释放资源（关闭连接池），不删除数据。

### 2.5 clear() 使用 DELETE 而非 TRUNCATE

**TS 问题**（`key-value-storage-base.ts:294-306`）：`DELETE FROM table` 全表扫描记录日志。Postgres 子类的 `clear()` 删除两个表无事务保护。

**Rust 修复**：
- SQLite: `DELETE FROM`（SQLite 不支持 TRUNCATE，但可用 `DELETE` + `VACUUM`）
- PostgreSQL: `TRUNCATE TABLE`（O(1)，比 DELETE 快 100x+）
- 多表 clear 使用事务包装

### 2.6 连接串日志泄露

**TS 问题**（`base-postgres-storage.ts:101-103`）：错误日志直接输出完整 connectionString（可能含密码）。

**Rust 修复**：日志输出前脱敏处理，只显示 host/port/database，不含 password。

### 2.7 SQLite max_connections(1) 串行化

**TS 问题**（`sqlite.rs:36`）：`max_connections(1)` 串行化所有操作，即使 WAL 模式支持并发读。

**Rust 修复**：
```rust
SqlitePoolOptions::new()
    .max_connections(8)           // WAL 模式支持并发读
    .connect_with(
        SqliteConnectOptions::new()
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5))
            .synchronous(SqliteSynchronous::Normal)
    )
```

### 2.8 update_status 读-改-写

**TS 问题**（`sqlite.rs:182-186`）：加载完整实体 → 修改字段 → 写回，两次 IO 且非原子。

**Rust 修复**：直接 SQL UPDATE：
```sql
UPDATE workflow_executions SET metadata = jsonb_set(metadata, '{status}', $1) WHERE id = $2
```

---

## 三、Rust 及库特性调整

### 3.1 sqlx 异步-only 约束

**差异**：TS 用 `better-sqlite3`（同步）包装为 async。Rust sqlx 纯异步。

**调整**：
- 所有 trait 方法为 `async fn`，需 tokio 运行时
- trait 增加 `Send + Sync` 边界（跨 task 安全）
- 不能从同步代码直接调用（需 `block_on`）

### 3.2 参数占位符差异

**差异**：SQLite 用 `?`，PostgreSQL 用 `$1/$2`。

**调整**：每个后端独立的 SQL 字符串，不能共享。使用 `const fn` 或宏生成后端特定 SQL。

### 3.3 sqlx 类型系统约束

**差异**：`Vec<u8>` 在 SQLite 映射为 `BLOB`，PostgreSQL 映射为 `BYTEA`。`serde_json::Value` 在 SQLite 映射为 `TEXT`，PostgreSQL 映射为 `JSONB`。

**调整**：
- `data` 列：统一使用 `Vec<u8>` 存储原始 bytes
- `metadata` 列：SQLite 用 `TEXT`（存储 JSON 字符串），PostgreSQL 用 `JSONB`（原生 JSON 支持索引）
- sqlx `query_as` 需显式类型标注

### 3.4 所有权消除防御性复制

**TS 问题**：`Uint8Array` 引用语义，每次传递都 `new Uint8Array(data)` 复制。

**Rust 优化**：
- `save` 接受 `&[u8]`（借用），不获取所有权
- `load` 返回 `Vec<u8>`（新分配，因数据来自 DB）
- `Entity::serialize` 返回 `Vec<u8>`，调用方可选择格式
- 缓存层使用 `Arc<[u8]>` 避免复制

### 3.5 枚举类型替代字符串

**TS 问题**：`status: string` 等字段运行时比较。

**Rust 优化**：
```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus { Pending, Running, Completed, Failed, Cancelled }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus { Queued, InProgress, Done, Failed }
```

穷尽式 `match` 编译期保证无遗漏。

### 3.6 sqlx::query! 编译期验证

**机会**：`sqlx::query!` 宏在编译期验证 SQL 语法和类型。

**权衡**：需要编译期数据库连接（`DATABASE_URL`）。

**决策**：
- 关键查询（CRUD 核心路径）使用 `sqlx::query!`，CI 提供 SQLite 测试库
- 动态查询（元数据过滤等）使用 `sqlx::query`（运行时字符串）
- 使用 `sqlx::migrate!` 管理 schema 变更

### 3.7 装饰器模式替代继承

**核心架构决策**：

```
                    ┌─────────────────┐
                    │  BaseStorageAdapter trait  │
                    └────────┬────────┘
                             │ 实现
              ┌──────────────┼──────────────┐
     ┌────────▼───────┐ ┌───▼──────┐ ┌────▼──────────┐
     │ SqliteStorage  │ │ Postgres │ │ MemoryStorage │
     └───────┬────────┘ └────┬─────┘ └───────┬───────┘
             │               │               │
             └───────────────┼───────────────┘
                             │ 被包裹
              ┌──────────────▼──────────────┐
              │   CachingStore<S>           │
              │   - cache: moka::Cache      │
              │   - inner: S                │
              │   - 实现 read-through/write- │
              │     through/invalidate       │
              └──────────────┬──────────────┘
                             │ 被包裹
              ┌──────────────▼──────────────┐
              │   InstrumentedStore<S>       │
              │   - metrics: Arc<Metrics>    │
              │   - inner: S                │
              │   - 每个操作自动计时计数       │
              └──────────────┬──────────────┘
                             │ 被包裹
              ┌──────────────▼──────────────┐
              │   EntityStore<I, T>          │
              │   - 类型 T 序列化/反序列化    │
              │   - 元数据提取               │
              │   - hash 计算                │
              └─────────────────────────────┘
```

每一层独立可测试，自由组合。

### 3.8 取消安全

**TS 问题**：`setInterval` 维护定时器不调用 `.unref()`，阻止 Node.js 退出。

**Rust 修复**：使用 `tokio::select!` 配合 `CancellationToken`：
```rust
pub async fn maintenance_loop(
    pool: SqlitePool,
    interval: Duration,
    cancel: CancellationToken,
) {
    let mut ticker = tokio::time::interval(interval);
    loop {
        tokio::select! {
            _ = ticker.tick() => { /* do maintenance */ }
            _ = cancel.cancelled() => break,
        }
    }
}
```

取消安全，资源自动释放。

---

## 四、数据模型重构

### 4.1 存储模型

```
┌──────────────────────────────────────────────────────────────┐
│ entity_table                                                  │
│ ─────────                                                    │
│ id          TEXT PRIMARY KEY                                 │
│ data        BLOB NOT NULL              -- 压缩后的原始数据     │
│ metadata    JSON NOT NULL              -- 结构化元数据        │
│ hash        TEXT NOT NULL              -- SHA-256 校验值      │
│ data_size   INTEGER NOT NULL           -- 原始数据大小        │
│ compressed  BOOLEAN NOT NULL DEFAULT FALSE                   │
│ created_at  TIMESTAMP NOT NULL                                │
│ updated_at  TIMESTAMP NOT NULL                                │
│                                                              │
│ INDEX idx_metadata_entity_type ON ((metadata->>'entityType')) │
│ INDEX idx_metadata_status ON ((metadata->>'status'))          │
└──────────────────────────────────────────────────────────────┘
```

### 4.2 RawStorage Trait（底层 BLOB+metadata 存储）

```rust
#[async_trait]
pub trait RawStorage: Send + Sync {
    async fn initialize(&self) -> Result<(), StorageError>;
    async fn close(&self) -> Result<(), StorageError>;
    async fn clear(&self) -> Result<(), StorageError>;

    async fn save(&self, id: &str, data: &[u8], metadata: &serde_json::Value) -> Result<(), StorageError>;
    async fn load(&self, id: &str) -> Result<Option<(Vec<u8>, serde_json::Value)>, StorageError>;
    async fn delete(&self, id: &str) -> Result<(), StorageError>;
    async fn list(&self, filter: Option<&MetadataFilter>) -> Result<Vec<(String, serde_json::Value)>, StorageError>;
    async fn exists(&self, id: &str) -> Result<bool, StorageError>;

    async fn save_batch(&self, items: &[BatchItem<'_>]) -> Result<(), StorageError>;
    async fn delete_batch(&self, ids: &[String]) -> Result<(), StorageError>;

    /// 维护操作（SQLite/Postgres 实现，Memory 空操作）
    async fn vacuum(&self) -> Result<(), StorageError> { Ok(()) }
    async fn checkpoint(&self) -> Result<(), StorageError> { Ok(()) }
}

pub struct MetadataFilter {
    pub entity_type: Option<String>,
    pub status: Option<String>,
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

pub struct BatchItem<'a> {
    pub id: String,
    pub data: Vec<u8>,
    pub metadata: &'a serde_json::Value,
}
```

### 4.3 Entity Trait

```rust
pub trait Entity: Serialize + DeserializeOwned + Send + Sync {
    type Metadata: Serialize + DeserializeOwned + Send + Sync + Clone;

    fn entity_id(&self) -> &str;
    fn entity_type() -> &'static str;
    fn metadata(&self) -> Self::Metadata;

    /// 序列化为 bytes（默认 JSON，可覆盖为 bincode/protobuf）
    fn to_bytes(&self) -> Result<Vec<u8>, StorageError> {
        serde_json::to_vec(self).map_err(StorageError::from)
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, StorageError> {
        serde_json::from_slice(bytes).map_err(StorageError::from)
    }

    /// 是否需要压缩（默认 > 1KB）
    fn should_compress(data_len: usize) -> bool {
        data_len > 1024
    }
}
```

### 4.4 EntityStore

```rust
pub struct EntityStore<S, T> {
    storage: S,
    _marker: PhantomData<T>,
}

impl<S, T> EntityStore<S, T>
where
    S: RawStorage,
    T: Entity,
{
    pub fn new(storage: S) -> Self {
        Self { storage, _marker: PhantomData }
    }

    pub async fn save(&self, entity: &T) -> Result<(), StorageError> {
        let id = entity.entity_id();
        let metadata_json = serde_json::to_value(entity.metadata())?;
        let data = entity.to_bytes()?;
        let (compressed, was_compressed) = maybe_compress(&data)?;
        self.storage.save(id, &compressed, &metadata_json).await
    }

    pub async fn load(&self, id: &str) -> Result<Option<T>, StorageError> {
        match self.storage.load(id).await? {
            Some((data, _metadata)) => {
                let decompressed = maybe_decompress(&data)?;
                T::from_bytes(&decompressed).map(Some)
            }
            None => Ok(None),
        }
    }
}
```

### 4.5 CachingStore（装饰器）

```rust
pub struct CachingStore<S> {
    inner: S,
    cache: EntityCache,
}

impl<S: RawStorage> CachingStore<S> {
    pub fn new(inner: S, cache_config: CacheConfig) -> Self {
        Self { inner, cache: EntityCache::new(cache_config) }
    }
}

impl<S: RawStorage> RawStorage for CachingStore<S> {
    async fn load(&self, id: &str) -> Result<Option<(Vec<u8>, serde_json::Value)>, StorageError> {
        if let Some(entry) = self.cache.get(id) {
            return Ok(Some((entry.data, entry.metadata)));
        }
        match self.inner.load(id).await? {
            Some(entry) => {
                self.cache.insert(id.to_string(), entry.clone());
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    async fn save(&self, id: &str, data: &[u8], metadata: &serde_json::Value) -> Result<(), StorageError> {
        self.inner.save(id, data, metadata).await?;
        self.cache.invalidate(id);
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<(), StorageError> {
        self.inner.delete(id).await?;
        self.cache.invalidate(id);
        Ok(())
    }

    // ... 其他方法委托给 inner
}
```

### 4.6 InstrumentedStore（装饰器）

```rust
pub struct InstrumentedStore<S> {
    inner: S,
    metrics: Arc<StorageMetrics>,
}

impl<S: RawStorage> InstrumentedStore<S> {
    pub fn new(inner: S) -> Self {
        Self { inner, metrics: Arc::new(StorageMetrics::default()) }
    }

    pub fn metrics(&self) -> &StorageMetrics { &self.metrics }
}

impl<S: RawStorage> RawStorage for InstrumentedStore<S> {
    async fn save(&self, id: &str, data: &[u8], metadata: &serde_json::Value) -> Result<(), StorageError> {
        let start = Instant::now();
        let result = self.inner.save(id, data, metadata).await;
        let elapsed = start.elapsed().as_millis() as u64;
        self.metrics.save.count.fetch_add(1, Ordering::Relaxed);
        self.metrics.save.total_time_ms.fetch_add(elapsed, Ordering::Relaxed);
        self.metrics.save.total_bytes.fetch_add(data.len() as u64, Ordering::Relaxed);
        result
    }
    // ... 其他操作类似
}
```

---

## 五、错误体系

### 5.1 StorageError 枚举

```rust
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Storage error in {operation}: {message}")]
    General {
        operation: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Entity not found: type={entity_type}, id={entity_id}")]
    NotFound { entity_type: String, entity_id: String },

    #[error("Storage quota exceeded: required={required}, available={available}")]
    QuotaExceeded { required: u64, available: u64 },

    #[error("Storage initialization failed: {backend}: {message}")]
    Initialization {
        backend: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Serialization failed for entity {entity}: {message}")]
    Serialization {
        entity: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Integrity check failed for {id}: expected={expected}, actual={actual}")]
    Integrity { id: String, expected: String, actual: String },

    #[error("Connection pool error: {backend}: {message}")]
    Pool { backend: String, message: String },

    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    #[error("Storage state error: expected {expected}, actual {actual}")]
    StateError { expected: String, actual: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

---

## 六、Phase A：核心特性补齐（1.5 周）

### A.1 数据模型重构

**文件变更**：

| 文件 | 操作 |
|------|------|
| `adapter/raw_storage.rs` | 新增 `RawStorage` trait |
| `adapter/entity.rs` | 新增 `Entity` trait |
| `adapter/base.rs` | 保留（简化为 re-export） |
| `backend/sqlite.rs` | 重构表结构 + SQL 查询 |
| `backend/postgres.rs` | 重构表结构 + SQL 查询 |
| `backend/memory.rs` | 重构 InnerStore |
| `backend/entity_store.rs` | 重构为基于 `RawStorage` + `Entity` |

**SQLite 表结构**：
```sql
CREATE TABLE IF NOT EXISTS {table_name} (
    id          TEXT PRIMARY KEY,
    data        BLOB NOT NULL,
    metadata    TEXT NOT NULL,            -- JSON 字符串
    hash        TEXT NOT NULL,
    data_size   INTEGER NOT NULL,
    compressed  BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_{table}_entity_type ON {table_name}(json_extract(metadata, '$.entityType'));
CREATE INDEX IF NOT EXISTS idx_{table}_status ON {table_name}(json_extract(metadata, '$.status'));
```

**PostgreSQL 表结构**：
```sql
CREATE TABLE IF NOT EXISTS {table_name} (
    id          TEXT PRIMARY KEY,
    data        BYTEA NOT NULL,
    metadata    JSONB NOT NULL,
    hash        TEXT NOT NULL,
    data_size   INTEGER NOT NULL,
    compressed  BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  BIGINT NOT NULL,
    updated_at  BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_{table}_metadata ON {table_name} USING GIN (metadata);
CREATE INDEX IF NOT EXISTS idx_{table}_entity_type ON {table_name}((metadata->>'entityType'));
CREATE INDEX IF NOT EXISTS idx_{table}_status ON {table_name}((metadata->>'status'));
```

### A.2 查询优化（修复 N+1 和内存过滤）

**Checkpoint 适配器**：
```rust
// 修复前（当前 Rust）：全表扫描 + 内存过滤
async fn list_by_entity(&self, entity_id: &str) -> Result<Vec<Checkpoint>, StorageError> {
    let all = self.list(None).await?;
    Ok(all.into_iter().filter(|c| c.entity_id == entity_id).collect())
}

// 修复后：SQL WHERE 子句
// SQLite: WHERE json_extract(metadata, '$.entityId') = $1
// Postgres: WHERE metadata->>'entityId' = $1

// get_latest_by_entity:
// SQLite: WHERE ... ORDER BY created_at DESC LIMIT $1
// Postgres: WHERE ... ORDER BY created_at DESC LIMIT $1

// delete_by_entity:
// DELETE FROM checkpoints WHERE json_extract(metadata, '$.entityId') = $1
```

**update_status 优化**：
```rust
// 修复前：读完整实体 → 修改 → 写回
// 修复后：
// UPDATE executions SET metadata = jsonb_set(metadata, '{status}', $1), updated_at = $2 WHERE id = $3
```

### A.3 批量操作（使用 SQL 批量语法）

```rust
// SQLite: 事务 + 批量 INSERT
async fn save_batch(&self, items: &[BatchItem<'_>]) -> Result<(), StorageError> {
    let mut tx = self.pool.begin().await?;
    for chunk in items.chunks(999) {  // SQLite 变量上限 999
        let placeholders: String = chunk.iter().map(|_| "(?, ?, ?, ?, ?)").collect::<Vec<_>>().join(", ");
        // ... 构建批量 INSERT
    }
    tx.commit().await?;
    Ok(())
}

// PostgreSQL: UNNEST 批量 INSERT
// INSERT INTO table (id, data, metadata, hash, data_size)
// SELECT * FROM UNNEST($1::text[], $2::bytea[], $3::jsonb[], $4::text[], $5::int4[])
```

### A.4 完整性校验

```rust
/// SHA-256 采样哈希（对齐 TS 策略）
/// - ≤ 1MB：全量
/// - > 1MB：首 64KB + 中间 64KB + 尾 64KB
pub fn compute_hash(data: &[u8]) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    if data.len() <= 1_048_576 {
        hasher.update(data);
    } else {
        const SAMPLE_SIZE: usize = 65536;
        let mid = data.len() / 2;
        hasher.update(&data[..SAMPLE_SIZE]);
        hasher.update(&data[mid - SAMPLE_SIZE/2..mid + SAMPLE_SIZE/2]);
        hasher.update(&data[data.len() - SAMPLE_SIZE..]);
    }
    format!("{:x}", hasher.finalize())
}

pub fn verify_integrity(data: &[u8], expected: &str) -> Result<(), StorageError> {
    let actual = compute_hash(data);
    if actual != expected {
        return Err(StorageError::Integrity {
            id: String::new(),
            expected: expected.into(),
            actual,
        });
    }
    Ok(())
}
```

### A.5 工作流版本实现

```sql
CREATE TABLE IF NOT EXISTS workflow_versions (
    workflow_id  TEXT NOT NULL,
    version      TEXT NOT NULL,
    data         BLOB NOT NULL,
    change_note  TEXT,
    metadata     JSON NOT NULL,
    hash         TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    PRIMARY KEY (workflow_id, version)
);
```

### A.6 clear() 使用 TRUNCATE（Postgres）

```rust
// SQLite 不支持 TRUNCATE，使用 DELETE + 重置自增
// PostgreSQL 使用 TRUNCATE（O(1)）
async fn clear(&self) -> Result<(), StorageError> {
    // Postgres:
    sqlx::query(&format!("TRUNCATE TABLE {}", self.table_name))
        .execute(&self.pool).await?;
    Ok(())
}
```

### A.7 Phase A 文件清单

```
crates/wf-storage/src/
├── adapter/
│   ├── raw_storage.rs       ← 新增 RawStorage trait
│   ├── entity.rs            ← 新增 Entity trait
│   ├── checkpoint.rs        ← 补全方法 + SQL 优化
│   ├── workflow.rs          ← 完整版本实现
│   ├── execution.rs         ← update_status 优化为 SQL UPDATE
│   └── ...其他保持
├── backend/
│   ├── sqlite.rs            ← 重构表结构 + 索引 + SQL 优化
│   ├── postgres.rs          ← 重构表结构 + 索引 + SQL 优化
│   ├── memory.rs            ← data + metadata 分离
│   ├── entity_store.rs      ← 基于 RawStorage + Entity
│   ├── integrity.rs         ← compute_hash / verify_integrity
│   └── compression.rs       ← compress / decompress
└── error.rs                 ← 重写错误体系
```

---

## 七、Phase B：生产级特性（1.5 周）

### B.1 LRU 数据缓存

```rust
use moka::sync::Cache;

pub struct CacheConfig {
    pub max_capacity: u64,     // 默认 1000
    pub ttl_seconds: u64,      // 默认 300
}

pub struct EntityCache {
    cache: Cache<String, CachedEntry>,
    hits: AtomicU64,
    misses: AtomicU64,
}

struct CachedEntry {
    data: Vec<u8>,
    metadata: serde_json::Value,
}
```

### B.2 指标收集

```rust
pub struct StorageMetrics {
    pub save: OperationMetrics,
    pub load: OperationMetrics,
    pub delete: OperationMetrics,
    pub list: OperationMetrics,
}

pub struct OperationMetrics {
    pub count: AtomicU64,
    pub total_time_ms: AtomicU64,
    pub total_bytes: AtomicU64,
}

impl OperationMetrics {
    pub fn avg_time_ms(&self) -> f64 {
        let c = self.count.load(Ordering::Relaxed);
        if c == 0 { 0.0 }
        else { self.total_time_ms.load(Ordering::Relaxed) as f64 / c as f64 }
    }

    pub fn record(&self, elapsed_ms: u64, bytes: u64) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.total_time_ms.fetch_add(elapsed_ms, Ordering::Relaxed);
        self.total_bytes.fetch_add(bytes, Ordering::Relaxed);
    }
}
```

### B.3 SQLite 维护工具

```rust
pub struct PragmaConfig {
    pub enable_wal: bool,
    pub auto_vacuum: AutoVacuumMode,
    pub journal_size_limit: u64,
    pub busy_timeout: u32,
    pub cache_size: i32,
    pub temp_store: TempStore,
    pub synchronous: SynchronousMode,
    pub foreign_keys: bool,
    pub wal_autocheckpoint: u32,
}

impl Default for PragmaConfig {
    fn default() -> Self {
        Self {
            enable_wal: true,
            auto_vacuum: AutoVacuumMode::Incremental,
            journal_size_limit: 64 * 1024 * 1024,
            busy_timeout: 5000,
            cache_size: -64000,
            temp_store: TempStore::Memory,
            synchronous: SynchronousMode::Normal,
            foreign_keys: true,
            wal_autocheckpoint: 1000,
        }
    }
}

pub async fn configure_pragmas(pool: &SqlitePool, config: &PragmaConfig) -> Result<(), StorageError> {
    let pragmas = format!(
        "PRAGMA journal_mode = {};
         PRAGMA auto_vacuum = {};
         PRAGMA journal_size_limit = {};
         PRAGMA busy_timeout = {};
         PRAGMA cache_size = {};
         PRAGMA temp_store = {};
         PRAGMA synchronous = {};
         PRAGMA foreign_keys = {};
         PRAGMA wal_autocheckpoint = {};",
        if config.enable_wal { "WAL" } else { "DELETE" },
        config.auto_vacuum.as_str(),
        config.journal_size_limit,
        config.busy_timeout,
        config.cache_size,
        config.temp_store.as_str(),
        config.synchronous.as_str(),
        if config.foreign_keys { "ON" } else { "OFF" },
        config.wal_autocheckpoint,
    );
    for pragma in pragmas.split(';') {
        let trimmed = pragma.trim();
        if !trimmed.is_empty() {
            sqlx::query(trimmed).execute(pool).await?;
        }
    }
    Ok(())
}
```

### B.4 PostgreSQL 连接池增强

```rust
pub struct PoolManager {
    pool: PgPool,
    metrics: Arc<PoolMetrics>,
}

struct PoolMetrics {
    hits: AtomicU64,
    misses: AtomicU64,
    acquire_time_ms: AtomicU64,
}

impl PoolManager {
    pub async fn health_check(&self) -> bool {
        sqlx::query("SELECT 1").fetch_optional(&self.pool).await.is_ok()
    }

    pub fn sanitize_connection_string(conn: &str) -> String {
        // 移除 password 部分，只保留 host/port/database
        use url::Url;
        if let Ok(mut url) = Url::parse(conn) {
            if url.password().is_some() {
                url.set_password(Some("***")).ok();
            }
            url.to_string()
        } else {
            conn.to_string()  // 无法解析时原样返回（不应发生）
        }
    }
}
```

### B.5 压缩支持

```rust
pub fn compress(data: &[u8]) -> Result<Vec<u8>, StorageError> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data)?;
    encoder.finish().map_err(|e| e.into())
}

pub fn decompress(data: &[u8]) -> Result<Vec<u8>, StorageError> {
    use flate2::read::ZlibDecoder;
    let mut decoder = ZlibDecoder::new(data);
    let mut result = Vec::new();
    decoder.read_to_end(&mut result)?;
    Ok(result)
}

pub fn maybe_compress(data: &[u8]) -> Result<(Vec<u8>, bool), StorageError> {
    if data.len() < 1024 {
        return Ok((data.to_vec(), false));
    }
    let compressed = compress(data)?;
    if compressed.len() < data.len() {
        Ok((compressed, true))
    } else {
        Ok((data.to_vec(), false))
    }
}

pub fn maybe_decompress(data: &[u8], compressed: bool) -> Result<Vec<u8>, StorageError> {
    if compressed {
        decompress(data)
    } else {
        Ok(data.to_vec())
    }
}
```

### B.6 装饰器 Store 实现

```rust
// CachingStore + InstrumentedStore
// 见第四章架构图

pub struct CachingStore<S> {
    inner: S,
    cache: EntityCache,
}

pub struct InstrumentedStore<S> {
    inner: S,
    metrics: Arc<StorageMetrics>,
}

// 使用示例：
// let store = InstrumentedStore::new(
//     CachingStore::new(
//         EntityStore::<_, WorkflowTemplate>::new(SqliteStorage::new(":memory:").await?),
//         CacheConfig::default()
//     )
// );
```

### B.7 Phase B 文件清单

```
crates/wf-storage/src/
├── adapter/
│   ├── cache.rs             ← 新增 CacheConfig + EntityCache
│   └── metrics.rs           ← 新增 StorageMetrics + OperationMetrics
├── backend/
│   ├── caching_store.rs     ← 新增 CachingStore<S>
│   ├── instrumented_store.rs← 新增 InstrumentedStore<S>
│   ├── maintenance.rs       ← 新增 MaintenanceService
│   ├── pragma.rs            ← 新增 PragmaConfig + configure_pragmas
│   ├── pool.rs              ← 新增 PoolManager
│   └── sqlite.rs            ← pool size 8 + WAL + 维护集成
└── Cargo.toml               ← 添加 moka, sha2, digest, flate2, async-trait, url
```

---

## 八、依赖变更

### 新增依赖（crates/wf-storage/Cargo.toml）

```toml
[dependencies]
# 已有
serde.workspace = true
serde_json.workspace = true
chrono.workspace = true
thiserror.workspace = true
tokio.workspace = true
tracing.workspace = true
uuid.workspace = true
sqlx = { workspace = true, optional = true }

# 新增
async-trait = "0.1"
moka = { version = "0.12", features = ["sync"] }
sha2 = "0.10"
flate2 = "1.0"
url = "2"                     # 连接串脱敏
```

---

## 九、测试策略

### 9.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_save_load_roundtrip() {
        let store = MemoryStorage::new("test");
        store.save("id1", b"hello", &json!({"type": "test"})).await.unwrap();
        let (data, meta) = store.load("id1").await.unwrap().unwrap();
        assert_eq!(data, b"hello");
        assert_eq!(meta, json!({"type": "test"}));
    }

    #[tokio::test]
    async fn test_metadata_query_no_blob_load() {
        // 验证 get_metadata 不触发 BLOB 加载
        // 验证 list 只返回 (id, metadata)，不返回 data
    }

    #[tokio::test]
    async fn test_integrity_verification() {
        let data = vec![0u8; 100_000];
        let hash = compute_hash(&data);
        assert!(verify_integrity(&data, &hash).is_ok());
        let mut corrupted = data.clone();
        corrupted[0] = 1;
        assert!(verify_integrity(&corrupted, &hash).is_err());
    }

    #[tokio::test]
    async fn test_compression_roundtrip() {
        let original = vec![0u8; 10000];
        let (compressed, true) = maybe_compress(&original).unwrap();
        assert!(is_compressed);
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let store = MemoryStorage::new("test");
        let items: Vec<BatchItem<'_>> = (0..1000).map(|i| BatchItem {
            id: format!("id_{}", i),
            data: vec![i as u8; 100],
            metadata: &json!({"index": i}),
        }).collect();
        store.save_batch(&items).await.unwrap();
        assert_eq!(store.list(None).await.unwrap().len(), 1000);
    }

    #[tokio::test]
    async fn test_sql_filtering() {
        // 验证 list_by_entity 使用 SQL WHERE 而非内存过滤
        // 可通过查询计划（EXPLAIN）验证索引使用
    }
}
```

### 9.2 多后端一致性

```rust
async fn test_cross_backend_consistency() {
    let memory = MemoryStorage::new("test");
    let sqlite = SqliteStorage::new(":memory:", "test").await.unwrap();

    for store in [&memory as &dyn RawStorage, &sqlite as &dyn RawStorage] {
        store.save("k1", b"data1", &json!({"a": 1})).await.unwrap();
        store.save("k2", b"data2", &json!({"a": 2})).await.unwrap();
        let list = store.list(None).await.unwrap();
        assert_eq!(list.len(), 2);
    }
}
```

### 9.3 性能基准

```rust
#[bench]
fn bench_save_throughput(b: &mut Bencher) { ... }

#[bench]
fn bench_list_with_metadata(b: &mut Bencher) {
    // 验证 list 不加载 BLOB
}

#[bench]
fn bench_cached_load(b: &mut Bencher) {
    // 验证缓存命中率 > 95%
}

#[bench]
fn bench_batch_save(b: &mut Bencher) {
    // 1000 条批量写入
}
```

---

## 十、与原始 Rust 方案的差异

| 维度 | 原始 Rust 方案（已废弃） | 本方案 |
|------|------------------------|--------|
| 数据模型 | 单一 JSON blob | BLOB + 元数据分离（继承 TS） |
| trait 签名 | `save(entity: &T)` 整体序列化 | `save(id, data, metadata)` 分离存储 |
| 架构模式 | 4 层继承链 → 具体类 | 装饰器组合：`Instrumented<Caching<EntityStore<Raw>>>` |
| 缓存 | 无 | moka LRU（装饰器模式） |
| 完整性 | 无 | SHA-256 采样哈希 |
| 维护 | 无 | VACUUM/ANALYZE/WAL checkpoint |
| 查询 | 内存过滤（N+1） | SQL WHERE + 索引 |
| 批量 | 串行循环 | SQL 批量 INSERT |
| 并发 | `max_connections(1)` 串行 | WAL + pool 8 + busy_timeout |
| 错误 | 8 种不对应变体 | 5 类对齐 TS + 状态错误 |
| close() | 无 | 仅释放资源，不删数据 |
| clear() | DELETE | TRUNCATE (Postgres) |
| 连接串 | 无处理 | 脱敏后日志 |
| napi-rs | 有 | 不存在 |
| 与 TS 关系 | 双写验证 | 无关系，纯 Rust |
