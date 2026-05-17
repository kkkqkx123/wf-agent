import { describe, it, expect, vi, beforeEach } from 'vitest';
import { routeHandler } from '../route-handler.js';
import type { WorkflowExecutionEntity } from '../../../../entities/workflow-execution-entity.js';
import type { RuntimeNode, RouteNodeConfig, Condition } from '@wf-agent/types';

const mockEntity = {
  getStatus: vi.fn(),
  getNodeResults: vi.fn().mockReturnValue([]),
  getAllVariables: vi.fn().mockReturnValue({}),
  getInput: vi.fn().mockReturnValue({}),
  getOutput: vi.fn().mockReturnValue({}),
  getCurrentNodeId: vi.fn().mockReturnValue('route-node-1'),
  getWorkflowId: vi.fn().mockReturnValue('workflow-1'),
} as unknown as WorkflowExecutionEntity;

beforeEach(() => {
  vi.clearAllMocks();
  (mockEntity.getStatus as any).mockReturnValue('RUNNING');
});

describe('routeHandler', () => {
  it('should select route with highest priority matching condition', async () => {
    const routeConfig: RouteNodeConfig = {
      routes: [
        { targetNodeId: 'node-b', condition: { expression: 'false' }, priority: 1 },
        { targetNodeId: 'node-c', condition: { expression: 'true' }, priority: 2 },
      ],
    };
    const node = { id: 'route-node-1', type: 'ROUTE', config: routeConfig } as RuntimeNode;

    const result = await routeHandler(mockEntity, node);

    expect(result).toEqual({ status: 'COMPLETED', selectedNode: 'node-c' });
  });

  it('should use defaultTargetNodeId when no routes match', async () => {
    const routeConfig: RouteNodeConfig = {
      routes: [
        { targetNodeId: 'node-b', condition: { expression: 'false' }, priority: 1 },
      ],
      defaultTargetNodeId: 'default-node',
    };
    const node = { id: 'route-node-1', type: 'ROUTE', config: routeConfig } as RuntimeNode;

    const result = await routeHandler(mockEntity, node);

    expect(result).toEqual({ status: 'COMPLETED', selectedNode: 'default-node' });
  });

  it('should throw ExecutionError when no route matches and no default', async () => {
    const routeConfig: RouteNodeConfig = {
      routes: [
        { targetNodeId: 'node-b', condition: { expression: 'false' }, priority: 1 },
      ],
    };
    const node = { id: 'route-node-1', type: 'ROUTE', config: routeConfig } as RuntimeNode;

    await expect(routeHandler(mockEntity, node)).rejects.toThrow();
  });

  it('should return SKIPPED when status is not RUNNING', async () => {
    (mockEntity.getStatus as any).mockReturnValue('COMPLETED');
    const node = { id: 'route-node-1', type: 'ROUTE', config: { routes: [] } } as RuntimeNode;

    const result = await routeHandler(mockEntity, node);

    expect(result).toEqual({
      nodeId: 'route-node-1',
      nodeType: 'ROUTE',
      status: 'SKIPPED',
      step: 1,
      executionTime: 0,
    });
  });
});
