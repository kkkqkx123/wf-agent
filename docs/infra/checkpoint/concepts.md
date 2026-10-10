# 核心概念语义总表

按子系统分层整理 checkpoint 模块全部核心概念的语义。定义均以代码实现为准(2026-10)。

## 1. 通用概念

| 概念 | 语义 |
| --- | --- |
| 检查点(Checkpoint) | 某个执行实体(工作流 / Agent 循环 / 子图)在某一时刻的可恢复状态记录。分两类:执行状态检查点与文件内容检查点,二者存储与恢复完全独立。 |
| 实体(entity) | 被检查点化的对象。`entity_type` 标识类别(`workflow` / `agent` / 子级类型),`entity_id` 是该次执行的运行期 id。 |
| 完整快照(Full) | 自包含的完整状态记录,是增量链的基线。写入时机由 `decide_checkpoint_type_by_count` 决定:计数为 0、有效间隔为 0 或计数命中基线间隔的倍数。 |
| 增量(Delta) | 相对前一状态的差分。与 Full 交替形成增量链,链长受 `max_delta_chain_length` 限制。 |
| 增量链(delta chain) | `previous_checkpoint_id` 串起的线性链。恢复时从最近的 Full 基线开始逐条回放 Delta;链回放上限 10000,断链报 `DeltaChainBroken`。 |
| 链位置(chainPosition) | 元数据字段:Full 重置为 0,Delta 在前值上加 1。用于快速判断链深度与压缩时机。 |
| 链根(chainRootId) | 增量链首个检查点的 id;`CheckpointDependencyGraph` 用它把链内成员分组,保护清理时不打断任何链。 |
| 触发时机(CheckpointTiming) | "何时创建检查点"的枚举信号。`CheckpointStrategy::should_checkpoint` 据此决策;`CadencedCheckpointStrategy` 可对每个时机附加"每 N 次一次"的节奏(cadence)。 |
| 检查点上下文(CheckpointContext) | 决策输入:当前计数、实体信息、自定义数据等,供策略判断使用。 |
| 统一策略(UnifiedCheckpointPolicy) | 一份分层配置:内容捕获范围、触发时机、保留策略、错误处理、压缩。配置解析按 Runtime > Node > Agent > Workflow > Global > Default 优先级逐层合并。 |
| 进度坐标(progress coordinates) | 写入每个检查点元数据的标量计数集合(Agent: iteration / toolCallCount / loopStatus / msgSeq* / pendingToolCallCount;Workflow: workflowStatus / currentNode / 各 map 的 FNV-1a 哈希等)。坐标相等 ⟺ 自该检查点以来无副作用落盘,重复创建可合并回既有行而非落新行。 |
| 待处理工具数(pendingToolCallCount) | Agent 元数据字段:正在飞行中的工具调用数。仅凭已提交进度无法看到飞行窗口,故单独记录;只记数量不记标识,流缓冲作为纯瞬态排除。 |
| 事件总线(Event Bus) | 两套发布/订阅:`ExecutionEventBus`(执行状态事件,订阅者按事件类型或通配注册,handler 异常被捕获不中断执行)与 `CheckpointEventBus`(文件检查点事件:创建/失败/清理跳过等)。 |
| 时钟(CheckpointClock) | 抽象时钟,生产用系统时间,测试用 `ManualClock` 手动驱动/置失败。所有时间判断显式传入,时钟缺失不触发清理/保留判定。 |
| 错误处理策略(CheckpointErrorHandler) | 对检查点错误做统一裁决,输出 `ErrorHandlingOutcome { should_rethrow, handled }`。默认严格(失败上抛),显式宽松配置才允许吞掉继续执行。 |
| 版本管理(VersionManager) | 序列化格式版本(`CURRENT_FORMAT_VERSION = 1.1.0`,最低兼容 1.0.0)的比较、兼容性判定与迁移 handler 注册。语义化版本比较,非字符串序。 |
| 序列化(CheckpointSerializer) | 执行状态信封的统一编码:bincode 或 JSON,外加单层压缩(>`512` 字节且策略为 Auto 时 gzip;读取按魔数透明解压)。文件侧压缩在 `checkpoint-file` 内部完成,不再叠加第三层。 |
| 缓存(CheckpointCache) | 基于 moka 的 TTL + 容量上限缓存,附单飞(single-flight):同 key 并发 miss 等待首个工厂调用完成后回读,避免重复昂贵加载。 |
| 清理策略(cleanup_policy) | 执行状态检查点的回收决策:按年龄/数量/大小/分层(RetentionTier)筛选,再施加链保护(链内成员不删)。文件侧回收不在此,见 GC。 |
| 依赖图(CheckpointDependencyGraph) | `referenced_by` / `chain_root_map` / `chain_groups` 三个映射,守卫 `wf-storage` 中线性 previous-id 链;与文件 Git DAG 完全分离。 |

## 2. Actor 与文件分区

| 概念 | 语义 |
| --- | --- |
| Actor(参与者) | 工作区内产生文件变更的编辑主体:某次执行(agent 循环 / 工作流 / 子图)。文件历史按 Actor 分区。 |
| ActorId | Actor 的稳定可逆编码,格式 A:`"{kind}:{hierarchy}"`,kind ∈ `wf` / `agent` / `sub`;嵌套执行追加 `/child:{exec_id}` 段(根到自身的路径)。字符白名单 `[A-Za-z0-9:_/-]`;是分区 UUIDv5 的种子,编码变更必须保持稳定。 |
| 执行隔离 | 隔离关系由 ActorId 层次表达,不用第二套分支索引;分支(refs)表达的是协作/审批维度。 |
| 分区(partition) | 一个 Actor 在一个工作区内的全部文件历史集合;溯源查询(按 Actor/路径)以分区为单位组织。 |

## 3. 文件检查点体系

| 概念 | 语义 |
| --- | --- |
| Git 对象库(GitStore) | 每个工作区一个独立 bare 仓库 `<workspace>/.wf-checkpoint-git`。文件字节、树、历史、文件分支 ref 的唯一事实来源;用户的 `.git` 永不读写。 |
| refs 体系 | 仅存于 `refs/wf/` 下:`main`(主线,唯一可物化到磁盘的树)、`edit/<actor>`(单个 Actor 的编辑线)、`review/<id>`(待审批提交)、`feat/<name>`(协作目标)、`human`(外部人为编辑,永不自动合并)。 |
| 归属 trailer | commit trailer 承载归属:`Wf-Actor` / `Wf-Session` / `Wf-Tool`;`Wf-State: conflict-unresolved / conflict-resolved` 标记合并状态。 |
| 精确事件(precise events) | 工具直接上报的单文件变更(Created / Modified / Deleted / Renamed),文件历史的主写通道,按 Actor 分区应用。 |
| 脚本捕获(script capture) | 脚本执行前后对工作区做哈希扫描,一次运行产出一个原子 commit;检测 Add / Modify / Delete。 |
| 外部变更服务(ManualChangeService) | 基于 notify 文件系统监听 + 轮询的泵,捕获人为外部编辑(Add / Change / Unlink / Rename)。 |
| 会话(CheckpointSession) | 每次执行一个的捕获句柄,取代旧 `ToolSideEffectObserver` trait。工具上下文携带它,文件/shell 工具直接调用其方法;内部持有 manager + actor + entity + workspace + `ScopeCapture`。 |
| 范围捕获(ScopeCapture) | shell 执行范围与工作区根的交集解析、前像快照、后像差分;请求范围在工作区外时返回 None(明确"不捕获",而非全库扫描)。 |
| 文件检查点(FileCheckpoint) | 一次文件提交的投影:id、Actor、提交统计、链长、是否完整快照等;一个操作 = 一个原子 commit。 |
| FileCheckpointManager | 文件层门面:`ManagerStore` 管持久化句柄(SQLite、Git 库、latest 索引),`ManagerPolicy` 管行为配置(扫描、审批、阈值、GC)。 |
| 状态→文件链接(state-file link) | 状态检查点成功投影文件后记录的关联,使恢复能解析"当时确切的文件集合"而非"最新"。 |
| 溯源(provenance) | 基于 commit DAG 的读域:按 Actor/路径查变更、Actor 间/对主线/跨工作区 diff、文件时间线(含重命名相似度阈值)、分区视图、冲突枚举。源索引只是加速,索引缺失/损坏回退图扫描。 |
| 审批(approval) | Actor 把变更提交到审批层,人工批准/拒绝后合并进 feature 分支。读模型:`PendingApproval`(提交内容按文件分块)、`MergeOutcome`、`ConflictView`(从合并字节中解析 `<<<<<<<` 标记区间;二进制冲突只在文件级)。 |
| 冲突行为 | 冲突落盘为标准三路标记字节,并阻塞对应合并直到解决;策略可配置(`ConflictBehavior`)。 |
| 文件分支(branch) | 以 Git ref 表达(edit/review/feat/main/human 五类);feature 分支名有保留名校验,执行隔离不经过分支。 |
| 作用域结束(ScopeEndOutcome) | 一次捕获作用域结束时:差异收集、合并或丢弃的结论文本。 |
| GC | 文件侧回收:按 ref 可达性 prune 不可达松散对象,源索引行随 commit 一并删除;不重建 mark-sweep 内容表。统计:`GcStats { removed_checkpoints, removed_snapshots, reclaimed_snapshots }`。 |
| 忽略叠加 | 扫描忽略顺序:仓库本地排除(`.git`、`node_modules`、`.wf-checkpoint-git`)→ 工作区自身 `.gitignore` → 自定义模式。恢复永不删除这些项。 |

## 4. 状态检查点体系

| 概念 | 语义 |
| --- | --- |
| 状态管理器(CheckpointStateManager) | trait:`save / load / load_batch / delete / list_by_entity / get_latest / load_metadata` 等;关联类型 `Checkpoint`。实现为 `StorageBackedStateManager<T>`,底座是 `wf-storage` 的 `StorageBackend`。 |
| 状态信封(BaseCheckpointCore) | 两种检查点共用同一核心形状:核心元数据 + 差分(`TDelta`)+ 快照(`TSnapshot`)+ 压缩/格式版本等字段。`WorkflowCheckpoint` 与 `AgentCheckpoint` 都是该泛型的别名。 |
| 差分计算器(DiffCalculator) | 从快照对计算 Delta:`WorkflowDiffCalculator`(变量、节点结果、消息)与 `AgentDiffCalculator`(消息序列等)。与文件行 diff(`common::line_diff`)同域不同实现,必须保持分离。 |
| Delta 回放器(DeltaRestorer) | `GenericDeltaRestorer`:沿 `previous_checkpoint_id` 建链 → 找 Full 基线 → 加载基线快照 → 逐条 apply Delta → 得到目标状态。链长超限报 `DeltaChainTooLong`。 |
| 链压缩(compaction) | 存储清理侧把过长 Delta 链折叠为 Full,回放侧同样有长度守卫,两侧镜像同一上限。 |
| 层级恢复(hierarchy restore) | 恢复一个父执行后,发现并恢复其子级:`ChildDiscovery`(遍历深度上限 8)→ resolver 解析子级检查点 → 经 `RestoreStrategyRegistry` 逐类型恢复(并发上限 5)。 |
| 恢复事务(RecoveryTransaction) | 批量层级恢复的事务包装:`RecoveryOperation`(restore/cleanup 等操作)及其状态,失败按 `RollbackStrategy` 处理。 |
| 恢复策略注册表(RestoreStrategyRegistry) | `entity_type → RestoreFn` 映射;`RestoreFn` 接收检查点 id 与原始字节,返回恢复后的实体 JSON。协调器恢复子级时按子级类型查表。 |

## 5. 协调器(门面层)

| 概念 | 语义 |
| --- | --- |
| CheckpointCoordinator | 统一入口 trait,Workflow 与 Agent 各有一实现,共享 base 工具(计数决策、链位、上下文准备、自定义字段盖章)与流程(创建→持久化→投影→清理)。 |
| WorkflowCheckpointCoordinator | 工作流实体(节点执行、变量、消息)的检查点协调;差分/回放用 Workflow 类型;进度坐标取 `workflow_progress_coords`。 |
| AgentCheckpointCoordinator | Agent 循环实体的检查点协调;进度坐标取 `progress_coords`,含飞行中工具数;时间线(`TimelineRow`)供恢复点列表展示。 |
| 持久化(persist) | `persist_checkpoint`:写库 → 发布 created 事件;失败 → 发布 failed 事件并交 `CheckpointErrorHandler` 裁决(默认上抛,绝不伪装成功)。`delete_checkpoint` 同理。 |
| 持久化队列(PersistenceQueue) | 后台文件投影任务的有界队列(≤128);队满先排空积压再入队,内存有界,并发布 `cleanup_skipped` + 记指标。 |
| 文件投影(file projection) | 状态检查点落地后的同步 best-effort 动作:对实体当前工作区做一次 `create_latest_file_checkpoint`,成功则记录状态→文件链接;文件层缺失或失败均不影响状态检查点。 |
| 内容策略(content_policy) | 各协调器的捕获范围决策:哪些内容进快照、消息上限截断、敏感域过滤(与统一策略的 content 配置配合)。 |
| 子级发现索引(ChildDiscoveryIndex) | 协调器侧缓存的父子元数据索引(`ChildMetadataIndex`),供层级恢复快速定位子级,避免 N+1 查询(配合 `list_latest_by_entities`)。 |
| 迁移(migration) | `load_migrated`:读取 blob → 版本兼容检查 → 需要时经注册的迁移 handler 重写 blob 到当前版本。 |
| 崩溃恢复(crash recovery) | 集成测试保证的语义:重启后经层级恢复 + 恢复事务把实体恢复到最近一致检查点(见 `wf-checkpoint/tests/crash_recovery.rs`)。 |
