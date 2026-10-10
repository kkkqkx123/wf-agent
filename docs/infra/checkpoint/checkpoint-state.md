# checkpoint-state:执行状态层

`crates/infra/checkpoint/checkpoint-state` 实现执行状态检查点的持久化与层级恢复,只依赖 `checkpoint-base`、`wf-storage`、`wf-types`。不含文件历史逻辑。

## 模块结构

```text
state/
  base.rs      CheckpointStateManager trait
  storage.rs   StorageBackedStateManager(契约 + 单行读)
  storage/     metadata / chain / cleanup / query / loader
  workflow.rs  WorkflowCheckpoint + WorkflowCheckpointStateManager
  agent.rs     AgentCheckpoint + AgentCheckpointStateManager
restore/
  registry.rs  RestoreStrategyRegistry
  hierarchy.rs 层级恢复:发现、解析、事务
```

## 状态管理器契约(state/base)

`CheckpointStateManager` trait,关联类型 `Checkpoint`:

| 方法 | 语义 |
| --- | --- |
| `save(checkpoint, entity_type, entity_id)` | 保存一个检查点 |
| `load(id)` / `load_batch(ids)` | 按 id 加载(批量版本) |
| `delete(id)` | 删除,返回是否存在 |
| `list_by_entity(entity_id)` | 实体的全部检查点元数据 |
| `get_latest(entity_id)` | 实体最新检查点元数据 |
| `load_metadata(id)` | 仅读元数据不加载 blob —— 从各管理器私有实现上提,使共享协调器辅助(描述合并回写、进度门控)可在 trait 上工作 |

另有 `CleanupStrategy` 相关的清理入口与 `CheckpointLoader` 的存储适配。

## 状态信封与具体类型(state/workflow、state/agent)

两种检查点共用 `wf_types` 的 `BaseCheckpointCore<TDelta, TSnapshot>` 泛型形状(核心元数据 + 差分 + 快照 + 格式版本等):

| 类型 | 定义 |
| --- | --- |
| `WorkflowCheckpoint` | `BaseCheckpointCore<WorkflowCheckpointDelta, WorkflowExecutionStateSnapshot>` |
| `AgentCheckpoint` | `BaseCheckpointCore<AgentCheckpointDelta, AgentStateSnapshot>` |

`WorkflowCheckpointStateManager` / `AgentCheckpointStateManager` 都是 `StorageBackedStateManager` 的薄包装:

- `new(storage: Arc<StorageBackend>)` 构造;`with_clock` 注入 `CheckpointClock`;
- `storage()` 暴露后端,供派生的恢复任务(spawn)重建管理器;
- `list_latest_by_entities(&[id])`:单条存储查询解析多实体的最新元数据,消除层级恢复中的 N+1 查询。

## 存储后端实现(state/storage)

`StorageBackedStateManager<T>` 是核心实现,职责拆分到兄弟子模块:

| 子模块 | 职责 |
| --- | --- |
| `metadata` | 索引化元数据文档构建(`MetadataArgs`);`parse_storage_metadata` 从存储行重建信封;payload 字段读取 |
| `chain` | Delta 链推导与压缩(compaction):过长链折叠为 Full,压缩守卫与回放侧上限镜像 |
| `cleanup` | 按实体的清理执行 + 持久化水位线(watermark),避免重复扫描 |
| `query` | 批量查询与父作用域的最新元数据解析 |
| `loader` | 增量恢复所需的原始数据与元数据加载(实现 `CheckpointLoader`) |

本模块自身保留:构造、`CheckpointStateManager` 的存储实现、单行读取。序列化统一走 `checkpoint_base::serializer`,清理决策来自 `checkpoint_base::cleanup_policy`,指标经 `wf_metrics::CheckpointMetricsCollector` 记录。

## 恢复策略注册表(restore/registry)

`RestoreStrategyRegistry`:`entity_type → RestoreFn` 的并发映射(`DashMap`)。

- `RestoreFn = Arc<dyn Fn(checkpoint_id, raw_bytes) -> Future<Result<serde_json::Value, CheckpointError>>>`:接收检查点 id 与**拥有的**原始字节,使返回 future 为 `'static`;输出恢复后的实体 JSON。
- 注册发生在应用装配时(各执行类型把自己的"从 blob 重建实体"函数注册进来);协调器做子级恢复时按子级 `entity_type` 查表,无注册则跳过该子级(仅记录发现结果)。

## 层级恢复(restore/hierarchy)

恢复一个父执行后,发现并恢复其子级(子工作流、子 Agent 循环)的全流程:

- **发现(ChildDiscovery)**:从父检查点的子级元数据(`ChildDiscoverySummary` / `ChildDiscoveryResult`)枚举子级;`ChildDiscoveryLoader` 负责批量取子级最新元数据;遍历深度有上限(协调器侧常量 8)。
- **解析(ChildCheckpointResolver)**:把"发现的子级"解析为可恢复的检查点目标;`InMemoryChildResolver` 是内存实现,`CachedChildResolver` 在其上加缓存。
- **事务(RecoveryTransaction)**:批量恢复的事务包装。每个 `RecoveryOperation`(操作类型 `RecoveryOperationType`,状态 `RecoveryOperationStatus`)记录执行进展;事务结果(`RecoveryTransactionResult` / `RecoveryTransactionStatus`)决定失败时的 `RollbackStrategy`(回滚已恢复子级 / 保留部分结果等)。

层级恢复读路径的关键设计:子级元数据用 `list_latest_by_entities` 批量解析;子级 blob 由各协调器提供 `load_bytes` 闭包加载;每子级恢复并发受 `ConcurrencyGate` 限制(协调器侧常量 5)。
