# 沙箱模块改进方案

> 基于 `ref/agentfs/sandbox` 参考实现的对比分析，制定本改进方案。

## 现状总览

当前沙箱模块已完成全部 8 个阶段的代码落地（P1~P8），但与 agentfs 参考实现相比存在以下核心差距：

| 维度 | agentfs | 当前项目 | 差距等级 |
|------|---------|---------|---------|
| 路径翻译/MountTable | 最长前缀匹配 + 多 VFS 挂载 | 仅 OverlayVFS 两层 | 高 |
| SQLite VFS POSIX 语义 | symlink/link/xattr 完整 | 基础文件操作 | 高 |
| OS Hook 可用性 | FUSE/namespace 可实际使用 | Linux seccomp 骨架 | 中 |
| Checkpoint-VFS 集成 | 原生集成 | CheckpointAwareVFS 未实现 | 中 |
| 审计日志 | 全部操作记录 SQLite | 无统一审计 | 低 |

---

## Phase 1: VFS MountTable 路径翻译机制

### 目标

为 VFS 层增加多挂载点管理和最长前缀匹配路径翻译能力，使 sandbox 可以透明地将沙箱内路径映射到宿主机的不同位置。

### 设计

```
MountTable
├── mounts: MountPoint[]
│   ├── sandbox_path: 沙箱内虚拟路径 (e.g., "/agent")
│   └── vfs: VFS 实现 (BindVfs / SqliteVfs / HostFS)
│
├── addMount(sandboxPath, vfs) → void
├── resolve(path) → { vfs, translatedPath } | null  // 最长前缀匹配
└── mounts() → MountPoint[]
```

### 实现文件

| 文件 | 操作 | 说明 |
|------|------|------|
| `sdk/services/vfs/types.ts` | 修改 | 新增 `MountPoint` 接口、`MountTable` 类 |
| `sdk/services/vfs/mount-table.ts` | 新建 | MountTable 实现（最长前缀匹配） |
| `sdk/services/vfs/bind-vfs.ts` | 新建 | BindVfs（路径翻译到宿主机） |
| `sdk/services/vfs/overlay-vfs.ts` | 修改 | 集成 MountTable，支持 multi-mount |
| `sdk/services/vfs/index.ts` | 修改 | 导出新组件 |
| `packages/types/src/script/script-sandbox.ts` | 修改 | VFSConfig 增加 mounts 配置 |

---

## Phase 2: SqliteDelta POSIX 语义增强

### 目标

为 SqliteDelta 增加符号链接和扩展属性支持，使其可作为独立虚拟文件系统使用。

### 实现文件

| 文件 | 操作 | 说明 |
|------|------|------|
| `sdk/services/vfs/types.ts` | 修改 | `DeltaFileSystem` 增加 `symlink`/`readlink`/`link` 方法 |
| `sdk/services/vfs/delta/sqlite-delta.ts` | 修改 | 增加 vfs_symlinks 表 + 相关方法 |
| `sdk/services/vfs/overlay-vfs.ts` | 修改 | 透传 symlink/readlink/link 操作 |

---

## Phase 3: OS Hook 策略增强

### 目标

增强 OS Hook 策略的可用性和审计能力。

### 3.1 LinuxSeccompStrategy 增强

- 增加 seccomp-loader 原生 helper 的编译脚本
- 完善 syscall 策略映射（增加更多的 syscall 类型）
- 增加审计日志输出

### 3.2 策略审计日志

- 所有 OS Hook 策略执行时记录操作到统一审计链
- 审计日志包含：时间戳、策略 ID、命令、是否放行

### 实现文件

| 文件 | 操作 | 说明 |
|------|------|------|
| `sdk/services/sandbox/strategies/os-hooks/base.ts` | 修改 | 增加审计日志辅助函数 |
| `sdk/services/sandbox/strategies/os-hooks/linux-seccomp.ts` | 修改 | 增强策略映射 + 审计 |
| `sdk/services/sandbox/strategies/os-hooks/windows-job-object.ts` | 修改 | 增加审计日志 |

---

## Phase 4: Checkpoint-VFS 集成

### 目标

将 VFS 快照能力接入 checkpoint 系统，使 checkpoint 可以同时保存和恢复文件状态。

### 设计

```
CheckpointAwareVFS
├── vfs: OverlayVFS
├── snapshotMapping: Map<checkpointId, vfsSnapshotId>
│
├── onCheckpointCreate(checkpointId) → 创建 VFS 快照并存储映射
├── onCheckpointRestore(checkpointId) → 从映射恢复 VFS 快照
└── onCheckpointDelete(checkpointId) → 清理对应快照
```

### 实现文件

| 文件 | 操作 | 说明 |
|------|------|------|
| `sdk/services/vfs/checkpoint-aware-vfs.ts` | 新建 | CheckpointAwareVFS 包装类 |
| `sdk/services/vfs/index.ts` | 修改 | 导出 CheckpointAwareVFS |

---

## 执行顺序

```
Phase 1 (MountTable)
  ↓
Phase 2 (SqliteDelta POSIX)
  ↓
Phase 3 (OS Hook 增强)
  ↓
Phase 4 (Checkpoint-VFS)
```

各阶段独立可交付，可按需单独实施。