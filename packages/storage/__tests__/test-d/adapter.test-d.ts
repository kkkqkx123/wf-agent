/**
 * Type tests for storage adapter interfaces
 * Validates that adapter interfaces have correct type signatures
 */

import { expectType, expectAssignable, expectNotAssignable } from 'tsd';
import type {
  BaseStorageAdapter,
  StorageLifecycle,
  AgentLoopStorageAdapter,
  AgentLoopCheckpointStorageAdapter,
  CheckpointStorageAdapter,
  TaskStorageAdapter,
  WorkflowStorageAdapter,
  WorkflowExecutionStorageAdapter,
} from '../../src/types/adapter/index.js';
import type { StorageMetrics } from '../../src/types/metrics.js';
import type {
  AgentEntityMetadata,
  AgentEntityListOptions,
  AgentCheckpointMetadata,
  AgentCheckpointListOptions,
  CheckpointStorageMetadata,
  CheckpointStorageListOptions,
  TaskStorageMetadata,
  TaskListOptions,
  TaskStats,
  TaskStatsOptions,
  WorkflowStorageMetadata,
  WorkflowListOptions,
  WorkflowVersionInfo,
  WorkflowVersionListOptions,
  WorkflowExecutionStorageMetadata,
  WorkflowExecutionListOptions,
  AgentLoopStatus,
  WorkflowExecutionStatus,
} from '@wf-agent/types';
import type { CheckpointOptions } from '../../src/types/checkpoint-options.js';

// Test StorageLifecycle interface
declare const lifecycle: StorageLifecycle;
expectType<Promise<void>>(lifecycle.initialize());
expectType<Promise<void>>(lifecycle.close());
expectType<Promise<void>>(lifecycle.clear());

// Test BaseStorageAdapter interface
declare const baseAdapter: BaseStorageAdapter<AgentEntityMetadata, AgentEntityListOptions>;

// Test lifecycle methods
expectType<Promise<void>>(baseAdapter.initialize());
expectType<Promise<void>>(baseAdapter.close());
expectType<Promise<void>>(baseAdapter.clear());

// Test CRUD operations
const testId = 'test-id';
const testData = new Uint8Array([1, 2, 3]);
const testMetadata: AgentEntityMetadata = {
  agentLoopId: 'agent-1',
  status: 'RUNNING' as AgentLoopStatus,
  createdAt: Date.now(),
};

expectType<Promise<void>>(baseAdapter.save(testId, testData, testMetadata));
expectType<Promise<Uint8Array | null>>(baseAdapter.load(testId));
expectType<Promise<void>>(baseAdapter.delete(testId));
expectType<Promise<string[]>>(baseAdapter.list());
expectType<Promise<string[]>>(baseAdapter.list({ status: 'RUNNING' as AgentLoopStatus }));
expectType<Promise<boolean>>(baseAdapter.exists(testId));
expectType<Promise<AgentEntityMetadata | null>>(baseAdapter.getMetadata(testId));

// Test metrics
expectType<Promise<StorageMetrics>>(baseAdapter.getMetrics());
expectType<void>(baseAdapter.resetMetrics());

// Test batch operations
const batchItems = [
  { id: 'id1', data: testData, metadata: testMetadata },
  { id: 'id2', data: testData, metadata: testMetadata },
];
expectType<Promise<void>>(baseAdapter.saveBatch(batchItems));
expectType<Promise<Array<{ id: string; data: Uint8Array | null }>>>(baseAdapter.loadBatch(['id1', 'id2']));
expectType<Promise<void>>(baseAdapter.deleteBatch(['id1', 'id2']));

// Test AgentLoopStorageAdapter
declare const agentLoopAdapter: AgentLoopStorageAdapter;

// Inherited methods should work
expectType<Promise<void>>(agentLoopAdapter.save(testId, testData, testMetadata));
expectType<Promise<AgentEntityMetadata | null>>(agentLoopAdapter.getMetadata(testId));

// Agent loop specific methods
expectType<Promise<void>>(agentLoopAdapter.updateAgentLoopStatus('agent-1', 'COMPLETED' as AgentLoopStatus));
expectType<Promise<string[]>>(agentLoopAdapter.listByStatus('RUNNING' as AgentLoopStatus));
expectType<Promise<{ total: number; byStatus: Record<string, number> }>>(agentLoopAdapter.getAgentLoopStats());

// Test AgentLoopCheckpointStorageAdapter
declare const agentCheckpointAdapter: AgentLoopCheckpointStorageAdapter;

const agentCheckpointMetadata: AgentCheckpointMetadata = {
  agentLoopId: 'agent-1',
  timestamp: Date.now(),
  type: 'FULL',
};

expectType<Promise<void>>(agentCheckpointAdapter.save('cp-1', testData, agentCheckpointMetadata));
expectType<Promise<string[]>>(agentCheckpointAdapter.listByAgentLoop('agent-1'));
expectType<Promise<string[]>>(agentCheckpointAdapter.listByAgentLoop('agent-1', { type: 'DELTA' }));
expectType<Promise<string | null>>(agentCheckpointAdapter.getLatestCheckpoint('agent-1'));
expectType<Promise<number>>(agentCheckpointAdapter.deleteByAgentLoop('agent-1'));

// Test CheckpointStorageAdapter (type alias)
declare const checkpointAdapter: CheckpointStorageAdapter;

const checkpointMetadata: CheckpointStorageMetadata = {
  executionId: 'exec-1',
  workflowId: 'wf-1',
  timestamp: Date.now(),
};

const checkpointOptions: CheckpointOptions = {
  sync: true,
  syncTimeout: 5000,
};

expectType<Promise<void>>(checkpointAdapter.save('cp-1', testData, checkpointMetadata, checkpointOptions));
expectType<Promise<void>>(checkpointAdapter.save('cp-1', testData, checkpointMetadata));
expectType<Promise<Uint8Array | null>>(checkpointAdapter.load('cp-1'));
expectType<Promise<CheckpointStorageMetadata | null>>(checkpointAdapter.getMetadata('cp-1'));

// Test TaskStorageAdapter
declare const taskAdapter: TaskStorageAdapter;

const taskMetadata: TaskStorageMetadata = {
  taskId: 'task-1',
  executionId: 'exec-1',
  workflowId: 'wf-1',
  status: 'QUEUED',
  submitTime: Date.now(),
};

expectType<Promise<void>>(taskAdapter.save('task-1', testData, taskMetadata));
expectType<Promise<TaskStats>>(taskAdapter.getTaskStats());
expectType<Promise<TaskStats>>(taskAdapter.getTaskStats({ workflowId: 'wf-1' }));
expectType<Promise<number>>(taskAdapter.cleanupTasks(86400000)); // 24 hours in ms

// Test WorkflowStorageAdapter
declare const workflowAdapter: WorkflowStorageAdapter;

const workflowMetadata: WorkflowStorageMetadata = {
  workflowId: 'wf-1',
  name: 'Test Workflow',
  version: '1.0.0',
  createdAt: Date.now(),
  updatedAt: Date.now(),
  nodeCount: 5,
  edgeCount: 4,
};

expectType<Promise<void>>(workflowAdapter.save('wf-1', testData, workflowMetadata));
expectType<Promise<void>>(workflowAdapter.updateWorkflowMetadata('wf-1', { name: 'Updated Name' }));
expectType<Promise<void>>(workflowAdapter.updateWorkflowMetadata('wf-1', { tags: ['tag1', 'tag2'] }));

const versionData = new Uint8Array([4, 5, 6]);
expectType<Promise<void>>(workflowAdapter.saveWorkflowVersion('wf-1', '1.0.0', versionData));
expectType<Promise<void>>(workflowAdapter.saveWorkflowVersion('wf-1', '1.0.0', versionData, 'Initial version'));

expectType<Promise<WorkflowVersionInfo[]>>(workflowAdapter.listWorkflowVersions('wf-1'));
expectType<Promise<WorkflowVersionInfo[]>>(workflowAdapter.listWorkflowVersions('wf-1', { limit: 10 }));

expectType<Promise<Uint8Array | null>>(workflowAdapter.loadWorkflowVersion('wf-1', '1.0.0'));
expectType<Promise<void>>(workflowAdapter.deleteWorkflowVersion('wf-1', '1.0.0'));

// Test WorkflowExecutionStorageAdapter
declare const executionAdapter: WorkflowExecutionStorageAdapter;

const executionMetadata: WorkflowExecutionStorageMetadata = {
  executionId: 'exec-1',
  workflowId: 'wf-1',
  workflowVersion: '1.0.0',
  status: 'RUNNING' as WorkflowExecutionStatus,
  startTime: Date.now(),
};

expectType<Promise<void>>(executionAdapter.save('exec-1', testData, executionMetadata));
expectType<Promise<void>>(executionAdapter.updateExecutionStatus('exec-1', 'COMPLETED' as WorkflowExecutionStatus));
expectType<Promise<WorkflowExecutionStorageMetadata | null>>(executionAdapter.getMetadata('exec-1'));

// Test type assignability
// BaseStorageAdapter should be assignable to StorageLifecycle
expectAssignable<StorageLifecycle>(baseAdapter);

// Specific adapters should be assignable to BaseStorageAdapter
expectAssignable<BaseStorageAdapter<AgentEntityMetadata, AgentEntityListOptions>>(agentLoopAdapter);
expectAssignable<BaseStorageAdapter<AgentCheckpointMetadata, AgentCheckpointListOptions>>(agentCheckpointAdapter);
expectAssignable<BaseStorageAdapter<CheckpointStorageMetadata, CheckpointStorageListOptions, CheckpointOptions>>(checkpointAdapter);
expectAssignable<BaseStorageAdapter<TaskStorageMetadata, TaskListOptions>>(taskAdapter);
expectAssignable<BaseStorageAdapter<WorkflowStorageMetadata, WorkflowListOptions>>(workflowAdapter);
expectAssignable<BaseStorageAdapter<WorkflowExecutionStorageMetadata, WorkflowExecutionListOptions>>(executionAdapter);

// Test that incorrect types are not assignable
declare const invalidMetadata: { wrong: string };
expectNotAssignable<AgentEntityMetadata>(invalidMetadata);
