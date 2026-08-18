# Layertwine gRPC API 参考

> Layertwine 的 gRPC API 以 Protobuf 定义为核心，通过 Tonic 框架提供。所有 RPC 方法均已完整实现。

## 当前状态

gRPC 服务端已完整实现，可正常使用。

- **Proto 文件**: `src/api/rpc/proto/layertwine.proto` — 定义了 25 个 RPC 方法
- **Rust 实现**: `src/api/rpc/mod.rs` — `LayertwineGrpc` 结构体实现了所有 RPC 方法
- **业务逻辑**: `ApiService`（`src/api/service.rs`）提供所有底层逻辑
- **启动命令**: 启用 `grpc` feature

---

## 环境变量

| 变量                | 默认值                | 说明                           |
| ------------------- | --------------------  | ------------------------------ |
| `LAYERTWINE_MODE`   | `cli`                 | 设为 `grpc` 启动 gRPC 服务     |
| `LAYERTWINE_DB_PATH`| `.layertwine/layertwine.db` | SQLite 数据库文件路径          |
| `LAYERTWINE_GRPC_ADDR`| `127.0.0.1:50051`   | gRPC 服务器绑定地址与端口       |

---

## Proto 服务定义

```protobuf
syntax = "proto3";

package layertwine;

service Layertwine {
    // Repository lifecycle
    rpc Init(InitRequest) returns (InitResponse);
    rpc Status(Empty) returns (StatusResponse);

    // Edit operations
    rpc Edit(EditRequest) returns (EditResponse);
    rpc AgentEdit(AgentEditRequest) returns (EditResponse);
    rpc AgentSubmit(AgentSubmitRequest) returns (SubmitResponse);
    rpc Approve(ApproveRequest) returns (ApproveResponse);

    // Checkpoint operations
    rpc Commit(CommitRequest) returns (CommitResponse);
    rpc Log(LogRequest) returns (LogResponse);

    // Branch operations
    rpc BranchCreate(BranchCreateRequest) returns (BranchCreateResponse);
    rpc BranchSwitch(BranchSwitchRequest) returns (BranchSwitchResponse);
    rpc BranchList(Empty) returns (BranchListResponse);
    rpc Merge(MergeRequest) returns (MergeResponse);

    // Backup operations
    rpc Backup(BackupRequest) returns (BackupResponse);
    rpc Restore(RestoreRequest) returns (RestoreResponse);

    // Maintenance
    rpc Gc(Empty) returns (GcResponse);
    rpc Compact(CompactRequest) returns (CompactResponse);
    rpc GitCommit(GitCommitRequest) returns (GitCommitResponse);
    rpc Clean(CleanRequest) returns (CleanResponse);
    rpc Pull(PullRequest) returns (PullResponse);

    // Show / diff display
    rpc Show(ShowRequest) returns (ShowResponse);

    // Checkpoint restore operations
    rpc CheckpointRestore(CheckpointRestoreRequest) returns (CheckpointRestoreResponse);
    rpc CheckpointRestoreByTime(CheckpointRestoreByTimeRequest) returns (CheckpointRestoreResponse);
    rpc CheckpointDiff(CheckpointDiffRequest) returns (CheckpointDiffResponse);
    rpc CheckpointRollback(CheckpointRollbackRequest) returns (CheckpointRollbackResponse);

    // Granular approval operations
    rpc ListPendingApprovals(Empty) returns (ListPendingApprovalsResponse);
    rpc ApproveAgent(ApproveAgentRequest) returns (ApproveAgentResponse);
    rpc RejectAgent(RejectAgentRequest) returns (RejectAgentResponse);
    rpc MergeToUnified(MergeToUnifiedRequest) returns (MergeToUnifiedResponse);
    rpc MergeToStaged(MergeToStagedRequest) returns (MergeToStagedResponse);
}
```

---

## 服务方法参考

### RPC 列表

| RPC 方法                  | 请求类型                       | 响应类型                        | 说明                       |
| ------------------------- | ------------------------------ | ------------------------------- | -------------------------- |
| `Init`                    | `InitRequest`                  | `InitResponse`                  | 初始化仓库                 |
| `Status`                  | `Empty`                        | `StatusResponse`                | 查看状态                   |
| `Edit`                    | `EditRequest`                  | `EditResponse`                  | 手动编辑                   |
| `AgentEdit`               | `AgentEditRequest`             | `EditResponse`                  | Agent 编辑                 |
| `AgentSubmit`             | `AgentSubmitRequest`           | `SubmitResponse`                | Agent 提交审核             |
| `Approve`                 | `ApproveRequest`               | `ApproveResponse`               | 审核通过（旧版单步）       |
| `Commit`                  | `CommitRequest`                | `CommitResponse`                | 提交检查点                 |
| `Log`                     | `LogRequest`                   | `LogResponse`                   | 查看提交历史               |
| `BranchCreate`            | `BranchCreateRequest`          | `BranchCreateResponse`          | 创建分支                   |
| `BranchSwitch`            | `BranchSwitchRequest`          | `BranchSwitchResponse`          | 切换分支                   |
| `BranchList`              | `Empty`                        | `BranchListResponse`            | 列出所有分支               |
| `Merge`                   | `MergeRequest`                 | `MergeResponse`                 | 合并分支                   |
| `Backup`                  | `BackupRequest`                | `BackupResponse`                | 备份快照                   |
| `Restore`                 | `RestoreRequest`               | `RestoreResponse`               | 从备份恢复                 |
| `Gc`                      | `Empty`                        | `GcResponse`                    | 垃圾回收                   |
| `Compact`                 | `CompactRequest`               | `CompactResponse`               | 数据库压缩                 |
| `GitCommit`               | `GitCommitRequest`             | `GitCommitResponse`             | 提交到本地 Git 分支        |
| `Clean`                   | `CleanRequest`                 | `CleanResponse`                 | 清理存储数据               |
| `Pull`                    | `PullRequest`                  | `PullResponse`                  | 从 Git 拉取                |
| `Show`                    | `ShowRequest`                  | `ShowResponse`                  | 查看差异                   |
| `CheckpointRestore`       | `CheckpointRestoreRequest`     | `CheckpointRestoreResponse`     | 检查点恢复                 |
| `CheckpointRestoreByTime` | `CheckpointRestoreByTimeRequest`| `CheckpointRestoreResponse`    | 按时间恢复检查点           |
| `CheckpointDiff`          | `CheckpointDiffRequest`        | `CheckpointDiffResponse`        | 检查点差异对比             |
| `CheckpointRollback`      | `CheckpointRollbackRequest`    | `CheckpointRollbackResponse`    | 回滚检查点                 |
| `ListPendingApprovals`    | `Empty`                        | `ListPendingApprovalsResponse`  | 待审批列表                 |
| `ApproveAgent`            | `ApproveAgentRequest`          | `ApproveAgentResponse`          | 审批通过 Agent             |
| `RejectAgent`             | `RejectAgentRequest`           | `RejectAgentResponse`           | 拒绝 Agent 提交            |
| `MergeToUnified`          | `MergeToUnifiedRequest`        | `MergeToUnifiedResponse`        | 合并到 unified 分区        |
| `MergeToStaged`           | `MergeToStagedRequest`         | `MergeToStagedResponse`         | 合并到 staged 层           |

---

## 消息类型定义

### 通用类型

```protobuf
message Empty {}
```

所有无参数的 RPC 使用 `Empty` 作为请求类型。

---

### Repository Lifecycle

#### Init — 初始化仓库

```protobuf
message InitRequest {
    optional string db_path = 1;
    optional string git_repo = 2;
    optional string git_ref = 3;
}

message InitResponse {
    string db_path = 1;
    string manual_partition_id = 2;
    string staged_partition_id = 3;
    string branch = 4;
}
```

| 字段                        | 类型           | 说明                                       |
| --------------------------- | -------------- | ------------------------------------------ |
| `InitRequest.db_path`       | string (可选)  | 数据库路径，默认 `.layertwine/layertwine.db`      |
| `InitRequest.git_repo`      | string (可选)  | Git 仓库路径                               |
| `InitRequest.git_ref`       | string (可选)  | 从 Git 初始化时的引用                      |
| `InitResponse.branch`       | string         | 当前分支名，默认 `"main"`                  |

#### Status — 查看状态

```protobuf
message StatusResponse {
    repeated PartitionInfo partitions = 1;
}

message PartitionInfo {
    string layer = 1;
    string name = 2;
    string current_snapshot = 3;
    uint32 history_len = 4;
}
```

| 字段                              | 类型   | 说明                                              |
| --------------------------------- | ------ | ------------------------------------------------- |
| `PartitionInfo.layer`             | string | 层标识：`manual_edit` / `agent_edit` / `staged`   |
| `PartitionInfo.current_snapshot`  | string | 当前快照 ID（十六进制）                           |
| `PartitionInfo.history_len`       | uint32 | 历史 Delta 数量                                   |

---

### Edit Operations

#### Edit / AgentEdit — 编辑文件

```protobuf
message EditRequest {
    string file = 1;
    optional string content = 2;
}

message AgentEditRequest {
    string agent_id = 1;
    string file = 2;
    optional string content = 3;
}

message EditResponse {
    string snapshot_id = 1;
    optional string staged_snapshot_id = 2;
}
```

| 字段                                 | 类型             | 说明                                       |
| ------------------------------------ | ---------------- | ------------------------------------------ |
| `AgentEditRequest.agent_id`          | string           | Agent 实例 ID                              |
| `EditResponse.snapshot_id`           | string           | 本次编辑生成的快照 ID                      |
| `EditResponse.staged_snapshot_id`    | string (可选)    | 自动合并到 staged 后的快照 ID              |

#### AgentSubmit — Agent 提交审核

```protobuf
message AgentSubmitRequest {
    string agent_id = 1;
}

message SubmitResponse {
    string snapshot_id = 1;
}
```

#### Approve — 审核通过（旧版单步）

```protobuf
message ApproveRequest {
    string agent_id = 1;
}

message ApproveResponse {
    string integrated_snapshot_id = 1;
    string staged_snapshot_id = 2;
}
```

---

### Checkpoint Operations

#### Commit — 提交检查点

```protobuf
message CommitRequest {
    string message = 1;
    optional string author = 2;
}

message CommitResponse {
    string checkpoint_id = 1;
    string message = 2;
}
```

#### Log — 查看提交历史

```protobuf
message LogRequest {
    optional uint32 count = 1;
}

message LogResponse {
    repeated CheckpointInfo checkpoints = 1;
    uint32 total = 2;
}

message CheckpointInfo {
    string id = 1;
    string author = 2;
    string message = 3;
    repeated string parents = 4;
    repeated string snapshots = 5;
    int64 created_at = 6;
    optional string git_anchor = 7;
}
```

| 字段                          | 类型          | 说明                            |
| ----------------------------  | ------------- | ------------------------------- |
| `LogRequest.count`            | uint32 (可选) | 最大返回数量，默认 20           |
| `CheckpointInfo.parents`      | string[]      | 父检查点 ID 列表                |
| `CheckpointInfo.snapshots`    | string[]      | 基线快照 ID 列表                |
| `CheckpointInfo.created_at`   | int64         | Unix 时间戳                     |
| `CheckpointInfo.git_anchor`   | string (可选) | Git 锚点（从 Git 同步时存在）   |

---

### Branch Operations

```protobuf
message BranchCreateRequest {
    string name = 1;
}

message BranchCreateResponse {
    string name = 1;
    string head = 2;
}

message BranchSwitchRequest {
    string name = 1;
}

message BranchSwitchResponse {
    string name = 1;
    string checkpoint_id = 2;
}

message BranchListResponse {
    repeated BranchInfo branches = 1;
    optional string current = 2;
}

message BranchInfo {
    string name = 1;
    string head = 2;
    string updated_at = 3;
    bool is_current = 4;
}

message MergeRequest {
    string branch = 1;
    optional string message = 2;
}

message MergeResponse {
    string checkpoint_id = 1;
    string source_branch = 2;
    string target_branch = 3;
}
```

---

### Backup Operations

```protobuf
message BackupRequest {
    string snapshot_id = 1;
    optional string label = 2;
}

message BackupResponse {
    string backup_id = 1;
    string source_snapshot_id = 2;
    optional string label = 3;
}

message RestoreRequest {
    string backup_id = 1;
}

message RestoreResponse {
    string backup_id = 1;
    string file = 2;
    uint32 deltas_restored = 3;
}
```

---

### Maintenance

```protobuf
message GcResponse {
    uint32 removed_checkpoints = 1;
    uint32 removed_snapshots = 2;
    uint64 freed_bytes = 3;
    bool delta_chain_depth_triggered = 4;
}

message GitCommitRequest {
    string git_repo = 1;
    optional string message = 2;
}

message GitCommitResponse {
    string git_commit_hash = 1;
}

message CleanRequest {
    optional string branch = 1;
    optional string layer = 2;
    bool all = 3;
}

message CleanResponse {
    uint32 removed_branches = 1;
    uint32 removed_checkpoints = 2;
    uint32 removed_snapshots = 3;
    uint32 removed_deltas = 4;
    uint32 removed_layers = 5;
    string message = 6;
}

message PullRequest {
    optional string remote = 1;
    string git_repo = 2;
    optional string git_ref = 3;
}

message PullResponse {
    string remote = 1;
    string git_ref = 2;
}

message CompactRequest {
    optional bool vacuum_full = 1;
}

message CompactResponse {
    bool wal_checkpointed = 1;
    int64 freelist_before = 2;
    int64 total_pages = 3;
    int64 freelist_after = 4;
    bool vacuum_performed = 5;
    string message = 6;
}
```

| 字段                                | 类型    | 说明                                    |
| ----------------------------------- | ------- | --------------------------------------- |
| `GitCommitRequest.git_repo`         | string  | Git 仓库路径（必填）                    |
| `CleanRequest.branch`               | string  | 清理指定分支（可选）                    |
| `CleanRequest.layer`                | string  | 清理指定层（可选）                      |
| `CleanRequest.all`                  | bool    | 全部清理                                |
| `PullRequest.git_repo`              | string  | Git 仓库路径（必填）                    |
| `GcResponse.freed_bytes`            | uint64  | 释放的字节数                            |
| `GcResponse.delta_chain_depth_triggered` | bool | 是否触发 Delta 链深度压缩           |

---

### Show / Diff

```protobuf
message ShowRequest {
    string show_what = 1;
    optional string target_id = 2;
}

message ShowResponse {
    string target = 1;
    repeated FileDiff diffs = 2;
}

message FileDiff {
    string file_path = 1;
    string unified_diff = 2;
    uint32 inserts = 3;
    uint32 deletes = 4;
}
```

---

### Checkpoint Operations（高级）

```protobuf
message CheckpointRestoreRequest {
    string checkpoint_id = 1;
    repeated string source_filter = 2;
}

message CheckpointRestoreByTimeRequest {
    int64 target_time = 1;
    repeated string source_filter = 2;
}

message CheckpointRestoreResponse {
    CheckpointInfo checkpoint = 1;
    repeated RestoredSnapshotInfo snapshots = 2;
    repeated string ancestry = 3;
}

message RestoredSnapshotInfo {
    string snapshot_id = 1;
    string source = 2;
    string content_hex = 3;
    string content_type = 4;
}

message CheckpointDiffRequest {
    string from_id = 1;
    string to_id = 2;
}

message CheckpointDiffResponse {
    string from_id = 1;
    string to_id = 2;
    repeated string added = 3;
    repeated string removed = 4;
    repeated string modified = 5;
    uint32 total_changes = 6;
}

message CheckpointRollbackRequest {
    string checkpoint_id = 1;
}

message CheckpointRollbackResponse {
    string checkpoint_id = 1;
    repeated string snapshot_ids = 2;
}
```

---

### Granular Approval Operations

```protobuf
message ApprovalInfo {
    string agent_id = 1;
    string partition_name = 2;
    string current_snapshot = 3;
    uint32 history_len = 4;
}

message ListPendingApprovalsResponse {
    repeated ApprovalInfo approvals = 1;
    uint32 total = 2;
}

message ApproveAgentRequest {
    string agent_id = 1;
    optional string integrated_name = 2;
}

message ApproveAgentResponse {
    string agent_id = 1;
    string integrated_snapshot_id = 2;
}

message RejectAgentRequest {
    string agent_id = 1;
}

message RejectAgentResponse {
    string agent_id = 1;
    string baseline_snapshot_id = 2;
}

message MergeToUnifiedRequest {
    repeated string integration_names = 1;
}

message MergeToUnifiedResponse {
    string unified_snapshot_id = 1;
    uint32 merged_count = 2;
}

message MergeToStagedRequest {}

message MergeToStagedResponse {
    string staged_snapshot_id = 1;
}
```

---

## 实现架构

gRPC 层与 HTTP 层共享 `ApiService` 结构体作为业务逻辑入口：

```
gRPC Client
    │
    ▼
tonic Server (LayertwineGrpc)
    │  ┌─ Init()                → service.init(InitRequest)
    │  ├─ Status()              → service.status()
    │  ├─ Edit()                → service.edit(EditRequest)
    │  ├─ AgentEdit()           → service.agent_edit(AgentEditRequest)
    │  ├─ AgentSubmit()         → service.agent_submit(AgentSubmitRequest)
    │  ├─ Approve()             → service.approve(ApproveRequest)
    │  ├─ Commit()              → service.commit(CommitRequest)
    │  ├─ Log()                 → service.log(LogRequest)
    │  ├─ BranchCreate()        → service.branch_create(BranchCreateRequest)
    │  ├─ BranchSwitch()        → service.branch_switch(BranchSwitchRequest)
    │  ├─ BranchList()          → service.branch_list()
    │  ├─ Merge()               → service.merge(MergeRequest)
    │  ├─ Backup()              → service.backup(BackupRequest)
    │  ├─ Restore()             → service.restore(RestoreRequest)
    │  ├─ Gc()                  → service.gc(GcRequest)
    │  ├─ Compact()             → service.compact(CompactRequest)
    │  ├─ GitCommit()           → service.git_commit(GitCommitRequest)
    │  ├─ Clean()               → service.clean(CleanRequest)
    │  ├─ Pull()                → service.pull(PullRequest)
    │  ├─ Show()                → service.show(ShowRequest)
    │  ├─ CheckpointRestore()   → service.checkpoint_restore(...)
    │  ├─ CheckpointRestoreByTime() → service.checkpoint_restore_by_time(...)
    │  ├─ CheckpointDiff()      → service.checkpoint_diff(...)
    │  ├─ CheckpointRollback()  → service.checkpoint_rollback(...)
    │  ├─ ListPendingApprovals()→ service.list_pending_approvals()
    │  ├─ ApproveAgent()        → service.approve_agent(...)
    │  ├─ RejectAgent()         → service.reject_agent(...)
    │  ├─ MergeToUnified()      → service.merge_to_unified(...)
    │  └─ MergeToStaged()       → service.merge_to_staged()
    ▼
ApiServiceImpl → StateMachine → SqliteStorage
```

每个 RPC handler 的处理模式：
1. 从 proto 请求反序列化
2. 转换为 `api::types::*Request`
3. 调用 `ApiService` 对应方法
4. 将 `api::types::*Response` 转换为 proto 响应
5. 错误映射为 `tonic::Status`

---

## 与 HTTP API 的对应关系

gRPC 服务和 HTTP 端点共享相同的业务逻辑，以下为对应关系：

| gRPC RPC                    | HTTP 端点                                           |
| --------------------------- | --------------------------------------------------- |
| `Init`                      | `POST /api/v1/init`                                 |
| `Status`                    | `GET  /api/v1/status`                               |
| `Edit`                      | `POST /api/v1/edit`                                 |
| `AgentEdit`                 | `POST /api/v1/agent/{id}/edit`                      |
| `AgentSubmit`               | `POST /api/v1/agent/{id}/submit`                    |
| `Approve`                   | ~~`POST /api/v1/approve/{agent_id}`~~（已弃用）     |
| `Commit`                    | `POST /api/v1/commit`                               |
| `Log`                       | `GET  /api/v1/log`                                  |
| `BranchCreate`              | `POST /api/v1/branches`                             |
| `BranchSwitch`              | `POST /api/v1/branches/{name}/switch`               |
| `BranchList`                | `GET  /api/v1/branches`                             |
| `Merge`                     | `POST /api/v1/merge`                                |
| `Backup`                    | `POST /api/v1/backup`                               |
| `Restore`                   | `POST /api/v1/restore`                              |
| `Gc`                        | `POST /api/v1/gc`                                   |
| `Compact`                   | `POST /api/v1/compact`                              |
| `GitCommit`                 | `POST /api/v1/git-commit`                           |
| `Clean`                     | `POST /api/v1/clean`                                |
| `Pull`                      | `POST /api/v1/pull`                                 |
| `Show`                      | `GET  /api/v1/show`                                 |
| `CheckpointRestore`         | `POST /api/v1/checkpoint/restore`                   |
| `CheckpointRestoreByTime`   | `POST /api/v1/checkpoint/restore-by-time`           |
| `CheckpointDiff`            | `POST /api/v1/checkpoint/diff`                      |
| `CheckpointRollback`        | `POST /api/v1/checkpoint/rollback`                  |
| `ListPendingApprovals`      | `GET  /api/v1/approvals`                            |
| `ApproveAgent`              | `POST /api/v1/approve-agent`                        |
| `RejectAgent`               | `POST /api/v1/reject-agent`                         |
| `MergeToUnified`            | `POST /api/v1/merge-to-unified`                     |
| `MergeToStaged`             | `POST /api/v1/merge-to-staged`                      |