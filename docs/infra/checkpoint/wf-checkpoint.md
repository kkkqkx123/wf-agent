# wf-checkpoint:门面与协调器层

`crates/infra/checkpoint/wf-checkpoint` 是对外门面:re-export 全部内部 crate 的公共 API,并实现 **CheckpointCoordinator** —— 状态检查点与文件检查点的统一编排入口。上层(`wf-tools` / `wf-execution-shared` / `wf-agent` / `wf-workflow` / `wf-api` / `wf-runtime`)只依赖本 crate。

## 门面 re-export(lib.rs)

- `checkpoint_base` 的错误、时钟、缓存、序列化、策略、元数据构建、增量计算/回放、版本管理、执行事件总线;
- `checkpoint_file` 的事件总线、`FileCheckpointManager`、会话、脚本捕获、作用域结束、SHA-256;
- `checkpoint_state` 的全部状态管理器与层级恢复类型。

## coordinator 模块结构

```text
base.rs      共享决策与 trait(agent/workflow 通用)
persist.rs   检查点行持久化与删除
events.rs    统一事件发布
queue.rs     后台持久化队列
projection.rs 文件投影(状态→文件)
child_restore.rs 子级恢复
migration.rs 格式版本迁移
agent/       AgentCheckpointCoordinator(+ content_policy / progress / restore / timeline)
workflow/    WorkflowCheckpointCoordinator(+ content_policy / progress / restore)
```

## 共享基座(base.rs)

- **trait `CheckpointCoordinator`**:统一契约;`CheckpointId` / `CheckpointBlob` 是协调器对信封的核心要求(checkpoint_id、序列化 blob)。
- **`decide_checkpoint_type_by_count(count, config)`**:Full/Delta 决策 —— Delta 未启用或计数为 0 或计数命中有效间隔(= min(baseline_interval, max_delta_chain_length))的倍数时写 Full,否则写 Delta。
- **`next_chain_position`**:Full 重置为 0,Delta 在前值上加 1。
- 上下文准备(`prepare_context`)与自定义字段盖章(`stamp_custom_fields`)等公共步骤。
- 子级元数据索引:`ChildMetadataIndex` / `ChildDiscoveryIndex`。

## 创建→持久化→投影→清理 流程

两个协调器共享同一管线,只差差分计算器、进度坐标与内容策略:

1. **决策**:策略(`StandardStrategy` / Cadenced 包装)判定本次触发是否创建;
2. **差分**:按计数决策 Full/Delta;Delta 由对应 `DiffCalculator` 对上一状态计算;
3. **元数据**:经 `build_checkpoint_metadata` 组装,盖章进度坐标与触发标注;进度坐标相等时**合并回**既有检查点而非落重复行;
4. **持久化**(`persist.rs`):`persist_checkpoint` 写库 → 成功发布 `persisted` 事件;失败发布 `persist_failed` 并交 `CheckpointErrorHandler` 裁决 —— 默认上抛,失败绝不伪装成功;宽松配置才允许无检查点继续。`delete_checkpoint` 同构;
5. **文件投影**(`projection.rs`):同步 best-effort —— `tokio::spawn_blocking` 调 `create_latest_file_checkpoint`,成功则记录**状态→文件链接**(恢复时解析"当时的文件集合"而非"最新");文件层缺失/失败均返回 Ok,状态检查点永不受牵连;成功/失败均带状态检查点 id 记日志便于关联;
6. **后台队列**(`queue.rs`):延迟的文件投影任务进入 `PersistenceQueue`(有界,≤128);队满时先排空积压再入队,发布 `cleanup_skipped(persistence_backlog)` 并记 `persistence_backlog` 指标,内存保持有界;
7. **清理**:按策略触发状态侧清理;文件侧由 gc 自治。

## Agent 协调器(coordinator/agent)

`AgentCheckpointCoordinator` + `AgentLoopEntity`:

- 差分/回放:`AgentDiffCalculator` + `GenericDeltaRestorer`;
- 进度坐标(`progress.rs`:`progress_coords` / `snapshot_progress_coords`):iteration、toolCallCount、loopStatus、消息序列界、`pendingToolCallCount`(飞行中工具数,补盲区);
- `timeline.rs`:`TimelineRow` —— 恢复点列表的行模型;
- `content_policy.rs`:Agent 捕获范围(消息上限截断、敏感域过滤);
- `restore.rs`:自身状态的增量链恢复 + 子级恢复编排;
- 合并/冲突语义接入 `GitMergeOutcome`(文件侧)。

## Workflow 协调器(coordinator/workflow)

`WorkflowCheckpointCoordinator` + `WorkflowExecutionEntity`:

- 差分/回放:`WorkflowDiffCalculator` + `GenericDeltaRestorer`;
- 进度坐标(`workflow_progress_coords` / `snapshot_workflow_coords`):workflowStatus、currentNode、节点结果/变量哈希(FNV-1a)、节点记录数;
- 其余管线与 Agent 完全对称。

## 子级恢复(child_restore.rs)

父级恢复完成后执行发现遍历:

- 遍历深度上限 `CHILD_DISCOVERY_DEPTH = 8`;每子级恢复并发 `CHILD_RESTORE_CONCURRENCY = 5`(`ConcurrencyGate`);
- 每个子级:从 `ChildDiscoverySummary` 取元数据 → 若 `RestoreStrategyRegistry` 注册了该 `entity_type`,经协调器提供的 `load_bytes` 闭包加载原始字节 → 调用注册的 `RestoreFn` 恢复实体;未注册的类型只记录发现结果不恢复;
- 事务与回滚语义见 checkpoint-state 的 `RecoveryTransaction`。

## 迁移(migration.rs)

- `CheckpointVersion` trait:读取信封的 `format_version`(对 `BaseCheckpointCore` 有覆盖实现,两种检查点共用);
- `load_migrated(manager, version_manager, id)`:加载 → 当前版本直接返回;不兼容报 `VersionIncompatible`;可迁移时**重新读取原始字节**交给迁移 handler 重写(迁移会改写字节,不能用已解码对象迁移)。

## 事件发布(events.rs)

统一封装给两个协调器,保证事件形状一致:`publish_persisted`(created)、`publish_persist_failed`(failed + "persist failed: …")、`publish_best_effort_failed`(投影失败)、`publish_cleanup_skipped`(清理/积压跳过)。全部发到 `CheckpointEventBus`,总线缺失时静默跳过。

## 集成测试(tests/)

- `crash_recovery.rs`:崩溃恢复 —— 重启后经层级恢复 + 恢复事务回到最近一致检查点;
- `file_branch_dag_integration.rs`:文件分支 DAG 与状态链的联动;
- `source_capture.rs`:源捕获(精确/脚本通道)端到端。

## 使用要点

1. 装配时构造:`StorageBackend` → 两个状态管理器(+时钟)、`FileCheckpointManager`(绑定工作区)、`CheckpointEventBus`、策略与错误处理器;
2. 按实体类型向 `RestoreStrategyRegistry` 注册恢复函数(否则层级恢复跳过该类型);
3. 执行侧通过 `CheckpointSession`(wf-tools 工具上下文)接入文件捕获,通过协调器接入状态检查点;
4. 需要版本迁移时向 `VersionManager` 注册迁移 handler;
5. 事件消费订阅 `CheckpointEventBus`(检查点生命周期)与 `ExecutionEventBus`(执行状态)。
