# Agent Module Serialization and Storage Integration Design

## Overview

This document outlines the design for integrating agent loop serialization and storage into the wf-agent framework. The current implementation lacks proper persistence, serialization registration, and lifecycle management for agent loops, while the workflow execution module provides a complete reference architecture.

## Problem Statement

### Current Gaps

1. **No Agent-Specific Storage Adapter**: Agent loops have no dedicated persistence mechanism
2. **Missing Serializer Registration**: No proper serialization/deserialization for `AgentLoopCheckpoint`
3. **No Lifecycle Management**: Completed agent loops accumulate in memory without cleanup
4. **Mock Dependencies in CLI**: CLI commands use mock storage instead of real implementations
5. **No Atomic Operations**: Risk of data corruption during concurrent checkpoint operations
6. **Inconsistent Architecture**: Agent module doesn't follow the same patterns as workflow execution

### Design Principle

**Strong Type Separation**: Workflow checkpoints and agent loop checkpoints are fundamentally different entities that should never be mixed. We will provide:
- A generic checkpoint abstraction in the core layer
- Two strongly-typed concrete implementations (workflow + agent)
- No metadata-based differentiation - each has its own adapter and serializer

## Architecture Design

### 1. Type System Enhancements

#### 1.1 Generic Checkpoint Abstraction

```typescript
// packages/types/src/checkpoint/base-checkpoint.ts

/**
 * Base checkpoint interface providing common structure
 */
export interface BaseCheckpoint<TDelta = unknown, TSnapshot = unknown> {
  /** Unique checkpoint identifier */
  id: string;
  
  /** Checkpoint type: FULL or DELTA */
  type: 'FULL' | 'DELTA';
  
  /** Creation timestamp */
  timestamp: number;
  
  /** Optional metadata */
  metadata?: CheckpointMetadata;
}

/**
 * Full checkpoint with complete state snapshot
 */
export interface FullCheckpoint<TSnapshot> extends BaseCheckpoint<never, TSnapshot> {
  type: 'FULL';
  snapshot: TSnapshot;
}

/**
 * Delta checkpoint with incremental changes
 */
export interface DeltaCheckpoint<TDelta> extends BaseCheckpoint<TDelta, never> {
  type: 'DELTA';
  baseCheckpointId: string;
  previousCheckpointId: string;
  delta: TDelta;
}

/**
 * Union type for any checkpoint
 */
export type AnyCheckpoint<TDelta, TSnapshot> = 
  | FullCheckpoint<TSnapshot>
  | DeltaCheckpoint<TDelta>;
```

#### 1.2 Workflow Checkpoint Types (Existing - Keep As Is)

```typescript
// packages/types/src/checkpoint/workflow/checkpoint.ts

export interface WorkflowExecutionDelta {
  addedMessages?: Message[];
  modifiedMessages?: Array<{ index: number; message: Message }>;
  deletedMessageIndices?: number[];
  addedVariables?: Record<string, unknown>;
  modifiedVariables?: Record<string, unknown>;
  statusChange?: {
    from: WorkflowExecutionStatus;
    to: WorkflowExecutionStatus;
  };
}

export interface WorkflowExecutionStateSnapshot {
  status: WorkflowExecutionStatus;
  currentNodeId?: string;
  variables: Record<string, WorkflowExecutionVariable>;
  messages: LLMMessage[];
  nodeResults: Record<string, NodeExecutionResult>;
  triggerState?: TriggerRuntimeState;
  [key: string]: unknown;
}

export type WorkflowCheckpoint = AnyCheckpoint<WorkflowExecutionDelta, WorkflowExecutionStateSnapshot> & {
  executionId: string;
  workflowId: string;
};
```

#### 1.3 Agent Loop Checkpoint Types (Enhanced)

```typescript
// packages/types/src/checkpoint/agent/checkpoint.ts

export interface AgentLoopDelta {
  addedMessages?: Message[];
  modifiedMessages?: Array<{ index: number; message: Message }>;
  deletedMessageIndices?: number[];
  addedVariables?: Record<string, unknown>;
  modifiedVariables?: Record<string, unknown>;
  stateChanges?: {
    iterationCount?: number;
    toolCallCount?: number;
    status?: AgentLoopStatus;
  };
}

export interface AgentLoopStateSnapshot {
  status: AgentLoopStatus;
  currentIteration: number;
  toolCallCount: number;
  startTime: number | null;
  endTime: number | null;
  error: unknown;
  messages: Message[];
  variables: Record<string, unknown>;
  config?: unknown;
  iterationHistory?: IterationRecord[];
  isStreaming?: boolean;
  pendingToolCalls?: string[];
  [key: string]: unknown;
}

export type AgentLoopCheckpoint = AnyCheckpoint<AgentLoopDelta, AgentLoopStateSnapshot> & {
  agentLoopId: string;
};
```

### 2. Storage Adapter Layer

#### 2.1 Workflow Checkpoint Storage Adapter (Existing)

```typescript
// packages/storage/src/types/adapter/checkpoint-adapter.ts

export interface CheckpointStorageMetadata {
  executionId: string;
  workflowId: string;
  timestamp: number;
  version: number;
  tags?: string[];
}

export interface CheckpointStorageListOptions {
  executionId?: string;
  workflowId?: string;
  tags?: string[];
  offset?: number;
  limit?: number;
}

export type CheckpointStorageAdapter = BaseStorageAdapter<
  CheckpointStorageMetadata,
  CheckpointStorageListOptions
>;
```

#### 2.2 Agent Loop Checkpoint Storage Adapter (NEW)

```typescript
// packages/storage/src/types/adapter/agent-loop-checkpoint-adapter.ts

export interface AgentLoopCheckpointStorageMetadata {
  agentLoopId: string;
  timestamp: number;
  type: 'FULL' | 'DELTA';
  version: number;
  tags?: string[];
}

export interface AgentLoopCheckpointListOptions {
  agentLoopId?: string;
  type?: 'FULL' | 'DELTA';
  tags?: string[];
  offset?: number;
  limit?: number;
}

export interface AgentLoopCheckpointStorageAdapter 
  extends BaseStorageAdapter<AgentLoopCheckpointStorageMetadata, AgentLoopCheckpointListOptions> {
  /**
   * List checkpoints for a specific agent loop
   */
  listByAgentLoop(agentLoopId: string, options?: Omit<AgentLoopCheckpointListOptions, 'agentLoopId'>): Promise<string[]>;
  
  /**
   * Get the latest checkpoint for an agent loop
   */
  getLatestCheckpoint(agentLoopId: string): Promise<string | null>;
  
  /**
   * Delete all checkpoints for an agent loop
   */
  deleteByAgentLoop(agentLoopId: string): Promise<number>;
}
```

#### 2.3 Agent Loop Entity Storage Adapter (NEW)

For persisting agent loop lifecycle (not just checkpoints):

```typescript
// packages/storage/src/types/adapter/agent-loop-adapter.ts

export interface AgentLoopStorageMetadata {
  agentLoopId: string;
  status: AgentLoopStatus;
  createdAt: number;
  updatedAt?: number;
  completedAt?: number;
  profileId?: string;
  tags?: string[];
}

export interface AgentLoopListOptions {
  status?: AgentLoopStatus;
  profileId?: string;
  tags?: string[];
  createdAfter?: number;
  createdBefore?: number;
  offset?: number;
  limit?: number;
}

export interface AgentLoopStorageAdapter 
  extends BaseStorageAdapter<AgentLoopStorageMetadata, AgentLoopListOptions> {
  /**
   * Update agent loop status
   */
  updateAgentLoopStatus(agentLoopId: string, status: AgentLoopStatus): Promise<void>;
  
  /**
   * List agent loops by status
   */
  listByStatus(status: AgentLoopStatus): Promise<string[]>;
  
  /**
   * Get agent loop statistics
   */
  getStats(): Promise<{
    total: number;
    byStatus: Record<AgentLoopStatus, number>;
  }>;
}
```

### 3. Serialization Layer

#### 3.1 Workflow Checkpoint Serializer (Existing - Enhanced)

```typescript
// sdk/core/serialization/entities/checkpoint-serializer.ts

export interface WorkflowCheckpointSnapshot extends SnapshotBase {
  _entityType: 'workflowCheckpoint';
  checkpoint: WorkflowCheckpoint;
}

export class WorkflowCheckpointSerializer extends Serializer<WorkflowCheckpointSnapshot> {
  constructor() {
    super({ prettyPrint: true, targetVersion: 1 });
  }

  async serializeCheckpoint(checkpoint: WorkflowCheckpoint): Promise<Uint8Array> {
    const snapshot: WorkflowCheckpointSnapshot = {
      _version: 1,
      _timestamp: Date.now(),
      _entityType: 'workflowCheckpoint',
      checkpoint,
    };
    return this.serialize(snapshot);
  }

  async deserializeCheckpoint(data: Uint8Array): Promise<WorkflowCheckpoint> {
    const snapshot = await this.deserialize(data);
    if (snapshot._entityType !== 'workflowCheckpoint') {
      throw new Error(`Expected workflowCheckpoint, got ${snapshot._entityType}`);
    }
    return snapshot.checkpoint;
  }
}

export function registerWorkflowCheckpointSerializer(): void {
  const registry = SerializationRegistry.getInstance();
  registry.register({
    entityType: 'workflowCheckpoint',
    serializer: new WorkflowCheckpointSerializer(),
    deltaCalculator: new WorkflowCheckpointDeltaCalculator(),
  });
}
```

#### 3.2 Agent Loop Checkpoint Serializer (NEW)

```typescript
// sdk/core/serialization/entities/agent-loop-checkpoint-serializer.ts

export interface AgentLoopCheckpointSnapshot extends SnapshotBase {
  _entityType: 'agentLoopCheckpoint';
  checkpoint: AgentLoopCheckpoint;
}

export class AgentLoopCheckpointSerializer extends Serializer<AgentLoopCheckpointSnapshot> {
  constructor() {
    super({ prettyPrint: true, targetVersion: 1 });
  }

  async serializeCheckpoint(checkpoint: AgentLoopCheckpoint): Promise<Uint8Array> {
    const snapshot: AgentLoopCheckpointSnapshot = {
      _version: 1,
      _timestamp: Date.now(),
      _entityType: 'agentLoopCheckpoint',
      checkpoint,
    };
    return this.serialize(snapshot);
  }

  async deserializeCheckpoint(data: Uint8Array): Promise<AgentLoopCheckpoint> {
    const snapshot = await this.deserialize(data);
    if (snapshot._entityType !== 'agentLoopCheckpoint') {
      throw new Error(`Expected agentLoopCheckpoint, got ${snapshot._entityType}`);
    }
    return snapshot.checkpoint;
  }
}

export function registerAgentLoopCheckpointSerializer(): void {
  const registry = SerializationRegistry.getInstance();
  registry.register({
    entityType: 'agentLoopCheckpoint',
    serializer: new AgentLoopCheckpointSerializer(),
    deltaCalculator: new AgentLoopCheckpointDeltaCalculator(),
  });
}
```

#### 3.3 Agent Loop Entity Serializer (NEW)

For full entity persistence:

```typescript
// sdk/core/serialization/entities/agent-loop-entity-serializer.ts

export interface AgentLoopEntitySnapshot extends SnapshotBase {
  _entityType: 'agentLoop';
  id: string;
  config: AgentLoopConfig;
  state: AgentLoopStateSnapshot;
  messages: Message[];
  variables: Record<string, unknown>;
}

export class AgentLoopEntitySerializer extends Serializer<AgentLoopEntitySnapshot> {
  constructor() {
    super({ prettyPrint: true, targetVersion: 1 });
  }

  async serializeEntity(entity: AgentLoopEntity): Promise<Uint8Array> {
    const snapshot: AgentLoopEntitySnapshot = {
      _version: 1,
      _timestamp: Date.now(),
      _entityType: 'agentLoop',
      id: entity.id,
      config: entity.config,
      state: entity.state.createSnapshot(),
      messages: entity.getMessages(),
      variables: entity.getAllVariables(),
    };
    return this.serialize(snapshot);
  }

  async deserializeEntity(data: Uint8Array): Promise<AgentLoopEntity> {
    const snapshot = await this.deserialize(data);
    if (snapshot._entityType !== 'agentLoop') {
      throw new Error(`Expected agentLoop, got ${snapshot._entityType}`);
    }
    
    // Reconstruct entity from snapshot
    return AgentLoopEntity.fromSnapshot(snapshot.id, snapshot.state);
  }
}

export function registerAgentLoopEntitySerializer(): void {
  const registry = SerializationRegistry.getInstance();
  registry.register({
    entityType: 'agentLoop',
    serializer: new AgentLoopEntitySerializer(),
    deltaCalculator: null, // Entity snapshots don't use delta calculation
  });
}
```

### 4. State Management Layer

#### 4.1 Agent Loop Checkpoint State Manager (NEW)

Mirrors `CheckpointState` for workflows:

```typescript
// sdk/agent/checkpoint/agent-loop-checkpoint-state-manager.ts

export class AgentLoopCheckpointStateManager implements LifecycleCapable<void> {
  private storageAdapter: AgentLoopCheckpointStorageAdapter;
  private eventManager?: EventRegistry;
  private serializationRegistry: SerializationRegistry;
  private checkpointSerializer: AgentLoopCheckpointSerializer;
  private checkpointSizes: Map<string, number> = new Map();
  private cleanupPolicy?: CleanupPolicy;

  constructor(
    storageAdapter: AgentLoopCheckpointStorageAdapter,
    eventManager?: EventRegistry
  ) {
    this.storageAdapter = storageAdapter;
    this.eventManager = eventManager;
    this.serializationRegistry = SerializationRegistry.getInstance();
    
    const serializer = this.serializationRegistry.getSerializer('agentLoopCheckpoint');
    if (serializer instanceof AgentLoopCheckpointSerializer) {
      this.checkpointSerializer = serializer;
    } else {
      this.checkpointSerializer = new AgentLoopCheckpointSerializer();
    }
  }

  /**
   * Save checkpoint atomically
   */
  async saveCheckpoint(checkpoint: AgentLoopCheckpoint): Promise<string> {
    const serializedData = await this.checkpointSerializer.serializeCheckpoint(checkpoint);
    
    const metadata: AgentLoopCheckpointStorageMetadata = {
      agentLoopId: checkpoint.agentLoopId,
      timestamp: checkpoint.timestamp,
      type: checkpoint.type,
      version: 1,
    };
    
    await this.storageAdapter.save(checkpoint.id, serializedData, metadata);
    this.checkpointSizes.set(checkpoint.id, serializedData.length);
    
    // Emit event
    if (this.eventManager) {
      safeEmit(this.eventManager, buildAgentLoopCheckpointCreatedEvent(checkpoint));
    }
    
    return checkpoint.id;
  }

  /**
   * Load checkpoint by ID
   */
  async getCheckpoint(checkpointId: string): Promise<AgentLoopCheckpoint | null> {
    const data = await this.storageAdapter.load(checkpointId);
    if (!data) {
      return null;
    }
    
    return await this.checkpointSerializer.deserializeCheckpoint(data);
  }

  /**
   * List checkpoints with filtering
   */
  async list(options?: AgentLoopCheckpointListOptions): Promise<string[]> {
    return await this.storageAdapter.list(options);
  }

  /**
   * Delete checkpoint
   */
  async deleteCheckpoint(checkpointId: string): Promise<void> {
    await this.storageAdapter.delete(checkpointId);
    this.checkpointSizes.delete(checkpointId);
    
    if (this.eventManager) {
      safeEmit(this.eventManager, buildAgentLoopCheckpointDeletedEvent(checkpointId));
    }
  }

  /**
   * Execute cleanup policy
   */
  async executeCleanup(): Promise<CleanupResult> {
    if (!this.cleanupPolicy) {
      return {
        deletedCheckpointIds: [],
        deletedCount: 0,
        freedSpaceBytes: 0,
        remainingCount: 0,
      };
    }

    const checkpointIds = await this.storageAdapter.list();
    const checkpointInfoArray: Array<{
      checkpointId: string;
      metadata: AgentLoopCheckpointStorageMetadata;
    }> = [];

    for (const checkpointId of checkpointIds) {
      const data = await this.storageAdapter.load(checkpointId);
      if (data) {
        const checkpoint = await this.checkpointSerializer.deserializeCheckpoint(data);
        const metadata: AgentLoopCheckpointStorageMetadata = {
          agentLoopId: checkpoint.agentLoopId,
          timestamp: checkpoint.timestamp,
          type: checkpoint.type,
          version: 1,
        };
        checkpointInfoArray.push({ checkpointId, metadata });
        this.checkpointSizes.set(checkpointId, data.length);
      }
    }

    const strategy = createCleanupStrategy(this.cleanupPolicy, this.checkpointSizes);
    const toDeleteIds = strategy.execute(checkpointInfoArray);

    let freedSpaceBytes = 0;
    for (const checkpointId of toDeleteIds) {
      const size = this.checkpointSizes.get(checkpointId) || 0;
      await this.storageAdapter.delete(checkpointId);
      this.checkpointSizes.delete(checkpointId);
      freedSpaceBytes += size;
    }

    const remainingCount = await this.storageAdapter.list().then(ids => ids.length);

    return {
      deletedCheckpointIds: toDeleteIds,
      deletedCount: toDeleteIds.length,
      freedSpaceBytes,
      remainingCount,
    };
  }

  setCleanupPolicy(policy: CleanupPolicy): void {
    this.cleanupPolicy = policy;
  }

  async initialize(): Promise<void> {
    await this.storageAdapter.initialize();
  }

  async cleanup(): Promise<void> {
    if ('close' in this.storageAdapter && typeof this.storageAdapter.close === 'function') {
      await this.storageAdapter.close();
    }
  }
}
```

#### 4.2 Agent Loop Entity State Manager (NEW)

For managing agent loop lifecycle:

```typescript
// sdk/agent/state-managers/agent-loop-state-manager.ts

export class AgentLoopStateManager implements LifecycleCapable<void> {
  private storageAdapter: AgentLoopStorageAdapter;
  private serializationRegistry: SerializationRegistry;
  private entitySerializer: AgentLoopEntitySerializer;

  constructor(storageAdapter: AgentLoopStorageAdapter) {
    this.storageAdapter = storageAdapter;
    this.serializationRegistry = SerializationRegistry.getInstance();
    
    const serializer = this.serializationRegistry.getSerializer('agentLoop');
    if (serializer instanceof AgentLoopEntitySerializer) {
      this.entitySerializer = serializer;
    } else {
      this.entitySerializer = new AgentLoopEntitySerializer();
    }
  }

  /**
   * Save agent loop entity
   */
  async saveAgentLoop(entity: AgentLoopEntity): Promise<void> {
    const serializedData = await this.entitySerializer.serializeEntity(entity);
    
    const metadata: AgentLoopStorageMetadata = {
      agentLoopId: entity.id,
      status: entity.state.status,
      createdAt: entity.state.startTime || Date.now(),
      updatedAt: Date.now(),
      profileId: entity.config.profileId,
    };
    
    await this.storageAdapter.save(entity.id, serializedData, metadata);
  }

  /**
   * Load agent loop entity
   */
  async loadAgentLoop(agentLoopId: string): Promise<AgentLoopEntity | null> {
    const data = await this.storageAdapter.load(agentLoopId);
    if (!data) {
      return null;
    }
    
    return await this.entitySerializer.deserializeEntity(data);
  }

  /**
   * Update agent loop status
   */
  async updateStatus(agentLoopId: string, status: AgentLoopStatus): Promise<void> {
    await this.storageAdapter.updateAgentLoopStatus(agentLoopId, status);
  }

  /**
   * List agent loops by status
   */
  async listByStatus(status: AgentLoopStatus): Promise<string[]> {
    return await this.storageAdapter.listByStatus(status);
  }

  /**
   * Delete agent loop
   */
  async deleteAgentLoop(agentLoopId: string): Promise<void> {
    await this.storageAdapter.delete(agentLoopId);
  }

  async initialize(): Promise<void> {
    await this.storageAdapter.initialize();
  }

  async cleanup(): Promise<void> {
    if ('close' in this.storageAdapter && typeof this.storageAdapter.close === 'function') {
      await this.storageAdapter.close();
    }
  }
}
```

### 5. Storage Initialization Service Integration

#### 5.1 Enhanced StorageAdapters Interface

```typescript
// sdk/core/services/storage-initialization-service.ts

export interface StorageAdapters {
  // Workflow execution storage
  checkpoint?: CheckpointStorageAdapter;
  task?: TaskStorageAdapter;
  workflow?: WorkflowStorageAdapter;
  workflowExecution?: WorkflowExecutionStorageAdapter;
  
  // Agent loop storage (NEW)
  agentLoopCheckpoint?: AgentLoopCheckpointStorageAdapter;
  agentLoop?: AgentLoopStorageAdapter;
}
```

#### 5.2 Updated Health Check and Shutdown

The service already supports dynamic adapter checking, so adding new adapters requires no code changes - they'll be automatically included in health checks and shutdown procedures.

### 6. Resource API Layer

#### 6.1 Agent Loop Checkpoint Resource API (Enhanced)

```typescript
// sdk/api/agent/resources/agent-loop-checkpoint-resource-api.ts

export class AgentLoopCheckpointResourceAPI 
  extends CrudResourceAPI<AgentLoopCheckpoint, string, AgentLoopCheckpointFilter> {
  
  private stateManager: AgentLoopCheckpointStateManager;
  
  constructor(stateManager: AgentLoopCheckpointStateManager) {
    super();
    this.stateManager = stateManager;
  }

  async create(checkpoint: AgentLoopCheckpoint): Promise<string> {
    return await this.stateManager.saveCheckpoint(checkpoint);
  }

  async get(id: string): Promise<AgentLoopCheckpoint | null> {
    return await this.stateManager.getCheckpoint(id);
  }

  async list(filter?: AgentLoopCheckpointFilter): Promise<AgentLoopCheckpoint[]> {
    const ids = await this.stateManager.list(filter);
    const checkpoints: AgentLoopCheckpoint[] = [];
    
    for (const id of ids) {
      const checkpoint = await this.stateManager.getCheckpoint(id);
      if (checkpoint) {
        checkpoints.push(checkpoint);
      }
    }
    
    return checkpoints;
  }

  async delete(id: string): Promise<void> {
    await this.stateManager.deleteCheckpoint(id);
  }

  /**
   * Get all checkpoints for an agent loop
   */
  async getAgentLoopCheckpoints(agentLoopId: string): Promise<AgentLoopCheckpoint[]> {
    const ids = await this.stateManager.list({ agentLoopId });
    const checkpoints: AgentLoopCheckpoint[] = [];
    
    for (const id of ids) {
      const checkpoint = await this.stateManager.getCheckpoint(id);
      if (checkpoint) {
        checkpoints.push(checkpoint);
      }
    }
    
    return checkpoints;
  }

  /**
   * Get latest checkpoint for an agent loop
   */
  async getLatestCheckpoint(agentLoopId: string): Promise<AgentLoopCheckpoint | null> {
    const latestId = await this.stateManager['storageAdapter'].getLatestCheckpoint(agentLoopId);
    if (!latestId) {
      return null;
    }
    return await this.stateManager.getCheckpoint(latestId);
  }

  /**
   * Delete all checkpoints for an agent loop
   */
  async deleteAgentLoopCheckpoints(agentLoopId: string): Promise<number> {
    return await this.stateManager['storageAdapter'].deleteByAgentLoop(agentLoopId);
  }
}
```

### 7. Dependency Injection Container Updates

```typescript
// sdk/core/di/container-config.ts

// Register AgentLoopCheckpointStateManager
container
  .bind(Identifiers.AgentLoopCheckpointStateManager)
  .toDynamicValue((c: IContainer): AgentLoopCheckpointStateManager => {
    const eventManager = c.get(Identifiers.EventRegistry) as EventRegistry;
    const adapter = c.get(Identifiers.AgentLoopCheckpointStorageAdapter) as AgentLoopCheckpointStorageAdapter;

    if (!adapter) {
      throw new Error(
        "AgentLoopCheckpointStateManager requires an AgentLoopCheckpointStorageAdapter implementation."
      );
    }

    return new AgentLoopCheckpointStateManager(adapter, eventManager);
  })
  .inSingletonScope();

// Register AgentLoopStateManager
container
  .bind(Identifiers.AgentLoopStateManager)
  .toDynamicValue((c: IContainer): AgentLoopStateManager => {
    const adapter = c.get(Identifiers.AgentLoopStorageAdapter) as AgentLoopStorageAdapter;

    if (!adapter) {
      throw new Error(
        "AgentLoopStateManager requires an AgentLoopStorageAdapter implementation."
      );
    }

    return new AgentLoopStateManager(adapter);
  })
  .inSingletonScope();
```

### 8. Implementation Files Structure

```
packages/
├── types/src/checkpoint/
│   ├── base-checkpoint.ts          # NEW: Generic abstraction
│   ├── workflow/
│   │   ├── checkpoint.ts           # EXISTING: Workflow-specific types
│   │   └── snapshot.ts
│   └── agent/
│       ├── checkpoint.ts           # ENHANCED: Agent-specific types
│       └── snapshot.ts
│
├── storage/src/
│   ├── types/adapter/
│   │   ├── checkpoint-adapter.ts                    # EXISTING: Workflow checkpoints
│   │   ├── agent-loop-checkpoint-adapter.ts         # NEW: Agent loop checkpoints
│   │   └── agent-loop-adapter.ts                    # NEW: Agent loop entities
│   ├── json/
│   │   ├── json-checkpoint-storage.ts               # EXISTING
│   │   ├── json-agent-loop-checkpoint-storage.ts    # NEW
│   │   └── json-agent-loop-storage.ts               # NEW
│   ├── sqlite/
│   │   ├── sqlite-checkpoint-storage.ts             # EXISTING
│   │   ├── sqlite-agent-loop-checkpoint-storage.ts  # NEW
│   │   └── sqlite-agent-loop-storage.ts             # NEW
│   └── memory/
│       ├── memory-checkpoint-storage.ts             # EXISTING
│       ├── memory-agent-loop-checkpoint-storage.ts  # NEW
│       └── memory-agent-loop-storage.ts             # NEW
│
sdk/
├── core/
│   ├── serialization/entities/
│   │   ├── checkpoint-serializer.ts                 # RENAMED: WorkflowCheckpointSerializer
│   │   ├── agent-loop-checkpoint-serializer.ts      # NEW
│   │   └── agent-loop-entity-serializer.ts          # NEW
│   └── services/
│       └── storage-initialization-service.ts        # ENHANCED
│
├── agent/
│   ├── checkpoint/
│   │   ├── checkpoint-coordinator.ts                # ENHANCED: Use new state manager
│   │   ├── agent-loop-checkpoint-state-manager.ts   # NEW
│   │   └── index.ts                                 # ENHANCED exports
│   └── state-managers/
│       └── agent-loop-state-manager.ts              # NEW
│
└── api/agent/resources/
    └── agent-loop-checkpoint-resource-api.ts        # ENHANCED
```

### 9. Migration Strategy

#### Phase 1: Type System Foundation (Week 1)
1. Create `base-checkpoint.ts` with generic abstractions
2. Refactor existing workflow checkpoint types to use generics
3. Define agent loop checkpoint types with strong typing
4. Update all imports and references

#### Phase 2: Storage Adapters (Week 2)
1. Implement `AgentLoopCheckpointStorageAdapter` interface
2. Implement `AgentLoopStorageAdapter` interface
3. Create memory implementations for testing
4. Create JSON implementations for CLI app
5. Create SQLite implementations for production

#### Phase 3: Serialization (Week 3)
1. Rename existing `CheckpointSnapshotSerializer` to `WorkflowCheckpointSerializer`
2. Create `AgentLoopCheckpointSerializer`
3. Create `AgentLoopEntitySerializer`
4. Register all serializers in `SerializationRegistry`
5. Add unit tests for serialization/deserialization

#### Phase 4: State Managers (Week 4)
1. Implement `AgentLoopCheckpointStateManager`
2. Implement `AgentLoopStateManager`
3. Integrate with DI container
4. Add cleanup policy support
5. Add event emission

#### Phase 5: Resource APIs (Week 5)
1. Enhance `AgentLoopCheckpointResourceAPI`
2. Create `AgentLoopResourceAPI` for entity management
3. Update SDK exports
4. Add integration tests

#### Phase 6: CLI Integration (Week 6)
1. Replace mock dependencies with real storage
2. Update CLI commands to use Resource APIs
3. Add storage configuration to CLI config
4. Test end-to-end workflows

#### Phase 7: Documentation & Testing (Week 7)
1. Update API documentation
2. Add comprehensive test coverage
3. Create migration guide
4. Performance testing and optimization

### 10. Backward Compatibility

To ensure smooth migration:

1. **Deprecate old checkpoint API**: Mark existing methods as deprecated with warnings
2. **Dual-write during transition**: Support both old and new storage formats temporarily
3. **Migration utility**: Provide tool to migrate existing checkpoints to new format
4. **Feature flag**: Allow gradual rollout via configuration

### 11. Testing Strategy

#### Unit Tests
- Serializer round-trip tests (serialize → deserialize → compare)
- Storage adapter CRUD operations
- State manager business logic
- Delta calculation accuracy

#### Integration Tests
- Full checkpoint creation and restoration flow
- Concurrent checkpoint operations
- Cleanup policy execution
- Storage adapter switching (memory → JSON → SQLite)

#### End-to-End Tests
- CLI commands with real storage
- Agent loop execution with checkpointing
- Recovery from checkpoints after restart
- Performance under load

### 12. Performance Considerations

1. **Delta Compression**: Only store changes between checkpoints
2. **Lazy Loading**: Load checkpoint data on-demand, not upfront
3. **Indexing**: Maintain indexes for fast queries by agentLoopId, status, timestamp
4. **Batch Operations**: Support batch delete for cleanup
5. **Caching**: Cache frequently accessed checkpoints in memory
6. **Compression**: Enable optional compression for large checkpoints

### 13. Security Considerations

1. **Encryption**: Support encryption at rest for sensitive agent loop data
2. **Access Control**: Validate permissions before checkpoint operations
3. **Audit Logging**: Log all checkpoint create/load/delete operations
4. **Data Validation**: Validate checkpoint data before deserialization
5. **Sandboxing**: Isolate agent loop storage per user/tenant

## Conclusion

This design provides a clean, type-safe separation between workflow and agent loop checkpoint systems while maintaining architectural consistency. By avoiding metadata-based differentiation and using strong typing throughout, we eliminate runtime errors and improve developer experience.

The phased implementation approach allows for gradual migration without breaking existing functionality, while the comprehensive testing strategy ensures reliability across all storage backends.

## Next Steps

1. Review and approve this design document
2. Create detailed task breakdown for Phase 1
3. Set up development environment with new type definitions
4. Begin implementation following the migration strategy
5. Schedule weekly progress reviews
