import { describe, it, expect, vi, beforeEach } from 'vitest';
import { scriptHandler } from '../script-handler.js';
import type { GlobalContext } from '../../../../../core/global-context.js';
import type { WorkflowExecutionEntity } from '../../../../entities/workflow-execution-entity.js';
import type { RuntimeNode, ScriptNodeConfig } from '@wf-agent/types';

const mockGlobalContext = {
  container: {
    get: vi.fn(),
  },
} as unknown as GlobalContext;

const mockEntity = {
  getNodeResults: vi.fn().mockReturnValue([]),
  addNodeResult: vi.fn(),
} as unknown as WorkflowExecutionEntity;

const mockScriptService = {
  execute: vi.fn(),
};

beforeEach(() => {
  vi.clearAllMocks();
  (mockGlobalContext.container.get as any).mockReturnValue(mockScriptService);
});

describe('scriptHandler', () => {
  it('should execute script and return result', async () => {
    mockScriptService.execute.mockResolvedValue({
      isErr: () => false,
      value: 'script result',
    });

    const config: ScriptNodeConfig = { scriptName: 'my-script', risk: 'none' };
    const node = { id: 'script-node-1', type: 'SCRIPT', config } as RuntimeNode;

    const result = await scriptHandler(mockGlobalContext, mockEntity, node);

    expect(mockScriptService.execute).toHaveBeenCalledWith('my-script');
    expect(mockEntity.addNodeResult).toHaveBeenCalledWith(
      expect.objectContaining({ nodeId: 'script-node-1', status: 'COMPLETED' })
    );
    expect(result).toBe('script result');
  });

  it('should handle script execution failure', async () => {
    const scriptError = new Error('Script failed');
    mockScriptService.execute.mockResolvedValue({
      isErr: () => true,
      error: scriptError,
    });

    const config: ScriptNodeConfig = { scriptName: 'failing-script', risk: 'none' };
    const node = { id: 'script-node-2', type: 'SCRIPT', config } as RuntimeNode;

    await expect(scriptHandler(mockGlobalContext, mockEntity, node)).rejects.toThrow('Script failed');
    expect(mockEntity.addNodeResult).toHaveBeenCalledWith(
      expect.objectContaining({ status: 'FAILED' })
    );
  });

  it('should handle script service throwing error', async () => {
    mockScriptService.execute.mockRejectedValue(new Error('Service error'));

    const config: ScriptNodeConfig = { scriptName: 'error-script', risk: 'none' };
    const node = { id: 'script-node-3', type: 'SCRIPT', config } as RuntimeNode;

    await expect(scriptHandler(mockGlobalContext, mockEntity, node)).rejects.toThrow('Service error');
    expect(mockEntity.addNodeResult).toHaveBeenCalledWith(
      expect.objectContaining({ status: 'FAILED' })
    );
  });
});
