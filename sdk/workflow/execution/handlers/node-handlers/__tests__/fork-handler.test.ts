import { describe, it, expect, vi, beforeEach } from 'vitest';
import { forkHandler } from '../fork-handler.js';
import type { WorkflowExecutionEntity } from '../../../../entities/workflow-execution-entity.js';
import type { RuntimeNode } from '@wf-agent/types';
import type { GlobalContext } from '../../../../../core/global-context.js';

const mockGlobalContext = {
  container: {
    get: vi.fn(),
  },
} as unknown as GlobalContext;

const mockEntity = {
  getStatus: vi.fn(),
  getNodeResults: vi.fn().mockReturnValue([]),
} as unknown as WorkflowExecutionEntity;

const mockNode: RuntimeNode = {
  id: 'fork-node-1',
  type: 'FORK',
  config: {},
} as RuntimeNode;

beforeEach(() => {
  vi.clearAllMocks();
  (mockEntity.getStatus as any).mockReturnValue('RUNNING');
});

describe('forkHandler', () => {
  it('should return empty object when status is RUNNING', async () => {
    const result = await forkHandler(mockGlobalContext, mockEntity, mockNode);

    expect(result).toEqual({});
  });

  it('should return SKIPPED when status is not RUNNING', async () => {
    (mockEntity.getStatus as any).mockReturnValue('COMPLETED');

    const result = await forkHandler(mockGlobalContext, mockEntity, mockNode);

    expect(result).toEqual({
      nodeId: 'fork-node-1',
      nodeType: 'FORK',
      status: 'SKIPPED',
      step: 1,
      executionTime: 0,
    });
  });
});
