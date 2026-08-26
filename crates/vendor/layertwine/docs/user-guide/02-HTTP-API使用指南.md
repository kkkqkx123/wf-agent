# Layertwine HTTP API 使用指南

> Layertwine 提供基于 REST/JSON 的 HTTP API，通过 Axum 实现，适用于多 Agent 协同编辑场景的远程调用。

## 启动服务

### 环境变量

| 变量               | 默认值               | 说明                           |
| ------------------ | -------------------- | ------------------------------ |
| `LAYERTWINE_MODE`     | `cli`                | 运行时模式，设为 `http` 启动   |
| `LAYERTWINE_DB_PATH`  | `.layertwine/layertwine.db` | SQLite 数据库文件路径          |
| `LAYERTWINE_HTTP_ADDR`| `127.0.0.1:8080`     | HTTP 服务器绑定地址与端口       |
| `LAYERTWINE_GIT_REPO` | 无                   | Git 仓库路径（用于 Git 同步操作） |

### 启动命令

```bash
# 编译时需启用 http feature
cargo run --features http

# 以 HTTP 模式运行
LAYERTWINE_MODE=http LAYERTWINE_DB_PATH=/path/to/layertwine.db cargo run --features http

# 自定义绑定地址
LAYERTWINE_MODE=http LAYERTWINE_HTTP_ADDR=0.0.0.0:9090 cargo run --features http
```

---

## 通用约定

### Base URL

```
http://127.0.0.1:8080
```

### 请求体限制

所有 POST 请求体最大 10 MB。

### 请求格式

- **GET** 请求：查询参数通过 URL query string 传递
- **POST** 请求：请求体为 JSON，`Content-Type: application/json`

### 响应格式

所有接口统一使用 `ApiEnvelope` 包裹：

```json
{
  "success": true,
  "data": { /* 具体响应数据 */ },
  "error": null
}
```

失败时：

```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "NOT_FOUND",
    "message": "branch 'foo' not found",
    "suggestion": "check that the name or ID is correct",
    "details": null
  }
}
```

### 错误码

| 错误码              | HTTP 状态码 | 说明           |
| ------------------- | ----------- | -------------- |
| `NOT_FOUND`         | 404         | 实体未找到     |
| `INVALID_PARAMS`    | 400         | 参数无效       |
| `ALREADY_EXISTS`    | 409         | 实体已存在     |
| `STORAGE_ERROR`     | 500         | 存储层错误     |
| `ENGINE_ERROR`      | 500         | 引擎层错误     |
| `STATE_MACHINE_ERROR`| 500        | 状态机错误     |
| `CHECKPOINT_ERROR`  | 500         | 检查点错误     |
| `GIT_SYNC_ERROR`    | 500         | Git 同步错误   |
| `GC_ERROR`          | 500         | 垃圾回收错误   |
| `INTERNAL_ERROR`    | 500         | 内部错误       |
| `ERROR`             | 500         | 一般性错误     |
| `CLI_ERROR`         | 500         | CLI 上下文错误 |

---

## API 端点

### 1. 初始化仓库 — `POST /api/v1/init`

初始化新的 Layertwine 仓库，可选从 Git 仓库导入基线。

**请求体：**

```json
{
  "db_path": ".layertwine/layertwine.db",
  "git_repo": null,
  "git_ref": null
}
```

| 字段       | 类型           | 说明                                      |
| ---------- | -------------- | ----------------------------------------- |
| `db_path`  | string (可选)  | 数据库路径，默认 `.layertwine/layertwine.db`    |
| `git_repo` | string (可选)  | Git 仓库路径，指定后从 Git 初始化         |
| `git_ref`  | string (可选)  | Git 引用（如 `HEAD`、分支名），配合 `git_repo` 使用 |

**响应：**

```json
{
  "success": true,
  "data": {
    "db_path": ".layertwine/layertwine.db",
    "manual_partition_id": "<uuid>",
    "staged_partition_id": "<uuid>",
    "branch": "main"
  }
}
```

**cURL 示例：**

```bash
# 初始化空仓库
curl -X POST http://127.0.0.1:8080/api/v1/init \
  -H 'Content-Type: application/json' \
  -d '{}'

# 从 Git 仓库初始化
curl -X POST http://127.0.0.1:8080/api/v1/init \
  -H 'Content-Type: application/json' \
  -d '{"git_repo": "/path/to/repo", "git_ref": "HEAD"}'
```

---

### 2. 查看状态 — `GET /api/v1/status`

查看所有层的分区当前状态，包括快照 ID 和历史记录数。

**请求：** 无参数。

**响应：**

```json
{
  "success": true,
  "data": {
    "partitions": [
      {
        "layer": "manual_edit",
        "name": "manual",
        "current_snapshot": "a1b2c3d4e5f6...",
        "history_len": 3
      },
      {
        "layer": "staged",
        "name": "staged",
        "current_snapshot": "f6e5d4c3b2a1...",
        "history_len": 5
      }
    ]
  }
}
```

| 字段               | 类型   | 说明                                        |
| ------------------ | ------ | ------------------------------------------- |
| `layer`            | string | 层标识：`manual_edit` / `agent_edit` / `staged` |
| `name`             | string | 分区名称                                    |
| `current_snapshot` | string | 当前快照 ID（十六进制）                     |
| `history_len`      | number | 历史 Delta 数量                             |

**cURL 示例：**

```bash
curl http://127.0.0.1:8080/api/v1/status
```

---

### 3. 手动编辑 — `POST /api/v1/edit`

对指定文件进行手动编辑，记录到 `manual_edit` 层并自动合并到 `staged` 层。

**请求体：**

```json
{
  "file": "src/main.rs",
  "content": "fn main() {\n    println!(\"Hello\");\n}\n"
}
```

| 字段      | 类型           | 说明                             |
| --------- | -------------- | -------------------------------- |
| `file`    | string (必需)  | 文件路径                         |
| `content` | string (可选)  | 新文件内容，未提供或为 null 时报错 |

**响应：**

```json
{
  "success": true,
  "data": {
    "snapshot_id": "a1b2c3d4e5f6...",
    "staged_snapshot_id": "f6e5d4c3b2a1..."
  }
}
```

| 字段                  | 类型             | 说明                                  |
| --------------------- | ---------------- | ------------------------------------- |
| `snapshot_id`         | string           | 本次编辑生成的快照 ID                 |
| `staged_snapshot_id`  | string 或 null   | 自动合并到 staged 层后的快照 ID       |

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/edit \
  -H 'Content-Type: application/json' \
  -d '{"file": "src/main.rs", "content": "fn main() {}"}'
```

---

### 4. Agent 编辑 — `POST /api/v1/agent/{id}/edit`

Agent 对指定文件进行编辑，记录到 `agent_edit` 层。

**路径参数：**

| 参数 | 类型   | 说明            |
| ---- | ------ | --------------- |
| `id` | string | Agent 实例 ID   |

**请求体：** 与 `/api/v1/edit` 相同。

**响应：** 与 `/api/v1/edit` 相同（`staged_snapshot_id` 为 null）。

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/agent/agent-01/edit \
  -H 'Content-Type: application/json' \
  -d '{"file": "src/module.rs", "content": "pub fn new() -> Self { Self }"}'
```

---

### 5. Agent 提交审核 — `POST /api/v1/agent/{id}/submit`

Agent 将当前编辑提交到 `approval` 层，等待人工审核。

**路径参数：**

| 参数 | 类型   | 说明            |
| ---- | ------ | --------------- |
| `id` | string | Agent 实例 ID   |

**请求：** 无请求体。

**响应：**

```json
{
  "success": true,
  "data": {
    "snapshot_id": "c3d4e5f6a1b2..."
  }
}
```

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/agent/agent-01/submit
```

---

### 6. 审批管理端点

Layertwine 提供一组细粒度审批管理端点，替代旧版单步 `POST /api/v1/approve/{agent_id}`。

#### 6a. 待审批列表 — `GET /api/v1/approvals`

列出所有待审批的 Agent 提交。

**请求：** 无参数。

**响应：**

```json
{
  "success": true,
  "data": {
    "approvals": [
      {
        "agent_id": "agent-01",
        "partition_name": "agent:agent-01",
        "current_snapshot": "c3d4e5f6a1b2...",
        "history_len": 2
      }
    ],
    "total": 1
  }
}
```

**cURL 示例：**

```bash
curl http://127.0.0.1:8080/api/v1/approvals
```

#### 6b. 审批通过 — `POST /api/v1/approve-agent`

审批通过指定 Agent 的提交，迁移到 integrated 分区。

**请求体：**

```json
{
  "agent_id": "agent-01",
  "integrated_name": null
}
```

| 字段              | 类型           | 说明                               |
| ----------------- | -------------- | ---------------------------------- |
| `agent_id`        | string (必需)  | Agent 实例 ID                      |
| `integrated_name` | string (可选)  | integrated 分区名称，默认自动生成  |

**响应：**

```json
{
  "success": true,
  "data": {
    "agent_id": "agent-01",
    "integrated_snapshot_id": "d4e5f6a1b2c3..."
  }
}
```

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/approve-agent \
  -H 'Content-Type: application/json' \
  -d '{"agent_id": "agent-01"}'
```

#### 6c. 拒绝提交 — `POST /api/v1/reject-agent`

拒绝指定 Agent 的提交，回滚到基线。

**请求体：**

```json
{
  "agent_id": "agent-01"
}
```

**响应：**

```json
{
  "success": true,
  "data": {
    "agent_id": "agent-01",
    "baseline_snapshot_id": "a1b2c3d4e5f6..."
  }
}
```

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/reject-agent \
  -H 'Content-Type: application/json' \
  -d '{"agent_id": "agent-01"}'
```

#### 6d. 合并到 Unified — `POST /api/v1/merge-to-unified`

将已审批的 integrated 分区合并到 unified 分区。

**请求体：**

```json
{
  "integration_names": ["agent-01", "agent-02"]
}
```

| 字段                | 类型             | 说明                                            |
| ------------------- | ---------------- | ----------------------------------------------- |
| `integration_names` | string[] (可选)  | 指定要合并的 integration 名称，为空时自动检测   |

**响应：**

```json
{
  "success": true,
  "data": {
    "unified_snapshot_id": "e5f6a1b2c3d4...",
    "merged_count": 2
  }
}
```

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/merge-to-unified \
  -H 'Content-Type: application/json' \
  -d '{"integration_names": ["agent-01"]}'
```

#### 6e. 合并到 Staged — `POST /api/v1/merge-to-staged`

将 unified 分区合并到 staged 层。

**请求：** 无请求体。

**响应：**

```json
{
  "success": true,
  "data": {
    "staged_snapshot_id": "f6e5d4c3b2a1..."
  }
}
```

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/merge-to-staged
```

---

### 7. 提交检查点 — `POST /api/v1/commit`

将 `staged` 层的当前快照提交为检查点（Checkpoint）。

**请求体：**

```json
{
  "message": "实现用户登录模块",
  "author": "developer-1"
}
```

| 字段      | 类型           | 说明                           |
| --------- | -------------- | ------------------------------ |
| `message` | string (必需)  | 提交信息                       |
| `author`  | string (可选)  | 作者名称，默认 `user`          |

**响应：**

```json
{
  "success": true,
  "data": {
    "checkpoint_id": "a1b2c3d4e5f6...",
    "message": "实现用户登录模块"
  }
}
```

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/commit \
  -H 'Content-Type: application/json' \
  -d '{"message": "实现用户登录模块", "author": "developer-1"}'
```

---

### 8. 查看提交历史 — `GET /api/v1/log`

查看当前分支的检查点历史。

**查询参数：**

| 参数    | 类型   | 说明                       |
| ------- | ------ | -------------------------- |
| `count` | number | 最大返回数量，默认 20      |

**响应：**

```json
{
  "success": true,
  "data": {
    "checkpoints": [
      {
        "id": "a1b2c3d4e5f6...",
        "author": "developer-1",
        "message": "实现用户登录模块",
        "parents": ["f6e5d4c3b2a1..."],
        "snapshots": ["b2c3d4e5f6a1..."],
        "created_at": 1684396800,
        "git_anchor": null
      }
    ],
    "total": 1
  }
}
```

| 字段          | 类型             | 说明                                  |
| ------------- | ---------------- | ------------------------------------- |
| `id`          | string           | 检查点 ID（十六进制）                 |
| `author`      | string           | 作者                                  |
| `message`     | string           | 提交信息                              |
| `parents`     | string[]         | 父检查点 ID 列表                      |
| `snapshots`   | string[]         | 基线快照 ID 列表                      |
| `created_at`  | number           | Unix 时间戳                           |
| `git_anchor`  | string/null      | Git 锚点（从 Git 同步时存在）          |

**cURL 示例：**

```bash
curl "http://127.0.0.1:8080/api/v1/log?count=10"
```

---

### 9. 分支列表 — `GET /api/v1/branches`

列出所有分支。

**响应：**

```json
{
  "success": true,
  "data": {
    "branches": [
      {
        "name": "main",
        "head": "a1b2c3d4e5f6...",
        "updated_at": "2026-05-22 10:00:00 UTC",
        "is_current": false
      }
    ],
    "current": null
  }
}
```

**cURL 示例：**

```bash
curl http://127.0.0.1:8080/api/v1/branches
```

---

### 10. 创建分支 — `POST /api/v1/branches`

基于当前分支的最新检查点创建新分支。

**请求体：**

```json
{
  "name": "feature/login"
}
```

**响应：**

```json
{
  "success": true,
  "data": {
    "name": "feature/login",
    "head": "a1b2c3d4e5f6..."
  }
}
```

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/branches \
  -H 'Content-Type: application/json' \
  -d '{"name": "feature/login"}'
```

---

### 11. 切换分支 — `POST /api/v1/branches/{name}/switch`

切换到指定分支，同时重置 staged 分区。

**路径参数：**

| 参数   | 类型   | 说明           |
| ------ | ------ | -------------- |
| `name` | string | 目标分支名称   |

**请求：** 无请求体。

**响应：**

```json
{
  "success": true,
  "data": {
    "name": "feature/login",
    "checkpoint_id": "b2c3d4e5f6a1..."
  }
}
```

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/branches/feature%2Flogin/switch
```

---

### 12. 合并分支 — `POST /api/v1/merge`

将指定源分支合并到当前分支。

**请求体：**

```json
{
  "branch": "feature/login",
  "message": "合并登录功能到主分支"
}
```

| 字段      | 类型           | 说明                               |
| --------- | -------------- | ---------------------------------- |
| `branch`  | string (必需)  | 源分支名称                         |
| `message` | string (可选)  | 合并信息，默认 `"merge"`           |

**响应：**

```json
{
  "success": true,
  "data": {
    "checkpoint_id": "d4e5f6a1b2c3...",
    "source_branch": "feature/login",
    "target_branch": "main"
  }
}
```

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/merge \
  -H 'Content-Type: application/json' \
  -d '{"branch": "feature/login", "message": "合并登录功能"}'
```

---

### 13. 备份快照 — `POST /api/v1/backup`

将指定快照备份到独立存储（默认写入 `layertwine-backup.db`）。

**请求体：**

```json
{
  "snapshot_id": "a1b2c3d4e5f6...",
  "label": "发布前基线"
}
```

| 字段          | 类型           | 说明                          |
| ------------- | -------------- | ----------------------------- |
| `snapshot_id` | string (必需)  | 要备份的快照 ID（十六进制）   |
| `label`       | string (可选)  | 备份标签                      |

**响应：**

```json
{
  "success": true,
  "data": {
    "backup_id": "f6e5d4c3b2a1...",
    "source_snapshot_id": "a1b2c3d4e5f6...",
    "label": "发布前基线"
  }
}
```

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/backup \
  -H 'Content-Type: application/json' \
  -d '{"snapshot_id": "a1b2c3d4e5f6", "label": "发布前基线"}'
```

---

### 14. 从备份恢复 — `POST /api/v1/restore`

从备份仓库恢复指定备份的快照数据到主存储。

**请求体：**

```json
{
  "backup_id": "f6e5d4c3b2a1..."
}
```

| 字段        | 类型           | 说明                        |
| ----------- | -------------- | --------------------------- |
| `backup_id` | string (必需)  | 备份 ID（十六进制）         |

**响应：**

```json
{
  "success": true,
  "data": {
    "backup_id": "f6e5d4c3b2a1...",
    "file": "src/main.rs",
    "deltas_restored": 5
  }
}
```

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/restore \
  -H 'Content-Type: application/json' \
  -d '{"backup_id": "f6e5d4c3b2a1"}'
```

---

### 15. 垃圾回收 — `POST /api/v1/gc`

清理冗余的检查点和快照数据。当 Delta 链深度超过阈值时触发深度压缩。

**请求：** 无请求体。

**响应：**

```json
{
  "success": true,
  "data": {
    "removed_checkpoints": 5,
    "removed_snapshots": 12,
    "freed_bytes": 102400,
    "delta_chain_depth_triggered": true
  }
}
```

| 字段                          | 类型    | 说明                               |
| ----------------------------- | ------- | ---------------------------------- |
| `removed_checkpoints`         | number  | 移除的检查点数量                   |
| `removed_snapshots`           | number  | 释放的快照数量                     |
| `freed_bytes`                 | number  | 释放的字节数                       |
| `delta_chain_depth_triggered` | boolean | 是否触发了 Delta 链深度压缩        |

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/gc
```

---

### 16. 数据库压缩 — `POST /api/v1/compact`

压缩数据库——截断 WAL 并回收空闲页。

**请求体：**

```json
{
  "vacuum_full": false
}
```

| 字段          | 类型          | 说明                                              |
| ------------- | ------------- | ------------------------------------------------- |
| `vacuum_full` | boolean (可选)| 是否强制执行完整 VACUUM（需要排它锁），默认 false |

**响应：**

```json
{
  "success": true,
  "data": {
    "wal_checkpointed": true,
    "freelist_before": 200,
    "total_pages": 1000,
    "freelist_after": 50,
    "vacuum_performed": false,
    "message": "Database compacted: WAL checkpointed, 150 free pages reclaimed"
  }
}
```

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/compact \
  -H 'Content-Type: application/json' \
  -d '{"vacuum_full": false}'
```

---

### 17. 提交到本地 Git 分支 — `POST /api/v1/git-commit`

将当前分支的检查点历史提交到关联的 Git 仓库的本地分支。**不执行远程推送**。

**请求体：**

```json
{
  "git_repo": "/path/to/repo",
  "message": "sync from layertwine"
}
```

| 字段      | 类型           | 说明                                          |
| --------- | -------------- | --------------------------------------------- |
| `git_repo`| string (必需)  | Git 仓库路径                                  |
| `message` | string (可选)  | Git 提交信息，默认 `"sync from layertwine"`      |

**响应：**

```json
{
  "success": true,
  "data": {
    "git_commit_hash": "9f8e7d6c5b4a..."
  }
}
```

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/git-commit \
  -H 'Content-Type: application/json' \
  -d '{"git_repo": "/path/to/repo", "message": "从 Layertwine 同步"}'
```

---

### 18. 从 Git 拉取 — `POST /api/v1/pull`

从 Git 远程仓库拉取并导入最新提交为 Layertwine 检查点。

**请求体：**

```json
{
  "remote": "origin",
  "git_repo": "/path/to/repo",
  "git_ref": "HEAD"
}
```

| 字段      | 类型           | 说明                                |
| --------- | -------------- | ----------------------------------- |
| `remote`  | string (可选)  | Git 远程名称，默认 `origin`         |
| `git_repo`| string (必需)  | Git 仓库路径                        |
| `git_ref` | string (可选)  | Git 引用，默认 `HEAD`               |

**响应：**

```json
{
  "success": true,
  "data": {
    "remote": "origin",
    "git_ref": "main"
  }
}
```

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/pull \
  -H 'Content-Type: application/json' \
  -d '{"remote": "origin", "git_repo": "/path/to/repo", "git_ref": "main"}'
```

---

### 19. 清理存储 — `POST /api/v1/clean`

清理 Layertwine 存储中的数据（不碰 Git 仓库和本地文件）。

**请求体：**

```json
{
  "all": false,
  "branch": null,
  "layer": "staged"
}
```

| 字段     | 类型           | 说明                                                       |
| -------- | -------------- | ---------------------------------------------------------- |
| `all`    | boolean (可选) | 清理所有 Layertwine 数据，重置为初始状态                   |
| `branch` | string (可选)  | 清理指定分支的所有检查点及相关数据                         |
| `layer`  | string (可选)  | 清理指定层的数据（如 `staged`、`unified`、`integrated`）  |

**响应：**

```json
{
  "success": true,
  "data": {
    "removed_branches": 1,
    "removed_checkpoints": 3,
    "removed_snapshots": 5,
    "removed_deltas": 12,
    "removed_layers": 1,
    "message": "Clean: 1 branch removed, 3 checkpoints, 5 snapshots, 12 deltas, 1 layer\n  All orphaned snapshots and deltas cleaned up."
  }
}
```

**cURL 示例：**

```bash
# 清理指定层
curl -X POST http://127.0.0.1:8080/api/v1/clean \
  -H 'Content-Type: application/json' \
  -d '{"layer": "staged"}'

# 全部清理
curl -X POST http://127.0.0.1:8080/api/v1/clean \
  -H 'Content-Type: application/json' \
  -d '{"all": true}'
```

---

### 20. 显示差异 — `GET /api/v1/show`

查看 staged / checkpoint / partition 与基准之间的 unified diff。

**查询参数：**

| 参数         | 类型   | 说明                                                      |
| ------------ | ------ | --------------------------------------------------------- |
| `show_what`  | string | 目标类型：`staged` / `checkpoint` / `partition`           |
| `target_id`  | string | 目标 ID：checkpoint 时必传 checkpoint ID，partition 时必传分区名 |

**响应：**

```json
{
  "success": true,
  "data": {
    "target": "staged",
    "diffs": [
      {
        "file_path": "src/main.rs",
        "unified_diff": "@@ -1,3 +1,3 @@\n fn main() {\n-    old\n+    new\n }\n",
        "inserts": 1,
        "deletes": 1
      }
    ]
  }
}
```

**cURL 示例：**

```bash
# 查看 staged 差异
curl "http://127.0.0.1:8080/api/v1/show?show_what=staged"

# 查看指定 checkpoint 差异
curl "http://127.0.0.1:8080/api/v1/show?show_what=checkpoint&target_id=a1b2c3d4e5f6"

# 查看指定 partition 差异
curl "http://127.0.0.1:8080/api/v1/show?show_what=partition&target_id=manual"
```

---

### 21. 检查点恢复 — `POST /api/v1/checkpoint/restore`

从指定检查点恢复文件到工作目录。

**请求体：**

```json
{
  "checkpoint_id": "a1b2c3d4e5f6...",
  "source_filter": null
}
```

| 字段            | 类型             | 说明                                     |
| --------------- | ---------------- | ---------------------------------------- |
| `checkpoint_id` | string (必需)    | 检查点 ID                                |
| `source_filter` | string[] (可选)  | 源过滤模式（glob），为空时恢复所有文件   |

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/checkpoint/restore \
  -H 'Content-Type: application/json' \
  -d '{"checkpoint_id": "a1b2c3d4e5f6"}'
```

---

### 22. 按时间恢复 — `POST /api/v1/checkpoint/restore-by-time`

恢复到距离目标时间最近的检查点。

**请求体：**

```json
{
  "target_time": 1684396800000,
  "source_filter": null
}
```

| 字段            | 类型             | 说明                                      |
| --------------- | ---------------- | ----------------------------------------- |
| `target_time`   | number (必需)    | 目标时间戳（Unix 毫秒）                   |
| `source_filter` | string[] (可选)  | 源过滤模式（glob）                        |

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/checkpoint/restore-by-time \
  -H 'Content-Type: application/json' \
  -d '{"target_time": 1684396800000}'
```

---

### 23. 检查点差异 — `POST /api/v1/checkpoint/diff`

对比两个检查点之间的差异。

**请求体：**

```json
{
  "from_id": "a1b2c3d4e5f6...",
  "to_id": "b2c3d4e5f6a1..."
}
```

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/checkpoint/diff \
  -H 'Content-Type: application/json' \
  -d '{"from_id": "a1b2c3d4e5f6", "to_id": "b2c3d4e5f6a1"}'
```

---

### 24. 回滚检查点 — `POST /api/v1/checkpoint/rollback`

将 staged 分区回滚到指定检查点。

**请求体：**

```json
{
  "checkpoint_id": "a1b2c3d4e5f6..."
}
```

**cURL 示例：**

```bash
curl -X POST http://127.0.0.1:8080/api/v1/checkpoint/rollback \
  -H 'Content-Type: application/json' \
  -d '{"checkpoint_id": "a1b2c3d4e5f6"}'
```

---

## 典型工作流程

### 单人编辑流程

```bash
# 1. 初始化
curl -X POST http://127.0.0.1:8080/api/v1/init -H 'Content-Type: application/json' -d '{}'

# 2. 编辑文件
curl -X POST http://127.0.0.1:8080/api/v1/edit \
  -H 'Content-Type: application/json' \
  -d '{"file":"src/main.rs","content":"fn main() { println!(\"Hello\"); }"}'

# 3. 提交检查点
curl -X POST http://127.0.0.1:8080/api/v1/commit \
  -H 'Content-Type: application/json' \
  -d '{"message":"初始提交"}'

# 4. 查看状态和提交历史
curl http://127.0.0.1:8080/api/v1/status
curl "http://127.0.0.1:8080/api/v1/log?count=5"
```

### 多 Agent 协同流程

```bash
# 1. 初始化
curl -X POST http://127.0.0.1:8080/api/v1/init -H 'Content-Type: application/json' -d '{}'

# 2. Agent A 编辑并提交审核
curl -X POST http://127.0.0.1:8080/api/v1/agent/agent-a/edit \
  -H 'Content-Type: application/json' \
  -d '{"file":"src/auth.rs","content":"pub fn login() {}"}'
curl -X POST http://127.0.0.1:8080/api/v1/agent/agent-a/submit

# 3. Agent B 编辑并提交审核
curl -X POST http://127.0.0.1:8080/api/v1/agent/agent-b/edit \
  -H 'Content-Type: application/json' \
  -d '{"file":"src/db.rs","content":"pub fn connect() {}"}'
curl -X POST http://127.0.0.1:8080/api/v1/agent/agent-b/submit

# 4. 查看待审批列表
curl http://127.0.0.1:8080/api/v1/approvals

# 5. 审批通过
curl -X POST http://127.0.0.1:8080/api/v1/approve-agent \
  -H 'Content-Type: application/json' \
  -d '{"agent_id":"agent-a"}'
curl -X POST http://127.0.0.1:8080/api/v1/approve-agent \
  -H 'Content-Type: application/json' \
  -d '{"agent_id":"agent-b"}'

# 6. 合并到 unified
curl -X POST http://127.0.0.1:8080/api/v1/merge-to-unified \
  -H 'Content-Type: application/json' \
  -d '{}'

# 7. 合并到 staged
curl -X POST http://127.0.0.1:8080/api/v1/merge-to-staged

# 8. 提交最终检查点
curl -X POST http://127.0.0.1:8080/api/v1/commit \
  -H 'Content-Type: application/json' \
  -d '{"message":"合并 auth 和 db 模块","author":"reviewer"}'
```

### Git 集成流程

```bash
# 1. 从 Git 仓库初始化
curl -X POST http://127.0.0.1:8080/api/v1/init \
  -H 'Content-Type: application/json' \
  -d '{"git_repo":"/path/to/repo","git_ref":"main"}'

# 2. 在 Layertwine 中编辑
curl -X POST http://127.0.0.1:8080/api/v1/edit \
  -H 'Content-Type: application/json' \
  -d '{"file":"src/lib.rs","content":"// new code"}'
curl -X POST http://127.0.0.1:8080/api/v1/commit \
  -H 'Content-Type: application/json' \
  -d '{"message":"Layertwine 编辑"}'

# 3. 提交到本地 Git 分支
curl -X POST http://127.0.0.1:8080/api/v1/git-commit \
  -H 'Content-Type: application/json' \
  -d '{"git_repo":"/path/to/repo","message":"从 Layertwine 同步"}'

# 4. 从 Git 拉取更新
curl -X POST http://127.0.0.1:8080/api/v1/pull \
  -H 'Content-Type: application/json' \
  -d '{"git_repo":"/path/to/repo","git_ref":"main"}'
```

### 分支与合并流程

```bash
# 创建新分支
curl -X POST http://127.0.0.1:8080/api/v1/branches \
  -H 'Content-Type: application/json' \
  -d '{"name":"feature/login"}'

# 切换到新分支
curl -X POST http://127.0.0.1:8080/api/v1/branches/feature%2Flogin/switch

# 在新分支上编辑并提交
curl -X POST http://127.0.0.1:8080/api/v1/edit \
  -H 'Content-Type: application/json' \
  -d '{"file":"src/login.rs","content":"pub fn login() {}"}'
curl -X POST http://127.0.0.1:8080/api/v1/commit \
  -H 'Content-Type: application/json' \
  -d '{"message":"添加登录功能"}'

# 切回主分支
curl -X POST http://127.0.0.1:8080/api/v1/branches/main/switch

# 合并功能分支
curl -X POST http://127.0.0.1:8080/api/v1/merge \
  -H 'Content-Type: application/json' \
  -d '{"branch":"feature/login","message":"合并登录功能到主分支"}'

# 查看所有分支
curl http://127.0.0.1:8080/api/v1/branches
```

---

## 架构对应关系

每个 HTTP 端点对应的内部模块及架构层：

| HTTP 端点                                           | 所属模块                                 | 架构层              |
| --------------------------------------------------- | ---------------------------------------- | ------------------- |
| `POST /api/v1/init`                                 | `state_machine` + `storage`              | P1 存储层           |
| `GET  /api/v1/status`                               | `api::service`                           | P3 状态机查询       |
| `POST /api/v1/edit`                                 | `layered::manual`                        | P3 manual_edit 层   |
| `POST /api/v1/agent/{id}/edit`                      | `layered::agent`                         | P3 agent_edit 层    |
| `POST /api/v1/agent/{id}/submit`                    | `layered::agent` + `approval`            | P3 approval 层      |
| `GET  /api/v1/approvals`                            | `api::service` + `state_machine`         | P3 审批查询         |
| `POST /api/v1/approve-agent`                        | `state_machine::approval`                | P3 approval 层      |
| `POST /api/v1/reject-agent`                         | `state_machine::approval`                | P3 approval 层      |
| `POST /api/v1/merge-to-unified`                     | `layered::integrated` + `unified`        | P3 多层流水线       |
| `POST /api/v1/merge-to-staged`                      | `layered::unified` + `staged`            | P3 多层流水线       |
| `POST /api/v1/commit`                               | `checkpoint::repo`                       | P4 检查点仓库       |
| `GET  /api/v1/log`                                  | `checkpoint::repo`                       | P4 历史查询         |
| `GET  /api/v1/branches`                             | `checkpoint::branch`                     | P4 分支管理         |
| `POST /api/v1/branches`                             | `checkpoint::repo`                       | P4 分支管理         |
| `POST /api/v1/branches/{name}/switch`               | `checkpoint::repo` + `state_machine`     | P4 分支切换         |
| `POST /api/v1/merge`                                | `checkpoint::repo` + `engine::merge`     | P4/P2 合并引擎      |
| `POST /api/v1/backup`                               | `backup::backup_repo`                    | P5 备份模块         |
| `POST /api/v1/restore`                              | `backup::backup_repo`                    | P5 恢复模块         |
| `POST /api/v1/gc`                                   | `git_sync::gc`                           | P6 垃圾回收         |
| `POST /api/v1/compact`                              | `storage::sqlite`                        | P1 数据库维护       |
| `POST /api/v1/git-commit`                           | `git_sync::git_bridge`                   | P6 Git 同步         |
| `POST /api/v1/pull`                                 | `git_sync::git_bridge`                   | P6 Git 同步         |
| `GET  /api/v1/show`                                 | `api::service` + `engine::diff`          | P2/P4 差异查看      |
| `POST /api/v1/clean`                                | `storage::sqlite` + `git_sync::gc`       | P1/P6 数据清理      |
| `POST /api/v1/checkpoint/restore`                   | `checkpoint::repo` + `engine`            | P4/P2 检查点恢复    |
| `POST /api/v1/checkpoint/restore-by-time`           | `checkpoint::repo` + `engine`            | P4/P2 检查点恢复    |
| `POST /api/v1/checkpoint/diff`                      | `checkpoint::repo` + `engine`            | P4/P2 差异对比      |
| `POST /api/v1/checkpoint/rollback`                  | `state_machine::staged` + `checkpoint`   | P3/P4 回滚          |

---

## 配置说明

HTTP 模式下的配置通过环境变量和 TOML 配置文件共同管理。

- `LAYERTWINE_MODE=http` — 启用 HTTP 模式
- `LAYERTWINE_DB_PATH` — 指定 SQLite 数据库路径
- `LAYERTWINE_HTTP_ADDR` — 指定监听地址

数据库维护配置（WAL checkpoint、vacuum 等）通过 `layertwine.toml` 配置，详情见配置参考文档。