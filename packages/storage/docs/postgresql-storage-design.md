# PostgreSQL Storage Backend Design

## Overview

This document outlines the design for adding PostgreSQL database support to the `@wf-agent/storage` package. The implementation will follow the existing architecture patterns established by SQLite and JSON storage backends, providing a production-ready relational database option with advanced features like connection pooling, transactions, and high availability.

**Design Goals**:
- Maintain API compatibility with existing storage adapters
- Leverage PostgreSQL's advanced features (connection pooling, ACID transactions, concurrent access)
- Follow the metadata-BLOB separation pattern for optimal performance
- Support schema migration and versioning
- Provide comprehensive error handling and logging

---

## Architecture

### Design Principles

1. **Adapter Pattern Consistency**: PostgreSQL implementations will implement the same adapter interfaces as SQLite/JSON
2. **Metadata-BLOB Separation**: Separate tables for metadata (frequent queries) and BLOB data (infrequent access)
3. **Connection Pooling**: Use pg-pool for efficient connection management
4. **Schema Versioning**: Built-in migration support with version tracking
5. **Compression Support**: Integrate with existing compression utilities
6. **Batch Operations**: Optimize bulk operations using PostgreSQL's transaction capabilities

### Module Structure

```
packages/storage/src/postgres/
├── index.ts                          # Public exports
├── base-postgres-storage.ts          # Abstract base class
├── connection-pool.ts                # Connection pool management
├── postgres-checkpoint-storage.ts    # Checkpoint implementation
├── postgres-workflow-storage.ts      # Workflow implementation
├── postgres-task-storage.ts          # Task implementation
├── postgres-workflow-execution-storage.ts  # Workflow execution implementation
├── postgres-agent-loop-storage.ts    # Agent loop implementation
└── postgres-agent-loop-checkpoint-storage.ts  # Agent loop checkpoint implementation
```

---

## Core Components

### 1. Base PostgreSQL Storage (`base-postgres-storage.ts`)

**Purpose**: Abstract base class providing common PostgreSQL functionality

**Key Features**:
- Connection pool management
- Schema initialization and migration
- Statement preparation and caching
- Error handling and logging
- Metrics collection
- Batch operation helpers

**Interface**:
```typescript
export interface BasePostgresStorageConfig {
  /** PostgreSQL connection string or config */
  connectionString: string;
  
  /** Connection pool configuration */
  poolConfig?: PoolConfig;
  
  /** Whether to enable logging */
  enableLogging?: boolean;
  
  /** Schema version for migration support (default: 1) */
  schemaVersion?: number;
  
  /** Enable data integrity verification on load (default: false) */
  verifyIntegrity?: boolean;
  
  /** Verify integrity every Nth load operation (default: 100) */
  integrityCheckFrequency?: number;
}

export abstract class BasePostgresStorage<TMetadata> {
  protected pool: Pool | null = null;
  protected initialized: boolean = false;
  protected metrics: StorageMetrics;
  
  // Abstract methods to be implemented by subclasses
  protected abstract getTableName(): string;
  protected abstract getBlobTableName(): string | null;
  protected abstract createTableSchema(client: PoolClient): Promise<void>;
  protected abstract migrateSchema(
    client: PoolClient, 
    fromVersion: number, 
    toVersion: number
  ): Promise<void>;
  
  // Common methods
  async initialize(): Promise<void>;
  async close(): Promise<void>;
  async clear(): Promise<void>;
  async optimize(): Promise<void>;
  
  // CRUD operations (can be overridden)
  async save(id: string, data: Uint8Array, metadata: TMetadata): Promise<void>;
  async load(id: string): Promise<Uint8Array | null>;
  async delete(id: string): Promise<void>;
  async exists(id: string): Promise<boolean>;
  async list(options?: any): Promise<string[]>;
  async getMetadata(id: string): Promise<TMetadata | null>;
  
  // Batch operations
  async saveBatch(items: Array<{id: string; data: Uint8Array; metadata: TMetadata}>): Promise<void>;
  async loadBatch(ids: string[]): Promise<Array<{id: string; data: Uint8Array | null}>>;
  async deleteBatch(ids: string[]): Promise<void>;
  
  // Utilities
  protected getClient(): Promise<PoolClient>;
  protected releaseClient(client: PoolClient): void;
  protected computeHash(data: Uint8Array): Promise<string>;
  protected updateMetric(operation: string, timeMs: number, dataSize?: number): void;
}
```

**Key Implementation Details**:

1. **Connection Management**:
   - Use `pg-pool` for connection pooling
   - Automatic connection acquisition/release
   - Transaction support with proper cleanup

2. **Schema Initialization**:
   ```sql
   CREATE TABLE IF NOT EXISTS _schema_versions (
     table_name TEXT PRIMARY KEY,
     version INTEGER NOT NULL,
     updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
   );
   ```

3. **Statement Caching**:
   - PostgreSQL supports prepared statements natively
   - Cache prepared statements per connection
   - Clear cache on schema changes

4. **Error Handling**:
   - Map PostgreSQL errors to `StorageError` types
   - Handle connection failures gracefully
   - Retry logic for transient errors

---

### 2. Connection Pool Manager (`connection-pool.ts`)

**Purpose**: Manage PostgreSQL connection pools with global sharing support

**Key Features**:
- Global connection pool registry
- Per-database connection pooling
- Pool lifecycle management
- Health checking and reconnection

**Interface**:
```typescript
export interface PostgresPoolConfig {
  max?: number;              // Max connections (default: 20)
  min?: number;              // Min connections (default: 1)
  idleTimeoutMillis?: number; // Idle timeout (default: 30000)
  connectionTimeoutMillis?: number; // Connection timeout (default: 5000)
  maxUses?: number;          // Max uses per connection (default: Infinity)
}

export class PostgresConnectionPool {
  private pools: Map<string, Pool> = new Map();
  
  getPool(connectionString: string, config?: PostgresPoolConfig): Pool;
  releasePool(connectionString: string): void;
  closeAll(): Promise<void>;
  getPoolStats(): Map<string, PoolStats>;
}

// Global pool instance
export function getGlobalConnectionPool(): PostgresConnectionPool;
export function resetGlobalConnectionPool(): void;
```

**Implementation Notes**:
- Singleton pattern for global pool
- Connection string as pool key
- Automatic pool creation on first request
- Graceful shutdown support

---

### 3. Storage Implementations

Each storage type implements its specific adapter interface while inheriting from `BasePostgresStorage`.

#### A. PostgreSQL Checkpoint Storage

**Adapter**: `CheckpointStorageAdapter`

**Schema**:
```sql
-- Metadata table
CREATE TABLE checkpoint_metadata (
  id TEXT PRIMARY KEY,
  thread_id TEXT NOT NULL,
  checkpoint_ns TEXT NOT NULL DEFAULT '',
  parent_checkpoint_id TEXT,
  timestamp BIGINT NOT NULL,
  blob_size INTEGER,
  blob_hash TEXT,
  tags TEXT,
  custom_fields JSONB,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- BLOB table
CREATE TABLE checkpoint_blob (
  checkpoint_id TEXT PRIMARY KEY REFERENCES checkpoint_metadata(id) ON DELETE CASCADE,
  blob_data BYTEA NOT NULL,
  compressed BOOLEAN DEFAULT FALSE,
  compression_algorithm TEXT,
  CHECK (
    (compressed = FALSE AND compression_algorithm IS NULL) OR
    (compressed = TRUE AND compression_algorithm IS NOT NULL)
  )
);

-- Indexes
CREATE INDEX idx_cp_meta_thread ON checkpoint_metadata(thread_id);
CREATE INDEX idx_cp_meta_timestamp ON checkpoint_metadata(timestamp);
CREATE INDEX idx_cp_meta_thread_ns ON checkpoint_metadata(thread_id, checkpoint_ns);
```

**Key Methods**:
- `save(checkpointId, data, metadata)` - Save with compression
- `load(checkpointId)` - Load with decompression
- `list(options)` - List with filtering (thread_id, namespace, etc.)
- `deleteByThread(threadId)` - Delete all checkpoints for a thread

---

#### B. PostgreSQL Workflow Storage

**Adapter**: `WorkflowStorageAdapter`

**Schema**:
```sql
-- Metadata table
CREATE TABLE workflow_metadata (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  description TEXT,
  version TEXT NOT NULL,
  status TEXT NOT NULL,
  category TEXT,
  tags TEXT,
  blob_size INTEGER,
  blob_hash TEXT,
  custom_fields JSONB,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
  updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- BLOB table
CREATE TABLE workflow_blob (
  workflow_id TEXT PRIMARY KEY REFERENCES workflow_metadata(id) ON DELETE CASCADE,
  blob_data BYTEA NOT NULL,
  compressed BOOLEAN DEFAULT FALSE,
  compression_algorithm TEXT
);

-- Version tracking
CREATE TABLE workflow_versions (
  id SERIAL PRIMARY KEY,
  workflow_id TEXT NOT NULL REFERENCES workflow_metadata(id) ON DELETE CASCADE,
  version TEXT NOT NULL,
  change_note TEXT,
  blob_size INTEGER,
  blob_hash TEXT,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
  UNIQUE(workflow_id, version)
);

CREATE TABLE workflow_version_blob (
  version_id INTEGER PRIMARY KEY REFERENCES workflow_versions(id) ON DELETE CASCADE,
  blob_data BYTEA NOT NULL,
  compressed BOOLEAN DEFAULT FALSE,
  compression_algorithm TEXT
);

-- Indexes
CREATE INDEX idx_wf_meta_status ON workflow_metadata(status);
CREATE INDEX idx_wf_meta_category ON workflow_metadata(category);
CREATE INDEX idx_wf_ver_workflow ON workflow_versions(workflow_id);
```

**Key Methods**:
- `saveWorkflowVersion(workflowId, version, data, changeNote)` - Version management
- `getWorkflowVersions(workflowId)` - List versions
- `updateWorkflowMetadata(workflowId, metadata)` - Partial updates

---

#### C. PostgreSQL Task Storage

**Adapter**: `TaskStorageAdapter`

**Schema**:
```sql
-- Metadata table
CREATE TABLE task_metadata (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  status TEXT NOT NULL,
  priority INTEGER DEFAULT 0,
  scheduled_time BIGINT,
  started_at BIGINT,
  completed_at BIGINT,
  failed_at BIGINT,
  retry_count INTEGER DEFAULT 0,
  max_retries INTEGER DEFAULT 3,
  error_message TEXT,
  blob_size INTEGER,
  blob_hash TEXT,
  tags TEXT,
  custom_fields JSONB,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- BLOB table
CREATE TABLE task_blob (
  task_id TEXT PRIMARY KEY REFERENCES task_metadata(id) ON DELETE CASCADE,
  blob_data BYTEA NOT NULL,
  compressed BOOLEAN DEFAULT FALSE,
  compression_algorithm TEXT
);

-- Indexes
CREATE INDEX idx_task_meta_status ON task_metadata(status);
CREATE INDEX idx_task_meta_scheduled ON task_metadata(scheduled_time);
CREATE INDEX idx_task_meta_priority ON task_metadata(priority DESC);
```

**Key Methods**:
- `getTaskStats(options)` - Task statistics
- `cleanupTasks(retentionTime)` - Clean expired tasks
- `listByStatus(status)` - Filter by status

---

#### D. PostgreSQL Workflow Execution Storage

**Adapter**: `WorkflowExecutionStorageAdapter`

**Schema**:
```sql
-- Metadata table
CREATE TABLE workflow_execution_metadata (
  id TEXT PRIMARY KEY,
  workflow_id TEXT NOT NULL,
  workflow_version TEXT,
  status TEXT NOT NULL,
  started_at BIGINT,
  completed_at BIGINT,
  duration_ms BIGINT,
  error_message TEXT,
  blob_size INTEGER,
  blob_hash TEXT,
  custom_fields JSONB,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- BLOB table
CREATE TABLE workflow_execution_blob (
  execution_id TEXT PRIMARY KEY REFERENCES workflow_execution_metadata(id) ON DELETE CASCADE,
  blob_data BYTEA NOT NULL,
  compressed BOOLEAN DEFAULT FALSE,
  compression_algorithm TEXT
);

-- Indexes
CREATE INDEX idx_we_meta_workflow ON workflow_execution_metadata(workflow_id);
CREATE INDEX idx_we_meta_status ON workflow_execution_metadata(status);
CREATE INDEX idx_we_meta_started ON workflow_execution_metadata(started_at);
```

**Key Methods**:
- `updateExecutionStatus(executionId, status)` - Status updates
- `listByWorkflow(workflowId)` - Filter by workflow

---

#### E. PostgreSQL Agent Loop Storage

**Adapter**: `AgentLoopStorageAdapter`

**Schema**:
```sql
-- Metadata table
CREATE TABLE agent_loop_metadata (
  id TEXT PRIMARY KEY,
  agent_id TEXT NOT NULL,
  status TEXT NOT NULL,
  current_step TEXT,
  started_at BIGINT,
  last_activity_at BIGINT,
  completed_at BIGINT,
  blob_size INTEGER,
  blob_hash TEXT,
  tags TEXT,
  custom_fields JSONB,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- BLOB table
CREATE TABLE agent_loop_blob (
  agent_loop_id TEXT PRIMARY KEY REFERENCES agent_loop_metadata(id) ON DELETE CASCADE,
  blob_data BYTEA NOT NULL,
  compressed BOOLEAN DEFAULT FALSE,
  compression_algorithm TEXT
);

-- Indexes
CREATE INDEX idx_al_meta_agent ON agent_loop_metadata(agent_id);
CREATE INDEX idx_al_meta_status ON agent_loop_metadata(status);
CREATE INDEX idx_al_meta_activity ON agent_loop_metadata(last_activity_at);
```

**Key Methods**:
- `updateAgentLoopStatus(agentLoopId, status)` - Status management
- `listByStatus(status)` - Filter by status
- `getAgentLoopStats()` - Statistics

---

#### F. PostgreSQL Agent Loop Checkpoint Storage

**Adapter**: `AgentLoopCheckpointStorageAdapter`

**Schema**:
```sql
-- Metadata table
CREATE TABLE agent_loop_checkpoint_metadata (
  id TEXT PRIMARY KEY,
  agent_loop_id TEXT NOT NULL,
  timestamp BIGINT NOT NULL,
  type TEXT NOT NULL,
  version INTEGER,
  blob_size INTEGER,
  blob_hash TEXT,
  tags TEXT,
  custom_fields JSONB,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- BLOB table
CREATE TABLE agent_loop_checkpoint_blob (
  checkpoint_id TEXT PRIMARY KEY REFERENCES agent_loop_checkpoint_metadata(id) ON DELETE CASCADE,
  blob_data BYTEA NOT NULL,
  compressed BOOLEAN DEFAULT FALSE,
  compression_algorithm TEXT
);

-- Indexes
CREATE INDEX idx_alcp_meta_agent ON agent_loop_checkpoint_metadata(agent_loop_id);
CREATE INDEX idx_alcp_meta_timestamp ON agent_loop_checkpoint_metadata(timestamp);
CREATE INDEX idx_alcp_meta_type ON agent_loop_checkpoint_metadata(type);
CREATE INDEX idx_alcp_meta_agent_ts ON agent_loop_checkpoint_metadata(agent_loop_id, timestamp);
```

**Key Methods**:
- `listByAgentLoop(agentLoopId, options)` - Filter by agent loop
- `getLatestCheckpoint(agentLoopId)` - Get latest checkpoint
- `deleteByAgentLoop(agentLoopId)` - Delete all checkpoints

---

## Migration Strategy

### Schema Versioning

Each storage implementation tracks its schema version in `_schema_versions` table:

```typescript
protected async migrateSchema(
  client: PoolClient,
  fromVersion: number,
  toVersion: number
): Promise<void> {
  if (fromVersion < 2 && toVersion >= 2) {
    // Migration v1 -> v2: Add new column
    await client.query('ALTER TABLE checkpoint_metadata ADD COLUMN IF NOT EXISTS tags TEXT');
  }
  
  if (fromVersion < 3 && toVersion >= 3) {
    // Migration v2 -> v3: Create new index
    await client.query('CREATE INDEX IF NOT EXISTS idx_new ON ...');
  }
}
```

### Backward Compatibility

- New columns use `ADD COLUMN IF NOT EXISTS`
- New indexes use `CREATE INDEX IF NOT EXISTS`
- Data migrations handled in transactions
- Rollback support for critical migrations

---

## Performance Optimizations

### 1. Connection Pooling

```typescript
const poolConfig: PostgresPoolConfig = {
  max: 20,                    // Max connections
  min: 2,                     // Min idle connections
  idleTimeoutMillis: 30000,   // 30s idle timeout
  connectionTimeoutMillis: 5000, // 5s connection timeout
  maxUses: 10000,             // Recycle after 10k uses
};
```

**Benefits**:
- Reuse connections across requests
- Avoid connection overhead
- Better resource utilization

### 2. Prepared Statements

```typescript
// Prepare once, execute many times
const stmt = await client.prepare('SELECT * FROM table WHERE id = $1');
const result = await stmt.execute([id]);
```

**Benefits**:
- Query plan caching
- SQL injection prevention
- Reduced parsing overhead

### 3. Batch Operations

```typescript
// Use COPY for bulk inserts (PostgreSQL-specific optimization)
await client.query('COPY table FROM STDIN WITH (FORMAT binary)');
```

**Benefits**:
- Significantly faster than individual INSERTs
- Reduced network round-trips
- Lower transaction overhead

### 4. Index Optimization

Strategic indexes for common query patterns:
- Foreign keys for JOIN operations
- Composite indexes for multi-column filters
- Partial indexes for filtered queries

### 5. Query Optimization

- Use `EXISTS` instead of `COUNT(*)` for existence checks
- Select only needed columns (avoid `SELECT *`)
- Use pagination for large result sets
- Leverage PostgreSQL's JSONB operators for metadata queries

---

## Error Handling

### Error Mapping

```typescript
protected handlePostgresError(
  error: unknown,
  operation: string,
  context?: Record<string, unknown>
): never {
  if (error instanceof DatabaseError) {
    switch (error.code) {
      case '23505': // unique_violation
        throw new StorageError('Duplicate entry', operation, context, error);
      case '23503': // foreign_key_violation
        throw new StorageError('Referential integrity violation', operation, context, error);
      case '08006': // connection_failure
        throw new StorageError('Connection failed', operation, context, error);
      default:
        throw new StorageError(`PostgreSQL error [${error.code}]`, operation, context, error);
    }
  }
  
  throw new StorageError(`Operation failed: ${operation}`, operation, context, error as Error);
}
```

### Retry Logic

For transient errors (connection timeouts, deadlocks):
```typescript
async function withRetry<T>(
  fn: () => Promise<T>,
  maxRetries: number = 3,
  delayMs: number = 100
): Promise<T> {
  for (let i = 0; i < maxRetries; i++) {
    try {
      return await fn();
    } catch (error) {
      if (i === maxRetries - 1 || !isTransientError(error)) {
        throw error;
      }
      await sleep(delayMs * Math.pow(2, i)); // Exponential backoff
    }
  }
}
```

---

## Testing Strategy

### Unit Tests

Test each storage implementation independently:
- CRUD operations
- Batch operations
- Error handling
- Edge cases (empty data, large data, etc.)

### Integration Tests

Test with real PostgreSQL instance:
- Connection pooling behavior
- Concurrent access
- Transaction isolation
- Schema migrations

### Performance Tests

Benchmark operations:
- Single vs batch operations
- Compression impact
- Connection pool sizing
- Query performance with large datasets

### Test Setup

```typescript
// Use docker-compose for test database
describe('PostgresCheckpointStorage', () => {
  let storage: PostgresCheckpointStorage;
  
  beforeAll(async () => {
    storage = new PostgresCheckpointStorage({
      connectionString: process.env.TEST_DATABASE_URL || 'postgresql://test:test@localhost:5432/test',
    });
    await storage.initialize();
  });
  
  afterAll(async () => {
    await storage.close();
  });
  
  beforeEach(async () => {
    await storage.clear();
  });
  
  // Tests...
});
```

---

## Configuration Examples

### Basic Usage

```typescript
import { PostgresCheckpointStorage } from '@wf-agent/storage/postgres';

const storage = new PostgresCheckpointStorage({
  connectionString: 'postgresql://user:password@localhost:5432/wf_agent',
});

await storage.initialize();

// Use storage...
await storage.save(checkpointId, data, metadata);

await storage.close();
```

### Advanced Configuration

```typescript
import { PostgresWorkflowStorage } from '@wf-agent/storage/postgres';

const storage = new PostgresWorkflowStorage({
  connectionString: 'postgresql://user:password@localhost:5432/wf_agent',
  poolConfig: {
    max: 30,
    min: 5,
    idleTimeoutMillis: 60000,
    connectionTimeoutMillis: 10000,
  },
  schemaVersion: 2,
  verifyIntegrity: true,
  integrityCheckFrequency: 50,
  enableLogging: true,
});
```

### Environment Variables

```bash
# .env
POSTGRES_CONNECTION_STRING=postgresql://user:password@localhost:5432/wf_agent
POSTGRES_POOL_MAX=20
POSTGRES_POOL_MIN=2
POSTGRES_SCHEMA_VERSION=1
```

```typescript
const storage = new PostgresCheckpointStorage({
  connectionString: process.env.POSTGRES_CONNECTION_STRING!,
  poolConfig: {
    max: parseInt(process.env.POSTGRES_POOL_MAX || '20'),
    min: parseInt(process.env.POSTGRES_POOL_MIN || '2'),
  },
});
```

---

## Deployment Considerations

### Database Setup

1. **Create Database**:
```sql
CREATE DATABASE wf_agent;
CREATE USER wf_agent_user WITH PASSWORD 'secure_password';
GRANT ALL PRIVILEGES ON DATABASE wf_agent TO wf_agent_user;
```

2. **Schema Initialization**:
- Automatic on first `initialize()` call
- Or run migration scripts manually

3. **Connection String Format**:
```
postgresql://[user[:password]@][host][:port][/dbname][?param1=value1&...]
```

### High Availability

- Use PostgreSQL replication for read replicas
- Configure connection pool for failover
- Implement health checks

### Monitoring

Track key metrics:
- Connection pool utilization
- Query performance (avg/percentile)
- Error rates
- Storage size growth

### Backup & Recovery

- Regular PostgreSQL backups (pg_dump)
- Point-in-time recovery with WAL archiving
- Test restore procedures

---

## Comparison: SQLite vs PostgreSQL

| Feature | SQLite | PostgreSQL |
|---------|--------|------------|
| **Concurrency** | Limited (WAL helps) | Excellent (MVCC) |
| **Scalability** | Single file | Distributed, sharding |
| **Connection Pool** | File-based | Network-based |
| **Transactions** | Basic | Full ACID, isolation levels |
| **Data Types** | Limited | Rich (JSONB, arrays, etc.) |
| **Backup** | Copy file | pg_dump, streaming |
| **Setup Complexity** | None | Requires server |
| **Best For** | Development, small-scale | Production, large-scale |

**Migration Path**: Applications can switch from SQLite to PostgreSQL by changing the storage implementation - adapter interfaces remain the same.

---

## Implementation Roadmap

### Phase 1: Foundation (Week 1-2)
- [ ] Create `BasePostgresStorage` class
- [ ] Implement `PostgresConnectionPool`
- [ ] Set up test infrastructure (docker-compose)
- [ ] Create basic schema migration framework

### Phase 2: Core Implementations (Week 3-4)
- [ ] Implement `PostgresCheckpointStorage`
- [ ] Implement `PostgresWorkflowStorage`
- [ ] Implement `PostgresTaskStorage`
- [ ] Write unit tests for core implementations

### Phase 3: Extended Implementations (Week 5)
- [ ] Implement `PostgresWorkflowExecutionStorage`
- [ ] Implement `PostgresAgentLoopStorage`
- [ ] Implement `PostgresAgentLoopCheckpointStorage`
- [ ] Write integration tests

### Phase 4: Optimization & Documentation (Week 6)
- [ ] Performance testing and optimization
- [ ] Add comprehensive documentation
- [ ] Create migration guides (SQLite → PostgreSQL)
- [ ] Write deployment guides

### Phase 5: Release (Week 7)
- [ ] Final testing and bug fixes
- [ ] Update package exports
- [ ] Publish to npm
- [ ] Announce in release notes

---

## Dependencies

Add to `packages/storage/package.json`:

```json
{
  "dependencies": {
    "@wf-agent/types": "workspace:*",
    "@wf-agent/common-utils": "workspace:*",
    "better-sqlite3": "^12.6.2",
    "pg": "^8.11.0",
    "pg-pool": "^3.6.0"
  },
  "devDependencies": {
    "@types/better-sqlite3": "^7.6.13",
    "@types/pg": "^8.10.0",
    "tsd": "^0.33.0"
  }
}
```

---

## Export Updates

Update `packages/storage/src/index.ts`:

```typescript
// PostgreSQL Storage Implementation
export * from "./postgres/index.js";
```

Create `packages/storage/src/postgres/index.ts`:

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

---

## Conclusion

The PostgreSQL storage backend provides a production-ready, scalable alternative to SQLite and JSON file storage. By following the established architecture patterns and leveraging PostgreSQL's advanced features, we can offer users a robust solution for high-concurrency, large-scale deployments.

**Key Benefits**:
✅ API compatibility with existing storage adapters  
✅ Advanced connection pooling and transaction support  
✅ Superior concurrency with MVCC  
✅ Rich data types (JSONB, arrays, etc.)  
✅ Enterprise-grade reliability and backup options  
✅ Seamless migration path from SQLite  

**Next Steps**:
1. Review and approve this design document
2. Begin Phase 1 implementation
3. Gather feedback from early adopters
4. Iterate based on real-world usage
