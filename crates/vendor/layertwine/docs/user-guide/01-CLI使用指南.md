# Layertwine CLI 使用指南

> Layertwine —— 轻量文件编辑历史存储层，专为多 Agent 协同 + 人工编辑混合场景设计。

## 概述

Layertwine 提供一套完整的命令行接口（CLI），用于管理文件编辑历史、分层状态机流转、检查点提交、分支管理、快照备份、数据清理以及 Git 双向同步。

当前 CLI 通过 Rust 库暴露，入口为 `layertwine::cli::run()`。所有功能均通过 clap v4 子命令系统组织。

---

## 全局选项

| 选项                    | 说明                         | 默认值                |
| ----------------------- | ---------------------------- | --------------------- |
| `-d, --db <PATH>`       | Layertwine 数据库文件路径       | `.layertwine/layertwine.db` |
| `-g, --git-repo <PATH>` | Git 仓库路径（用于同步操作） | 无                    |
| `--json`                | JSON 输出模式（全局标志）    | 否                    |
| `-h, --help`            | 打印帮助信息                 | —                     |
| `-V, --version`         | 打印版本号                   | —                     |

---

## 命令参考

### `layertwine init` — 初始化仓库

```
layertwine init [--git-ref <REF>]
```

在当前目录初始化一个新的 Layertwine 仓库。若指定了 `--git-repo`，则同时从 Git 仓库导入基线。

**选项：**

| 选项              | 说明                                                         |
| ----------------- | ------------------------------------------------------------ |
| `--git-ref <REF>` | 从 Git 仓库初始化时指定的引用（如 `HEAD`、分支名、提交哈希） |

**示例：**

```bash
# 初始化空仓库
layertwine init

# 从 Git 仓库导入基线初始化
layertwine --git-repo /path/to/repo init --git-ref HEAD
```

**输出：**

```
  Initializing layertwine repository ... done
Initialized empty layertwine repository at '.layertwine/layertwine.db'
  Manual partition: <uuid>
  Staged partition: <uuid>
  Branch: main
```

---

### `layertwine status` — 查看当前状态

```
layertwine status
```

显示所有层的分区状态，包括当前快照 ID 和历史记录数量。

**输出示例（Plain 模式）：**

```
------------------------------------------------------------------------
Layer            Partition                Current Snapshot    History
------------------------------------------------------------------------
manual_edit      manual                   a1b2c3d4e5f6       3 snapshots
staged           staged                   f6e5d4c3b2a1       5 snapshots
agent_edit       agent:agent-01           b2c3d4e5f6a1       2 snapshots
approval         approved:agent-01        c3d4e5f6a1b2       1 snapshots
------------------------------------------------------------------------
```

**输出示例（JSON 模式，`--json`）：**

```json
{
  "status": "ok",
  "partitions": [
    {
      "layer": "manual_edit",
      "partition": "manual",
      "current_snapshot": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4",
      "history_len": 3
    }
  ]
}
```

---

### `layertwine edit` — 手动编辑文件

```
layertwine edit <FILE> [-c, --content <CONTENT>]
```

对指定文件进行手动编辑，记录到 `manual_edit` 层。编辑后自动合并到 `staged` 层。

**参数：**

| 参数                      | 说明                             |
| ------------------------- | -------------------------------- |
| `<FILE>`                  | 要编辑的文件路径（必需）         |
| `-c, --content <CONTENT>` | 新文件内容。若未提供，从 stdin 读取 |

**示例：**

```bash
layertwine edit src/main.rs -c "fn main() {}"
```

**输出：**

```
Edited 'src/main.rs' -> new snapshot a1b2c3d4e5f6
Merged to staged -> snapshot f6e5d4c3b2a1
```

---

### `layertwine agent` — Agent 操作

```
layertwine agent <AGENT_ID> edit <FILE> [-c, --content <CONTENT>]
layertwine agent <AGENT_ID> submit
```

管理 Agent 的编辑和提交流程。

**子命令：**

| 子命令                       | 说明                                                 |
| ---------------------------- | ---------------------------------------------------- |
| `edit <FILE> [-c <CONTENT>]` | Agent 编辑指定文件，记录到 `agent_edit` 层           |
| `submit`                     | Agent 提交当前编辑内容到 `approval` 层，等待人工审核 |

**参数：**

| 参数                      | 说明                                |
| ------------------------- | ----------------------------------- |
| `<AGENT_ID>`              | Agent 实例 ID（必需）               |
| `<FILE>`                  | 要编辑的文件路径（edit 子命令必需） |
| `-c, --content <CONTENT>` | 新文件内容（edit 子命令可选）       |

**示例：**

```bash
# Agent 编辑文件
layertwine agent agent-01 edit src/module.rs -c "pub fn new() -> Self { Self }"

# Agent 提交审核
layertwine agent agent-01 submit
```

**输出：**

```
Agent 'agent-01' edited 'src/module.rs' -> snapshot b2c3d4e5f6a1
Agent 'agent-01' submitted for approval -> snapshot c3d4e5f6a1b2
```

---

### `layertwine approval` — 审批管理

```
layertwine approval list
layertwine approval approve <AGENT_ID> [--integrated-name <NAME>]
layertwine approval reject <AGENT_ID>
layertwine approval merge-to-unified [--names <NAMES>...]
layertwine approval merge-to-staged
```

细粒度的审批操作管理，替代旧版 `approve <AGENT_ID>` 单步命令。

**子命令：**

| 子命令                                          | 说明                                               |
| ----------------------------------------------- | -------------------------------------------------- |
| `list`                                          | 列出所有待审批的 Agent 提交                         |
| `approve <AGENT_ID> [--integrated-name <NAME>]` | 审批通过指定 Agent，迁移到 integrated 分区         |
| `reject <AGENT_ID>`                             | 拒绝指定 Agent 的提交，回滚到基线                   |
| `merge-to-unified [--names <NAMES>...]`         | 将已审批的 integrated 分区合并到 unified 分区      |
| `merge-to-staged`                               | 将 unified 分区合并到 staged 层                     |

**示例：**

```bash
# 列出待审批项
layertwine approval list

# 审批 Agent
layertwine approval approve agent-01

# 拒绝 Agent
layertwine approval reject agent-01

# 合并到 unified
layertwine approval merge-to-unified

# 合并到 staged
layertwine approval merge-to-staged
```

---

### `layertwine commit` — 提交检查点

```
layertwine commit -m <MESSAGE> [-a <AUTHOR>]
```

将 `staged` 层的当前状态提交为一个检查点（Checkpoint），记录到自研检查点仓库。

**参数：**

| 参数                      | 说明                          |
| ------------------------- | ----------------------------- |
| `-m, --message <MESSAGE>` | 提交信息（必需）              |
| `-a, --author <AUTHOR>`   | 作者名称（可选，默认 `user`） |

**示例：**

```bash
layertwine commit -m "实现用户登录模块" -a "developer-1"
```

**输出：**

```
Committed checkpoint a1b2c3d4e5f6: 实现用户登录模块
```

---

### `layertwine log` — 查看提交历史

```
layertwine log [--count <N>]
```

查看检查点提交历史，支持表格和 JSON 输出。

**参数：**

| 参数          | 说明                          |
| ------------- | ----------------------------- |
| `--count <N>` | 最大显示数量（可选，默认 20） |

**输出示例（Plain 模式）：**

```
----------------------------------------------------------------------------------------------------
Checkpoint ID         Author           Parents      Snapshots     Message
----------------------------------------------------------------------------------------------------
a1b2c3d4e5f6         developer-1      1            3             实现用户登录模块
b2c3d4e5f6a1         developer-1      1            2             修复边界条件
c3d4e5f6a1b2 [git]   developer-1      1            2             从 Git 同步 [git]
----------------------------------------------------------------------------------------------------
Total: 3 checkpoint(s)
```

---

### `layertwine show` — 查看差异

```
layertwine show <SHOW_WHAT> [--id <ID>]
```

查看 staged / checkpoint / partition 与基准之间的 unified diff。

**参数：**

| 参数              | 说明                                                      |
| ----------------- | --------------------------------------------------------- |
| `<SHOW_WHAT>`     | 目标类型：`staged` / `checkpoint` / `partition`           |
| `--id, -i <ID>`   | 目标 ID：checkpoint 时必传 checkpoint ID，partition 时必传分区名 |

**示例：**

```bash
# 查看 staged 差异
layertwine show staged

# 查看指定 checkpoint 差异
layertwine show checkpoint --id a1b2c3d4e5f6

# 查看指定 partition 差异
layertwine show partition --id manual
```

---

### `layertwine branch` — 分支管理

```
layertwine branch create <NAME>
layertwine branch switch <NAME>
layertwine branch list
```

**子命令：**

| 子命令          | 说明                             |
| --------------- | -------------------------------- |
| `create <NAME>` | 创建新分支（基于当前最新检查点） |
| `switch <NAME>` | 切换到指定分支                   |
| `list`          | 列出所有分支                     |

**示例：**

```bash
# 创建新分支
layertwine branch create feature/login

# 切换分支
layertwine branch switch feature/login

# 列出所有分支
layertwine branch list
```

**输出（list）：**

```
------------------------------------------------------------
Branch                   Head                 Updated
------------------------------------------------------------
* main                   a1b2c3d4e5f6         2026-05-22 10:00:00
  feature/login          b2c3d4e5f6a1         2026-05-22 10:30:00
------------------------------------------------------------
```

---

### `layertwine merge` — 合并分支

```
layertwine merge <BRANCH> [-m, --message <MESSAGE>]
```

将指定分支合并到当前分支。

**参数：**

| 参数                      | 说明                               |
| ------------------------- | ---------------------------------- |
| `<BRANCH>`                | 源分支名称（必需）                 |
| `-m, --message <MESSAGE>` | 合并提交信息（可选，默认 `merge`） |

**示例：**

```bash
layertwine merge feature/login -m "合并登录功能到主分支"
```

**输出：**

```
Merged 'feature/login' into 'main' -> checkpoint d4e5f6a1b2c3
```

---

### `layertwine checkpoint` — 检查点操作

```
layertwine checkpoint restore <CHECKPOINT_ID> [--source-filter <PATTERN>...]
layertwine checkpoint restore-by-time <TARGET_TIME> [--source-filter <PATTERN>...]
layertwine checkpoint diff <FROM_ID> <TO_ID>
layertwine checkpoint rollback <CHECKPOINT_ID>
```

对检查点执行恢复、差异对比和回滚操作。

**子命令：**

| 子命令                                          | 说明                                                       |
| ----------------------------------------------- | ---------------------------------------------------------- |
| `restore <ID> [--source-filter <PATTERN>...]`   | 从指定检查点恢复文件到工作目录                             |
| `restore-by-time <TIME> [--source-filter ...]`  | 恢复到距离目标时间最近的检查点                             |
| `diff <FROM_ID> <TO_ID>`                        | 对比两个检查点之间的差异                                   |
| `rollback <ID>`                                 | 将 staged 分区回滚到指定检查点                             |

**示例：**

```bash
# 从检查点恢复
layertwine checkpoint restore a1b2c3d4e5f6

# 按时间恢复
layertwine checkpoint restore-by-time 1684396800000

# 对比检查点
layertwine checkpoint diff a1b2c3d4e5f6 b2c3d4e5f6a1

# 回滚到检查点
layertwine checkpoint rollback a1b2c3d4e5f6
```

---

### `layertwine backup` — 备份快照

```
layertwine backup <SNAPSHOT_ID> [--label <LABEL>]
```

将指定快照备份到独立的备份仓库（默认写入 `layertwine-backup.db`）。

**参数：**

| 参数              | 说明                              |
| ----------------- | --------------------------------- |
| `<SNAPSHOT_ID>`   | 要备份的快照 ID（十六进制，必需） |
| `--label <LABEL>` | 备份标签（可选）                  |

**示例：**

```bash
layertwine backup a1b2c3d4e5f6 --label "发布前基线"
```

**输出：**

```
Backup f6e5d4c3b2a1 created for snapshot a1b2c3d4e5f6 (label: 发布前基线)
```

---

### `layertwine restore` — 从备份恢复

```
layertwine restore <BACKUP_ID>
```

从备份仓库恢复指定备份的快照数据到主存储。

**参数：**

| 参数          | 说明                      |
| ------------- | ------------------------- |
| `<BACKUP_ID>` | 备份 ID（十六进制，必需） |

**示例：**

```bash
layertwine restore f6e5d4c3b2a1
```

**输出：**

```
Restored backup f6e5d4c3b2a1 -> file: src/main.rs
```

---

### `layertwine gc` — 执行垃圾回收

```
layertwine gc
```

清理冗余检查点和快照数据。当 Delta 链深度超过阈值时触发深度压缩。

**示例：**

```bash
layertwine gc
```

**输出：**

```
  Running garbage collection ... done
GC complete: 5 checkpoints removed, 12 snapshots freed, 102400 bytes
  Note: delta chain depth exceeded threshold (15)
```

---

### `layertwine compact` — 数据库压缩

```
layertwine compact [--vacuum-full]
```

压缩数据库——截断 WAL 并回收空闲页。

**参数：**

| 参数                | 说明                                               |
| ------------------- | -------------------------------------------------- |
| `--vacuum-full`     | 强制执行完整 VACUUM 而非增量模式（需要排它锁）     |

**示例：**

```bash
layertwine compact
```

**输出：**

```
  Compacting database ... done
Database compacted: WAL checkpointed, 100 free pages reclaimed
```

---

### `layertwine git-commit` — 提交到本地 Git 分支

```
layertwine git-commit [-m, --message <MESSAGE>]
```

将当前分支的检查点历史提交到关联的 Git 仓库的本地分支。**不执行远程推送**，需要手动执行 `git push` 将变更推送到远程。

需要预先通过 `--git-repo` 指定 Git 仓库路径。

**参数：**

| 参数                      | 说明                                           |
| ------------------------- | ---------------------------------------------- |
| `-m, --message <MESSAGE>` | Git 提交信息（可选，默认 `sync from layertwine`） |

**示例：**

```bash
layertwine --git-repo /path/to/repo git-commit -m "同步检查点到 Git"
```

**输出：**

```
  Committing to local Git branch ... done
Committed to local Git branch (commit: 9f8e7d6c5b4a)
  Run `git push` manually to push to remote.
```

---

### `layertwine pull` — 从 Git 拉取

```
layertwine pull [--remote <REMOTE>] [--git-ref <REF>]
```

从关联的 Git 仓库拉取并导入最新提交作为 Layertwine 检查点。需要预先通过 `--git-repo` 指定 Git 仓库路径。

**参数：**

| 参数                | 说明                                |
| ------------------- | ----------------------------------- |
| `--remote <REMOTE>` | Git 远程名称（可选，默认 `origin`） |
| `--git-ref <REF>`   | Git 引用（可选，默认 `HEAD`）       |

**示例：**

```bash
layertwine --git-repo /path/to/repo pull --remote origin --git-ref main
```

**输出：**

```
  Pulling from Git remote ... done
Pulled from remote 'origin' ref 'main'
```

---

### `layertwine clean` — 清理 Layertwine 存储

```
layertwine clean --branch <NAME>
layertwine clean --layer <TYPE>
layertwine clean --all
```

清理 Layertwine 存储中的数据（不碰 Git 仓库和本地文件）。

**参数：**

| 参数                | 说明                                                       |
| ------------------- | ---------------------------------------------------------- |
| `--branch <NAME>`   | 清理指定分支的所有检查点及相关数据                         |
| `--layer <TYPE>`    | 清理指定层的数据（如 `staged`、`unified`、`integrated`）  |
| `--all`             | 清理所有 Layertwine 数据，重置为初始状态                   |

**示例：**

```bash
# 清理指定分支
layertwine clean --branch feature/login

# 清理指定层
layertwine clean --layer staged

# 全部清理
layertwine clean --all
```

**输出：**

```
  Cleaning layertwine storage ... done
Clean: 1 branch removed, 3 checkpoints, 5 snapshots, 12 deltas, 1 layer
  All orphaned snapshots and deltas cleaned up.
```

---

## 典型工作流程

### 单人编辑流程

```bash
# 1. 初始化仓库
layertwine init

# 2. 编辑文件
layertwine edit src/main.rs -c "fn main() { println!(\"Hello\"); }"

# 3. 提交检查点
layertwine commit -m "初始提交"

# 4. 查看状态和历史
layertwine status
layertwine log
```

### 多 Agent 协同流程

```bash
# 1. 初始化
layertwine init

# 2. Agent A 编辑并提交审核
layertwine agent agent-a edit src/auth.rs -c "pub fn login() {}"
layertwine agent agent-a submit

# 3. Agent B 编辑并提交审核
layertwine agent agent-b edit src/db.rs -c "pub fn connect() {}"
layertwine agent agent-b submit

# 4. 查看待审批列表
layertwine approval list

# 5. 审批通过
layertwine approval approve agent-a
layertwine approval approve agent-b

# 6. 合并到 unified
layertwine approval merge-to-unified

# 7. 合并到 staged
layertwine approval merge-to-staged

# 8. 提交最终检查点
layertwine commit -m "合并 auth 和 db 模块"
```

### Git 集成流程

```bash
# 1. 从已有 Git 仓库初始化
layertwine --git-repo /path/to/repo init --git-ref main

# 2. 在 Layertwine 中进行编辑
layertwine edit src/lib.rs -c "// new code"
layertwine commit -m "Layertwine 编辑"

# 3. 将检查点提交到本地 Git 分支
layertwine --git-repo /path/to/repo git-commit

# 4. 手动推送到远程
cd /path/to/repo && git push

# 5. 拉取 Git 更新到 Layertwine
layertwine --git-repo /path/to/repo pull
```

---

## 架构对应关系

每个 CLI 命令对应的内部模块：

| CLI 命令                      | 所属模块                                 | 对应架构层        |
| ----------------------------- | ---------------------------------------- | ----------------- |
| `init`                        | `state_machine` + `storage`              | P1 存储层         |
| `status`                      | `cli::output`                            | P3 状态机查询     |
| `edit`                        | `state_machine::manual`                  | P3 manual_edit 层 |
| `agent edit/submit`           | `state_machine::agent`                   | P3 agent_edit 层  |
| `approval list/approve/reject`| `state_machine::approval`                | P3 approval 层    |
| `approval merge-to-unified`   | `state_machine::integrated` + `unified`  | P3 多层流水线     |
| `approval merge-to-staged`    | `state_machine::unified` + `staged`      | P3 多层流水线     |
| `commit`                      | `state_machine::staged` + `checkpoint`   | P4 检查点仓库     |
| `log`                         | `checkpoint` + `storage`                 | P4 历史查询       |
| `show`                        | `api::service` + `engine::diff`          | P2/P4 差异查看    |
| `branch`                      | `checkpoint::branch` + `checkpoint::dag` | P4 分支管理       |
| `merge`                       | `checkpoint::repo` + `engine::merge`     | P4/P2 合并引擎    |
| `checkpoint restore/diff`     | `checkpoint::repo` + `engine`            | P4/P2 检查点操作  |
| `checkpoint rollback`         | `state_machine::staged` + `checkpoint`   | P3/P4 回滚        |
| `backup`                      | `backup`                                 | P5 备份模块       |
| `restore`                     | `backup`                                 | P5 恢复模块       |
| `gc`                          | `git_sync::gc`                           | P6 垃圾回收       |
| `compact`                     | `storage::sqlite`                        | P1 数据库维护     |
| `git-commit`                  | `git_sync::git_bridge`                   | P6 Git 同步       |
| `pull`                        | `git_sync::git_bridge`                   | P6 Git 同步       |
| `clean`                       | `storage::sqlite` + `git_sync::gc`       | P1/P6 数据清理    |

---

## 错误码说明

| 退出码 | 含义     | 常见原因                               |
| ------ | -------- | -------------------------------------- |
| 0      | 成功     | —                                      |
| 1      | 一般错误 | 数据库损坏、模块内部错误、实体未找到   |
| 2      | 参数错误 | 缺少必选参数、参数格式错误、分支已存在 |

所有错误信息均包含上下文描述和修复建议（hint）。