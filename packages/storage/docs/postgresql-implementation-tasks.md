# PostgreSQL Storage Implementation - Remaining Tasks

## Overview

This document outlines the remaining tasks to complete the PostgreSQL storage backend implementation for `@wf-agent/storage`.

**Current Status**: 
- ✅ Phase 1 Foundation: Connection pool and base class completed
- ✅ Checkpoint Storage: Fully implemented
- ⏳ Phase 2-3: 5 more storage implementations needed

---

## Completed Work

### ✅ Phase 1: Foundation (Completed)

1. **Connection Pool Manager** (`src/postgres/connection-pool.ts`)
   - Global connection pool management
   - Per-database pooling with configuration
   - Health monitoring and statistics
   - Graceful shutdown support

2. **Base PostgreSQL Storage** (`src/postgres/base-postgres-storage.ts`)
   - Abstract base class with common functionality
   - Schema initialization and versioning
   - Connection lifecycle management
   - Error handling and metrics
   - Batch operation helpers
   - Integrity verification support

3. **Checkpoint Storage** (`src/postgres/postgres-checkpoint-storage.ts`)
   - Full implementation of `CheckpointStorageAdapter`
   - Metadata-BLOB separation
   - Compression support
   - Filtering and pagination
   - Transaction support

---

## Remaining Tasks

### Task 1: Workflow Storage Implementation

**File**: `src/postgres/postgres-workflow-storage.ts`

**Adapter Interface**: `WorkflowStorageAdapter`

**Requirements**:
1. Implement metadata-BLOB separation schema:
   - `workflow_metadata` table
   - `workflow_blob` table
   - `workflow_versions` table (for version tracking)
   - `workflow_version_blob` table

2. Key methods to implement:
   ```typescript
   - save(workflowId, data, metadata): Promise<void>
   - load(workflowId): Promise<Uint8Array | null>
   - delete(workflowId): Promise<void>
   - list(options?: WorkflowListOptions): Promise<string[]>
   - exists(workflowId): Promise<boolean>
   - getMetadata(workflowId): Promise<WorkflowStorageMetadata | null>
   - updateWorkflowMetadata(workflowId, metadata): Promise<void>
   - saveWorkflowVersion(workflowId, version, data, changeNote): Promise<void>
   - getWorkflowVersions(workflowId): Promise<WorkflowVersionInfo[]>
   - getWorkflowVersion(workflowId, version): Promise<Uint8Array | null>
   ```

3. Schema design considerations:
   - Use JSONB for tags and customFields
   - Index on: workflowId, name, category, enabled, createdAt, updatedAt
   - Foreign key constraints for version tables
   - ON DELETE CASCADE for blob tables

4. Special features:
   - Version management with change notes
   - Partial metadata updates
   - Fuzzy search support for name field

**Reference**: See `src/sqlite/sqlite-workflow-storage.ts` for SQLite implementation patterns

---

### Task 2: Task Storage Implementation

**File**: `src/postgres/postgres-task-storage.ts`

**Adapter Interface**: `TaskStorageAdapter`

**Requirements**:
1. Implement schema:
   - `task_metadata` table
   - `task_blob` table

2. Key methods to implement:
   ```typescript
   - save(taskId, data, metadata): Promise<void>
   - load(taskId): Promise<Uint8Array | null>
   - delete(taskId): Promise<void>
   - list(options?: TaskListOptions): Promise<string[]>
   - exists(taskId): Promise<boolean>
   - getMetadata(taskId): Promise<TaskStorageMetadata | null>
   - getTaskStats(options?: TaskStatsOptions): Promise<TaskStats>
   - cleanupTasks(retentionTime: number): Promise<number>
   ```

3. Schema fields for task_metadata:
   - id (PK), name, status, priority
   - scheduled_time, started_at, completed_at, failed_at
   - retry_count, max_retries, error_message
   - blob_size, blob_hash, tags (JSONB), custom_fields (JSONB)
   - created_at (TIMESTAMP WITH TIME ZONE)

4. Indexes:
   - status (for filtering by status)
   - scheduled_time (for finding pending tasks)
   - priority DESC (for priority-based retrieval)
   - Composite: (status, scheduled_time)

5. Special features:
   - Task statistics aggregation
   - Automatic cleanup of expired tasks
   - Priority-based ordering

**Reference**: See `src/sqlite/sqlite-task-storage.ts`

---

### Task 3: Workflow Execution Storage Implementation

**File**: `src/postgres/postgres-workflow-execution-storage.ts`

**Adapter Interface**: `WorkflowExecutionStorageAdapter`

**Requirements**:
1. Implement schema:
   - `workflow_execution_metadata` table
   - `workflow_execution_blob` table

2. Key methods to implement:
   ```typescript
   - save(executionId, data, metadata): Promise<void>
   - load(executionId): Promise<Uint8Array | null>
   - delete(executionId): Promise<void>
   - list(options?: WorkflowExecutionListOptions): Promise<string[]>
   - exists(executionId): Promise<boolean>
   - getMetadata(executionId): Promise<WorkflowExecutionStorageMetadata | null>
   - updateExecutionStatus(executionId, status): Promise<void>
   ```

3. Schema fields:
   - id (PK), workflow_id, workflow_version
   - status, started_at, completed_at, duration_ms
   - error_message, blob_size, blob_hash
   - custom_fields (JSONB), created_at

4. Indexes:
   - workflow_id (for filtering by workflow)
   - status (for active/completed filtering)
   - started_at (for time-based queries)
   - Composite: (workflow_id, started_at)

5. Special features:
   - Status updates without full rewrite
   - Duration calculation
   - Error message storage

**Reference**: See `src/sqlite/sqlite-workflow-execution-storage.ts`

---

### Task 4: Agent Loop Storage Implementation

**File**: `src/postgres/postgres-agent-loop-storage.ts`

**Adapter Interface**: `AgentLoopStorageAdapter`

**Requirements**:
1. Implement schema:
   - `agent_loop_metadata` table
   - `agent_loop_blob` table

2. Key methods to implement:
   ```typescript
   - save(agentLoopId, data, metadata): Promise<void>
   - load(agentLoopId): Promise<Uint8Array | null>
   - delete(agentLoopId): Promise<void>
   - list(options?: AgentEntityListOptions): Promise<string[]>
   - exists(agentLoopId): Promise<boolean>
   - getMetadata(agentLoopId): Promise<AgentEntityMetadata | null>
   - updateAgentLoopStatus(agentLoopId, status): Promise<void>
   - listByStatus(status: AgentLoopStatus): Promise<string[]>
   - getAgentLoopStats(): Promise<{ total: number; byStatus: Record<string, number> }>
   ```

3. Schema fields:
   - id (PK), agent_id, status
   - current_step, started_at, last_activity_at, completed_at
   - blob_size, blob_hash, tags (JSONB), custom_fields (JSONB)
   - created_at

4. Indexes:
   - agent_id (for filtering by agent)
   - status (for status-based queries)
   - last_activity_at (for finding stale loops)
   - Composite: (agent_id, status)

5. Special features:
   - Status management
   - Activity tracking
   - Statistics aggregation by status

**Reference**: See `src/sqlite/sqlite-agent-loop-storage.ts`

---

### Task 5: Agent Loop Checkpoint Storage Implementation

**File**: `src/postgres/postgres-agent-loop-checkpoint-storage.ts`

**Adapter Interface**: `AgentLoopCheckpointStorageAdapter`

**Requirements**:
1. Implement schema:
   - `agent_loop_checkpoint_metadata` table
   - `agent_loop_checkpoint_blob` table

2. Key methods to implement:
   ```typescript
   - save(checkpointId, data, metadata): Promise<void>
   - load(checkpointId): Promise<Uint8Array | null>
   - delete(checkpointId): Promise<void>
   - list(options?: AgentCheckpointListOptions): Promise<string[]>
   - exists(checkpointId): Promise<boolean>
   - getMetadata(checkpointId): Promise<AgentCheckpointMetadata | null>
   - listByAgentLoop(agentLoopId, options?): Promise<string[]>
   - getLatestCheckpoint(agentLoopId): Promise<string | null>
   - deleteByAgentLoop(agentLoopId): Promise<number>
   ```

3. Schema fields:
   - id (PK), agent_loop_id, timestamp, type
   - version, blob_size, blob_hash
   - tags (JSONB), custom_fields (JSONB)
   - created_at

4. Indexes:
   - agent_loop_id (primary filter)
   - timestamp (for ordering)
   - type (for filtering by checkpoint type)
   - Composite: (agent_loop_id, timestamp) - critical for latest checkpoint queries

5. Special features:
   - Efficient latest checkpoint retrieval
   - Bulk deletion by agent loop
   - Type-based filtering

**Reference**: See `src/sqlite/sqlite-agent-loop-checkpoint-storage.ts`

---

### Task 6: Module Exports and Integration

**Files to Update**:

1. **Create index file**: `src/postgres/index.ts`
   ```typescript
   /**
    * PostgreSQL storage implementation export
    */

   export { 
     BasePostgresStorage, 
     type BasePostgresStorageConfig 
   } from "./base-postgres-storage.js";

   export { PostgresCheckpointStorage } from "./postgres-checkpoint-storage.js";
   export { PostgresWorkflowStorage } from "./postgres-workflow-storage.js";
   export { PostgresTaskStorage } from "./postgres-task-storage.js";
   export { PostgresWorkflowExecutionStorage } from "./postgres-workflow-execution-storage.js";
   export { PostgresAgentLoopStorage } from "./postgres-agent-loop-storage.js";
   export { PostgresAgentLoopCheckpointStorage } from "./postgres-agent-loop-checkpoint-storage.js";

   export {
     PostgresConnectionPool,
     type PostgresPoolConfig,
     getGlobalConnectionPool,
     resetGlobalConnectionPool,
   } from "./connection-pool.js";
   ```

2. **Update main exports**: `src/index.ts`
   Add after line 31:
   ```typescript
   // PostgreSQL Storage Implementation
   export * from "./postgres/index.js";
   ```

---

### Task 7: Testing (Optional for Now)

**Test Files to Create** (when ready):

1. `src/postgres/__tests__/postgres-checkpoint-storage.test.ts`
2. `src/postgres/__tests__/postgres-workflow-storage.test.ts`
3. `src/postgres/__tests__/postgres-task-storage.test.ts`
4. `src/postgres/__tests__/postgres-workflow-execution-storage.test.ts`
5. `src/postgres/__tests__/postgres-agent-loop-storage.test.ts`
6. `src/postgres/__tests__/postgres-agent-loop-checkpoint-storage.test.ts`
7. `src/postgres/__tests__/connection-pool.test.ts`

**Test Setup Requirements**:
- PostgreSQL test database (can use local instance or Docker)
- Environment variable: `TEST_POSTGRES_URL`
- Test isolation: clear tables before each test
- Connection pool cleanup after tests

---

## Implementation Guidelines

### Code Patterns to Follow

1. **Metadata-BLOB Separation**:
   - Always use two tables: `{entity}_metadata` and `{entity}_blob`
   - Metadata table for queries, BLOB table for data
   - Foreign key with ON DELETE CASCADE

2. **Compression**:
   ```typescript
   import { selectCompressionStrategy } from "@wf-agent/common-utils";
   import { compressBlob, decompressBlob } from "@wf-agent/common-utils";

   const config = selectCompressionStrategy(data);
   const { compressed, algorithm } = await compressBlob(data, config);
   ```

3. **Transaction Pattern**:
   ```typescript
   const client = await this.getClient();
   try {
     await client.query('BEGIN');
     // ... operations ...
     await client.query('COMMIT');
   } catch (error) {
     await client.query('ROLLBACK');
     throw error;
   } finally {
     this.releaseClient(client);
   }
   ```

4. **Error Handling**:
   ```typescript
   try {
     // ... operations ...
   } catch (error) {
     return this.handlePostgresError(error, 'operationName', { context });
   } finally {
     this.releaseClient(client);
   }
   ```

5. **Pagination**:
   ```typescript
   const { limit, offset } = this.validatePagination(options?.limit, options?.offset);
   ```

6. **JSONB Fields**:
   ```typescript
   // Store
   tags: metadata.tags ? JSON.stringify(metadata.tags) : null,
   
   // Retrieve
   tags: row.tags ? JSON.parse(row.tags) : undefined,
   ```

### Schema Design Checklist

For each storage implementation, ensure:

- [ ] Metadata table with appropriate fields
- [ ] BLOB table with BYTEA column
- [ ] Foreign key constraint (ON DELETE CASCADE)
- [ ] CHECK constraint for compression fields
- [ ] Strategic indexes for common queries
- [ ] TIMESTAMP WITH TIME ZONE for timestamps
- [ ] JSONB for flexible metadata (tags, customFields)
- [ ] BIGINT for large numbers (timestamps, sizes)

### Common SQL Patterns

**UPSERT (Insert or Update)**:
```sql
INSERT INTO table (id, ...) VALUES ($1, ...)
ON CONFLICT (id) DO UPDATE SET
  field1 = EXCLUDED.field1,
  field2 = EXCLUDED.field2
```

**Batch Delete with ANY**:
```sql
DELETE FROM table WHERE id = ANY($1)
```

**Conditional Query Building**:
```typescript
const conditions: string[] = [];
const params: any[] = [];
let paramIndex = 1;

if (options?.field) {
  conditions.push(`field = $${paramIndex++}`);
  params.push(options.field);
}

const whereClause = conditions.length > 0 
  ? `WHERE ${conditions.join(' AND ')}`
  : '';
```

---

## Priority Order

Recommended implementation order:

1. **High Priority** (Core functionality):
   - Task 1: Workflow Storage (most commonly used)
   - Task 6: Module Exports (to make code usable)

2. **Medium Priority** (Extended functionality):
   - Task 2: Task Storage
   - Task 3: Workflow Execution Storage

3. **Lower Priority** (Agent-specific):
   - Task 4: Agent Loop Storage
   - Task 5: Agent Loop Checkpoint Storage

---

## Estimated Effort

| Task | Complexity | Estimated Time |
|------|-----------|----------------|
| Workflow Storage | High | 2-3 hours |
| Task Storage | Medium | 1.5-2 hours |
| Workflow Execution Storage | Medium | 1.5-2 hours |
| Agent Loop Storage | Medium | 1.5-2 hours |
| Agent Loop Checkpoint Storage | Medium | 1.5-2 hours |
| Module Exports | Low | 15 minutes |
| **Total** | | **8-11 hours** |

---

## Quick Start Template

For each new storage implementation, use this template:

```typescript
/**
 * PostgreSQL {Entity} Storage Implementation
 */

import type { {Entity}Metadata, {Entity}ListOptions } from "@wf-agent/types";
import type { {Entity}StorageAdapter } from "../types/adapter/index.js";
import { BasePostgresStorage, BasePostgresStorageConfig } from "./base-postgres-storage.js";
import { selectCompressionStrategy } from "@wf-agent/common-utils";
import { compressBlob, decompressBlob } from "@wf-agent/common-utils";
import { PoolClient } from 'pg';
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("postgres-{entity}-storage");

export class Postgres{Entity}Storage
  extends BasePostgresStorage<{Entity}Metadata>
  implements {Entity}StorageAdapter
{
  constructor(config: BasePostgresStorageConfig) {
    super(config);
  }

  protected getTableName(): string {
    return "{entity}_metadata";
  }

  protected getBlobTableName(): string | null {
    return "{entity}_blob";
  }

  protected async createTableSchema(client: PoolClient): Promise<void> {
    // Create metadata table
    await client.query(`CREATE TABLE IF NOT EXISTS ...`);
    
    // Create blob table
    await client.query(`CREATE TABLE IF NOT EXISTS ...`);
    
    // Create indexes
    await client.query(`CREATE INDEX IF NOT EXISTS ...`);
  }

  async save(id: string, data: Uint8Array, metadata: {Entity}Metadata): Promise<void> {
    const client = await this.getClient();
    try {
      await client.query('BEGIN');
      await this.saveToClient(client, id, data, metadata);
      await client.query('COMMIT');
    } catch (error) {
      await client.query('ROLLBACK');
      throw error;
    } finally {
      this.releaseClient(client);
    }
  }

  protected async saveToClient(
    client: PoolClient,
    id: string,
    data: Uint8Array,
    metadata: {Entity}Metadata
  ): Promise<void> {
    // Compress and save logic
  }

  async load(id: string): Promise<Uint8Array | null> {
    const client = await this.getClient();
    try {
      // Load and decompress logic
    } finally {
      this.releaseClient(client);
    }
  }

  async delete(id: string): Promise<void> {
    const client = await this.getClient();
    try {
      await this.deleteFromClient(client, id);
    } finally {
      this.releaseClient(client);
    }
  }

  protected async deleteFromClient(client: PoolClient, id: string): Promise<void> {
    await client.query('DELETE FROM {entity}_metadata WHERE id = $1', [id]);
  }

  async list(options?: {Entity}ListOptions): Promise<string[]> {
    const client = await this.getClient();
    try {
      // Build query with filters
      // Return array of IDs
    } finally {
      this.releaseClient(client);
    }
  }

  async exists(id: string): Promise<boolean> {
    const client = await this.getClient();
    try {
      const result = await client.query(
        'SELECT 1 FROM {entity}_metadata WHERE id = $1',
        [id]
      );
      return result.rows.length > 0;
    } finally {
      this.releaseClient(client);
    }
  }

  async getMetadata(id: string): Promise<{Entity}Metadata | null> {
    const client = await this.getClient();
    try {
      const result = await client.query(
        'SELECT * FROM {entity}_metadata WHERE id = $1',
        [id]
      );
      
      if (result.rows.length === 0) {
        return null;
      }
      
      const row = result.rows[0];
      // Parse and return metadata
    } finally {
      this.releaseClient(client);
    }
  }
}
```

---

## Next Steps

1. **Start with Workflow Storage** - Most complex but most important
2. **Test each implementation** as you complete it
3. **Update exports** after completing all implementations
4. **Build the package** to verify no compilation errors
5. **Create basic integration tests** to validate functionality

---

## Resources

- **Design Document**: `docs/postgresql-storage-design.md`
- **SQLite Reference**: `src/sqlite/*.ts` (pattern reference)
- **Type Definitions**: `packages/types/src/storage/*.ts`
- **Adapter Interfaces**: `src/types/adapter/*.ts`

---

## Notes

- All implementations should follow the same patterns as Checkpoint Storage
- Use TypeScript strict mode - no `any` types
- Comprehensive error handling with proper logging
- Support for compression is mandatory
- Transaction safety for all write operations
- Proper resource cleanup in finally blocks

Good luck with the implementation! 🚀
