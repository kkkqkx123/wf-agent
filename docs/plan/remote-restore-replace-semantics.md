# 远端恢复的替换语义设计

## 0. 背景与范围

恢复路径现在要求目标不存在：`restore_bundle` 在预校验通过后先做一次执行解析，命中即返回 `AlreadyExists`（`crates/app/wf-api/src/execution_bundle.rs:487`，错误体 `:491`）。

而恢复被引入的动机是远端数据损坏，属于目标已经存在的场景。两者互斥，结果是恢复只能在数据还没受损时使用，也就是用不上。

本文只讨论恢复写入的语义与保护，不涉及备份调度、库存比对和凭证。

## 1. 现状语义与缺陷

### 1.1 解析器把三种状态压成一态

存在性判断走 `resolve_execution`（`crates/app/wf-api/src/entity/execution.rs:101`），它按代价分层探测：活跃注册表、持久执行记录、检查点记录（`:143` 起）。任何一层命中都算存在，于是以下五种完全不同的情况得到同一个结论：

| 实际状态 | 现在的结果 | 应有的结果 |
|---|---|---|
| 完全不存在 | 拒绝，正确 | 新建 |
| 存在且与快照一致 | 拒绝 | 幂等成功，不写 |
| 存在但与快照不一致（损坏） | 拒绝 | 替换 |
| 只剩检查点残留、记录已丢 | 拒绝 | 替换，且这是最该恢复的情况 |
| 正在运行 | 拒绝，正确 | 拒绝，需先取消 |

第四行是解析器分层的副作用：检查点记录也算命中，所以“记录丢了但检查点还在”这种最典型的损坏形态反而无法恢复。

### 1.2 写入不可重入

写入按执行记录、检查点、文件归档、修订行的顺序逐条进行，中间没有任何事务边界（`execution_bundle.rs:506`、`:519`、`:543`、`:555`）。任一步失败即返回错误，此前写入的批次已经落地。

在“目标必须不存在”的语义下，这个中间态是不可恢复的：目标由不存在变成存在，重试必然再次撞上 `AlreadyExists`（`crates/app/wf-server/src/envelope.rs:124` 映射为 409）。一次网络抖动就能把目标永久锁死，只能人工删库。

这不是保护，是不可重入缺陷。

### 1.3 与状态机断链

远端被判定为 `remote_corrupt` 后（`crates/app/wf-server/src/backends/manager.rs:506`），CLI 的恢复入口会把它选为候选（`crates/app/cli/wf-cli-shared/src/cmd/remote.rs:44`）。候选一定存在，恢复一定冲突。系统自己制造了一个没有出口的状态。

### 1.4 覆盖本身不完整

即使去掉存在性检查，直接写也达不到“内容等于快照”：

- 检查点导出按执行全量列出（`execution_bundle.rs:188`），恢复只写快照里的行而不删多余行，多余的元数据会进入下次导出，摘要必然不符。
- 归档导入只提交快照内的路径（`crates/infra/checkpoint/checkpoint-file/src/archive.rs:37`），旧归档里多出的文件不会被删除，`archive_export` 仍会把它们导出来（`archive.rs:126`）。

这两点决定了替换必须是“删除多余 + 写入快照”，而不是“往里追加”。

## 2. 正确语义

恢复是一个收敛操作，不是一次插入：

> `apply(bundle)` 的后置条件是目标的可恢复投影等于 bundle 的可恢复投影；同一个 bundle 重复 apply 结果不变。

三个推论：

1. **创建只是替换的特例**。目标缺席时，替换的“删除多余”部分恰好为空，不需要单独的创建路径。
2. **幂等必须显式**。第二次 apply 与第一次的差别只体现在返回的动作上（`create` / `replace` / `noop`），不体现在数据上。
3. **保护要落在意图上，不要落在存在性上**。该拒绝的是“运行中的目标”和“越界目录”，不是“存在的目标”。

为什么不是“先删后写”：删与写之间的窗口一旦崩溃就是净数据丢失，而且删除是不可回滚的。正确做法是一次性提交“删除多余 + 写入快照”，让目标在任何时刻都处于旧状态或新状态之一。

## 3. 目标状态判定

替换 `resolve_execution`，改成一次投影探测：

```rust
struct TargetProbe {
    agent_record: bool,
    workflow_record: bool,
    checkpoints: usize,
    archive_files: usize,
}

enum TargetState {
    Absent,
    Identical { sequence: u64, digest: String },
    Divergent { sequence: Option<u64>, reason: DivergenceReason },
    Live,
}

enum DivergenceReason {
    Partial,          // 记录残缺，无法导出
    DigestMismatch,   // 可导出，但与快照摘要不同
    ForeignDomain,    // 现存记录属于另一个域
}
```

判定顺序：

| 步骤 | 依据 | 结论 |
|---|---|---|
| 1 | 活跃注册表命中（`ctx.agent_loop` / `ctx.workflow_execution`，`entity/execution.rs:149`） | `Live`，拒绝 |
| 2 | 两张执行记录表都无行、无检查点、无归档 | `Absent` |
| 3 | 目标现记录属于另一域 | `Divergent { ForeignDomain }` |
| 4 | `build_bundle(ctx, id, false, false)` 失败（`execution_bundle.rs:234`） | `Divergent { Partial }` |
| 5 | 导出摘要等于 `bundle.payload_digest` | `Identical` |
| 6 | 其余 | `Divergent { DigestMismatch }` |

第 4 步复用 `build_bundle` 而不是另写一套投影计算，保证“判定用的摘要”和“备份/库存用的摘要”是同一个函数，不会各自漂移。它本来就是一次完整导出，成本与恢复本身同量级，恢复又是低频动作，可以接受。

第 3 步单独列出，因为域切换会触发 id 双域歧义：`execution_verdict` 在 agent 与 workflow 同时命中时返回 `Conflict`（`entity/execution.rs:115`）。替换若不清理对侧记录，恢复后目标根本解析不出来。

## 4. 替换保护

三道保护，全部落在意图与边界上。

### 4.1 运行中目标一律拒绝

活跃注册表命中即拒绝，不能替换。替换会与运行期写入竞争，且快照里的 `status` 会把一个运行中的执行改写成历史态。

拒绝信息要可操作：`execution '<id>' is live on this node; cancel it before restoring`。使用 `ApiError::Conflict`，不使用 `AlreadyExists`，因为此时问题不是“已存在”。

### 4.2 显式意图

恢复请求增加模式，类型定义在 `wf-api`（与 `restore_bundle` 同处，避免服务端反向依赖）：

```rust
pub enum RestoreMode {
    /// 允许 Absent 与 Identical。
    Auto,
    /// 只允许 Absent 与 Identical；存在但内容不一致时拒绝。
    Create,
    /// 允许 Absent、Identical 与 Divergent。
    Replace,
}
```

`Create` 也接受 `Identical`：重复应用同一份快照不应该失败，否则批量恢复会因“已经恢复过”而报错。拒绝的只是内容不一致的目标。

默认 `Auto`。`remote_corrupt` 的批量恢复显式传 `Replace`，因为那个状态本身就是“存在且不一致”。

dry run 必须报告判定结果与预计动作，让操作者在 apply 之前看到“这会覆盖一份现存数据”：

```rust
pub struct RestorePlan {
    pub execution_id: String,
    pub dry_run: bool,
    pub records: usize,
    pub checkpoints: usize,
    pub file_archive: usize,
    pub digest: String,
    pub remote_sequence: u64,
    pub remote_digest: String,
    // 新增
    pub target_state: String,          // absent | identical | divergent
    pub action: String,                // create | noop | replace
    pub previous_sequence: Option<u64>,
    pub previous_digest: Option<String>,
}
```

### 4.3 目录边界

恢复端点已经要求目录作用域（`crates/app/wf-server/src/api/recovery.rs:42`），且预校验会比对记录目录（`execution_bundle.rs:440`）。替换语义下补一条：现存目标的目录与当前作用域不符时拒绝，不跨目录覆盖。这是隔离边界，不能靠“反正会被覆盖”绕过。

## 5. 原子性改造

### 5.1 现有写入边界

当前写入分成五段，互相之间没有事务：

| 段 | 位置 | 事务 |
|---|---|---|
| 执行记录 | `execution_bundle.rs:506` | 每次 save 行与修订成对（`crates/infra/wf-storage/src/backend.rs:273`） |
| 检查点 blob | `:519` | 走裸 backend，不推进修订 |
| 检查点元数据 | `:538` | 走适配器，推进修订 |
| 文件归档 | `:543` | git ref，不在 SQL 内 |
| 修订行 | `:555` | 单独覆盖写 |

单次 save 是自洽的（数据行与修订同行落地），但四次 save 之间不是。替换语义要求它们成为一个原子单位，否则每次都留下一个中间摘要。

### 5.2 表内覆盖可以一次提交

`StorageContext::apply_atomic` 已支持多表原子提交，Sqlite 与 PostgreSQL 在共享 pool 上开一个事务，内存后端按注册顺序加锁（`crates/infra/wf-storage/src/context.rs:453`）。执行记录表、检查点元数据表都在同一 pool。

它绕过了适配器的修订联动，正好符合需要：整个覆盖过程只应该在最后写一次修订行，而不是每行一次。

### 5.3 修订行必须进同一个事务

障碍是 `EntityStoreId` 只枚举实体表（`context.rs:45`），`backend_of` 也只按实体字段取 backend（`:204`），而修订是独立的 `Arc<RevisionStore>`。所以 `apply_atomic` 目前无法把修订表纳入。

两种做法：

- **A（推荐）**：给 `StorageContext` 增加一个带修订行的批量入口，内部把修订行作为一条额外的表操作并入同一个事务。

```rust
/// Apply entity writes and the matching revision row in one transaction.
/// The revision row is written last so a reader never sees a position that
/// describes data the same transaction has not committed.
pub async fn apply_atomic_with_revision(
    &self,
    operations: &[AtomicOperation],
    revision: Option<RevisionWrite>,
) -> Result<(), StorageError>
```

`RevisionWrite` 就是修订行的原始三元组（id、data、metadata），由 `revision.rs` 提供构造函数，避免调用方手写编码。

- **B**：把 `EntityStoreId` 扩展出一个修订变体。代价是 `backend_of` 的宏匹配要特殊处理，修订不进实体注册表，收益不抵复杂度。

选 A。

### 5.4 归档只能两阶段

文件归档是 git ref（`archive.rs:37`），不能进 SQL 事务。分两阶段：

1. 事务提交前：**不**改 ref。
2. 事务提交后：一次提交完成“删除多余路径 + 写入快照路径”。

整体替换是可以做到的，因为 `build_tree_from_parent` 对 `None` 就是删除路径（`crates/infra/checkpoint/checkpoint-file/src/git_store/objects.rs:227`），而现有 `archive_write_files` 只构造 `Some`（`archive.rs:55`）。需要补一个替换方法：

```rust
/// Replace the whole archive of one execution. Paths absent from `files`
/// are removed, so a later export cannot surface stale archive content.
pub fn archive_replace(
    &self,
    entity_id: &str,
    files: &HashMap<String, Vec<u8>>,
) -> Result<ArchiveWriteOutcome, CheckpointError> {
    let existing = self.archive_export(entity_id)?;
    let mut changes: HashMap<String, Option<(String, Vec<u8>)>> = HashMap::new();
    for (path, _) in existing {
        if !files.contains_key(&path) {
            changes.insert(path, None);
        }
    }
    for (path, bytes) in files {
        changes.insert(path.clone(), Some((MODE_FILE.to_string(), bytes.clone())));
    }
    let outcome = self.git_ref()?.commit_on_ref(
        &context_archive_ref(entity_id),
        &changes,
        "context-archive",
        "context archive replace",
    )?;
    Ok(ArchiveWriteOutcome { commit: outcome.id, blobs: Vec::new() })
}
```

`commit_on_ref` 自带 CAS 重试（`objects.rs:412`），并发写不会互相覆盖。

### 5.5 失败窗口与重放

| 失败点 | 后果 | 恢复方式 |
|---|---|---|
| 写检查点 blob 阶段 | 无引用残留，不影响摘要 | 直接重试 |
| 事务提交前 | 事务回滚，数据未变 | 直接重试 |
| 事务提交后、归档前 | 记录与检查点已新，归档仍旧 | 摘要在下次比对时不符，重放恢复即可修正 |
| 归档提交后 | 全部新 | 无 |

关键点：**任何失败都能通过重放同一个 bundle 收敛**，因为替换是幂等的。这正是把语义从“创建”改成“收敛”换来的能力。

“事务已提交但归档未替换”是唯一一个对外可见的中间态，恢复响应必须如实返回它，不能报成功。

检查点 blob 的写先于事务，走裸 backend（`ApiContext::checkpoint_store` 默认就是 `storage.checkpoint.store()`，`crates/app/wf-api/src/infra/context.rs:150`），按 id upsert，重复执行无副作用。快照外的旧 blob 不必删除：导出按元数据取 blob（`execution_bundle.rs:194`），无元数据引用的 blob 不进摘要，只是占空间。

## 6. 覆盖写清单

| 目标 | 动作 | 位置 |
|---|---|---|
| 执行记录（快照域） | save | 事务 |
| 执行记录（对侧域） | delete，仅当现存 | 事务 |
| 检查点元数据（快照内） | save | 事务 |
| 检查点元数据（快照外） | delete | 事务 |
| 检查点 blob（快照内） | save，可重入 | 事务前 |
| 检查点 blob（快照外） | 保留，不引用 | — |
| 文件归档 | 整体替换 | 事务后 |
| 修订行 | 一次写，序号取 `max(本地位点, 快照序号) + 1` | 事务内 |

修订序号仍然加一，不沿用快照序号：沿用会让“恢复”与“恢复前”看起来一样，管理端刚建立的基线立刻失配；加一则明确表达“内容被重写过一次”。

## 7. 接口与错误语义

| 情况 | 现状 | 目标 |
|---|---|---|
| 目标不存在 | 200 | 200，`action=create` |
| 目标一致 | 409 `ALREADY_EXISTS` | 200，`action=noop` |
| 目标不一致 + `Auto` | 409 | 409 `CONFLICT`，提示需要 `Replace` |
| 目标不一致 + `Replace` | 409 | 200，`action=replace` |
| 目标运行中 | 409 | 409 `CONFLICT`，提示先取消 |
| 目录不符 | 400 | 400 / 403 |

请求体（`crates/app/wf-server/src/api/recovery.rs:14`）：

```rust
pub(crate) struct RestoreRequest {
    pub(crate) bundle: ExecutionBundle,
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) mode: RestoreMode,
}
```

## 8. 上下游配合

- 管理端 `restore_backup`（`crates/app/wf-server/src/backends/manager.rs:1503`）增加模式参数并透传；恢复成功后仍走 `publish_restored_position` 采纳新位点（`:1566`）。
- `wf remote backup-restore ID --apply` 默认 `Auto`，增加 `--replace`。
- `wf remote recover` 按状态选模式：`remote_missing` 用 `Create`，`remote_corrupt` 用 `Replace`（筛选逻辑在 `crates/app/cli/wf-cli-shared/src/cmd/remote.rs:44`）。
- 取证：`Replace` 前把目标现值导出一份快照落盘，路径与备份目录同构，索引标记来源为恢复前状态。这样“覆盖”是可追溯的删除，而不是不可逆的删除。

## 9. 代码改动清单

| 文件 | 改动 |
|---|---|
| `crates/app/wf-api/src/execution_bundle.rs` | 存在性检查换成三态探测；新增 `RestoreMode`；`RestorePlan` 增加目标状态与动作；写入改为“blob → 事务 → 归档”三段；归档用整体替换 |
| `crates/app/wf-api/src/entity/execution.rs` | 新增 live 判定辅助（只查活跃注册表），供恢复路径复用 |
| `crates/app/wf-server/src/api/recovery.rs` | 请求体增加模式；错误映射区分 live 与 divergent |
| `crates/app/wf-server/src/backends/manager.rs` | 模式透传；取证快照落盘；`recover` 按状态选模式 |
| `crates/infra/wf-storage/src/context.rs` | 新增 `apply_atomic_with_revision` |
| `crates/infra/wf-storage/src/revision.rs` | 暴露一次性的修订写入构造，供事务内使用 |
| `crates/infra/checkpoint/checkpoint-file/src/archive.rs` | 新增 `archive_replace` |
| `crates/app/cli/wf-cli-shared/src/args_remote.rs` | `--replace` |
| `crates/app/cli/wf-cli-shared/src/cmd/remote.rs` | 按状态选模式 |
| `docs/features/remote/backup.md` | 第 154 行“远端恢复只接受当前不存在的同一执行 id”整段换成三态语义 |
| `docs/features/remote/error-handling.md` | 恢复失败的分类按第 7 节重写 |

## 10. 待拍板开放点

1. **运行中目标是否允许强制替换**。建议否。若确有需求，应做成“先取消再替换”的复合操作，而不是一个开关。
2. **取证的默认行为**。建议 `Replace` 时默认开启，保留一份；是否允许关闭、保留多份，需要运维侧决定。
3. **`Identical` 是否推进序号**。幂等要求不推进；但“恢复过一次”这件事是否需要留痕，可以在审计里记而不动修订。
4. **跨目录替换**。建议拒绝。若允许，需要先明确目录作用域的所有权规则。
5. **归档阶段的失败如何对外表达**。“已提交事务、未替换归档”需要一个明确的部分成功状态码，不能混在 500 里。
6. **是否补一个执行删除接口**。有了替换语义，删除接口不再是恢复的前置条件，但运维清理仍可能需要。

## 11. 验证计划与实施状态

已实现的判定与用例：

- `Absent` 目标恢复成功，`action=create`（既有往返用例）。
- `Identical` 目标重复恢复成功且不写数据，序号不变（`matching_target_restores_without_writing`）。
- `Divergent`（摘要不符）在 `Auto` 与 `Create` 下被拒且不改动目标（`mismatching_target_is_refused_without_replace`）。
- `Divergent`（摘要不符）在 `Replace` 下覆盖成功，恢复后导出摘要等于快照（`replace_converges_a_mismatching_target`）。
- `Divergent`（只剩检查点残留）在 `Replace` 下覆盖成功，且快照外的检查点行被删除（`partial_target_is_replaceable`）。

仍未覆盖，需要后续补：

- 运行中目标拒绝：活跃注册表需要构造运行期句柄，当前测试上下文没有注册入口。
- 域切换：现存记录为另一域时的替换，需要构造另一域记录。
- 归档多余文件：需要挂载文件检查点管理器。
- 事务失败重放：需要可注入失败的存储后端。

实施情况：

| 项 | 状态 |
|---|---|
| 三态探测与写入三段 | 已实现 |
| `RestoreMode` 与请求体、CLI `--replace` | 已实现 |
| `apply_atomic_with_revision` | 已实现 |
| `archive_replace` | 已实现 |
| 运行中与目录边界保护 | 已实现 |
| 覆盖前取证快照 | 未实施，见下 |

取证快照没有实施：现有落盘入口要求远端修订行的序号大于零，而部分损坏的目标恰恰没有修订行；同时备份索引没有来源字段，取证文件会被下一次基线回落当成普通备份取用，反而干扰位点比较。要落地需要先给索引加来源标记并调整基线选择，属第 10 章第 2 项待拍板内容。
