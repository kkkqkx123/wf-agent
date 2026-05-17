import { describe, it, expect, vi, beforeEach } from 'vitest';
import { joinHandler } from '../join-handler.js';
import type { WorkflowExecutionEntity } from '../../../../entities/workflow-execution-entity.js';
import type { RuntimeNode } from '@wf-agent/types';

const mockEntity = {
  getStatus: vi.fn(),
  getNodeResults: vi.fn().mockReturnValue([]),
} as unknown as WorkflowExecutionEntity;

const mockNode: RuntimeNode = {
  id: 'join-node-1',
  type: 'JOIN',
  config: {},
} as RuntimeNode;

beforeEach(() => {
  vi.clearAllMocks();
  (mockEntity.getStatus as any).mockReturnValue('RUNNING');
});

describe('joinHandler', () => {
  it('should return empty object when status is RUNNING', async () => {
    const result = await joinHandler(mockEntity, mockNode);

    expect(result).toEqual({});
  });

  it('should return SKIPPED when status is not RUNNING', async () => {
    (mockEntity.getStatus as any).mockReturnValue('COMPLETED');

    const result = await joinHandler(mockEntity, mockNode);

    expect(result).toEqual({
      nodeId: 'join-node-1',
      nodeType: 'JOIN',
      status: 'SKIPPED',
      step: 1,
      executionTime: 0,
    });
  });
});
