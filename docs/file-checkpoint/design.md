# File Checkpoint Optimization Design

## Background

The current `FileCheckpointManager` implementation uses a scan-and-compare approach:

1. On each `createCheckpoint()`, recursively scan the entire workspace
2. Compute MD5 hash for all files
3. Compare with previous checkpoint to detect changes
4. Store changed files to storage adapter

This approach has O(N) complexity per checkpoint, where N is the total file count.

## Problems with Shadow Git Alternative

The Roo-Code style shadow git approach was considered but rejected due to:

### 1. Nested Git Repository Issue

Shadow git uses `core.worktree` to point to the workspace directory. If the workspace contains nested `.git` directories (submodules, monorepos with per-package git), git treats them as submodules, causing:

- File changes in nested repos may be incorrectly included/excluded
- `git clean -fd` may destroy nested repo structure
- Roo-Code's solution is to **reject** workspaces with nested git repos entirely

This is unacceptable for agent scenarios where user workspaces commonly have nested repos.

### 2. Initial Storage Bloat

Git stores complete file content as blob objects. Our actual requirement:

```
Initial checkpoint: Only need file hashes (for change detection)
Delta checkpoint:   Only need content of changed files
```

Hash = 32 bytes vs Full file content = potentially MB. Orders of magnitude difference.

### 3. Current Approach is More Flexible

With chokidar + hash baseline + diff engine, we get:
- Full control over storage format
- No external git dependency
- Support for nested repos
- Minimal initial storage

## Optimization Strategy

### Phase 1: Chokidar File Watcher

Replace full workspace scan with incremental file watching.

**Current:**
```
createCheckpoint() → collectFiles(full scan) → hash(all) → compare(all)
```

**Optimized:**
```
chokidar.watch(workspaceRoot)  ← persistent watcher
  → on file change → record to changedFiles: Set<string>

createCheckpoint() → only process changedFiles
  → hash changed files → compare → backup
  → update in-memory baseline hashes
```

**Complexity:** O(N) → O(K), where K = actual changed file count

### Phase 2: Hash Baseline Storage

Separate baseline (hashes only) from delta (content).

```typescript
interface FileBaseline {
  type: "baseline";
  files: Map<string, string>;   // relative path → MD5 hash
  createdAt: number;
}

interface FileDelta {
  type: "delta";
  baseCheckpointId: string;
  changes: {
    added: Array<{ path: string; hash: string; content: Buffer }>;
    modified: Array<{ path: string; oldHash: string; newHash: string; content: Buffer }>;
    deleted: string[];
  };
  createdAt: number;
}
```

**Storage comparison (1000 files, 10 changed):**

| Approach | Initial Storage | Per Delta |
|----------|-----------------|-----------|
| Current | 1000 × full content | 10 × full content |
| Git | 1000 blobs | 10 blobs |
| Hash baseline | 1000 × 32 bytes = 32KB | 10 × full content |

### Phase 3: Diff Engine

Implement Myers/Patience diff algorithm for unified diff output.

Reference: `ref/similar-rs` (Rust implementation)

```typescript
interface DiffChange {
  tag: "equal" | "delete" | "insert";
  value: string;
  oldIndex?: number;
  newIndex?: number;
}

function unifiedDiff(
  oldContent: string,
  newContent: string,
  options?: { context?: number; algorithm?: "myers" | "patience" }
): DiffChange[]
```

**Algorithms:**
- **Myers** (default): O(ND) complexity, best for most text diffs
- **Patience**: Better for code with reordered blocks
- **Histogram**: Alternative with different trade-offs

## Architecture

```
packages/common-utils/src/file-checkpoint/
├── file-watcher.ts           ← chokidar wrapper
├── hash-baseline-store.ts    ← hash baseline storage
├── file-delta-store.ts       ← delta content storage
├── diff-engine.ts            ← Myers/Patience diff
├── file-checkpoint-manager.ts ← public API (CheckpointCoordinator compatible)
└── types.ts
```

## API Compatibility

`CheckpointCoordinator` interface remains unchanged:

```typescript
fileCheckpointManager.createCheckpoint(entityId)   // → snapshot
fileCheckpointManager.restoreCheckpoint(entityId)  // → restore
fileCheckpointManager.getDiff(fromId, toId)        // → DiffChange[] (new)
```

## Implementation Phases

### Phase 1: Chokidar File Watcher

**Goal:** Replace full scan with incremental watching

**Files:**
- `packages/common-utils/src/file-checkpoint/file-watcher.ts`

**Key features:**
- Debounced change events (default 100ms)
- Support for ignore patterns (same as current)
- Handle add/change/unlink events
- Provide `getChangedFiles()` and `reset()` methods

**Dependencies:**
- `chokidar` (already in dependencies)

### Phase 2: Hash Baseline Storage

**Goal:** Store only hashes for baseline, content for deltas

**Files:**
- `packages/common-utils/src/file-checkpoint/hash-baseline-store.ts`
- `packages/common-utils/src/file-checkpoint/file-delta-store.ts`

**Key features:**
- Baseline: `Map<path, hash>` stored as JSON/SQLite
- Delta: Only store content for changed files
- Chain resolution for restore

### Phase 3: Diff Engine

**Goal:** Generate unified diff output

**Files:**
- `packages/common-utils/src/file-checkpoint/diff-engine.ts`

**Key features:**
- Myers algorithm implementation
- Optional Patience algorithm
- Context line configuration
- Unified diff format output

### Phase 4: Integrate into FileCheckpointManager

**Goal:** Refactor existing manager to use new components

**Changes:**
- Replace `collectFiles()` with `fileWatcher.getChangedFiles()`
- Replace full content storage with hash baseline + delta
- Add `getDiff()` method using diff engine

### Phase 5: Tests and Validation

**Goal:** Ensure correctness and performance

**Tests:**
- Unit tests for each new component
- Integration tests with CheckpointCoordinator
- Performance benchmarks vs old implementation

## Migration Path

The refactoring is backward compatible:

1. New `FileCheckpointManager` can read old checkpoint format
2. New checkpoints use new format
3. Gradual migration as new checkpoints are created

## Performance Expectations

| Metric | Current | Optimized |
|--------|---------|-----------|
| Initial checkpoint | O(N) scan + hash | O(N) scan + hash (one-time) |
| Subsequent checkpoint | O(N) scan + hash | O(K) hash only |
| Storage (initial) | N × content size | N × 32 bytes |
| Storage (delta) | K × content size | K × content size |
| Diff generation | Not supported | O(L) where L = line count |

## References

- `ref/roo-code/src/services/checkpoints/` - Shadow git implementation
- `ref/similar-rs/` - Diff algorithm reference
- `docs/arch/file-checkpoint-design.md` - Original design document
