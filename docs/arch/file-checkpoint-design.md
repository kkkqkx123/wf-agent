# File Checkpoint Architecture Design

## Overview

This document describes the design of the file checkpoint subsystem, which adds workspace file state snapshot/restore capability to the existing execution-state checkpoint system.

### Motivation

The current `CheckpointCoordinator` manages **execution state** (variables, conversation, node results) but has no awareness of **file state**. When tools modify workspace files, those changes are invisible to the checkpoint system. This means:

1. A checkpoint restore can revert execution state but leave modified files unchanged — inconsistent state.
2. There is no rollback capability for file changes made by tools.
3. The VFS (used by sandbox scripts) and file editing tools operate independently, creating two uncoordinated sources of file state.

### Design Principle (Plan A)

Keep VFS as a sandbox isolation layer only. File checkpointing is a separate subsystem that operates independently:

```
VFS (SandboxRuntime)    → Sandbox script isolation, no checkpoint involvement
File editing tools      → Direct Host FS writes, no code change
FileCheckpointManager   → Scans Host FS at checkpoint time, stores delta snapshots
CheckpointCoordinator   → Coordinates both execution + file checkpoints
```

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                      CheckpointCoordinator                           │
│  createCheckpoint() {                                                │
│    1. extractExecutionState()     → CheckpointState.save()          │
│    2. fileCP.createCheckpoint()   → FileCheckpointManager            │
│  }                                                                   │
│  restoreFromCheckpoint() {                                           │
│    1. restoreExecutionState()     ← CheckpointState.load()           │
│    2. fileCP.restoreCheckpoint()  ← FileCheckpointManager            │
│  }                                                                   │
└────────────┬────────────────────────────────────┬────────────────────┘
             │                                    │
             ▼                                    ▼
┌─────────────────────────┐   ┌───────────────────────────────────────┐
│   CheckpointState       │   │   FileCheckpointManager               │
│   (execution state)     │   │   (file state)                        │
│                         │   │                                       │
│   ┌─────────────────┐   │   │   createCheckpoint() {                │
│   │ StorageAdapter   │   │   │   1. collectFiles()  → scan Host FS  │
│   │ (SQLite/JSON)    │   │   │   2. computeHashes() → MD5 all files │
│   └─────────────────┘   │   │   3. compareHashes() → delta from prev│
│                         │   │   4. backupChanges() → store in SQLite │
│   Stores:               │   │   }                                   │
│   - Variables           │   │                                       │
│   - Conversation        │   │   restoreCheckpoint() {               │
│   - Node results        │   │   1. resolveDeltaChain()              │
│   - Trigger state       │   │   2. deleteExtraFiles()               │
│                         │   │   3. restoreFiles()  → copy to Host FS│
│   Persistence:          │   │   4. restoreEmptyDirs()               │
│   checkpoint_metadata   │   │   }                                   │
│   checkpoint_blob       │   │                                       │
│                         │   │   Persistence:                        │
│                         │   │   file_cp_metadata                    │
│                         │   │   file_cp_files                       │
└─────────────────────────┘   └───────────────────────────────────────┘
                                     │
                                     ▼
                          ┌─────────────────────────┐
                          │   Host Filesystem        │
                          │   (workspace directory)  │
                          └─────────────────────────┘
```

---

## Module Layout

```
packages/types/src/storage/
├── file-checkpoint.ts          NEW: type definitions for file checkpoint

packages/storage/src/
├── file-checkpoint/            NEW directory
│   ├── types.ts                Business types
│   ├── file-checkpoint-manager.ts  Core logic
│   └── index.ts                Exports
├── sqlite/
│   ├── sqlite-file-checkpoint-store.ts  NEW: SQLite implementation
│   └── ...
├── index.ts                    Updated exports

sdk/workflow/checkpoint/
├── checkpoint-coordinator.ts   Extended with file checkpoint integration
├── checkpoint-dependencies.ts  NEW: extracted dependency types
```

---

## Type Definitions

### File Checkpoint Metadata (`packages/types/src/storage/file-checkpoint.ts`)

```typescript
/** Single file change record */
export interface FileChangeRecord {
  path: string;      // Relative path from workspace root
  type: 'added' | 'modified' | 'deleted';
  hash: string;      // MD5 hash of file content
}

/** File checkpoint metadata (stored in file_cp_metadata) */
export interface FileCheckpointMetadata {
  entityId: string;             // workflowExecutionId
  timestamp: number;
  type: 'full' | 'incremental';
  baseCheckpointId?: string;    // Base for incremental checkpoints
  changes?: FileChangeRecord[];
  fileCount: number;
  fileHashSnapshot: Record<string, string>;  // Complete hash map
  emptyDirs: string[];          // Empty directory paths
  totalSize: number;
  workspaceRoot: string;
}

/** File checkpoint list options */
export interface FileCheckpointListOptions {
  entityId?: string;
  type?: 'full' | 'incremental';
  timestampFrom?: number;
  timestampTo?: number;
  limit?: number;
  offset?: number;
}

/** File checkpoint storage adapter interface */
export interface FileCheckpointStorageAdapter extends StorageLifecycle {
  save(id: string, metadata: FileCheckpointMetadata, files: Map<string, Buffer>): Promise<void>;
  load(id: string): Promise<{ metadata: FileCheckpointMetadata; files: Map<string, Buffer> } | null>;
  delete(id: string): Promise<void>;
  list(options?: FileCheckpointListOptions): Promise<string[]>;
  listByEntity(entityId: string, options?: { limit?: number }): Promise<Array<{ id: string; metadata: FileCheckpointMetadata }>>;
}
```

---

## Core Logic: FileCheckpointManager

### createCheckpoint()

```
Input: entityId (workflowExecutionId)
Output: { id, metadata }

Steps:
1. Collect workspace files (respecting .gitignore)
   - Recursively scan workspace root
   - Parse .gitignore files at each directory level
   - Skip: .git, node_modules, gitignored files, user-configured ignore patterns

2. Compute MD5 hashes for all collected files
   - Result: Map<relativePath, hash>
   - Also track empty directories

3. Load previous checkpoint's hash snapshot
   - Query latest checkpoint for this entityId
   - If none exists → full backup
   - If exists → incremental

4. Compute delta
   - Compare current hashes with previous snapshot
   - Classify: added, modified, deleted, unchanged

5. Backup changed files
   - For added/modified: read file content from Host FS, store in SQLite
   - For deleted: record in metadata only (no content to store)
   - For unchanged: skip

6. Save checkpoint record
   - Store metadata (type, fileCount, changes, hashSnapshot, etc.)
   - Store file contents in file_cp_files table
```

### restoreCheckpoint()

```
Input: entityId, checkpointId
Output: { restored, deleted, skipped }

Steps:
1. Load checkpoint metadata
2. Resolve delta chain
   - Walk baseCheckpointId links to find the full backup base
   - Collect all incremental checkpoints in chain

3. Compute target file set
   - From chain, determine the complete set of files at target checkpoint
   - File path → latest version in chain (walk chain backward)

4. Scan current workspace state
   - Get current file hashes

5. Compute restore actions
   - Files to delete: in workspace but not in target set
   - Files to restore: different hash between workspace and target
   - Files to skip: same hash

6. Execute restore
   - Delete extra files
   - Copy backup files to workspace
   - Restore empty directories
```

### .gitignore Handling

Follow Lim-Code's recursive .gitignore parsing strategy:

```typescript
private async collectFiles(rootDir: string): Promise<{ files: string[]; dirs: string[] }> {
  const patterns = await this.loadAllGitignorePatterns(rootDir);

  // Hardcoded ignores
  patterns.push('.git');
  patterns.push('node_modules');

  // User-configured ignores from FileCheckpointConfig
  patterns.push(...this.config.customIgnorePatterns);

  return this.scanDirectory(rootDir, rootDir, patterns);
}

private async loadAllGitignorePatterns(rootDir: string): Promise<string[]> {
  const patterns: string[] = [];
  await this.collectGitignoreFiles(rootDir, rootDir, patterns);
  return patterns;
}
```

---

## SQLite Schema

```sql
-- File checkpoint metadata table
CREATE TABLE IF NOT EXISTS file_cp_metadata (
  id TEXT PRIMARY KEY,
  entity_id TEXT NOT NULL,
  timestamp INTEGER NOT NULL,
  type TEXT NOT NULL CHECK(type IN ('full', 'incremental')),
  base_checkpoint_id TEXT,
  file_count INTEGER NOT NULL,
  empty_dirs TEXT DEFAULT '[]',       -- JSON array
  total_size INTEGER NOT NULL DEFAULT 0,
  workspace_root TEXT NOT NULL,
  file_hash_snapshot TEXT NOT NULL,   -- JSON: Record<string, string>
  changes TEXT DEFAULT '[]',          -- JSON: FileChangeRecord[]
  created_at INTEGER NOT NULL
);

-- File checkpoint file content table
CREATE TABLE IF NOT EXISTS file_cp_files (
  checkpoint_id TEXT NOT NULL,
  path TEXT NOT NULL,
  data BLOB,
  hash TEXT,
  is_deleted INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (checkpoint_id, path),
  FOREIGN KEY (checkpoint_id) REFERENCES file_cp_metadata(id) ON DELETE CASCADE
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_file_cp_entity_id ON file_cp_metadata(entity_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_file_cp_base ON file_cp_metadata(base_checkpoint_id);
```

---

## CheckpointCoordinator Integration

### Extended Dependencies

```typescript
export interface CheckpointDependencies {
  // ... existing fields ...

  /** File checkpoint manager (optional) */
  fileCheckpointManager?: FileCheckpointManager;
}
```

### createCheckpoint Flow

```
CheckpointCoordinator.createCheckpoint()
  │
  ├── 1. Extract execution state (existing)
  ├── 2. Save execution checkpoint (existing)
  │
  └── 3. File checkpoint (NEW, non-fatal on failure)
        └── fileCheckpointManager.createCheckpoint(entityId)
              ├── collectFiles() + computeHashes()
              ├── compare with previous checkpoint
              └── store delta in file_cp_*
```

### restoreFromCheckpoint Flow

```
CheckpointCoordinator.restoreFromCheckpoint()
  │
  ├── 1. Restore execution state (existing)
  ├── 2. Rebuild execution entity (existing)
  │
  └── 3. File restore (NEW, non-fatal on failure)
        └── fileCheckpointManager.restoreCheckpoint(entityId, fileCpId)
              ├── resolve delta chain
              ├── delete extra files
              ├── restore backed-up files
              └── restore empty dirs
```

---

## File Editing Tools

**No changes required.** File editing tools continue to write directly to Host FS. The `FileCheckpointManager` captures the resulting file state at checkpoint boundaries through scanning, not interception.

---

## File Filtering (Ignore Patterns)

| Source | Priority | Example |
|--------|----------|---------|
| Hardcoded | Highest | `.git`, `node_modules` |
| `.gitignore` (recursive) | High | `*.log`, `dist/` |
| User config | Medium | Custom patterns from workflow config |
| Workspace root only | Low | Everything else is tracked |

---

## Rollback Safety

File checkpoint restore is designed to be **safe by default**:

1. **Hash verification before restore**: Each restored file's hash is checked against the stored hash.
2. **Delta chain integrity**: All checkpoint directories/files in the chain are verified to exist before restore begins.
3. **Non-fatal errors**: File checkpoint failure does not block execution state checkpoint.
4. **Cancellation safety**: Pending diffs and tool calls are cancelled before restore (following Lim-Code's pattern).

---

## Implementation Phases

| Phase | Scope | Files |
|-------|-------|-------|
| 1 | Type definitions | `packages/types/src/storage/file-checkpoint.ts` |
| 2 | Core logic | `packages/storage/src/file-checkpoint/` |
| 3 | SQLite store | `packages/storage/src/sqlite/sqlite-file-checkpoint-store.ts` |
| 4 | Coordinator integration | `sdk/workflow/checkpoint/checkpoint-coordinator.ts` |
| 5 | DI wiring | `sdk/core/di/container-config.ts` |
| 6 | Integration tests | `packages/storage/__tests__/` |