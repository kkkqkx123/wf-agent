import { describe, it, expect, vi, beforeEach } from 'vitest';
import { forkHandler } from '../fork-handler.js';
import type { WorkflowExecutionEntity } from '../../../../entities/workflow-execution-entity.js';
import type { RuntimeNode } from '@wf-agent/types';
import type { GlobalContext } from '../../../../../core/global-context.js';

const mockGlobalContext = {
  container: {
    get: vi.fn().mockReturnValue({ emit: vi.fn() }),
  },
} as unknown as GlobalContext;

const mockEntity = {
  id: 'exec-1',
  getStatus: vi.fn(),
  getNodeResults: vi.fn().mockReturnValue([]),
  hasSyncBarrier: vi.fn().mockReturnValue(false),
  getWorkflowId: vi.fn().mockReturnValue('wf-1'),
  getSyncBarrier: vi.fn().mockReturnValue({
    registerPath: vi.fn(),
  }),
  initializeSyncBarrier: vi.fn(),
} as unknown as WorkflowExecutionEntity;

const mockBuilder = {
  createChildExecution: vi.fn(),
};

const mockExecutor = {
  executeWorkflow: vi.fn(),
};

const mockContext = {
  executionBuilder: mockBuilder,
  workflowExecutor: mockExecutor,
};

const mockNode: RuntimeNode = {
  id: 'fork-node-1',
  type: 'FORK',
  config: {
    forkPaths: [{ id: 'branch-1', label: 'Branch 1' }],
  },
} as RuntimeNode;

beforeEach(() => {
  vi.clearAllMocks();
  (mockEntity.getStatus as any).mockReturnValue('RUNNING');
});

describe('forkHandler', () => {
  it('should return launchedBranches when status is RUNNING', async () => {
    mockBuilder.createChildExecution.mockResolvedValue({
      workflowExecutionEntity: { id: 'branch-exec-1' },
    });
    mockExecutor.executeWorkflow.mockResolvedValue({
      metadata: { status: 'COMPLETED' },
      executionTime: 10,
    });

    const result = await forkHandler(mockGlobalContext, mockEntity, mockNode, mockContext as any);

    expect(result).toHaveProperty('launchedBranches');
    expect(Array.isArray(result.launchedBranches)).toBe(true);
  });

  it('should return SKIPPED when status is not RUNNING', async () => {
    (mockEntity.getStatus as any).mockReturnValue('COMPLETED');

    const result = await forkHandler(mockGlobalContext, mockEntity, mockNode, mockContext as any);

    expect(result).toEqual({
      nodeId: 'fork-node-1',
      nodeType: 'FORK',
      status: 'SKIPPED',
      step: 1,
      executionTime: 0,
    });
  });
});
