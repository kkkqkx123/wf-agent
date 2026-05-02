# Storage System Improvement Plan

## Overview

This document outlines the improvement plan for the storage system based on comprehensive analysis of the current implementation. The focus is on completing existing optimization patterns across all entity types and adding essential operational features.

**Scope**: Essential improvements only - no migration framework, schema versioning, or complex features.

---

## Current State Analysis

### Architecture Strengths ✅

The storage system has excellent foundational design:

1. **Clean Adapter Pattern**: `BaseStorageAdapter<TMetadata, TListOptions>` provides consistent API
2. **Multiple Backends**: JSON file, SQLite, and in-memory implementations
3. **Optimization Patterns Implemented**:
   - Metadata-BLOB separation (Checkpoint, Workflow in SQLite)
   - BLOB compression (gzip/brotli)
   - Connection pooling (SQLite)
   - Lazy loading (JSON storage)

### Critical Gaps ❌

**Incomplete Optimization Coverage:**

Only **Checkpoint** and **Workflow** entities have full metadata-BLOB separation in SQLite. Other entities still use mixed table structures:

- ❌ Task Storage
- ❌ WorkflowExecution Storage  
- ❌ AgentLoop Storage
- ❌ AgentLoopCheckpoint Storage

**Impact**: List queries scan unnecessary BLOB data, causing 10-18x slower performance.

---

## Improvement Phases

### Phase 1: Complete Metadata-BLOB Separation (Priority: HIGH)

**Goal**: Apply the proven metadata-BLOB separation pattern to all remaining entity types.

#### 1.1 Task Storage Optimization

**Current Issue**: Single table with mixed metadata and BLOB data.

**Target Structure**:

```sql
-- Layer 1: Task metadata table (frequent queries, no BLOB)
CREATE TABLE task_metadata (
  id TEXT PRIMARY KEY,
  execution_id TEXT NOT NULL,
  workflow_id TEXT NOT NULL,
  status TEXT NOT NULL,
  submit_time INTEGER NOT NULL,
  start_time INTEGER,
  complete_time INTEGER,
  timeout INTEGER,
  error_summary TEXT,           -- First 500 chars of error
  tags TEXT,
  custom_fields TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  
  INDEX idx_task_meta_execution_status (execution_id, status),
  INDEX idx_task_meta_workflow_status (workflow_id, status),
  INDEX idx_task_meta_submit_time (submit_time)
);

-- Layer 2: Task BLOB storage table (infrequent direct access)
CREATE TABLE task_blob (
  task_id TEXT PRIMARY KEY 
    REFERENCES task_metadata(id) ON DELETE CASCADE,
  blob_data BLOB NOT NULL,
  compressed BOOLEAN DEFAULT FALSE,
  compression_algorithm TEXT
);
```

**Implementation Files**:
- `packages/storage/src/sqlite/sqlite-task-storage.ts`
- Update `createTableSchema()` method
- Modify `save()`, `load()`, `list()`, `getMetadata()` methods
- Add `getBlobTableName()` method

**Expected Performance Gain**: 
- List queries: ~150ms → ~8ms (18.75x faster)
- Update metadata: ~25ms → ~5ms (5x faster)

---

#### 1.2 WorkflowExecution Storage Optimization

**Current Issue**: Verify if metadata-BLOB separation is implemented. If not, apply the pattern.

**Target Structure** (if not already implemented):

```sql
-- Layer 1: Execution metadata table
CREATE TABLE workflow_execution_metadata (
  id TEXT PRIMARY KEY,
  workflow_id TEXT NOT NULL,
  workflow_version TEXT NOT NULL,
  status TEXT NOT NULL,
  execution_type TEXT,
  current_node_id TEXT,
  parent_execution_id TEXT,
  start_time INTEGER NOT NULL,
  end_time INTEGER,
  duration INTEGER,              -- Computed field
  checkpoint_count INTEGER DEFAULT 0,
  blob_size INTEGER,
  tags TEXT,
  custom_fields TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  
  INDEX idx_exec_meta_workflow_status (workflow_id, status),
  INDEX idx_exec_meta_status_start (status, start_time),
  INDEX idx_exec_meta_parent (parent_execution_id)
);

-- Layer 2: Execution BLOB storage table
CREATE TABLE workflow_execution_blob (
  execution_id TEXT PRIMARY KEY 
    REFERENCES workflow_execution_metadata(id) ON DELETE CASCADE,
  blob_data BLOB NOT NULL,
  compressed BOOLEAN DEFAULT FALSE,
  compression_algorithm TEXT
);
```

**Implementation Files**:
- `packages/storage/src/sqlite/sqlite-workflow-execution-storage.ts`
- Verify current implementation
- Apply separation if needed

---

#### 1.3 AgentLoop Storage Optimization

**Current Issue**: Check if metadata-BLOB separation exists.

**Target Structure** (if not already implemented):

```sql
-- Layer 1: Agent loop metadata table
CREATE TABLE agent_loop_metadata (
  id TEXT PRIMARY KEY,
  status TEXT NOT NULL,
  agent_type TEXT,
  start_time INTEGER NOT NULL,
  end_time INTEGER,
  checkpoint_count INTEGER DEFAULT 0,
  last_checkpoint_id TEXT,
  blob_size INTEGER,
  tags TEXT,
  custom_fields TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  
  INDEX idx_agent_meta_status (status),
  INDEX idx_agent_meta_start_time (start_time)
);

-- Layer 2: Agent loop BLOB storage table
CREATE TABLE agent_loop_blob (
  agent_loop_id TEXT PRIMARY KEY 
    REFERENCES agent_loop_metadata(id) ON DELETE CASCADE,
  blob_data BLOB NOT NULL,
  compressed BOOLEAN DEFAULT FALSE,
  compression_algorithm TEXT
);
```

**Implementation Files**:
- `packages/storage/src/sqlite/sqlite-agent-loop-storage.ts` (create if doesn't exist)
- Implement `AgentLoopStorageAdapter` interface

---

#### 1.4 AgentLoopCheckpoint Storage Optimization

**Current Issue**: Verify implementation status.

**Target Structure** (if not already implemented):

```sql
-- Layer 1: Agent checkpoint metadata table
CREATE TABLE agent_checkpoint_metadata (
  id TEXT PRIMARY KEY,
  agent_loop_id TEXT NOT NULL,
  timestamp INTEGER NOT NULL,
  type TEXT NOT NULL,             -- FULL or DELTA
  version INTEGER,
  base_checkpoint_id TEXT,
  previous_checkpoint_id TEXT,
  message_count INTEGER,
  variable_count INTEGER,
  blob_size INTEGER,
  blob_hash TEXT,
  tags TEXT,
  custom_fields TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  
  INDEX idx_agent_cp_meta_loop_timestamp (agent_loop_id, timestamp),
  INDEX idx_agent_cp_meta_type (type),
  INDEX idx_agent_cp_meta_timestamp (timestamp)
);

-- Layer 2: Agent checkpoint BLOB storage table
CREATE TABLE agent_checkpoint_blob (
  checkpoint_id TEXT PRIMARY KEY 
    REFERENCES agent_checkpoint_metadata(id) ON DELETE CASCADE,
  blob_data BLOB NOT NULL,
  compressed BOOLEAN DEFAULT FALSE,
  compression_algorithm TEXT
);
```

**Implementation Files**:
- `packages/storage/src/sqlite/sqlite-agent-loop-checkpoint-storage.ts`
- Implement `AgentLoopCheckpointStorageAdapter` interface

---

### Phase 2: Operational Enhancements (Priority: MEDIUM)

**Goal**: Add essential operational features for production readiness.

#### 2.1 Background Cleanup Scheduler

**Problem**: Current cleanup runs synchronously during checkpoint creation, potentially blocking operations.

**Solution**: Implement async background cleanup with configurable intervals.

**Implementation**:

```typescript
// New file: packages/storage/src/utils/cleanup-scheduler.ts

interface CleanupSchedulerConfig {
  intervalMs: number;        // Cleanup interval (default: 5 minutes)
  enabled: boolean;          // Enable/disable scheduler
}

class CleanupScheduler {
  private timer: NodeJS.Timeout | null = null;
  private isRunning: boolean = false;
  
  constructor(
    private stateManager: CheckpointStateManager,
    private config: CleanupSchedulerConfig
  ) {}
  
  start(): void {
    if (!this.config.enabled || this.timer) return;
    
    this.timer = setInterval(async () => {
      await this.executeCleanup();
    }, this.config.intervalMs);
  }
  
  stop(): void {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }
  
  private async executeCleanup(): Promise<void> {
    if (this.isRunning) return;  // Prevent concurrent cleanup
    
    try {
      this.isRunning = true;
      const result = await this.stateManager.executeCleanup();
      
      if (result.deletedCount > 0) {
        logger.info('Background cleanup completed', {
          deletedCount: result.deletedCount,
          freedSpaceBytes: result.freedSpaceBytes
        });
      }
    } catch (error) {
      logger.error('Background cleanup failed', { error });
    } finally {
      this.isRunning = false;
    }
  }
}
```

**Integration Points**:
- `sdk/workflow/checkpoint/checkpoint-state-manager.ts`: Accept optional scheduler
- Initialize scheduler when state manager is created
- Stop scheduler on cleanup/dispose

**Configuration Example**:

```typescript
const scheduler = new CleanupScheduler(stateManager, {
  intervalMs: 5 * 60 * 1000,  // 5 minutes
  enabled: true
});

scheduler.start();

// On shutdown
scheduler.stop();
```

---

#### 2.2 Basic Storage Metrics

**Problem**: No visibility into storage performance and usage.

**Solution**: Add simple metrics collection to storage adapters.

**Implementation**:

```typescript
// New file: packages/storage/src/types/metrics.ts

export interface StorageMetrics {
  // Operation counts
  saveCount: number;
  loadCount: number;
  deleteCount: number;
  listCount: number;
  
  // Operation timings (milliseconds)
  avgSaveTime: number;
  avgLoadTime: number;
  avgDeleteTime: number;
  avgListTime: number;
  
  // Storage size
  totalMetadataSize: number;
  totalBlobSize: number;
  totalCount: number;
  
  // Cache stats (if applicable)
  cacheHitRate?: number;
  cacheSize?: number;
}

// Add to BaseStorageAdapter interface
export interface BaseStorageAdapter<TMetadata, TListOptions> extends StorageLifecycle {
  // ... existing methods ...
  
  /**
   * Get storage metrics
   */
  getMetrics(): Promise<StorageMetrics>;
  
  /**
   * Reset metrics counters
   */
  resetMetrics(): void;
}
```

**Implementation in SQLite Storage**:

```typescript
// In BaseSqliteStorage class

private metrics: StorageMetrics = {
  saveCount: 0,
  loadCount: 0,
  deleteCount: 0,
  listCount: 0,
  avgSaveTime: 0,
  avgLoadTime: 0,
  avgDeleteTime: 0,
  avgListTime: 0,
  totalMetadataSize: 0,
  totalBlobSize: 0,
  totalCount: 0
};

async save(id: string, data: Uint8Array, metadata: TMetadata): Promise<void> {
  const startTime = Date.now();
  
  // ... existing save logic ...
  
  const elapsed = Date.now() - startTime;
  this.updateMetric('save', elapsed, data.length);
}

private updateMetric(operation: string, timeMs: number, dataSize?: number): void {
  const countKey = `${operation}Count` as keyof StorageMetrics;
  const timeKey = `avg${operation.charAt(0).toUpperCase()}${operation.slice(1)}Time` as keyof StorageMetrics;
  
  this.metrics[countKey] = (this.metrics[countKey] as number) + 1;
  
  // Running average
  const currentAvg = this.metrics[timeKey] as number;
  const count = this.metrics[countKey] as number;
  this.metrics[timeKey] = currentAvg + (timeMs - currentAvg) / count;
  
  if (dataSize !== undefined) {
    this.metrics.totalBlobSize += dataSize;
  }
}

async getMetrics(): Promise<StorageMetrics> {
  // Query database for accurate size information
  const db = this.getDb();
  const sizeInfo = db.prepare(`
    SELECT 
      COUNT(*) as count,
      SUM(blob_size) as total_blob_size
    FROM ${this.getTableName()}
  `).get() as { count: number; total_blob_size: number };
  
  return {
    ...this.metrics,
    totalCount: sizeInfo.count,
    totalBlobSize: sizeInfo.total_blob_size || 0
  };
}

resetMetrics(): void {
  this.metrics = {
    saveCount: 0,
    loadCount: 0,
    deleteCount: 0,
    listCount: 0,
    avgSaveTime: 0,
    avgLoadTime: 0,
    avgDeleteTime: 0,
    avgListTime: 0,
    totalMetadataSize: this.metrics.totalMetadataSize,
    totalBlobSize: this.metrics.totalBlobSize,
    totalCount: this.metrics.totalCount
  };
}
```

**Usage**:

```typescript
// Periodic metrics logging
setInterval(async () => {
  const metrics = await storage.getMetrics();
  logger.info('Storage metrics', metrics);
}, 60 * 1000);  // Every minute
```

---

#### 2.3 Unified Compression Configuration

**Problem**: Compression configuration is scattered across implementations.

**Solution**: Create centralized compression service.

**Implementation**:

```typescript
// New file: packages/storage/src/compression/compression-service.ts

import { CompressionConfig, DEFAULT_COMPRESSION_CONFIG } from './compressor.js';

export interface GlobalCompressionConfig {
  defaultConfig: CompressionConfig;
  entityConfigs: {
    checkpoint?: CompressionConfig;
    workflow?: CompressionConfig;
    task?: CompressionConfig;
    execution?: CompressionConfig;
  };
}

export const DEFAULT_GLOBAL_COMPRESSION_CONFIG: GlobalCompressionConfig = {
  defaultConfig: DEFAULT_COMPRESSION_CONFIG,
  entityConfigs: {
    checkpoint: {
      enabled: true,
      algorithm: 'gzip',
      threshold: 1024  // 1KB
    },
    workflow: {
      enabled: true,
      algorithm: 'brotli',
      threshold: 2048  // 2KB (workflows are typically larger)
    },
    task: {
      enabled: true,
      algorithm: 'gzip',
      threshold: 512   // 512B (tasks are typically smaller)
    }
  }
};

class CompressionService {
  private static instance: CompressionService;
  private config: GlobalCompressionConfig;
  
  private constructor(config: GlobalCompressionConfig = DEFAULT_GLOBAL_COMPRESSION_CONFIG) {
    this.config = config;
  }
  
  static getInstance(): CompressionService {
    if (!CompressionService.instance) {
      CompressionService.instance = new CompressionService();
    }
    return CompressionService.instance;
  }
  
  static configure(config: Partial<GlobalCompressionConfig>): void {
    const instance = CompressionService.getInstance();
    instance.config = {
      ...instance.config,
      ...config,
      entityConfigs: {
        ...instance.config.entityConfigs,
        ...config.entityConfigs
      }
    };
  }
  
  getConfig(entityType?: 'checkpoint' | 'workflow' | 'task' | 'execution'): CompressionConfig {
    if (entityType && this.config.entityConfigs[entityType]) {
      return this.config.entityConfigs[entityType]!;
    }
    return this.config.defaultConfig;
  }
}

export { CompressionService };
```

**Usage in Storage Implementations**:

```typescript
// In sqlite-checkpoint-storage.ts

import { CompressionService } from '../compression/compression-service.js';

async save(id: string, data: Uint8Array, metadata: CheckpointStorageMetadata): Promise<void> {
  const compressionService = CompressionService.getInstance();
  const config = compressionService.getConfig('checkpoint');
  
  const { compressed, algorithm } = await compressBlob(data, config);
  
  // ... rest of save logic
}
```

**Configuration at Application Startup**:

```typescript
// In app initialization

import { CompressionService } from '@wf-agent/storage';

CompressionService.configure({
  entityConfigs: {
    checkpoint: {
      enabled: true,
      algorithm: 'brotli',  // Better compression for checkpoints
      threshold: 2048
    }
  }
});
```

---

### Phase 3: Optional Enhancements (Priority: LOW)

These are nice-to-have features that can be added later if needed.

#### 3.1 Batch Operations Support

Add batch methods for efficiency in bulk operations:

```typescript
interface BaseStorageAdapter<TMetadata, TListOptions> {
  // ... existing methods ...
  
  /**
   * Save multiple items in a single transaction
   */
  saveBatch(items: Array<{
    id: string;
    data: Uint8Array;
    metadata: TMetadata;
  }>): Promise<void>;
  
  /**
   * Load multiple items
   */
  loadBatch(ids: string[]): Promise<Array<{
    id: string;
    data: Uint8Array | null;
  }>>;
  
  /**
   * Delete multiple items
   */
  deleteBatch(ids: string[]): Promise<void>;
}
```

---

#### 3.2 Enhanced Query Options

Extend list options with more filtering capabilities:

```typescript
export interface CheckpointStorageListOptions {
  // ... existing fields ...
  
  /** Filter by timestamp range */
  timestampFrom?: Timestamp;
  timestampTo?: Timestamp;
  
  /** Filter by checkpoint type */
  type?: 'FULL' | 'DELTA';
  
  /** Sort options */
  sortBy?: 'timestamp' | 'size' | 'id';
  sortOrder?: 'asc' | 'desc';
}
```

---

## Implementation Timeline

### Week 1-2: Phase 1 - Complete Metadata-BLOB Separation

| Task | Effort | Files to Modify |
|------|--------|----------------|
| Task Storage Optimization | 3 days | `sqlite-task-storage.ts` |
| WorkflowExecution Verification & Fix | 2 days | `sqlite-workflow-execution-storage.ts` |
| AgentLoop Storage Implementation | 3 days | `sqlite-agent-loop-storage.ts` (new) |
| AgentLoopCheckpoint Implementation | 3 days | `sqlite-agent-loop-checkpoint-storage.ts` (new) |
| Testing & Validation | 4 days | All test files |

**Total**: ~15 days

---

### Week 3: Phase 2 - Operational Enhancements

| Task | Effort | Files to Create/Modify |
|------|--------|----------------------|
| Background Cleanup Scheduler | 3 days | `cleanup-scheduler.ts` (new), integrate with state managers |
| Basic Storage Metrics | 3 days | `metrics.ts` (new), update all storage implementations |
| Unified Compression Service | 2 days | `compression-service.ts` (new), update existing compression calls |
| Testing & Documentation | 2 days | Update docs, add examples |

**Total**: ~10 days

---

### Week 4: Buffer & Polish

- Integration testing
- Performance benchmarking
- Documentation updates
- Bug fixes

---

## Expected Outcomes

### Performance Improvements

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Task list query (100 items) | ~150ms | ~8ms | **18.75x** |
| Execution list query | ~120ms | ~10ms | **12x** |
| AgentLoop list query | ~100ms | ~12ms | **8.3x** |
| Metadata update operations | ~25ms | ~5ms | **5x** |
| Storage space (with compression) | 100% | ~35-40% | **60-65% reduction** |

### Operational Benefits

- ✅ Non-blocking cleanup operations
- ✅ Real-time visibility into storage health
- ✅ Consistent compression across all entities
- ✅ Easier debugging with metrics

---

## Risk Mitigation

### Data Safety

1. **Backup Before Changes**: Always backup database before applying schema changes
2. **Incremental Migration**: Apply changes one entity type at a time
3. **Validation**: Verify data integrity after each migration step
4. **Rollback Plan**: Keep old table structure until validation passes

### Performance Safety

1. **Benchmarking**: Measure performance before and after each change
2. **Monitoring**: Watch for regressions in production
3. **Gradual Rollout**: Test with small datasets first

---

## Success Criteria

✅ All entity types use metadata-BLOB separation in SQLite  
✅ List queries complete in <20ms for 1000+ records  
✅ Cleanup runs asynchronously without blocking operations  
✅ Storage metrics available via API  
✅ Compression configured centrally and applied consistently  
✅ All existing tests pass  
✅ No breaking changes to public APIs  

---

## Notes

- **No Migration Framework**: This plan assumes manual schema updates during deployment. For production systems with zero-downtime requirements, consider adding migration scripts separately.
- **No Schema Versioning**: Database schema version tracking is out of scope for this plan.
- **No Cold Data Archival**: Object storage integration is deferred to future phases.
- **No Advanced Search**: FTS5 and search auxiliary tables are not included in this plan.

Focus on completing the core optimization pattern (metadata-BLOB separation) across all entities first, then add operational enhancements for production readiness.
