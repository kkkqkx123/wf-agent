# Rust common 层重构设计

## 1. TS common-utils 模块必要性分析

基于 TS 侧实际消费统计（194 个文件，334 处符号引用），按使用频率分层：

### 1.1 核心原语（wf-common 层）

| 模块 | TS 消费频次 | Rust 处理方式 |
|------|-----------|--------------|
| `now()` / `diffTimestamp()` | 78 / 25 | 保留，毫秒时间戳是跨 crate 原语 |
| `ok()` / `err()` | 52 / 36 | **删除**。Rust 有 `Result` + `?` 操作符，无需包装函数 |
| `generateId()` | 11 | 保留，但用 `Uuid` 直接生成，不做 `type Id = String` 别名 |
| `getErrorMessage()` / `normalizeError()` / `isError()` | 17+4+4 | 保留核心部分，Rust 用 `Report` trait 或 `Error::source()` 链 |
| `getErrorOrNew()` | 28 | **合并**。Rust 的 `anyhow::Error` 或 `std::error::Error` 已覆盖此场景 |

### 1.2 运行时基础设施（wf-runtime 层）

| 模块 | TS 消费频次 | Rust 处理方式 |
|------|-----------|--------------|
| `logger` | 17+ | 迁移，`tracing` crate 已声明但 wf-common 未使用，移至 wf-runtime |
| `cache` | 1 | **暂缓**。仅 evaluation compiler 使用，且 wf-storage 已引入 `moka`，可复用 |
| `di` (Container) | 10+ | **删除**。见 §3 详细分析 |
| `circuit-breaker` | 0 | **删除**。SDK 自实现了更好的版本，common-utils 版本无人使用 |
| `process` (信号处理) | 按需 | 迁移到 wf-runtime，Rust 有 `tokio::signal` |

### 1.3 业务特化（归属具体业务 crate）

| 模块 | 归属 | 说明 |
|------|------|------|
| `file-monitoring` | `wf-checkpoint` | FileWatcher 在 TS 侧 0 消费（死代码），仅 FileCheckpointManager 属于检查点领域 |
| `codec` | `wf-checkpoint` | StateCodec/ErrorCodec 仅用于检查点序列化 |
| `compression` | `wf-storage` | flate2 已引入，无需独立模块 |
| `metrics` | 独立 crate 或暂缓 | 仅 1 处消费，可用 `prometheus-client` 替代 |
| `script-security` | `wf-sandbox` | TS 侧 0 消费（死代码），若需要则属于沙箱层 |

### 1.4 TS 侧死代码（仅标注，不修改）

| 模块 | 文件路径 | 状态 |
|------|---------|------|
| `FileWatcher` | `packages/common-utils/src/file-monitoring/file-watcher.ts` | 0 处消费 |
| `CircuitBreaker` | `packages/common-utils/src/utils/circuit-breaker.ts` | 0 处消费（SDK 自实现） |
| `script-security` | `packages/common-utils/src/script-security/` | 0 处消费 |
| `metrics` | `packages/common-utils/src/metrics/` | 1 处消费（可忽略） |

---

## 2. wf-common 重构方案

### 2.1 当前问题

| 问题 | 现状 | 目标 |
|------|------|------|
| `WfResult` 自定义枚举 | 无法使用 `?` 操作符，与 `std::result::Result` 割裂 | **删除**，全库统一用 `Result<T, E>` |
| `CommonError` 手动 impl | 未用 `thiserror`，缺少 `From` trait | 用 `#[derive(thiserror::Error)]` + `From impls` |
| `type Id = String` | 无类型安全，与 `Uuid` 重复 | **删除**，直接用 `Uuid` |
| `tokio`/`tracing` 未使用 | 依赖膨胀 | 移除，移至 wf-runtime |
| `thiserror` 未使用 | 手动实现 `Error` trait | 用起来 |

### 2.2 目标结构

```
crates/wf-common/
├── Cargo.toml
└── src/
    ├── lib.rs           ← include!("wf_common.rs")
    ├── wf_common.rs     ← 根模块
    ├── error.rs         ← Error 类型 + From impls
    ├── time.rs          ← 毫秒时间戳 + 转换
    └── report.rs        ← 错误上下文传播（可选）
```

### 2.3 模块定义

#### `error.rs`

```rust
#[derive(thiserror::Error, Debug)]
pub enum CommonError {
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Already exists: {0}")]
    AlreadyExists(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("Internal: {0}")]
    Internal(String),
    #[error("Serialization: {0}")]
    Serialization(String),
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
}

impl From<serde_json::Error> for CommonError {
    fn from(e: serde_json::Error) -> Self {
        CommonError::Serialization(e.to_string())
    }
}

impl From<chrono::ParseError> for CommonError {
    fn from(e: chrono::ParseError) -> Self {
        CommonError::Serialization(e.to_string())
    }
}

pub type CommonResult<T> = Result<T, CommonError>;
```

#### `time.rs`

```rust
pub type Timestamp = i64;

pub fn now() -> Timestamp {
    chrono::Utc::now().timestamp_millis()
}

pub fn diff_millis(start: Timestamp, end: Timestamp) -> i64 {
    end - start
}

pub fn timestamp_to_iso(ts: Timestamp) -> String {
    chrono::DateTime::from_timestamp_millis(ts)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}
```

#### `Cargo.toml`（精简后）

```toml
[dependencies]
serde.workspace = true
serde_json.workspace = true
chrono.workspace = true
thiserror.workspace = true
uuid.workspace = true
# 移除: tokio, tracing
```

---

## 3. DI 容器消除方案

### 3.1 TS 侧 DI 使用模式

TS SDK 使用自定义 DI 容器（~1800 行基础设施），注册 60+ 服务。核心用途：

| 模式 | TS 实现 | Rust 替代 |
|------|---------|----------|
| 服务组合 | `container.get(Ids.Xxx)` | 结构体字段直接持有 |
| 单例 | `.inSingletonScope()` | `Arc<T>` |
| 工厂 | `.toDynamicValue(c => new Xxx(c.get(...)))` | 关联函数 / 闭包 |
| 策略 | 运行时替换绑定 | `Arc<dyn Trait>` 或泛型 |
| 多租户 | `ContainerManager` | `HashMap<String, Runtime>` |

### 3.2 Rust 替代设计

```rust
// TS: container.get(Ids.WorkflowRegistry)
// Rust: 结构体字段
pub struct Runtime {
    pub workflow_registry: Arc<dyn WorkflowRegistry>,
    pub tool_registry: Arc<dyn ToolRegistry>,
    pub checkpoint_storage: Arc<dyn CheckpointStorage>,
    // ...
}

// TS: IdBasedServiceFactory<ConversationSession>.create(executionId)
// Rust: 直接构造
impl Runtime {
    pub fn create_session(&self, execution_id: &str) -> ConversationSession {
        ConversationSession::new(execution_id, self.checkpoint_storage.clone())
    }
}

// TS: ContainerManager 多租户
// Rust: 应用层持有
pub struct RuntimeManager {
    runtimes: HashMap<String, Runtime>,
}
```

### 3.3 继承层次扁平化

TS 的抽象类层次在 Rust 中用 trait + 默认方法替代：

```rust
// TS: BaseCheckpointCoordinator<T> (abstract)
// Rust:
pub trait CheckpointCoordinator {
    type Checkpoint;
    type Entity;
    type State;

    fn extract_state(&self, entity: &Self::Entity) -> Result<Self::State>;
    fn build_checkpoint(&self, state: Self::State) -> Result<Self::Checkpoint>;
    fn create_entity_from_snapshot(&self, snapshot: &Self::Checkpoint) -> Result<Self::Entity>;
}

// TS: RegistryImpl<T> + 10+ 子类
// Rust: 单个泛型结构体
pub struct Registry<T> {
    items: HashMap<String, T>,
    storage: Option<Arc<dyn RegistryStorage>>,
}
```

### 3.4 工厂类简化

```rust
// TS: AgentLoopFactory.create() / fromCheckpoint() / fromMessages()
// Rust: 关联函数
impl AgentLoop {
    pub fn new(options: AgentLoopOptions) -> Self { /* ... */ }
    pub fn from_checkpoint(cp: Checkpoint, overrides: Option<Overrides>) -> Result<Self> { /* ... */ }
    pub fn from_messages(msgs: Vec<Message>) -> Self { /* ... */ }
}
```

---

## 4. wf-runtime 层设计

### 4.1 定位

承载所有需要 `tokio`/`tracing` 的运行时基础设施，是 `wf-common` 的上层依赖。

### 4.2 模块规划

```
crates/wf-runtime/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── runtime.rs       ← RuntimeContext 主结构体
    ├── logger.rs        ← tracing subscriber 配置
    ├── cache.rs         ← 基于 moka 的通用缓存（可选）
    ├── circuit.rs       ← 断路器（可选）
    └── shutdown.rs      ← 优雅退出
```

### 4.3 RuntimeContext 定义

```rust
pub struct RuntimeContext {
    // 存储
    pub workflow_storage: Arc<dyn WorkflowStorage>,
    pub checkpoint_storage: Arc<dyn CheckpointStorage>,
    
    // 注册表
    pub workflow_registry: Arc<dyn WorkflowRegistry>,
    pub tool_registry: Arc<dyn ToolRegistry>,
    pub event_registry: Arc<EventRegistry>,
    
    // 执行器
    pub llm_executor: Arc<dyn LlmExecutor>,
    pub tool_executor: Arc<dyn ToolExecutor>,
}

impl RuntimeContext {
    pub async fn new(options: RuntimeOptions) -> Result<Self> { /* ... */ }
}
```

### 4.4 依赖 DAG（调整后）

```
wf-types  ←  wf-common  ←  wf-runtime  ←  wf-storage
                ↑                          ↑
                └──────── wf-core ─────────┘
```

---

## 5. 迁移优先级

### P0（当前阶段，wf-common 完善）

1. **删除 `WfResult`**，全库改用 `std::result::Result`
2. **重写 `error.rs`**，使用 `thiserror` + `From` trait
3. **精简 `Cargo.toml`**，移除 `tokio`/`tracing`
4. **删除 `result.rs`**

### P1（wf-runtime 创建）

1. 创建 `wf-runtime` crate，承载 logger/cache/process
2. 定义 `RuntimeContext` 主结构体
3. 迁移 `tracing` 配置（subscriber 初始化、Layer 抽象）

### P2（业务 crate 适配）

1. `wf-checkpoint` 接管 `file-monitoring` + `codec`
2. `wf-sandbox` 接管 `script-security`（如需要）
3. `wf-storage` 已内嵌压缩能力

### P3（可选）

1. `metrics` 模块独立评估，或用 `prometheus-client` 替代
2. `cache` 模块复用 `moka`（wf-storage 已引入）

---

## 6. 总结

| 决策 | 理由 |
|------|------|
| 删除 `WfResult` | Rust `Result` + `?` 是语言级原语，自定义枚举制造割裂 |
| 删除 DI 容器 | Rust 所有权系统天然解决生命周期管理，结构体字段替代服务定位器 |
| 删除继承层次 | trait + 泛型 + 结构体组合，更扁平、更静态、零开销 |
| 删除 `type Id = String` | `Uuid` 直接可用，别名无类型安全价值 |
| wf-common 做薄 | 只做跨 crate 共享的原语（Error + Timestamp），运行时能力下沉到 wf-runtime |
| 工厂类→关联函数 | Rust 惯例，无需抽象层 |
