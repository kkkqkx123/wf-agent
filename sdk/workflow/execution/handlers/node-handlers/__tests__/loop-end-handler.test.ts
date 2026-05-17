import { describe, it, expect, vi, beforeEach } from 'vitest';
import { loopEndHandler } from '../loop-end-handler.js';
import type { WorkflowExecutionEntity } from '../../../../entities/workflow-execution-entity.js';
import type { RuntimeNode, LoopEndNodeConfig } from '@wf-agent/types';

const mockManager = {
  getVariable: vi.fn(),
  deleteVariable: vi.fn(),
};

const mockEntity = {
  getStatus: vi.fn(),
  getNodeResults: vi.fn().mockReturnValue([]),
  getExecution: vi.fn(),
  variableStateManager: mockManager,
} as unknown as WorkflowExecutionEntity;

const mockExecution = {
  nodeResults: [],
  input: {},
  output: {},
  currentNodeId: 'loop-end-1',
  workflowId: 'wf-1',
  variableScopes: { global: {}, execution: {} },
};

let loopState: any;

beforeEach(() => {
  vi.clearAllMocks();
  loopState = {
    loopId: 'loop-1',
    iterable: [1, 2, 3],
    currentIndex: 0,
    maxIterations: 10,
    iterationCount: 0,
    variableName: 'item',
  };
  (mockEntity.getStatus as any).mockReturnValue('RUNNING');
  (mockEntity.getExecution as any).mockReturnValue(mockExecution);
  mockExecution.nodeResults = [];
  mockManager.getVariable.mockReturnValue(loopState);
});

describe('loopEndHandler', () => {
  it('should continue loop when condition met and no break', async () => {
    const config: LoopEndNodeConfig = { loopId: 'loop-1', loopStartNodeId: 'loop-start-1' };
    const node = { id: 'loop-end-1', type: 'LOOP_END', config } as RuntimeNode;

    const result = await loopEndHandler(mockEntity, node);

    expect(result).toEqual({
      loopId: 'loop-1',
      shouldContinue: true,
      shouldBreak: false,
      loopConditionMet: true,
      iterationCount: 1,
      nextNodeId: 'loop-start-1',
    });
  });

  it('should not continue when break condition is true', async () => {
    const config: LoopEndNodeConfig = {
      loopId: 'loop-1',
      loopStartNodeId: 'loop-start-1',
      breakCondition: { expression: 'true' },
    };
    const node = { id: 'loop-end-1', type: 'LOOP_END', config } as RuntimeNode;

    const result = await loopEndHandler(mockEntity, node);

    expect(result).toEqual({
      loopId: 'loop-1',
      shouldContinue: false,
      shouldBreak: true,
      loopConditionMet: true,
      iterationCount: 0,
      nextNodeId: undefined,
    });
  });

  it('should not continue when maxIterations reached', async () => {
    const endedState = { ...loopState, maxIterations: 1, iterationCount: 1 };
    mockManager.getVariable.mockReturnValue(endedState);

    const config: LoopEndNodeConfig = { loopId: 'loop-1', loopStartNodeId: 'loop-start-1' };
    const node = { id: 'loop-end-1', type: 'LOOP_END', config } as RuntimeNode;

    const result = await loopEndHandler(mockEntity, node);

    expect(result.shouldContinue).toBe(false);
    expect(result.loopConditionMet).toBe(false);
  });

  it('should return SKIPPED when loop state not found (canExecute returns false)', async () => {
    mockManager.getVariable.mockReturnValue(undefined);

    const config: LoopEndNodeConfig = { loopId: 'loop-1', loopStartNodeId: 'loop-start-1' };
    const node = { id: 'loop-end-1', type: 'LOOP_END', config } as RuntimeNode;

    const result = await loopEndHandler(mockEntity, node);

    expect(result.status).toBe('SKIPPED');
  });

  it('should return SKIPPED when status is not RUNNING', async () => {
    (mockEntity.getStatus as any).mockReturnValue('COMPLETED');
    const node = { id: 'loop-end-1', type: 'LOOP_END', config: { loopId: 'l1', loopStartNodeId: 'ls1' } } as RuntimeNode;

    const result = await loopEndHandler(mockEntity, node);

    expect(result.status).toBe('SKIPPED');
  });

  it('should clear loop state when loop ends', async () => {
    const endedState = { ...loopState, iterationCount: 1, maxIterations: 1 };
    mockManager.getVariable.mockReturnValue(endedState);

    const config: LoopEndNodeConfig = { loopId: 'loop-1', loopStartNodeId: 'loop-start-1' };
    const node = { id: 'loop-end-1', type: 'LOOP_END', config } as RuntimeNode;

    await loopEndHandler(mockEntity, node);

    expect(mockManager.deleteVariable).toHaveBeenCalledWith('__loop_state');
  });
});
