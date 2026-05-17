import { describe, it, expect, vi, beforeEach } from 'vitest';
import { startFromTriggerHandler } from '../start-from-trigger-handler.js';
import type { WorkflowExecutionEntity } from '../../../../entities/workflow-execution-entity.js';
import type { RuntimeNode, WorkflowStartConfig, LLMMessage } from '@wf-agent/types';
import type { StartFromTriggerHandlerContext } from '../start-from-trigger-handler.js';

const mockEntity = {
  getStatus: vi.fn(),
  getNodeResults: vi.fn().mockReturnValue([]),
  setStatus: vi.fn(),
  setCurrentNodeId: vi.fn(),
  state: { start: vi.fn() },
  getExecution: vi.fn(),
  addNodeResult: vi.fn(),
  getInput: vi.fn().mockReturnValue({}),
} as unknown as WorkflowExecutionEntity;

const mockExecution = {
  variables: [],
  errors: [],
  input: {},
};

const mockConversationManager = {
  addMessages: vi.fn(),
};

const mockRegistry = {
  get: vi.fn(),
  has: vi.fn(),
  register: vi.fn(),
  update: vi.fn(),
  getAll: vi.fn(),
};

beforeEach(() => {
  vi.clearAllMocks();
  (mockEntity.getStatus as any).mockReturnValue('CREATED');
  (mockEntity.getExecution as any).mockReturnValue(mockExecution);
  mockExecution.variables = [];
  mockExecution.errors = [];
  mockExecution.input = {};
  mockRegistry.register.mockReset();
});

describe('startFromTriggerHandler', () => {
  it('should initialize triggered subworkflow with trigger input', async () => {
    const context: StartFromTriggerHandlerContext = {
      triggerInput: {
        variables: [{ name: 'var1', value: 'test' }],
      },
      conversationManager: mockConversationManager as any,
    };
    const node = { id: 'trigger-start-1', type: 'START_FROM_TRIGGER', config: {} } as RuntimeNode;

    const result = await startFromTriggerHandler(mockEntity, node, context);

    expect(mockEntity.setStatus).toHaveBeenCalledWith('RUNNING');
    expect(mockExecution.variables).toEqual([{ name: 'var1', value: 'test' }]);
    expect(result).toEqual({
      message: 'Triggered subgraph started',
      input: { variables: [{ name: 'var1', value: 'test' }] },
    });
  });

  it('should validate messageInputs configuration', async () => {
    const context: StartFromTriggerHandlerContext = {
      triggerInput: {
        userName: 'Alice',
      },
    };
    const config: WorkflowStartConfig = {
      messageInputs: [
        { externalName: 'userName', internalName: 'name', required: true },
      ],
    };
    const node = { id: 'trigger-start-2', type: 'START_FROM_TRIGGER', config } as RuntimeNode;

    const result = await startFromTriggerHandler(mockEntity, node, context);

    expect(mockExecution.input).toEqual({ name: 'Alice' });
  });

  it('should throw when required input is missing', async () => {
    const context: StartFromTriggerHandlerContext = {
      triggerInput: {},
    };
    const config: WorkflowStartConfig = {
      messageInputs: [
        { externalName: 'requiredField', internalName: 'field', required: true },
      ],
    };
    const node = { id: 'trigger-start-3', type: 'START_FROM_TRIGGER', config } as RuntimeNode;

    await expect(
      startFromTriggerHandler(mockEntity, node, context)
    ).rejects.toThrow("Required input 'requiredField' (mapped to 'field') is missing");
  });

  it('should handle conversation history', async () => {
    const messages: LLMMessage[] = [{ role: 'user', content: 'hello' }];
    const context: StartFromTriggerHandlerContext = {
      triggerInput: { conversationHistory: messages },
      conversationManager: mockConversationManager as any,
    };
    const node = { id: 'trigger-start-4', type: 'START_FROM_TRIGGER', config: {} } as RuntimeNode;

    await startFromTriggerHandler(mockEntity, node, context);

    expect(mockConversationManager.addMessages).toHaveBeenCalledWith(...messages);
  });

  it('should return SKIPPED when already executed', async () => {
    (mockEntity.getNodeResults as any).mockReturnValue([{ nodeId: 'trigger-start-5' }]);
    const node = { id: 'trigger-start-5', type: 'START_FROM_TRIGGER', config: {} } as RuntimeNode;

    const result = await startFromTriggerHandler(mockEntity, node);

    expect(result.status).toBe('SKIPPED');
  });
});
