/**
 * Storage Adapter Type Tests
 * 
 * Tests for storage types including:
 * - WorkflowStorageMetadata and WorkflowListOptions
 * - WorkflowInfo, WorkflowVersionInfo, WorkflowStats
 * - CheckpointStorageMetadata and CheckpointStorageListOptions
 * - CleanupPolicy (time/count/size based)
 * - CleanupResult and CheckpointCleanupStrategy
 * - AgentLoopStorage, TaskStorage interfaces
 * 
 * Priority: MEDIUM (Phase 2)
 */

import { expectType, expectAssignable } from "tsd";
import type {
  WorkflowStorageMetadata,
  WorkflowListOptions,
  WorkflowInfo,
  WorkflowVersionInfo,
  WorkflowVersionListOptions,
  WorkflowStats,
  CheckpointStorageMetadata,
  CheckpointStorageListOptions,
  CheckpointInfo,
  CleanupStrategyType,
  TimeBasedCleanupPolicy,
  CountBasedCleanupPolicy,
  SizeBasedCleanupPolicy,
  CleanupPolicy,
  CleanupResult,
  CheckpointCleanupStrategy,
} from "../../../src/index.js";

// =============================================================================
// Test 1: WorkflowStorageMetadata Structure
// =============================================================================

const workflowMetadata: WorkflowStorageMetadata = {
  workflowId: "workflow-123",
  name: "My Workflow",
  version: "1.0.0",
  description: "A test workflow",
  author: "John Doe",
  category: "automation",
  tags: ["test", "demo"],
  createdAt: Date.now(),
  updatedAt: Date.now(),
  nodeCount: 5,
  edgeCount: 4,
  enabled: true,
  customFields: {
    priority: "high",
    department: "engineering",
  },
};

expectType<WorkflowStorageMetadata>(workflowMetadata);
expectType<string>(workflowMetadata.workflowId);
expectType<string>(workflowMetadata.name);
expectType<string>(workflowMetadata.version);
expectType<string | undefined>(workflowMetadata.description);
expectType<string | undefined>(workflowMetadata.author);
expectType<string | undefined>(workflowMetadata.category);
expectType<string[] | undefined>(workflowMetadata.tags);
expectType<number>(workflowMetadata.createdAt);
expectType<number>(workflowMetadata.updatedAt);
expectType<number>(workflowMetadata.nodeCount);
expectType<number>(workflowMetadata.edgeCount);
expectType<boolean | undefined>(workflowMetadata.enabled);
expectType<Record<string, unknown> | undefined>(workflowMetadata.customFields);

// Minimal metadata
const minimalWorkflowMetadata: WorkflowStorageMetadata = {
  workflowId: "workflow-minimal",
  name: "Minimal Workflow",
  version: "0.1.0",
  createdAt: Date.now(),
  updatedAt: Date.now(),
  nodeCount: 1,
  edgeCount: 0,
};

expectType<WorkflowStorageMetadata>(minimalWorkflowMetadata);

// =============================================================================
// Test 2: WorkflowListOptions
// =============================================================================

const listOptions: WorkflowListOptions = {
  name: "test",
  author: "John",
  category: "automation",
  tags: ["important"],
  enabled: true,
  createdAtFrom: Date.now() - 86400000,
  createdAtTo: Date.now(),
  updatedAtFrom: Date.now() - 3600000,
  updatedAtTo: Date.now(),
  limit: 10,
  offset: 0,
  sortBy: "createdAt",
  sortOrder: "desc",
};

expectType<WorkflowListOptions>(listOptions);
expectType<string | undefined>(listOptions.name);
expectType<string | undefined>(listOptions.author);
expectType<string | undefined>(listOptions.category);
expectType<string[] | undefined>(listOptions.tags);
expectType<boolean | undefined>(listOptions.enabled);
expectType<number | undefined>(listOptions.limit);
expectType<number | undefined>(listOptions.offset);
expectType<"name" | "createdAt" | "updatedAt" | undefined>(listOptions.sortBy);
expectType<"asc" | "desc" | undefined>(listOptions.sortOrder);

// Empty options (get all)
const emptyOptions: WorkflowListOptions = {};

expectType<WorkflowListOptions>(emptyOptions);

// =============================================================================
// Test 3: WorkflowInfo Structure
// =============================================================================

const workflowInfo: WorkflowInfo = {
  workflowId: "workflow-123",
  metadata: workflowMetadata,
};

expectType<WorkflowInfo>(workflowInfo);
expectType<string>(workflowInfo.workflowId);
expectType<WorkflowStorageMetadata>(workflowInfo.metadata);

// =============================================================================
// Test 4: WorkflowVersionInfo Structure
// =============================================================================

const versionInfo: WorkflowVersionInfo = {
  version: "1.0.0",
  createdAt: Date.now(),
  createdBy: "Jane Smith",
  changeNote: "Added new feature",
  isCurrent: true,
};

expectType<WorkflowVersionInfo>(versionInfo);
expectType<string>(versionInfo.version);
expectType<number>(versionInfo.createdAt);
expectType<string | undefined>(versionInfo.createdBy);
expectType<string | undefined>(versionInfo.changeNote);
expectType<boolean | undefined>(versionInfo.isCurrent);

// Version without optional fields
const minimalVersionInfo: WorkflowVersionInfo = {
  version: "0.1.0",
  createdAt: Date.now(),
};

expectType<WorkflowVersionInfo>(minimalVersionInfo);

// =============================================================================
// Test 5: WorkflowVersionListOptions
// =============================================================================

const versionListOptions: WorkflowVersionListOptions = {
  limit: 5,
  offset: 0,
};

expectType<WorkflowVersionListOptions>(versionListOptions);
expectType<number | undefined>(versionListOptions.limit);
expectType<number | undefined>(versionListOptions.offset);

// =============================================================================
// Test 6: WorkflowStats Structure
// =============================================================================

const workflowStats: WorkflowStats = {
  total: 100,
  enabled: 80,
  disabled: 20,
  byCategory: {
    automation: 30,
    analysis: 25,
    reporting: 20,
    other: 25,
  },
  byAuthor: {
    "John Doe": 40,
    "Jane Smith": 35,
    "Bob Wilson": 25,
  },
};

expectType<WorkflowStats>(workflowStats);
expectType<number>(workflowStats.total);
expectType<number>(workflowStats.enabled);
expectType<number>(workflowStats.disabled);
expectType<Record<string, number>>(workflowStats.byCategory);
expectType<Record<string, number>>(workflowStats.byAuthor);

// =============================================================================
// Test 7: CheckpointStorageMetadata Structure
// =============================================================================

const checkpointMetadata: CheckpointStorageMetadata = {
  entityType: "agent",
  entityId: "exec-123",
  timestamp: Date.now(),
  tags: ["recovery", "important"],
  customFields: {
    nodeId: "node-789",
    iteration: 5,
  },
};

expectType<CheckpointStorageMetadata>(checkpointMetadata);
expectType<string>(checkpointMetadata.entityId);
expectType<number>(checkpointMetadata.timestamp);
expectType<string[] | undefined>(checkpointMetadata.tags);
expectType<Record<string, unknown> | undefined>(checkpointMetadata.customFields);

// Minimal checkpoint metadata
const minimalCheckpointMetadata: CheckpointStorageMetadata = {
  entityType: "agent",
  entityId: "exec-minimal",
  timestamp: Date.now(),
};

expectType<CheckpointStorageMetadata>(minimalCheckpointMetadata);

// =============================================================================
// Test 8: CheckpointStorageListOptions
// =============================================================================

const checkpointListOptions: CheckpointStorageListOptions = {
  entityType: "workflow",
  entityId: "entity-123",
  tags: ["full"],
  timestampFrom: Date.now() - 86400000,
  timestampTo: Date.now(),
  type: "FULL",
  limit: 20,
  offset: 0,
  sortBy: "timestamp",
  sortOrder: "desc",
};

expectType<CheckpointStorageListOptions>(checkpointListOptions);
expectType<"workflow" | "agent" | "task" | undefined>(checkpointListOptions.entityType);
expectType<string | undefined>(checkpointListOptions.entityId);
expectType<string[] | undefined>(checkpointListOptions.tags);
expectType<number | undefined>(checkpointListOptions.timestampFrom);
expectType<number | undefined>(checkpointListOptions.timestampTo);
expectType<"FULL" | "DELTA" | undefined>(checkpointListOptions.type);
expectType<number | undefined>(checkpointListOptions.limit);
expectType<number | undefined>(checkpointListOptions.offset);
expectType<"timestamp" | "size" | "id" | undefined>(checkpointListOptions.sortBy);
expectType<"asc" | "desc" | undefined>(checkpointListOptions.sortOrder);

// Filter by delta type
const deltaOnlyOptions: CheckpointStorageListOptions = {
  type: "DELTA",
  limit: 50,
};

expectType<CheckpointStorageListOptions>(deltaOnlyOptions);

// =============================================================================
// Test 9: CheckpointInfo Structure
// =============================================================================

const checkpointInfo: CheckpointInfo = {
  checkpointId: "checkpoint-123",
  metadata: checkpointMetadata,
};

expectType<CheckpointInfo>(checkpointInfo);
expectType<string>(checkpointInfo.checkpointId);
expectType<CheckpointStorageMetadata>(checkpointInfo.metadata);

// =============================================================================
// Test 10: CleanupStrategyType Literal Types
// =============================================================================

const timeStrategy: CleanupStrategyType = "time";
const countStrategy: CleanupStrategyType = "count";
const sizeStrategy: CleanupStrategyType = "size";

expectAssignable<CleanupStrategyType>(timeStrategy);
expectAssignable<CleanupStrategyType>(countStrategy);
expectAssignable<CleanupStrategyType>(sizeStrategy);

// =============================================================================
// Test 11: TimeBasedCleanupPolicy
// =============================================================================

const timePolicy: TimeBasedCleanupPolicy = {
  type: "time",
  retentionDays: 30,
  minRetention: 5,
};

expectType<TimeBasedCleanupPolicy>(timePolicy);
expectType<"time">(timePolicy.type);
expectType<number>(timePolicy.retentionDays);
expectType<number | undefined>(timePolicy.minRetention);

// Without minRetention
const timePolicyNoMin: TimeBasedCleanupPolicy = {
  type: "time",
  retentionDays: 7,
};

expectType<TimeBasedCleanupPolicy>(timePolicyNoMin);

// =============================================================================
// Test 12: CountBasedCleanupPolicy
// =============================================================================

const countPolicy: CountBasedCleanupPolicy = {
  type: "count",
  maxCount: 100,
  minRetention: 10,
};

expectType<CountBasedCleanupPolicy>(countPolicy);
expectType<"count">(countPolicy.type);
expectType<number>(countPolicy.maxCount);
expectType<number | undefined>(countPolicy.minRetention);

// Aggressive cleanup
const aggressiveCountPolicy: CountBasedCleanupPolicy = {
  type: "count",
  maxCount: 10,
};

expectType<CountBasedCleanupPolicy>(aggressiveCountPolicy);

// =============================================================================
// Test 13: SizeBasedCleanupPolicy
// =============================================================================

const sizePolicy: SizeBasedCleanupPolicy = {
  type: "size",
  maxSizeBytes: 1073741824, // 1 GB
  minRetention: 20,
};

expectType<SizeBasedCleanupPolicy>(sizePolicy);
expectType<"size">(sizePolicy.type);
expectType<number>(sizePolicy.maxSizeBytes);
expectType<number | undefined>(sizePolicy.minRetention);

// Small size limit
const smallSizePolicy: SizeBasedCleanupPolicy = {
  type: "size",
  maxSizeBytes: 104857600, // 100 MB
};

expectType<SizeBasedCleanupPolicy>(smallSizePolicy);

// =============================================================================
// Test 14: CleanupPolicy Union Type
// =============================================================================

declare const anyPolicy: CleanupPolicy;

// Type narrowing
if (anyPolicy.type === "time") {
  expectType<TimeBasedCleanupPolicy>(anyPolicy);
  expectType<number>(anyPolicy.retentionDays);
} else if (anyPolicy.type === "count") {
  expectType<CountBasedCleanupPolicy>(anyPolicy);
  expectType<number>(anyPolicy.maxCount);
} else if (anyPolicy.type === "size") {
  expectType<SizeBasedCleanupPolicy>(anyPolicy);
  expectType<number>(anyPolicy.maxSizeBytes);
}

// All policies are assignable to union
const policies: CleanupPolicy[] = [timePolicy, countPolicy, sizePolicy];

expectType<CleanupPolicy[]>(policies);

// =============================================================================
// Test 15: CleanupResult Structure
// =============================================================================

const cleanupResult: CleanupResult = {
  deletedCheckpointIds: ["cp-1", "cp-2", "cp-3"],
  deletedCount: 3,
  freedSpaceBytes: 1048576, // 1 MB
  remainingCount: 47,
};

expectType<CleanupResult>(cleanupResult);
expectType<string[]>(cleanupResult.deletedCheckpointIds);
expectType<number>(cleanupResult.deletedCount);
expectType<number>(cleanupResult.freedSpaceBytes);
expectType<number>(cleanupResult.remainingCount);

// No checkpoints deleted
const noCleanupResult: CleanupResult = {
  deletedCheckpointIds: [],
  deletedCount: 0,
  freedSpaceBytes: 0,
  remainingCount: 50,
};

expectType<CleanupResult>(noCleanupResult);

// =============================================================================
// Test 16: CheckpointCleanupStrategy Interface
// =============================================================================

class TimeBasedStrategy implements CheckpointCleanupStrategy {
  private retentionDays: number;

  constructor(retentionDays: number) {
    this.retentionDays = retentionDays;
  }

  execute(checkpoints: CheckpointInfo[]): string[] {
    const cutoffTime = Date.now() - this.retentionDays * 86400000;
    return checkpoints
      .filter(cp => cp.metadata.timestamp < cutoffTime)
      .map(cp => cp.checkpointId);
  }
}

const strategy = new TimeBasedStrategy(30);
const checkpointsToDelete = strategy.execute([checkpointInfo]);

expectType<string[]>(checkpointsToDelete);

// Strategy interface compliance
const strategyInterface: CheckpointCleanupStrategy = strategy;
expectType<CheckpointCleanupStrategy>(strategyInterface);

// =============================================================================
// Test 17: Integration Pattern - Workflow Repository
// =============================================================================

interface WorkflowRepository {
  list(options?: WorkflowListOptions): Promise<WorkflowInfo[]>;
  get(workflowId: string): Promise<WorkflowInfo | null>;
  save(metadata: WorkflowStorageMetadata): Promise<void>;
  delete(workflowId: string): Promise<void>;
  getStats(): Promise<WorkflowStats>;
}

declare const repo: WorkflowRepository;

// Usage patterns
const workflows = await repo.list({ limit: 10, sortBy: "updatedAt" });
expectType<WorkflowInfo[]>(workflows);

const workflowStatsResult = await repo.getStats();
expectType<WorkflowStats>(workflowStatsResult);

// =============================================================================
// Test 18: Integration Pattern - Checkpoint Manager
// =============================================================================

interface CheckpointManager {
  list(options?: CheckpointStorageListOptions): Promise<CheckpointInfo[]>;
  applyCleanup(policy: CleanupPolicy): Promise<CleanupResult>;
  getStrategy(): CheckpointCleanupStrategy;
}

declare const manager: CheckpointManager;

// Usage patterns
const checkpoints = await manager.list({ type: "FULL", limit: 5 });
expectType<CheckpointInfo[]>(checkpoints);

const result = await manager.applyCleanup(countPolicy);
expectType<CleanupResult>(result);

const cleanupStrategy = manager.getStrategy();
expectType<CheckpointCleanupStrategy>(cleanupStrategy);

// =============================================================================
// Test 19: Integration Pattern - Storage Configuration
// =============================================================================

interface StorageConfig {
  workflowRetention?: WorkflowListOptions;
  checkpointCleanup?: CleanupPolicy;
  maxWorkflows?: number;
  maxCheckpointsPerExecution?: number;
}

const config: StorageConfig = {
  workflowRetention: {
    limit: 100,
    sortBy: "createdAt",
    sortOrder: "desc",
  },
  checkpointCleanup: timePolicy,
  maxWorkflows: 500,
  maxCheckpointsPerExecution: 50,
};

expectType<StorageConfig>(config);
expectType<WorkflowListOptions | undefined>(config.workflowRetention);
expectType<CleanupPolicy | undefined>(config.checkpointCleanup);
expectType<number | undefined>(config.maxWorkflows);
expectType<number | undefined>(config.maxCheckpointsPerExecution);
