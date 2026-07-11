/**
 * Checkpoint Type Tests
 * 
 * Tests for checkpoint types including:
 * - BaseCheckpoint and generic types
 * - FullCheckpoint and DeltaCheckpoint
 * - CheckpointMetadata and CheckpointOptions
 * - DeltaStorageConfig
 * - Checkpoint trigger types and config results
 * 
 * Priority: MEDIUM (Phase 2)
 */

import { expectType, expectAssignable } from "tsd";
import type {
  SnapshotBase,
  SnapshotVersion,
  CheckpointMetadata,
  CheckpointOptions,
  DeltaStorageConfig,
  CheckpointConfigSource,
  WorkflowCheckpointTriggerType,
  AgentLoopCheckpointTriggerType,
  CheckpointConfigResult,
  CheckpointListOptions,
  BaseCheckpointCore,
  BaseCheckpoint,
  FullCheckpoint,
  DeltaCheckpoint,
  AnyCheckpoint,
} from "../../../src/index.js";
import type { TCheckpointType as CheckpointType } from "../../../src/index.js";

// =============================================================================
// Test 1: SnapshotBase Structure
// =============================================================================

const snapshotBase: SnapshotBase = {
  _version: 1,
  _timestamp: Date.now(),
  _entityType: "checkpoint",
};

expectType<SnapshotBase>(snapshotBase);
expectType<SnapshotVersion>(snapshotBase._version);
expectType<number>(snapshotBase._timestamp);
expectType<string>(snapshotBase._entityType);

// Different entity types
const taskSnapshot: SnapshotBase = {
  _version: 2,
  _timestamp: Date.now(),
  _entityType: "task",
};

expectType<SnapshotBase>(taskSnapshot);

// =============================================================================
// Test 2: CheckpointType Literal Types
// =============================================================================

const fullType: CheckpointType = "FULL";
const deltaType: CheckpointType = "DELTA";

expectAssignable<CheckpointType>(fullType);
expectAssignable<CheckpointType>(deltaType);

// =============================================================================
// Test 3: CheckpointMetadata Structure
// =============================================================================

const metadata: CheckpointMetadata = {
  description: "Before tool execution",
  tags: ["tool", "pre-execution"],
  customFields: {
    nodeId: "node-123",
    toolName: "search",
  },
};

expectType<CheckpointMetadata>(metadata);
expectType<string | undefined>(metadata.description);
expectType<string[] | undefined>(metadata.tags);
expectType<Record<string, unknown> | undefined>(metadata.customFields);

// Minimal metadata
const minimalMetadata: CheckpointMetadata = {};

expectType<CheckpointMetadata>(minimalMetadata);

// =============================================================================
// Test 4: CheckpointOptions
// =============================================================================

const syncOptions: CheckpointOptions = {
  sync: true,
  syncTimeout: 5000,
};

expectType<CheckpointOptions>(syncOptions);
expectType<boolean | undefined>(syncOptions.sync);
expectType<number | undefined>(syncOptions.syncTimeout);

// Async options (default)
const asyncOptions: CheckpointOptions = {
  sync: false,
};

expectType<CheckpointOptions>(asyncOptions);

// Empty options
const emptyOptions: CheckpointOptions = {};

expectType<CheckpointOptions>(emptyOptions);

// =============================================================================
// Test 5: DeltaStorageConfig
// =============================================================================

const deltaConfig: DeltaStorageConfig = {
  enabled: true,
  baselineInterval: 10,
  maxDeltaChainLength: 20,
};

expectType<DeltaStorageConfig>(deltaConfig);
expectType<boolean>(deltaConfig.enabled);
expectType<number>(deltaConfig.baselineInterval);
expectType<number>(deltaConfig.maxDeltaChainLength);

// Disabled delta storage
const disabledDeltaConfig: DeltaStorageConfig = {
  enabled: false,
  baselineInterval: 10,
  maxDeltaChainLength: 20,
};

expectType<DeltaStorageConfig>(disabledDeltaConfig);

// =============================================================================
// Test 6: CheckpointConfigSource Literal Types
// =============================================================================

const runtimeSource: CheckpointConfigSource = "runtime";
const workflowSource: CheckpointConfigSource = "workflow";
const nodeSource: CheckpointConfigSource = "node";
const agentSource: CheckpointConfigSource = "agent";
const globalSource: CheckpointConfigSource = "global";
const defaultSource: CheckpointConfigSource = "default";

expectAssignable<CheckpointConfigSource>(runtimeSource);
expectAssignable<CheckpointConfigSource>(workflowSource);
expectAssignable<CheckpointConfigSource>(nodeSource);
expectAssignable<CheckpointConfigSource>(agentSource);
expectAssignable<CheckpointConfigSource>(globalSource);
expectAssignable<CheckpointConfigSource>(defaultSource);

// =============================================================================
// Test 7: WorkflowCheckpointTriggerType Literal Types
// =============================================================================

const nodeBeforeTrigger: WorkflowCheckpointTriggerType = "NODE_BEFORE_EXECUTE";
const nodeAfterTrigger: WorkflowCheckpointTriggerType = "NODE_AFTER_EXECUTE";
const toolBeforeTrigger: WorkflowCheckpointTriggerType = "TOOL_BEFORE";
const toolAfterTrigger: WorkflowCheckpointTriggerType = "TOOL_AFTER";
const hookTrigger: WorkflowCheckpointTriggerType = "HOOK";
const triggerTrigger: WorkflowCheckpointTriggerType = "TRIGGER";

expectAssignable<WorkflowCheckpointTriggerType>(nodeBeforeTrigger);
expectAssignable<WorkflowCheckpointTriggerType>(nodeAfterTrigger);
expectAssignable<WorkflowCheckpointTriggerType>(toolBeforeTrigger);
expectAssignable<WorkflowCheckpointTriggerType>(toolAfterTrigger);
expectAssignable<WorkflowCheckpointTriggerType>(hookTrigger);
expectAssignable<WorkflowCheckpointTriggerType>(triggerTrigger);

// =============================================================================
// Test 8: AgentLoopCheckpointTriggerType Literal Types
// =============================================================================

const iterationEndTrigger: AgentLoopCheckpointTriggerType = "ITERATION_END";
const errorTrigger: AgentLoopCheckpointTriggerType = "ERROR";
const intervalTrigger: AgentLoopCheckpointTriggerType = "INTERVAL";

expectAssignable<AgentLoopCheckpointTriggerType>(iterationEndTrigger);
expectAssignable<AgentLoopCheckpointTriggerType>(errorTrigger);
expectAssignable<AgentLoopCheckpointTriggerType>(intervalTrigger);

// =============================================================================
// Test 9: CheckpointConfigResult
// =============================================================================

const shouldCreateConfig: CheckpointConfigResult = {
  shouldCreate: true,
  description: "Node execution checkpoint",
  effectiveSource: "node",
  triggerType: "NODE_AFTER_EXECUTE",
};

expectType<CheckpointConfigResult>(shouldCreateConfig);
expectType<boolean>(shouldCreateConfig.shouldCreate);
expectType<string | undefined>(shouldCreateConfig.description);
expectType<CheckpointConfigSource>(shouldCreateConfig.effectiveSource);
expectType<WorkflowCheckpointTriggerType | AgentLoopCheckpointTriggerType | undefined>(
  shouldCreateConfig.triggerType
);

// Should not create config
const noCreateConfig: CheckpointConfigResult = {
  shouldCreate: false,
  effectiveSource: "default",
};

expectType<CheckpointConfigResult>(noCreateConfig);

// Agent loop trigger type
const agentConfig: CheckpointConfigResult = {
  shouldCreate: true,
  description: "Iteration end checkpoint",
  effectiveSource: "agent",
  triggerType: "ITERATION_END",
};

expectType<CheckpointConfigResult>(agentConfig);

// =============================================================================
// Test 10: CheckpointListOptions
// =============================================================================

const listOptions: CheckpointListOptions = {
  parentId: "exec-123",
  tags: ["important", "recovery"],
  limit: 10,
  offset: 0,
};

expectType<CheckpointListOptions>(listOptions);
expectType<string | undefined>(listOptions.parentId);
expectType<string[] | undefined>(listOptions.tags);
expectType<number | undefined>(listOptions.limit);
expectType<number | undefined>(listOptions.offset);

// Minimal options
const minimalListOptions: CheckpointListOptions = {};

expectType<CheckpointListOptions>(minimalListOptions);

// =============================================================================
// Test 11: BaseCheckpointCore Generic Interface
// =============================================================================

interface TestDelta {
  messagesAdded: number;
  variablesChanged: string[];
}

interface TestSnapshot {
  messages: Array<{ role: string; content: string }>;
  variables: Record<string, unknown>;
}

const fullCheckpointCore: BaseCheckpointCore<never, TestSnapshot> = {
  id: "checkpoint-full-1",
  type: "FULL",
  snapshot: {
    messages: [{ role: "user", content: "Hello" }],
    variables: { count: 5 },
  },
  timestamp: Date.now(),
};

expectType<BaseCheckpointCore<never, TestSnapshot>>(fullCheckpointCore);
expectType<string>(fullCheckpointCore.id);
expectType<CheckpointType | undefined>(fullCheckpointCore.type);
expectType<TestSnapshot | undefined>(fullCheckpointCore.snapshot);

const deltaCheckpointCore: BaseCheckpointCore<TestDelta, never> = {
  id: "checkpoint-delta-1",
  type: "DELTA",
  baseCheckpointId: "base-123",
  previousCheckpointId: "prev-456",
  delta: {
    messagesAdded: 2,
    variablesChanged: ["count", "status"],
  },
  timestamp: Date.now(),
};

expectType<BaseCheckpointCore<TestDelta, never>>(deltaCheckpointCore);
expectType<string>(deltaCheckpointCore.id);
expectType<CheckpointType | undefined>(deltaCheckpointCore.type);
expectType<string | undefined>(deltaCheckpointCore.baseCheckpointId);
expectType<string | undefined>(deltaCheckpointCore.previousCheckpointId);
expectType<TestDelta | undefined>(deltaCheckpointCore.delta);

// =============================================================================
// Test 12: BaseCheckpoint with Metadata
// =============================================================================

const fullCheckpointWithMeta: BaseCheckpoint<never, TestSnapshot> = {
  id: "checkpoint-meta-full",
  type: "FULL",
  snapshot: {
    messages: [],
    variables: {},
  },
  metadata: {
    description: "Full state snapshot",
    tags: ["full"],
  },
};

expectType<BaseCheckpoint<never, TestSnapshot>>(fullCheckpointWithMeta);
expectType<CheckpointMetadata | undefined>(fullCheckpointWithMeta.metadata);

const deltaCheckpointWithMeta: BaseCheckpoint<TestDelta, never> = {
  id: "checkpoint-meta-delta",
  type: "DELTA",
  baseCheckpointId: "base-789",
  previousCheckpointId: "prev-012",
  delta: {
    messagesAdded: 1,
    variablesChanged: ["status"],
  },
  metadata: {
    description: "Incremental changes",
    tags: ["delta"],
  },
};

expectType<BaseCheckpoint<TestDelta, never>>(deltaCheckpointWithMeta);

// =============================================================================
// Test 13: FullCheckpoint Specific Type
// =============================================================================

const strictFullCheckpoint: FullCheckpoint<TestSnapshot> = {
  id: "strict-full-1",
  type: "FULL",
  snapshot: {
    messages: [{ role: "assistant", content: "Response" }],
    variables: { status: "active" },
  },
  timestamp: Date.now(),
};

expectType<FullCheckpoint<TestSnapshot>>(strictFullCheckpoint);
expectType<"FULL">(strictFullCheckpoint.type);
expectType<TestSnapshot>(strictFullCheckpoint.snapshot);
// delta should not exist on FullCheckpoint
expectType<undefined>(strictFullCheckpoint.delta);

// =============================================================================
// Test 14: DeltaCheckpoint Specific Type
// =============================================================================

const strictDeltaCheckpoint: DeltaCheckpoint<TestDelta> = {
  id: "strict-delta-1",
  type: "DELTA",
  baseCheckpointId: "base-checkpoint",
  previousCheckpointId: "prev-checkpoint",
  delta: {
    messagesAdded: 3,
    variablesChanged: ["x", "y", "z"],
  },
  timestamp: Date.now(),
};

expectType<DeltaCheckpoint<TestDelta>>(strictDeltaCheckpoint);
expectType<"DELTA">(strictDeltaCheckpoint.type);
expectType<string>(strictDeltaCheckpoint.baseCheckpointId);
expectType<string>(strictDeltaCheckpoint.previousCheckpointId);
expectType<TestDelta>(strictDeltaCheckpoint.delta);
// snapshot should not exist on DeltaCheckpoint
expectType<undefined>(strictDeltaCheckpoint.snapshot);

// =============================================================================
// Test 15: AnyCheckpoint Union Type
// =============================================================================

declare const anyCheckpoint: AnyCheckpoint<TestDelta, TestSnapshot>;

// Type guard simulation
if (anyCheckpoint.type === "FULL") {
  expectType<FullCheckpoint<TestSnapshot>>(anyCheckpoint);
  expectType<TestSnapshot>(anyCheckpoint.snapshot);
} else if (anyCheckpoint.type === "DELTA") {
  expectType<DeltaCheckpoint<TestDelta>>(anyCheckpoint);
  expectType<TestDelta>(anyCheckpoint.delta);
  expectType<string>(anyCheckpoint.baseCheckpointId);
  expectType<string>(anyCheckpoint.previousCheckpointId);
}

// Assign both types to union
const fullAsAny: AnyCheckpoint<TestDelta, TestSnapshot> = strictFullCheckpoint;
const deltaAsAny: AnyCheckpoint<TestDelta, TestSnapshot> = strictDeltaCheckpoint;

expectAssignable<AnyCheckpoint<TestDelta, TestSnapshot>>(fullAsAny);
expectAssignable<AnyCheckpoint<TestDelta, TestSnapshot>>(deltaAsAny);

// =============================================================================
// Test 16: Integration Pattern - Checkpoint with Execution Context
// =============================================================================

interface CheckpointWithContext {
  checkpoint: AnyCheckpoint<TestDelta, TestSnapshot>;
  executionId: string;
  workflowId: string;
  createdAt: number;
}

const checkpointWithContext: CheckpointWithContext = {
  checkpoint: strictFullCheckpoint,
  executionId: "exec-123",
  workflowId: "workflow-456",
  createdAt: Date.now(),
};

expectType<CheckpointWithContext>(checkpointWithContext);
expectType<AnyCheckpoint<TestDelta, TestSnapshot>>(checkpointWithContext.checkpoint);

// =============================================================================
// Test 17: Integration Pattern - Checkpoint Chain
// =============================================================================

interface CheckpointChain {
  baseline: FullCheckpoint<TestSnapshot>;
  deltas: DeltaCheckpoint<TestDelta>[];
}

const chain: CheckpointChain = {
  baseline: strictFullCheckpoint,
  deltas: [strictDeltaCheckpoint],
};

expectType<CheckpointChain>(chain);
expectType<FullCheckpoint<TestSnapshot>>(chain.baseline);
expectType<DeltaCheckpoint<TestDelta>[]>(chain.deltas);
