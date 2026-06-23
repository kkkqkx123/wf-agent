# 虚拟文件系统 (VFS) 方案

## 概述

VFS 是实现 Copy-on-Write 和文件隔离的核心基础设施。参考 AgentFS OverlayFS 实现。

VFS 独立于沙箱模块, 可独立开启/关闭。

## 架构

```
OverlayVFS
├── stat / readFile / writeFile / remove
├── readdir / mkdir / snapshot / restore
│
├── Delta 层 (可写)
│   ├── MemoryDelta: 运行时内存存储, 进程结束丢弃
│   └── SQLiteDelta: SQLite 持久化, 支持快照
│
├── Whiteout 缓存 (Trie 结构)
│   └── O(depth) 查找, 祖先 whiteout 传播
│
└── Base 层 (只读)
    └── HostFS + PathPolicy (白名单路径)
```

## 查找语义

```
1. 检查 Delta 层 → 存在则返回
2. 检查 Whiteout 缓存 → 存在则返回"不存在"
3. 检查 Base 层 + 路径策略 → 返回或拒绝
```

## 核心接口

```typescript
export interface VFSEntry {
  name: string;
  path: string;
  type: "file" | "directory";
  size: number;
  mode: number;
  createdAt: number;
  modifiedAt: number;
  isWhiteout: boolean;
}

export interface VFSOperations {
  // 文件
  stat(path: string): Promise<VFSEntry | null>;
  readFile(path: string): Promise<Buffer | null>;
  writeFile(path: string, data: Buffer): Promise<void>;
  remove(path: string): Promise<void>;
  rename(oldPath: string, newPath: string): Promise<void>;

  // 目录
  readdir(path: string): Promise<VFSEntry[]>;
  mkdir(path: string): Promise<void>;
  rmdir(path: string): Promise<void>;

  // 快照 (CoW)
  snapshot(): Promise<string>;
  restore(snapshotId: string): Promise<void>;
  diff(snapshotA: string, snapshotB: string): Promise<VFSEntry[]>;

  // 路径检查
  exists(path: string): Promise<boolean>;
  isAllowed(path: string): boolean;
}
```

## Whiteout 缓存 (Trie)

直接移植自 AgentFS：

```typescript
class WhiteoutCache {
  private root = { children: new Map<string, WhiteoutNode>(), isWhiteout: false };

  hasWhiteout(path: string): boolean {
    let node = this.root;
    for (const component of path.split("/").filter(Boolean)) {
      if (node.isWhiteout) return true;   // 祖先 whiteout
      if (!node.children.has(component)) return false;
      node = node.children.get(component)!;
    }
    return node.isWhiteout;
  }

  markWhiteout(path: string): void {
    let node = this.root;
    for (const component of path.split("/").filter(Boolean)) {
      if (!node.children.has(component)) {
        node.children.set(component, { children: new Map(), isWhiteout: false });
      }
      node = node.children.get(component)!;
    }
    node.isWhiteout = true;
  }
}
```

## Delta 层实现

### MemoryDelta

```typescript
class MemoryDelta implements DeltaFileSystem {
  private files = new Map<string, { data: Buffer; entry: VFSEntry }>();
  private snapshots = new Map<string, Map<string, { data: Buffer; entry: VFSEntry }>>();

  async stat(path: string): Promise<VFSEntry | null> {
    return this.files.get(path)?.entry || null;
  }

  async writeFile(path: string, data: Buffer): Promise<void> {
    this.files.set(path, {
      data,
      entry: {
        name: basename(path),
        path,
        type: "file",
        size: data.length,
        mode: 0o644,
        createdAt: Date.now(),
        modifiedAt: Date.now(),
        isWhiteout: false,
      },
    });
  }

  async snapshot(): Promise<string> {
    const id = crypto.randomUUID();
    this.snapshots.set(id, new Map(this.files));
    return id;
  }

  async restore(snapshotId: string): Promise<void> {
    const snapshot = this.snapshots.get(snapshotId);
    if (!snapshot) throw new Error(`Snapshot not found: ${snapshotId}`);
    this.files = new Map(snapshot);
  }
}
```

### SQLiteDelta (未来)

```typescript
class SQLiteDelta implements DeltaFileSystem {
  // SQLite schema:
  // CREATE TABLE fs_inode (ino INTEGER PRIMARY KEY, parent INTEGER, name TEXT,
  //   mode INTEGER, size INTEGER, is_dir INTEGER, created_at INTEGER, modified_at INTEGER);
  // CREATE TABLE fs_data (ino INTEGER, chunk_index INTEGER, data BLOB, PRIMARY KEY(ino, chunk_index));
  // CREATE TABLE fs_origin (ino INTEGER, original_path TEXT);  -- CoW origin tracking
  // CREATE TABLE fs_snapshot (id TEXT PRIMARY KEY, created_at INTEGER);
  // CREATE TABLE fs_whiteout (path TEXT PRIMARY KEY);
}
```

## VFS 配置

```typescript
export interface VFSConfig {
  enabled: boolean;           // 默认 false
  storage: "memory" | "sqlite";
  workspaceRoot: string;
  dbPath?: string;            // SQLite 模式
  pathPolicy?: {
    readable?: string[];
    writable?: string[];
  };
}
```

## Checkpoint 集成

```typescript
export class CheckpointAwareVFS {
  private vfs: OverlayVFS;

  async onCheckpointCreate(checkpointId: string): Promise<void> {
    const snapshotId = await this.vfs.snapshot();
    await this.storeMapping(checkpointId, snapshotId);
  }

  async onCheckpointRestore(checkpointId: string): Promise<void> {
    const snapshotId = await this.getMapping(checkpointId);
    if (snapshotId) await this.vfs.restore(snapshotId);
  }
}
```