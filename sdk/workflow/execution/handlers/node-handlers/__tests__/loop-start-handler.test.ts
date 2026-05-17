import { describe, it, expect, vi, beforeEach } from 'vitest';
import { loopStartHandler } from '../loop-start-handler.js';
import type { WorkflowExecutionEntity } from '../../../../entities/workflow-execution-entity.js';
import type { RuntimeNode, LoopStartNodeConfig } from '@wf-agent/types';

const mockManager = {
  getVariable: vi.fn(),
  setVariable: vi.fn(),
  deleteVariable: vi.fn(),
  importVariables: vi.fn(),
};

const mockEntity = {
  getStatus: vi.fn(),
  getExecution: vi.fn(),
  variableStateManager: mockManager,
} as unknown as WorkflowExecutionEntity;

const mockExecution = {
  nodeResults: [],
  variables: [],
  input: {},
  output: {},
  currentNodeId: 'loop-start-1',
  workflowId: 'wf-1',
  variableScopes: { global: {}, execution: {} },
};

beforeEach(() => {
  vi.clearAllMocks();
  (mockEntity.getStatus as any).mockReturnValue('RUNNING');
  (mockEntity.getExecution as any).mockReturnValue(mockExecution);
  mockExecution.nodeResults = [];
  mockExecution.variables = [];
  mockManager.getVariable.mockReturnValue(undefined);
});

describe('loopStartHandler', () => {
  it('should initialize loop state and continue for array iterable', async () => {
    const config: LoopStartNodeConfig = {
      loopId: 'loop-1',
      maxIterations: 10,
      dataSource: {
        iterable: [1, 2, 3],
        variableName: 'item',
      },
    };
    const node = { id: 'loop-start-1', type: 'LOOP_START', config } as RuntimeNode;

    const result = await loopStartHandler(mockEntity, node);

    expect(result).toEqual({
      loopId: 'loop-1',
      variableName: 'item',
      currentValue: 1,
      iterationCount: 1,
      shouldContinue: true,
    });
    expect(mockManager.setVariable).toHaveBeenCalledWith('item', 1);
    expect(mockManager.setVariable).toHaveBeenCalledWith('__loop_state', expect.any(Object));
  });

  it('should resume loop state and continue from saved state', async () => {
    const savedState = {
      loopId: 'loop-1',
      iterable: [1, 2, 3],
      currentIndex: 1,
      maxIterations: 10,
      iterationCount: 1,
      variableName: 'item',
    };
    mockManager.getVariable.mockReturnValue(savedState);

    const config: LoopStartNodeConfig = {
      loopId: 'loop-1',
      maxIterations: 10,
      dataSource: {
        iterable: [1, 2, 3],
        variableName: 'item',
      },
    };
    const node = { id: 'loop-start-1', type: 'LOOP_START', config } as RuntimeNode;

    const result = await loopStartHandler(mockEntity, node);

    expect((result as any).shouldContinue).toBe(true);
    expect((result as any).currentValue).toBe(2);
    expect((result as any).iterationCount).toBe(2);
  });

  it('should return shouldContinue false when maxIterations reached', async () => {
    const config: LoopStartNodeConfig = {
      loopId: 'loop-1',
      maxIterations: 0,
    };
    const node = { id: 'loop-start-1', type: 'LOOP_START', config } as RuntimeNode;

    const result = await loopStartHandler(mockEntity, node);

    expect(result).toEqual({
      loopId: 'loop-1',
      shouldContinue: false,
      iterationCount: 0,
      message: 'Loop completed',
    });
    expect(mockManager.deleteVariable).toHaveBeenCalledWith('__loop_state');
  });

  it('should handle counting loop (no dataSource)', async () => {
    const config: LoopStartNodeConfig = {
      loopId: 'loop-1',
      maxIterations: 5,
    };
    const node = { id: 'loop-start-1', type: 'LOOP_START', config } as RuntimeNode;

    const result = await loopStartHandler(mockEntity, node);

    expect(result).toEqual({
      loopId: 'loop-1',
      variableName: null,
      currentValue: 0,
      iterationCount: 1,
      shouldContinue: true,
    });
  });

  it('should handle string iterable', async () => {
    const config: LoopStartNodeConfig = {
      loopId: 'loop-1',
      maxIterations: 10,
      dataSource: {
        iterable: 'abc',
        variableName: 'char',
      },
    };
    const node = { id: 'loop-start-1', type: 'LOOP_START', config } as RuntimeNode;

    const result = await loopStartHandler(mockEntity, node);

    expect((result as any).currentValue).toBe('a');
    expect((result as any).shouldContinue).toBe(true);
  });

  it('should return SKIPPED when status is not RUNNING', async () => {
    (mockEntity.getStatus as any).mockReturnValue('COMPLETED');
    const node = { id: 'loop-start-1', type: 'LOOP_START', config: { loopId: 'l1', maxIterations: 1 } } as RuntimeNode;

    const result = await loopStartHandler(mockEntity, node);

    expect((result as any).status).toBe('SKIPPED');
  });
});
