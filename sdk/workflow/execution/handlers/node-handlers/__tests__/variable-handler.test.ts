import { describe, it, expect, vi, beforeEach } from 'vitest';
import { variableHandler } from '../variable-handler.js';
import type { WorkflowExecutionEntity } from '../../../../entities/workflow-execution-entity.js';
import type { RuntimeNode, VariableNodeConfig } from '@wf-agent/types';

const mockManager = {
  getAllVariables: vi.fn().mockReturnValue({}),
  setVariable: vi.fn(),
};

const mockEntity = {
  getStatus: vi.fn(),
  getNodeResults: vi.fn().mockReturnValue([]),
  getExecution: vi.fn(),
  getInput: vi.fn().mockReturnValue({}),
  getOutput: vi.fn().mockReturnValue({}),
  variableStateManager: mockManager,
  addNodeResult: vi.fn(),
} as unknown as WorkflowExecutionEntity;

const mockExecution = {
  variables: [],
};

beforeEach(() => {
  vi.clearAllMocks();
  (mockEntity.getStatus as any).mockReturnValue('RUNNING');
  (mockEntity.getExecution as any).mockReturnValue(mockExecution);
  mockExecution.variables = [];
});

describe('variableHandler', () => {
  it('should evaluate expression and update variable', async () => {
    const config: VariableNodeConfig = {
      variableName: 'myVar',
      variableType: 'string',
      expression: "'hello'",
    };
    const node = { id: 'var-node-1', type: 'VARIABLE', config } as RuntimeNode;

    const result = await variableHandler(mockEntity, node);

    expect(mockManager.setVariable).toHaveBeenCalledWith('myVar', 'hello');
    expect(mockEntity.addNodeResult).toHaveBeenCalledWith(
      expect.objectContaining({ nodeId: 'var-node-1', status: 'COMPLETED' })
    );
    expect(result).toEqual({
      variableName: 'myVar',
      value: 'hello',
      type: 'string',
    });
  });

  it('should handle number type conversion', async () => {
    const config: VariableNodeConfig = {
      variableName: 'numVar',
      variableType: 'number',
      expression: '42',
    };
    const node = { id: 'var-node-2', type: 'VARIABLE', config } as RuntimeNode;

    const result = await variableHandler(mockEntity, node);

    expect(result).toEqual({
      variableName: 'numVar',
      value: 42,
      type: 'number',
    });
  });

  it('should handle boolean type conversion', async () => {
    const config: VariableNodeConfig = {
      variableName: 'boolVar',
      variableType: 'boolean',
      expression: 'true',
    };
    const node = { id: 'var-node-3', type: 'VARIABLE', config } as RuntimeNode;

    const result = await variableHandler(mockEntity, node);

    expect((result as any).value).toBe(true);
  });

  it('should skip readonly variables', async () => {
    (mockExecution as any).variables = [{ name: 'myVar', readonly: true }];
    const config: VariableNodeConfig = {
      variableName: 'myVar',
      variableType: 'string',
      expression: "'new value'",
    };
    const node = { id: 'var-node-4', type: 'VARIABLE', config } as RuntimeNode;

    const result = await variableHandler(mockEntity, node);

    expect((result as any).status).toBe('SKIPPED');
    expect(mockManager.setVariable).not.toHaveBeenCalled();
  });

  it('should update existing variable value', async () => {
    (mockExecution as any).variables = [{ name: 'myVar', value: 'old', type: 'string' }];
    const config: VariableNodeConfig = {
      variableName: 'myVar',
      variableType: 'string',
      expression: "'updated'",
    };
    const node = { id: 'var-node-6', type: 'VARIABLE', config } as RuntimeNode;

    await variableHandler(mockEntity, node);

    expect((mockExecution as any).variables[0].value).toBe('updated');
  });
});
