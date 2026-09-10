# 6. Git 双向同步与冗余检查点 GC

## 6.1 核心定位

| 角色 | 定位 | 特性 |
|------|------|------|
| 自研仓库 | **实时编辑层** | 轻量、分层、分支、快速检查点 |
| Git | **持久化存储层** | 长期归档、远程同步、标准兼容 |

双向同步**仅做基线对齐**，不干涉编辑分层。

## 6.2 Git → 自研仓库（初始化 / 拉取）

```rust
fn init_from_git(
    git_repo: &GitRepo,
    checkpoint_repo: &mut CheckpointRepo,
    git_ref: &str,           // HEAD / branch / commit hash
) -> Result<()> {
    // 1. 从 Git 导出文件
    let files = git_repo.checkout(git_ref)?;

    // 2. 构建 FileNode + Snapshot
    let mut snapshots = Vec::new();
    for file in files {
        let file_node = FileNode::new(&file.path, hash(&file.content));
        let delta = Delta::new_initial(&file_node, &file.content);
        let snapshot = Snapshot::new_initial(file_node, delta);
        snapshots.push(snapshot);
    }

    // 3. 生成初始 Checkpoint
    let checkpoint = Checkpoint {
        id: CheckpointId::new(),
        parents: vec![],
        baseline_snapshot: SnapshotId::from_snapshots(&snapshots),
        metadata: CheckpointMetadata {
            author: "git-sync".into(),
            message: format!("Sync from Git ref: {}", git_ref),
            git_anchor: Some(git_repo.resolve_ref(git_ref)?),
        },
        created_at: chrono::Utc::now(),
    };

    // 4. 重置仓库基线
    checkpoint_repo.reset_to_checkpoint(checkpoint)?;
    Ok(())
}
```

## 6.3 自研仓库 → Git（推送 / 归档）

```rust
fn push_to_git(
    checkpoint_repo: &CheckpointRepo,
    git_repo: &mut GitRepo,
    branch_name: &str,
    message: &str,
) -> Result<()> {
    // 1. 获取当前分支最新 Checkpoint
    let checkpoint = checkpoint_repo
        .get_branch_checkpoint(branch_name)?;

    // 2. 从基线 Snapshot 还原文件内容
    let files = checkpoint_repo
        .restore_snapshot(checkpoint.baseline_snapshot)?;

    // 3. 写入 Git 工作区并提交
    git_repo.write_files(&files)?;
    let git_commit = git_repo.commit(message)?;

    // 4. 在 Checkpoint 中记录 Git 锚点
    checkpoint_repo.anchor_to_git(checkpoint.id, git_commit)?;

    Ok(())
}
```

## 6.4 同步铁律

1. 同步仅操作**基线**，不同步 `manual/agent_edit/approval` 分层状态
2. 同步后，自研仓库只保留必要的最新检查点，Git 承担全量历史存储
3. 同步操作**不触发核心编辑层的任何流转**

## 6.5 冗余检查点 GC

### 保护规则（永不删除）

- 当前分支 head 检查点 → 保护
- 绑定 `git_anchor` 的检查点 → 保护
- 在当前分支 head 的祖先链上的检查点 → 保护

### 清理规则

```rust
fn gc_checkpoints(
    repo: &mut CheckpointRepo,
    retention_days: u64,
) -> Result<u64> {
    let protected = repo.collect_protected_checkpoints();
    let now = chrono::Utc::now();
    let mut removed = 0;

    let all_checkpoints: Vec<CheckpointId> = repo
        .checkpoint_dag
        .all_nodes()
        .collect();

    for cpid in all_checkpoints {
        if protected.contains(&cpid) {
            continue;
        }
        let cp = repo.get_checkpoint(cpid)?;
        let age = now - chrono::DateTime::from_timestamp_millis(cp.created_at).unwrap();

        if age.num_days() > retention_days as i64 {
            // 安全：没有分支引用，无 Git 锚点，超过保留期
            repo.remove_checkpoint(cpid)?;
            removed += 1;
        }
    }

    Ok(removed)
}

fn collect_protected_checkpoints(
    repo: &CheckpointRepo,
) -> HashSet<CheckpointId> {
    let mut protected = HashSet::new();

    // 所有分支的 head 及其祖先
    for branch in &repo.branches {
        let mut current = branch.head;
        loop {
            protected.insert(current);
            let cp = repo.get_checkpoint(current).ok()?;
            if cp.parents.is_empty() {
                break;
            }
            current = cp.parents[0];  // 线性祖先
        }
    }

    // 所有绑定了 Git 锚点的
    for cp in repo.checkpoint_dag.all_nodes() {
        if let Ok(cp) = repo.get_checkpoint(cp) {
            if cp.metadata.git_anchor.is_some() {
                protected.insert(cp.id);
            }
        }
    }

    protected
}
```

### 效果

- 自研仓库永远保持极小体积
- 仅存当前编辑 + 必要检查点
- 全量历史托管给 Git
- 无存储膨胀风险
