# checkpoint-file:文件内容历史层

`crates/infra/checkpoint/checkpoint-file` 实现文件内容检查点:每个工作区的文件历史,以独立 bare Git 对象库为唯一内容事实来源,辅以小型 SQLite 元数据库。用户的仓库(`.git`)从不读写。

## 总体架构

```text
写通道(三条,全部落到 GitStore):
  precise        工具精确上报的单文件事件(主通道,按 Actor 分区应用)
  script_capture 脚本执行前后哈希扫描,一次运行一个原子 commit
  watcher        外部人为编辑的轮询/监听泵(ManualChangeService)
工具层入口:
  session        CheckpointSession(每次执行一个)+ ScopeCapture(范围采样)
读域:
  provenance     基于 commit DAG 的溯源查询(索引加速,图扫描兜底)
  approval       审批读模型与冲突视图
  gc             基于 ref 可达性的对象回收
```

## Git 对象库(git_store)

每工作区一个 bare 仓库:`<workspace>/.wf-checkpoint-git`。冻结规则(仅此模块定义,外部不得重定义):

- **refs 体系**(只存在于 `refs/wf/` 下):
  | ref | 语义 |
  | --- | --- |
  | `refs/wf/main` | 主线,整合事实;唯一允许物化到磁盘的树 |
  | `refs/wf/edit/<actor>` | 单个 Actor 的编辑线 |
  | `refs/wf/review/<id>` | 一次待审批的提交 |
  | `refs/wf/feat/<name>` | 一个协作目标(feature) |
  | `refs/wf/human` | 外部人为编辑;永不自动合并 |
- **归属 trailer**:`Wf-Actor` / `Wf-Session` / `Wf-Tool` 记录写者;`Wf-State: conflict-unresolved` / `conflict-resolved` 标记合并状态。
- **忽略叠加顺序**:仓库本地排除(`.git`、`.wf-checkpoint-git`、检查点数据库文件)→ 工作区自身 `.gitignore` → 自定义模式。
- **冲突落盘**:标准 `<<<<<<<` / `=======` / `>>>>>>>` 标记写入文件,并阻塞对应合并直至解决。

子模块:`objects`(对象读写)、`refs`(引用操作)、`commit`(提交构造)、`merge`(三路合并)、`errors`。

## SQLite 元数据(storage)

Git 无法廉价表达的数据存 SQLite(子模块:`repository`、`migrations`、`connection`、`meta`、`git_meta`):

- 审批状态、空目录清单、源索引缓存(source index)、状态→文件链接、KV 元数据。

## Manager(file/manager 组合)

`FileCheckpointManager` 是文件层门面,内部组合两个拆分组件(`manager_store.rs`):

- `ManagerStore`:持久化句柄 —— `Option<Arc<SqliteStorage>>`、`Option<Arc<GitStore>>`(未绑定工作区前为 None,Git 操作显式报"未初始化"而非静默回退旧 SQLite 内容表)、Actor→最新检查点的内存镜像(DB 为权威)。
- `ManagerPolicy`:行为配置 —— 扫描规则(`ScanConfig`)、审批策略(`ApprovalPolicy`)、冲突行为(`ConflictBehavior`)、阈值、GC 配置。

Manager 的行为按文件拆分:

| 文件 | 职责 |
| --- | --- |
| `file/checkpoint.rs` | 检查点创建:暂存全部条目,在 Actor 的 edit ref 上一次原子 commit;记录创建耗时/大小/链长/是否完整快照指标 |
| `file/actor.rs` | 分区生命周期与单文件编辑原语(`PreciseFileEvent` 应用) |
| `file/git_write.rs` / `git_read.rs` | Git 写入与读取,错误映射 |
| `file/git_merge.rs` / `merge.rs` | 合并执行与 `MergeCommitResult` |
| `file/restore.rs` | 工作区恢复:`WorkspaceRestoreResult`,按状态→文件链接检出当时的树 |
| `file/workspace.rs` | 工作区绑定与条目枚举 |
| `file/session.rs` | 会话相关内部逻辑 |
| `file/approval.rs` | 审批编排(submit / approve / reject / 合并进 feature)——注意与 `approval.rs` 的读模型分离 |
| `file/lifecycle.rs`、`queries.rs`、`capture.rs`、`types.rs`(manager/) | 生命周期、查询、捕获与类型 |

## 捕获三通道

### 精确事件(precise)

工具执行直接上报的单文件事件:`PreciseFileEventKind { Created, Modified, Deleted, Renamed { from } }`,携带绝对路径与新内容哈希。批量应用入口在 `precise.rs`,分区生命周期与单编辑原语在 `file/actor.rs`;应用结果统计为 `PreciseApplyStats`。这是文件历史的主写通道。

### 脚本捕获(script_capture)

脚本/命令执行前后各做一次工作区哈希扫描,差分出 `CollectedChange { path, kind: Add/Modify/Delete }`;一次运行的所有变更合并为**一个原子 commit**。失败行为按 `FailureBehavior`(Error/Warn/Ignore)处理。`scope.rs` 的 `ScopeCapture` 负责范围解析:请求的 shell 目录与工作区根取交集;请求范围在工作区外时返回 `None` —— 明确"不做同步捕获",绝不做全库扫描;范围包含工作区时收窄到工作区根。作用域结束产出 `ScopeEndOutcome`。

### 外部编辑(watcher)

`ManualChangeService` 是生产环境的泵:notify 文件系统事件 + 轮询,产出 `FileChangeRecord { path, kind: Add/Change/Unlink/Rename }`(Rename 时 `path` 为新路径、`from` 为旧路径),经 `WorkspaceScanner` 捕获为人为编辑提交;`normalize_absolute_path` 统一路径形态。

### 会话(session.rs)

`CheckpointSession` 是工具层的捕获句柄,取代旧 `wf_tools::ToolSideEffectObserver` trait:工具上下文携带 `Option<CheckpointSession>`(`Arc<ScopeCapture>` 廉价克隆),文件/shell 工具直接调用其方法。每个方法对应一条内部记录,由会话的单调 `sequence` 排序;内部委托 `FileCheckpointManager` + `ScopeCapture`。

## 扫描(scan.rs)

`WorkspaceScanner` + `ScanConfig { custom_ignore_patterns, failure_behavior }`:

- 硬编码忽略目录(任意深度、恢复时永不删除):`.git`、`node_modules`、`.wf-checkpoint-git`;
- 忽略顺序按 git_store 冻结规则叠加;
- 输出 `WorkspaceScan`(含每文件 `FileState`:哈希、大小、mtime)。

## 溯源(provenance)

基于 commit DAG 的读域;源索引只加速热查询(Actor / 路径),索引为空或损坏时回退到有界 commit 图扫描。diff 渲染复用 `checkpoint-base` 的行 diff 展示能力(仅展示,从不用于存储寻址)。

| API | 语义 |
| --- | --- |
| `list_changes_by_actor` / `list_changes_by_path` | 按 Actor / 路径列变更 |
| `list_partitions` | 列出分区(`PartitionView`) |
| `diff_actors` / `diff_against_main` / `diff_workspaces` | Actor 间 / 对主线 / 跨工作区 diff(`FileDiffView`,含 `FileDiffKind` 与 `DeltaSummary`) |
| `file_timeline` | 单文件时间线(`FileTimeline` / entry),含重命名检测(`RENAME_SIMILARITY_THRESHOLD`) |
| `list_conflicts` / `parse_marker_conflicts` / `unresolved_merges` | 冲突枚举与标记区间解析(`ConflictFile`) |
| `get_main_workspace` / `get_actor_workspace` / `get_feature_workspace` / `workspace_entries` | 树物化读取(`WorkspaceFile`) |
| `rebuild_source_index` | 重建源索引 |

## 审批(approval.rs 读模型 / file/approval.rs 编排)

- 流程:Actor 把变更提交进审批层(`refs/wf/review/<id>`),人工批准或拒绝后合并进 feature 分支;手动审批模式下历史长度 > 1 即视为有待审批项。
- 读模型(`PendingApproval`):Actor id(如 `agent:{loop_id}`)、审批快照 id(hex)、提交时间(Unix ms)、按文件分块的时序变更(`Vec<DeltaSummary>`)。
- 冲突视图(`ConflictView`):三路合并冲突的读视图,`GitConflictDetail` / `GitConflictRegion` 从合并后的字节解析真实标记区间;二进制冲突只有文件级,不产生区间。

## 回收(gc.rs)

对象库自有可达性即策略:从任何 ref 不可达的松散对象被 prune,被删 commit 的源索引行随之删除;不存在内容表上的独立 mark-sweep(文件字节只活在对象库里)。统计 `GcStats { removed_checkpoints, removed_snapshots, reclaimed_snapshots }`(不可达 commit / 树 / blob)。

## 分支与命名(branch.rs)

文件分支即 Git refs(edit / review / feature / main / human 五类);`naming` 提供 feature 分支名校验与保留名检查,`feature` 负责确保 feature 分支存在。执行隔离由 ActorId 层次表达,不在文件层维护第二套分支索引。

## 事件(event.rs)

`CheckpointEvent` + `CheckpointEventBus`:检查点生命周期事件(创建 / 失败 / 清理跳过等)的发布订阅,供状态层协调器与上层 UI 使用。

## 集成测试(仓库级)

`crates/infra/checkpoint/wf-checkpoint/tests/` 中的 `file_branch_dag_integration.rs`、`source_capture.rs` 覆盖本层与协调器的联合语义。
