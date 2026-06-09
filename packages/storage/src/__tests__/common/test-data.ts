/**
 * Common Test Data Factories for Storage Integration Tests
 *
 * Provides factory functions to create consistent test data for all storage backends.
 * Each factory includes:
 * - A full/complete variant with all fields populated
 * - A minimal variant with only required fields
 * - Helper utilities for creating batches of related records
 */

import type {
  WorkflowStorageMetadata,
  WorkflowListOptions,
  WorkflowVersionListOptions,
  WorkflowExecutionStorageMetadata,
  WorkflowExecutionListOptions,
  TaskStorageMetadata,
  TaskListOptions,
  TaskStatus,
  AgentEntityMetadata,
  AgentEntityListOptions,
  AgentLoopStatus,
  CheckpointStorageMetadata,
  CheckpointStorageListOptions,
  CheckpointEntityType,
  FileCheckpointMetadata,
  FileCheckpointListOptions,
} from "@wf-agent/types";
import type { MetricDataPoint, MetricsQuery } from "../../types/adapter/metrics-storage-adapter.js";
import type { CheckpointOptions } from "../../types/checkpoint-options.js";

// =============================================================================
// Shared Constants
// =============================================================================

export const TEST_WORKFLOW_ID = "test-workflow-001";
export const TEST_EXECUTION_ID = "test-execution-001";
export const TEST_AGENT_LOOP_ID = "test-agent-loop-001";
export const TEST_TASK_ID = "test-task-001";
export const TEST_CHECKPOINT_ID = "test-checkpoint-001";
export const TEST_FILE_CHECKPOINT_ID = "test-file-chk-001";
export const TEST_ENTITY_ID = "test-entity-001";

// =============================================================================
// Binary data helpers
// =============================================================================

export function createTestData(size: number = 64): Uint8Array {
  const data = new Uint8Array(size);
  for (let i = 0; i < size; i++) {
    data[i] = i % 256;
  }
  return data;
}

export function createTestBuffer(size: number = 64): Buffer {
  return Buffer.from(createTestData(size));
}

export function createTestMetadataMap(): Map<string, Buffer> {
  const map = new Map<string, Buffer>();
  map.set("file1.txt", Buffer.from("file1 content"));
  map.set("subdir/file2.txt", Buffer.from("file2 content"));
  map.set("config.json", Buffer.from(JSON.stringify({ key: "value" })));
  return map;
}

// =============================================================================
// Workflow Storage
// =============================================================================

export function createWorkflowMetadata(
  overrides?: Partial<WorkflowStorageMetadata>,
): WorkflowStorageMetadata {
  return {
    workflowId: TEST_WORKFLOW_ID,
    name: "Test Workflow",
    version: "1.0.0",
    description: "A test workflow for integration testing",
    author: "test-author",
    category: "integration-test",
    tags: ["test", "integration", "e2e"],
    createdAt: Date.now() - 3600000,
    updatedAt: Date.now(),
    nodeCount: 5,
    edgeCount: 4,
    enabled: true,
    customFields: { env: "testing", priority: "high" },
    ...overrides,
  };
}

export function createMinimalWorkflowMetadata(): WorkflowStorageMetadata {
  return {
    workflowId: "minimal-workflow-001",
    name: "Minimal Workflow",
    version: "0.1.0",
    createdAt: Date.now(),
    updatedAt: Date.now(),
    nodeCount: 1,
    edgeCount: 0,
  };
}

export function createWorkflowListOptions(
  overrides?: Partial<WorkflowListOptions>,
): WorkflowListOptions {
  return {
    name: "Test",
    author: "test",
    category: "integration-test",
    tags: ["test"],
    enabled: true,
    ...overrides,
  };
}

export function createWorkflowVersionEntry(
  workflowId: string = TEST_WORKFLOW_ID,
  version: string = "1.0.0",
): { version: string; data: Uint8Array; changeNote?: string } {
  return {
    version,
    data: createTestData(32),
    changeNote: `Version ${version} of workflow ${workflowId}`,
  };
}

export function createWorkflowVersionListOptions(
  overrides?: Partial<WorkflowVersionListOptions>,
): WorkflowVersionListOptions {
  return { limit: 10, offset: 0, ...overrides };
}

// =============================================================================
// Workflow Execution Storage
// =============================================================================

export function createWorkflowExecutionMetadata(
  overrides?: Partial<WorkflowExecutionStorageMetadata>,
): WorkflowExecutionStorageMetadata {
  return {
    executionId: TEST_EXECUTION_ID,
    workflowId: TEST_WORKFLOW_ID,
    workflowVersion: "1.0.0",
    status: "RUNNING",
    executionType: "MAIN",
    currentNodeId: "node-001",
    startTime: Date.now() - 60000,
    tags: ["test", "integration"],
    customFields: { triggeredBy: "api" },
    ...overrides,
  };
}

export function createMinimalExecutionMetadata(): WorkflowExecutionStorageMetadata {
  return {
    executionId: "minimal-exec-001",
    workflowId: "minimal-wf-001",
    workflowVersion: "0.1.0",
    status: "CREATED",
    startTime: Date.now(),
  };
}

export function createExecutionListOptions(
  overrides?: Partial<WorkflowExecutionListOptions>,
): WorkflowExecutionListOptions {
  return {
    workflowId: TEST_WORKFLOW_ID,
    status: "RUNNING",
    limit: 20,
    offset: 0,
    ...overrides,
  };
}

// =============================================================================
// Task Storage
// =============================================================================

export function createTaskMetadata(
  overrides?: Partial<TaskStorageMetadata>,
): TaskStorageMetadata {
  return {
    taskId: TEST_TASK_ID,
    executionId: TEST_EXECUTION_ID,
    workflowId: TEST_WORKFLOW_ID,
    status: "RUNNING",
    submitTime: Date.now() - 30000,
    startTime: Date.now() - 25000,
    timeout: 60000,
    tags: ["test", "data-processing"],
    ...overrides,
  };
}

export function createMinimalTaskMetadata(): TaskStorageMetadata {
  return {
    taskId: "minimal-task-001",
    executionId: "minimal-exec-001",
    workflowId: "minimal-wf-001",
    status: "QUEUED",
    submitTime: Date.now(),
  };
}

export function createTaskListOptions(
  overrides?: Partial<TaskListOptions>,
): TaskListOptions {
  return {
    executionId: TEST_EXECUTION_ID,
    status: "RUNNING",
    ...overrides,
  };
}

export function createCompletedTaskMetadata(
  overrides?: Partial<TaskStorageMetadata>,
): TaskStorageMetadata {
  return {
    taskId: "completed-task-001",
    executionId: TEST_EXECUTION_ID,
    workflowId: TEST_WORKFLOW_ID,
    status: "COMPLETED",
    submitTime: Date.now() - 60000,
    startTime: Date.now() - 55000,
    completeTime: Date.now() - 10000,
    tags: ["test", "completed"],
    ...overrides,
  };
}

export function createFailedTaskMetadata(
  overrides?: Partial<TaskStorageMetadata>,
): TaskStorageMetadata {
  return {
    taskId: "failed-task-001",
    executionId: TEST_EXECUTION_ID,
    workflowId: TEST_WORKFLOW_ID,
    status: "FAILED",
    submitTime: Date.now() - 60000,
    startTime: Date.now() - 55000,
    completeTime: Date.now() - 5000,
    error: "Task execution timeout",
    errorStack: "Error: timeout\n    at Task.run (...)",
    tags: ["test", "error"],
    ...overrides,
  };
}

// =============================================================================
// Agent Loop Storage
// =============================================================================

export function createAgentEntityMetadata(
  overrides?: Partial<AgentEntityMetadata>,
): AgentEntityMetadata {
  return {
    agentLoopId: TEST_AGENT_LOOP_ID,
    status: "RUNNING",
    createdAt: Date.now() - 3600000,
    updatedAt: Date.now(),
    profileId: "profile-001",
    tags: ["test", "agent", "integration"],
    customFields: { model: "gpt-4" },
    ...overrides,
  };
}

export function createMinimalAgentEntityMetadata(): AgentEntityMetadata {
  return {
    agentLoopId: "minimal-agent-001",
    status: "CREATED",
    createdAt: Date.now(),
  };
}

export function createAgentEntityListOptions(
  overrides?: Partial<AgentEntityListOptions>,
): AgentEntityListOptions {
  return { status: "RUNNING", limit: 20, offset: 0, ...overrides };
}

// =============================================================================
// Checkpoint Storage
// =============================================================================

export function createCheckpointMetadata(
  overrides?: Partial<CheckpointStorageMetadata>,
): CheckpointStorageMetadata {
  return {
    entityType: "workflow" as CheckpointEntityType,
    entityId: TEST_ENTITY_ID,
    timestamp: Date.now(),
    tags: ["full", "recovery"],
    customFields: { nodeId: "node-001", iteration: 1 },
    ...overrides,
  };
}

export function createMinimalCheckpointMetadata(): CheckpointStorageMetadata {
  return {
    entityType: "workflow" as CheckpointEntityType,
    entityId: "minimal-entity-001",
    timestamp: Date.now(),
  };
}

export function createCheckpointListOptions(
  overrides?: Partial<CheckpointStorageListOptions>,
): CheckpointStorageListOptions {
  return {
    entityType: "workflow" as CheckpointEntityType,
    entityId: TEST_ENTITY_ID,
    limit: 20,
    offset: 0,
    ...overrides,
  };
}

export function createCheckpointOptions(
  overrides?: Partial<CheckpointOptions>,
): CheckpointOptions {
  return { sync: false, ...overrides };
}

// =============================================================================
// File Checkpoint Store
// =============================================================================

export function createFileCheckpointMetadata(
  overrides?: Partial<FileCheckpointMetadata>,
): FileCheckpointMetadata {
  return {
    entityId: TEST_EXECUTION_ID,
    timestamp: Date.now(),
    type: "full",
    fileCount: 3,
    fileHashSnapshot: {
      "file1.txt": "hash1",
      "subdir/file2.txt": "hash2",
      "config.json": "hash3",
    },
    emptyDirs: ["subdir"],
    totalSize: 128,
    workspaceRoot: "/workspace/test",
    tags: ["backup", "milestone"],
    ...overrides,
  };
}

export function createMinimalFileCheckpointMetadata(): FileCheckpointMetadata {
  return {
    entityId: "minimal-exec-001",
    timestamp: Date.now(),
    type: "full",
    fileCount: 1,
    fileHashSnapshot: { "readme.md": "hash0" },
    emptyDirs: [],
    totalSize: 32,
    workspaceRoot: "/workspace/minimal",
  };
}

export function createFileCheckpointListOptions(
  overrides?: Partial<FileCheckpointListOptions>,
): FileCheckpointListOptions {
  return {
    entityId: TEST_EXECUTION_ID,
    limit: 20,
    offset: 0,
    ...overrides,
  };
}

// =============================================================================
// Metrics Storage
// =============================================================================

export function createMetricDataPoint(
  overrides?: Partial<MetricDataPoint>,
): MetricDataPoint {
  return {
    metricName: "workflow.execution.count",
    metricType: "counter",
    value: 1,
    timestamp: Date.now(),
    labels: { workflowId: TEST_WORKFLOW_ID, status: "COMPLETED" },
    collectorName: "workflow-collector",
    ...overrides,
  };
}

export function createMetricsQuery(
  overrides?: Partial<MetricsQuery>,
): MetricsQuery {
  return {
    metricName: "workflow.execution.count",
    startTime: Date.now() - 3600000,
    endTime: Date.now(),
    ...overrides,
  };
}

// =============================================================================
// Batch generators
// =============================================================================

export function createWorkflowBatch(count: number): Array<{
  id: string;
  metadata: WorkflowStorageMetadata;
  data: Uint8Array;
}> {
  return Array.from({ length: count }, (_, i) => ({
    id: `batch-workflow-${String(i).padStart(3, "0")}`,
    metadata: createWorkflowMetadata({
      workflowId: `batch-workflow-${String(i).padStart(3, "0")}`,
      name: `Batch Workflow ${i}`,
    }),
    data: createTestData(16),
  }));
}

export function createExecutionBatch(count: number): Array<{
  id: string;
  metadata: WorkflowExecutionStorageMetadata;
  data: Uint8Array;
}> {
  return Array.from({ length: count }, (_, i) => ({
    id: `batch-exec-${String(i).padStart(3, "0")}`,
    metadata: createWorkflowExecutionMetadata({
      executionId: `batch-exec-${String(i).padStart(3, "0")}`,
      status: i % 2 === 0 ? "COMPLETED" : "FAILED",
    }),
    data: createTestData(16),
  }));
}

export function createCheckpointBatch(
  entityId: string,
  count: number,
): Array<{
  id: string;
  metadata: CheckpointStorageMetadata;
  data: Uint8Array;
}> {
  return Array.from({ length: count }, (_, i) => ({
    id: `chk-${entityId}-${String(i).padStart(3, "0")}`,
    metadata: createCheckpointMetadata({
      entityId,
      timestamp: Date.now() - (count - i) * 60000,
      tags: i === 0 ? ["full", "baseline"] : ["incremental"],
    }),
    data: createTestData(8),
  }));
}

export function createAgentEntityBatch(count: number): Array<{
  id: string;
  metadata: AgentEntityMetadata;
  data: Uint8Array;
}> {
  const statuses: AgentLoopStatus[] = [
    "CREATED",
    "RUNNING",
    "PAUSED",
    "COMPLETED",
    "FAILED",
    "CANCELLED",
  ];
  return Array.from({ length: count }, (_, i) => ({
    id: `batch-agent-${String(i).padStart(3, "0")}`,
    metadata: createAgentEntityMetadata({
      agentLoopId: `batch-agent-${String(i).padStart(3, "0")}`,
      status: statuses[i % statuses.length],
    }),
    data: createTestData(16),
  }));
}

export function createTaskBatch(count: number): Array<{
  id: string;
  metadata: TaskStorageMetadata;
  data: Uint8Array;
}> {
  const statuses: TaskStatus[] = [
    "QUEUED",
    "RUNNING",
    "COMPLETED",
    "FAILED",
    "CANCELLED",
    "TIMEOUT",
  ];
  return Array.from({ length: count }, (_, i) => ({
    id: `batch-task-${String(i).padStart(3, "0")}`,
    metadata: createTaskMetadata({
      taskId: `batch-task-${String(i).padStart(3, "0")}`,
      status: statuses[i % statuses.length],
    }),
    data: createTestData(16),
  }));
}