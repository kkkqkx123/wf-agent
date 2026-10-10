# checkpoint-base:基础层

`crates/infra/checkpoint/checkpoint-base` 是子系统叶子 crate(仅依赖 `wf-types` / `wf-common` / `wf-metrics`),提供全部检查点共享的基础设施。不含任何存储或协调逻辑。

## 模块清单

| 模块 | 职责 |
| --- | --- |
| `error` | `CheckpointError` 统一错误枚举 |
| `error_handling` | 错误处理裁决 |
| `clock` | 时钟抽象与测试时钟 |
| `cache` | TTL + 单飞缓存 |
| `actor` | 文件分区 Actor 身份 |
| `common` | 文本 diff 域(行/词级、差异统计、二进制判定、完整快照阈值) |
| `delta` | 状态差分域(计算、回放、加载 trait) |
| `strategy` | 检查点时机策略 |
| `metadata` | 元数据构建与字段键 |
| `serializer` | 编码与压缩 |
| `cleanup_policy` | 执行状态检查点回收策略 |
| `checkpoint_graph` | 线性链依赖守卫图 |
| `config_resolver` | 分层配置解析 |
| `execution_events` | 执行事件总线 |
| `version_manager` | 格式版本管理 |

## 错误(error)

`CheckpointError` 覆盖全部检查点失败形态,主要变体:

- `NotFound` / `Validation` / `Corrupted`:常规查找与完整性失败;
- `Storage(StorageError)` / `Serialization` / `Io`:底层透传;
- `VersionIncompatible { current, required }`:格式版本不兼容;
- `DeltaChainBroken { checkpoint_id, missing_id }` / `DeltaChainTooLong { length, max }`:增量链断链与超长;
- `MergeConflict { actor, files }`:文件合并冲突;
- `Branch` / `Strategy` / `Coordinator`:各层自有失败。

错误处理不在调用点逐个匹配,统一交给 `CheckpointErrorHandler`。

## 错误裁决(error_handling)

`CheckpointErrorHandler` 接收 `CheckpointErrorContext`(操作名、检查点 id、实体等)与错误,输出 `ErrorHandlingOutcome { should_rethrow, handled }`:

- 默认严格:失败上抛,`should_rethrow = true`;
- 显式宽松配置才吞掉错误继续执行(`handled = true`);
- 裁决与事件发布分离:发布(如 `persist_failed`)由调用方完成,handler 只决定错误是否继续传播。

## 时钟(clock)

`CheckpointClock` 抽象"现在"的毫秒值;`ManualClock` 是测试句柄:可设置、推进、置为失败(`failed` 后读取报错),克隆共享同一状态。设计约束:所有时间判断都显式传参 `now`,时钟缺失(或失败)时保留类决策(清理、保留判定)一律不触发,而不是隐式用系统时间。

## 缓存(cache)

`CheckpointCache<V>` 基于 moka:容量上限 + TTL,另附 `hits / misses / hit_rate` 统计。核心特性是**单飞(single-flight)**:同 key 并发 miss 时,后到者等待首个工厂调用完成后直接回读缓存,而不是各自执行昂贵的加载。用于检查点元数据等热读路径。

## Actor 身份(actor)

见 concepts.md 第 2 节。要点:

- `ActorId = "{kind}:{hierarchy}"`,kind ∈ `wf`(工作流分区)/ `agent`(Agent 循环分区)/ `sub`(其他);
- 嵌套执行追加 `/child:{exec_id}`,层次链即根到自身路径;
- 字符白名单 `[A-Za-z0-9:_/-]`,解析失败返回 `ActorIdError`;
- 编码是分区 UUIDv5 的种子,格式 A 一旦发布必须保持稳定;
- fork/join 等分支语义不属于 ActorId,由文件分支名表达。

## 文本 diff 域(common)

面向**文件文本**的展示与判定能力(与 `delta` 状态差分无关,二者只是名字相近):

- `LineDiff` / `Hunk` / `DiffOp`:行级差异;`WordDiff` / `WordChange`:行内词级差异(内联高亮用);
- `DiffStats` / `diff_stat_counts` / `diff_stats_for_text`:增删行统计;
- `format_unified_diff` / `unified_diff_text`:统一 diff 渲染;
- `is_binary`:二进制内容判定;
- `should_use_full_snapshot(_content)` + `DEFAULT_FULL_SNAPSHOT_THRESHOLD`:按差异比例决定走增量还是完整快照;
- `content_hash`:内容哈希;`LineInstanceId`:行实例标识(重命名/移动检测辅助)。

## 状态差分域(delta)

面向**结构化执行状态快照**(消息、变量、节点结果)的差分与恢复。模块头明确声明:与 `common::line_diff` 无共享逻辑,必须保持分离。

- trait(`diff` 子模块):
  - `DiffCalculator<SS, DS>`:快照对 → Delta;
  - `CheckpointLoader`:按 id 加载原始数据与元数据(存储层适配点);
  - `DeltaRestorer<SS, DS>`:目标检查点 id → 完整状态。
- 实现(`calculator` 子模块):`WorkflowDiffCalculator` 与 `AgentDiffCalculator`,分别对工作流与 Agent 快照做差分;同一目录下的 `*_tests.rs` 是差分语义的单元测试,`test_support.rs` 提供构造工具。
- 回放(`restorer` 子模块):`GenericDeltaRestorer` —— 建链 → 定位 Full 基线 → 加载基线快照 → 逐条 apply → 返回目标状态。链回放上限 10000(与存储清理的压缩守卫镜像),断链/超长各自报错。

## 策略(strategy)

- `CheckpointStrategy` trait(在 `inner`):
  - `should_checkpoint(trigger, context)`:本次是否创建检查点;
  - `content_config()` / `retention_config()`:内容捕获与保留配置;
  - `is_retention_exceeded(count, oldest, now)`:数量超限或最老记录超过 `max_age`(毫秒);now 缺失不判定;
  - `compression_strategy()`:默认 `auto`。
- `StandardStrategy`:标准实现(启用 + 触发匹配),另提供 `policy_none` / `policy_minimal` / `policy_standard` / `policy_comprehensive` 预设与 `create_checkpoint_strategy[_by_name]` 工厂。
- `cadenced` 子模块:`CadencedCheckpointStrategy<T>` 包裹 StandardStrategy,加两步过滤——时机需在启用集合内;为配置了 cadence > 1 的时机按计数取模(每 N 次一次)。`CheckpointTimingVariant` 允许上层用自己的时机枚举驱动(`CheckpointTiming` 自身即恒等映射)。

## 元数据(metadata)

`metadata/builder`:

- `CheckpointMetadataBuilder` 与 `build_checkpoint_metadata` / `build_checkpoint_state`:统一组装检查点核心元数据;
- 自定义字段常量(写入元数据 custom fields / storage 链元数据):
  - `formatVersion`、`createdAt`、`chainPosition`;
  - 消息序列界:`msgSeqStart` / `msgSeqEnd` / `msgNextSeq`(进度可只读元数据得出);
  - Agent 进度坐标:`iteration` / `toolCallCount` / `loopStatus` / `pendingToolCallCount`;
  - Workflow 进度坐标:`workflowStatus` / `workflowCurrentNode` / `workflowNodeResultsHash` / `workflowVariablesHash` / `workflowNodeRecordCount`(map 值域存 FNV-1a 排序哈希,同 key 值变更仍强制新行,又不把 blob 复制进元数据);
  - `trigger_tag` / `trigger_description`:触发源的可读标注;
  - `custom_fields_equal`:合并回判定的相等比较。

`metadata/keys` 提供其余键名常量。

## 序列化(serializer)

`CheckpointSerializer` + `CheckpointCodec { Bincode, Json }`:

- 编码选择按调用方传入;二进制 bincode 带魔数 `0xBC`;
- 单层压缩:策略为 `Auto` 且编码后超过 `COMPRESSION_THRESHOLD`(512 字节)时 gzip(魔数 `1F 8B`),读取按魔数透明解压;
- 文件侧快照压缩在 `checkpoint-file` 存储内部完成,此处不再叠加第三层。

## 清理策略(cleanup_policy)

决定**哪些执行状态检查点可以删除**(文件回收在 `checkpoint-file::gc`,二者共享结果形状但不共享实现):

- 筛选维度:年龄、数量、大小、分层(`RetentionTier { min_age_days, max_age_days?, retention_interval_days }`,每窗口至多保留一个,0 表示全保留);
- 决策后施加**链保护**:借助 `CheckpointDependencyGraph` 保证被链内后续检查点引用的行不被删除;
- 输出按实体/链分组,交由状态管理器的 cleanup 执行;指标经 `wf_metrics::CheckpointMetricsCollector` 记录。

## 依赖守卫图(checkpoint_graph)

`CheckpointDependencyGraph` 从 `CheckpointStorageMetadata` 列表构建:

- `referenced_by`:id → 以它为 `previous_checkpoint_id` 的后续检查点;
- `chain_root_map`:id → 链根(`chainRootId ?? id`);
- `chain_groups`:链根 → 成员集合。

只守卫 `wf-storage` 的线性链;文件历史的 Git 多父 DAG 有自己的 mark-sweep,两图不共享节点、不合并。

## 分层配置解析(config_resolver)

`CheckpointConfigResolver` 把 `UnifiedCheckpointPolicy` 从多层来源合并:

- 层与优先级:`Runtime > Node > Agent > Workflow > Global > Default`(数值越小越优先,首次命中即胜出);
- Node/Agent 比包裹它们的 Workflow 层更具体,故优先级更高;
- 合并规则:触发列表取第一个非空层;内容、保留、错误处理逐字段由低层补缺。

## 执行事件总线(execution_events)

`ExecutionEventBus`:

- 订阅粒度:按 `ExecutionEventType` 注册,或 `*` 通配;返回退订函数;
- 容错:handler panic / 返回错误被捕获并路由到已注册的错误处理器,坏订阅者不中断执行流;
- 与文件层 `CheckpointEventBus`(`checkpoint-file::event`)是两套独立总线:前者面向执行状态事件,后者面向检查点生命周期事件。

## 版本管理(version_manager)

- 当前格式 `CURRENT_FORMAT_VERSION = "1.1.0"`,最低兼容 `MIN_COMPATIBLE_VERSION = "1.0.0"`;
- `SemanticVersion` 数字化解析(接受 3 段或 2 段形式),避免字符串比较(`"1.10.0" > "1.9.0"` 正确成立);
- `VersionManager` 维护迁移 handler 注册表(`DashMap<版本, handler>`),对载入 blob 判定 `VersionCompatibility`(当前 / 可迁移 / 不兼容),迁移时以原始字节调用 handler 重写。
