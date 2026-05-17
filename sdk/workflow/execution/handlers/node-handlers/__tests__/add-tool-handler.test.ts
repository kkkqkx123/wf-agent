import { describe, it, expect, vi, beforeEach } from 'vitest';
import { addToolHandler } from '../add-tool-handler.js';
import type { WorkflowExecution } from '@wf-agent/types';
import type { RuntimeNode, AddToolNodeConfig } from '@wf-agent/types';
import type { AddToolHandlerContext } from '../add-tool-handler.js';

const mockExecution = {
  id: 'exec-1',
  workflowId: 'wf-1',
} as unknown as WorkflowExecution;

const mockToolService = {
  getTool: vi.fn(),
};

const mockToolContextStore = {
  addTools: vi.fn(),
};

const mockEventManager = {
  emit: vi.fn(),
};

const defaultContext: AddToolHandlerContext = {
  toolContextStore: mockToolContextStore as any,
  toolService: mockToolService as any,
  eventManager: mockEventManager as any,
};

beforeEach(() => {
  vi.clearAllMocks();
  mockToolService.getTool.mockReturnValue({ id: 'tool-1', name: 'TestTool' });
  mockToolContextStore.addTools.mockReturnValue(2);
});

describe('addToolHandler', () => {
  it('should add valid tools and emit event', async () => {
    const config: AddToolNodeConfig = {
      toolIds: ['tool-1', 'tool-2'],
    };
    mockToolService.getTool.mockImplementation((id: string) => {
      return id === 'tool-1' || id === 'tool-2' ? { id } : null;
    });
    const node = { id: 'add-tool-1', type: 'ADD_TOOL', config } as RuntimeNode;

    const result = await addToolHandler(mockExecution, node, defaultContext);

    expect(mockToolContextStore.addTools).toHaveBeenCalledWith(
      'exec-1', ['tool-1', 'tool-2'], 'EXECUTION', false, undefined, undefined
    );
    expect(mockEventManager.emit).toHaveBeenCalled();
    expect(result.status).toBe('COMPLETED');
    expect(result.addedCount).toBe(2);
  });

  it('should throw ExecutionError when tool ID is invalid', async () => {
    mockToolService.getTool.mockReturnValue(null);
    const config: AddToolNodeConfig = {
      toolIds: ['invalid-tool'],
    };
    const node = { id: 'add-tool-2', type: 'ADD_TOOL', config } as RuntimeNode;

    const result = await addToolHandler(mockExecution, node, defaultContext);

    expect(result.status).toBe('FAILED');
    expect(result.error).toBeDefined();
  });

  it('should handle empty tool IDs array', async () => {
    mockToolContextStore.addTools.mockReturnValue(0);
    const config: AddToolNodeConfig = { toolIds: [] };
    const node = { id: 'add-tool-3', type: 'ADD_TOOL', config } as RuntimeNode;

    const result = await addToolHandler(mockExecution, node, defaultContext);

    expect(result.status).toBe('COMPLETED');
    expect(result.addedCount).toBe(0);
  });
});
