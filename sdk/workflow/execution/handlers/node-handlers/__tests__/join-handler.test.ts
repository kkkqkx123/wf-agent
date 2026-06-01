import { describe, it, expect, vi, beforeEach } from 'vitest';
import { joinHandler } from '../join-handler.js';
import type { WorkflowExecutionEntity } from '../../../../entities/workflow-execution-entity.js';
import type { RuntimeNode, JoinNodeConfig } from '@wf-agent/types';
import type { GlobalContext } from '../../../../../core/global-context.js';
import * as Identifiers from '../../../../../core/di/service-identifiers.js';

const mockGlobalContext = {
  container: {
    get: vi.fn(),
  },
} as unknown as GlobalContext;

const mockSyncBarrier = {
  getExecutionIdByPath: vi.fn(),
  getAllPathIds: vi.fn(),
};

const mockExecutionRegistry = {
  get: vi.fn(),
};

const mockEventRegistry = {
  emit: vi.fn(),
};

const mockVariableManager = {
  exportVariables: vi.fn(),
  getVariable: vi.fn(),
  getAllVariables: vi.fn(),
  deleteVariable: vi.fn(),
};

const mockMessageContextRegistry = {
  register: vi.fn(),
  get: vi.fn(),
  has: vi.fn(),
  listIds: vi.fn(),
  delete: vi.fn(),
  update: vi.fn(),
};

const mockEntity = {
  id: 'parent-exec-1',
  getStatus: vi.fn(),
  getNodeResults: vi.fn().mockReturnValue([]),
  getSyncBarrier: vi.fn(),
  getOutput: vi.fn().mockReturnValue({}),
  setOutput: vi.fn(),
  getWorkflowId: vi.fn().mockReturnValue('wf-1'),
  variableStateManager: mockVariableManager,
  getExecution: vi.fn().mockReturnValue({
    messageContextRegistry: mockMessageContextRegistry,
  }),
} as unknown as WorkflowExecutionEntity;

const mockBranchEntity = {
  id: 'branch-exec-1',
  getStatus: vi.fn().mockReturnValue('COMPLETED'),
  getSyncBarrier: vi.fn(),
  variableStateManager: mockVariableManager,
  getExecution: vi.fn().mockReturnValue({
    messageContextRegistry: mockMessageContextRegistry,
  }),
} as unknown as WorkflowExecutionEntity;

function createJoinConfig(overrides: Partial<JoinNodeConfig> = {}): JoinNodeConfig {
  return {
    forkPathIds: ['path-1', 'path-2'],
    joinStrategy: 'ALL_COMPLETED',
    mainPathId: 'path-1',
    threshold: 1,
    timeout: 0,
    variableOutputs: [],
    messageOutputs: [],
    dataOutputs: [],
    ...overrides,
  } as JoinNodeConfig;
}

function createMockNode(config: JoinNodeConfig): RuntimeNode {
  return {
    id: 'join-node-1',
    type: 'JOIN',
    config,
  } as RuntimeNode;
}

beforeEach(() => {
  vi.clearAllMocks();

  // Default mock state
  (mockEntity.getStatus as any).mockReturnValue('RUNNING');
  mockEntity.getSyncBarrier = vi.fn().mockReturnValue(mockSyncBarrier);
  (mockGlobalContext.container.get as any) = vi.fn((id: symbol) => {
    if (id === Identifiers.WorkflowExecutionRegistry) {
      return mockExecutionRegistry;
    }
    if (id === Identifiers.EventRegistry) {
      return mockEventRegistry;
    }
    return undefined;
  });

  mockSyncBarrier.getExecutionIdByPath = vi.fn().mockImplementation((pathId: string) => {
    if (pathId === 'path-1') return 'branch-exec-1';
    if (pathId === 'path-2') return 'branch-exec-2';
    return undefined;
  });
  mockSyncBarrier.getAllPathIds = vi.fn().mockReturnValue(['path-1', 'path-2']);

  mockExecutionRegistry.get = vi.fn().mockImplementation((execId: string) => {
    if (execId === 'branch-exec-1' || execId === 'branch-exec-2') {
      return mockBranchEntity;
    }
    return undefined;
  });
});

describe('joinHandler', () => {
  it('should return error in aggregatedOutput when ALL_COMPLETED strategy not met', async () => {
    const config = createJoinConfig({
      forkPathIds: ['path-1', 'missing-path'],
      joinStrategy: 'ALL_COMPLETED',
    });
    const node = createMockNode(config);

    const result = await joinHandler(mockGlobalContext, mockEntity, node);

    expect(result.completedBranches).toEqual(['path-1']);
    expect(result.aggregatedOutput).toBeDefined();
    expect((result.aggregatedOutput as any).error).toContain('ALL_COMPLETED');
  });

  it('should return error in aggregatedOutput when ANY_COMPLETED strategy not met', async () => {
    mockSyncBarrier.getExecutionIdByPath = vi.fn().mockReturnValue(undefined);
    const config = createJoinConfig({
      joinStrategy: 'ANY_COMPLETED',
    });
    const node = createMockNode(config);

    const result = await joinHandler(mockGlobalContext, mockEntity, node);

    expect(result.aggregatedOutput).toBeDefined();
    expect((result.aggregatedOutput as any).error).toContain('ANY_COMPLETED');
  });

  it('should complete successfully with ALL_COMPLETED strategy', async () => {
    const config = createJoinConfig({
      forkPathIds: ['path-1', 'path-2'],
      joinStrategy: 'ALL_COMPLETED',
      mainPathId: 'path-1',
    });
    const node = createMockNode(config);

    const result = await joinHandler(mockGlobalContext, mockEntity, node);

    expect(result.completedBranches).toEqual(['path-1', 'path-2']);
    expect(result.failedBranches).toEqual([]);
    expect(result.skippedBranches).toEqual([]);
    expect(result.strategy).toBe('ALL_COMPLETED');
  });

  it('should aggregate variableOutputs when configured', async () => {
    const config = createJoinConfig({
      forkPathIds: ['path-1'],
      joinStrategy: 'ALL_COMPLETED',
      mainPathId: 'path-1',
      variableOutputs: [
        { internalName: 'branchVar', externalName: 'parentVar' },
      ],
    });
    const node = createMockNode(config);

    await joinHandler(mockGlobalContext, mockEntity, node);

    expect(mockVariableManager.exportVariables).toHaveBeenCalledWith(
      mockEntity.variableStateManager,
      config.variableOutputs,
    );
  });

  it('should aggregate messageOutputs when configured', async () => {
    mockMessageContextRegistry.get = vi.fn().mockReturnValue({
      id: 'branch-context',
      messages: [{ role: 'user', content: 'test' }],
      createdAt: Date.now(),
      updatedAt: Date.now(),
    });

    const config = createJoinConfig({
      forkPathIds: ['path-1'],
      joinStrategy: 'ALL_COMPLETED',
      mainPathId: 'path-1',
      messageOutputs: [
        {
          internalName: 'branch-context',
          externalName: 'parent-context',
          sourcePathId: 'path-1',
        },
      ],
    });
    const node = createMockNode(config);

    await joinHandler(mockGlobalContext, mockEntity, node);

    expect(mockMessageContextRegistry.register).toHaveBeenCalled();
  });

  it('should aggregate dataOutputs when configured', async () => {
    mockVariableManager.getVariable = vi.fn().mockReturnValue('test-result');

    const config = createJoinConfig({
      forkPathIds: ['path-1'],
      joinStrategy: 'ALL_COMPLETED',
      mainPathId: 'path-1',
      dataOutputs: [
        { internalName: 'branchResult', outputKey: 'finalResult' },
      ],
    });
    const node = createMockNode(config);

    await joinHandler(mockGlobalContext, mockEntity, node);

    expect(mockEntity.setOutput).toHaveBeenCalledWith(
      expect.objectContaining({ finalResult: 'test-result' }),
    );
  });

  it('should handle SUCCESS_COUNT_THRESHOLD strategy with threshold met', async () => {
    const config = createJoinConfig({
      forkPathIds: ['path-1', 'path-2'],
      joinStrategy: 'SUCCESS_COUNT_THRESHOLD',
      threshold: 2,
    });
    const node = createMockNode(config);

    const result = await joinHandler(mockGlobalContext, mockEntity, node);

    expect(result.strategy).toBe('SUCCESS_COUNT_THRESHOLD');
    expect(result.completedBranches).toEqual(['path-1', 'path-2']);
  });

  it('should handle SUCCESS_COUNT_THRESHOLD strategy with threshold not met', async () => {
    mockSyncBarrier.getExecutionIdByPath = vi.fn().mockImplementation((pathId: string) => {
      if (pathId === 'path-1') return 'branch-exec-1';
      return undefined;
    });

    const config = createJoinConfig({
      forkPathIds: ['path-1', 'path-2'],
      joinStrategy: 'SUCCESS_COUNT_THRESHOLD',
      threshold: 2,
    });
    const node = createMockNode(config);

    const result = await joinHandler(mockGlobalContext, mockEntity, node);

    expect(result.aggregatedOutput).toBeDefined();
    expect((result.aggregatedOutput as any).error).toContain('SUCCESS_COUNT_THRESHOLD');
  });

  it('should throw when WorkflowExecutionRegistry is not available', async () => {
    mockGlobalContext.container.get = vi.fn().mockReturnValue(undefined);

    const config = createJoinConfig();
    const node = createMockNode(config);

    await expect(
      joinHandler(mockGlobalContext, mockEntity, node)
    ).rejects.toThrow('WorkflowExecutionRegistry not available');
  });
});