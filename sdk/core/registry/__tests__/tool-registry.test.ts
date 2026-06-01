import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ToolRegistry } from '../tool-registry.js';
import type { Tool } from '@wf-agent/types';

function createValidTool(overrides: Partial<Tool> = {}): Tool {
  return {
    id: 'test_tool',
    type: 'REST',
    description: 'A test tool',
    parameters: {
      type: 'object',
      properties: {},
      required: [],
    },
    ...overrides,
  } as Tool;
}

describe('ToolRegistry', () => {
  let registry: ToolRegistry;

  beforeEach(() => {
    registry = new ToolRegistry();
  });

  describe('registerTool', () => {
    it('should register a valid tool', () => {
      registry.registerTool(createValidTool());
      expect(registry.has('test_tool')).toBe(true);
    });

    it('should throw if tool id already exists', () => {
      registry.registerTool(createValidTool());
      expect(() => registry.registerTool(createValidTool())).toThrow('already exists');
    });

    it('should skip if skipIfExists option is set', () => {
      registry.registerTool(createValidTool());
      expect(() =>
        registry.registerTool(createValidTool(), { skipIfExists: true }),
      ).not.toThrow();
    });

    it('should throw on invalid tool id pattern', () => {
      expect(() =>
        registry.registerTool(createValidTool({ id: 'Invalid-ID' })),
      ).toThrow();
    });

    it('should throw on missing description', () => {
      expect(() =>
        registry.registerTool(createValidTool({ description: '' })),
      ).toThrow();
    });

    it('should throw on invalid tool type', () => {
      expect(() =>
        registry.registerTool(createValidTool({ type: 'UNKNOWN' as any })),
      ).toThrow();
    });
  });

  describe('registerTools', () => {
    it('should register multiple tools', () => {
      const t1 = createValidTool({ id: 'tool_1' });
      const t2 = createValidTool({ id: 'tool_2' });
      registry.registerTools([t1, t2]);
      expect(registry.size()).toBe(2);
    });

    it('should throw on duplicate in batch', () => {
      const t1 = createValidTool({ id: 'tool_1' });
      const dup = createValidTool({ id: 'tool_1' });
      expect(() => registry.registerTools([t1, dup])).toThrow('already exists');
    });

    it('should skip duplicates with skipIfExists option', () => {
      registry.registerTool(createValidTool({ id: 'tool_1' }));
      expect(() =>
        registry.registerTools([createValidTool({ id: 'tool_1' })], { skipIfExists: true }),
      ).not.toThrow();
    });
  });

  describe('unregisterTool', () => {
    it('should delete a tool', () => {
      registry.registerTool(createValidTool());
      registry.unregisterTool('test_tool');
      expect(registry.has('test_tool')).toBe(false);
    });

    it('should throw if tool does not exist', () => {
      expect(() => registry.unregisterTool('non_existent')).toThrow('not found');
    });
  });

  describe('getTool', () => {
    it('should return tool by id', () => {
      const tool = createValidTool();
      registry.registerTool(tool);
      expect(registry.getTool('test_tool').id).toBe('test_tool');
    });

    it('should throw if tool does not exist', () => {
      expect(() => registry.getTool('non_existent')).toThrow('not found');
    });
  });

  describe('has / hasTool', () => {
    it('should return true if tool exists', () => {
      registry.registerTool(createValidTool());
      expect(registry.has('test_tool')).toBe(true);
      expect(registry.hasTool('test_tool')).toBe(true);
    });

    it('should return false if tool does not exist', () => {
      expect(registry.has('non_existent')).toBe(false);
    });
  });

  describe('listTools', () => {
    it('should return all tools', () => {
      registry.registerTool(createValidTool({ id: 'tool_1' }));
      registry.registerTool(createValidTool({ id: 'tool_2' }));
      expect(registry.listTools()).toHaveLength(2);
    });
  });

  describe('listToolsByType', () => {
    it('should filter by type', () => {
      registry.registerTool(createValidTool({ id: 'tool_1', type: 'REST' }));
      registry.registerTool(createValidTool({ id: 'tool_2', type: 'BUILTIN', config: { execute: vi.fn() } }));
      expect(registry.listToolsByType('REST')).toHaveLength(1);
    });
  });

  describe('listToolsByCategory', () => {
    it('should filter by category', () => {
      registry.registerTool(createValidTool({ id: 'tool_1', metadata: { category: 'data' } }));
      registry.registerTool(createValidTool({ id: 'tool_2', metadata: { category: 'util' } }));
      expect(registry.listToolsByCategory('data')).toHaveLength(1);
    });
  });

  describe('searchTools', () => {
    it('should search by id', () => {
      registry.registerTool(createValidTool({ id: 'file_reader' }));
      expect(registry.searchTools('reader')).toHaveLength(1);
    });

    it('should search by description', () => {
      registry.registerTool(createValidTool({ id: 't1', description: 'data transformer' }));
      expect(registry.searchTools('transformer')).toHaveLength(1);
    });

    it('should search by tag', () => {
      registry.registerTool(createValidTool({ id: 't1', metadata: { tags: ['important'] } }));
      expect(registry.searchTools('important')).toHaveLength(1);
    });

    it('should search by category', () => {
      registry.registerTool(createValidTool({ id: 't1', metadata: { category: 'machine-learning' } }));
      expect(registry.searchTools('machine')).toHaveLength(1);
    });

    it('should return empty for no match', () => {
      registry.registerTool(createValidTool({ id: 't1' }));
      expect(registry.searchTools('nonexistent')).toHaveLength(0);
    });
  });

  describe('clear', () => {
    it('should remove all tools', () => {
      registry.registerTool(createValidTool({ id: 'tool_1' }));
      registry.registerTool(createValidTool({ id: 'tool_2' }));
      registry.clear();
      expect(registry.size()).toBe(0);
    });
  });

  describe('size', () => {
    it('should return correct count', () => {
      expect(registry.size()).toBe(0);
      registry.registerTool(createValidTool());
      expect(registry.size()).toBe(1);
    });
  });

  describe('updateTool', () => {
    it('should update existing tool fields', () => {
      registry.registerTool(createValidTool());
      registry.updateTool('test_tool', { description: 'Updated description' });
      expect(registry.getTool('test_tool').description).toBe('Updated description');
    });

    it('should throw if tool does not exist', () => {
      expect(() => registry.updateTool('non_existent', { description: 'test' })).toThrow('not found');
    });

    it('should re-validate updated tool', () => {
      registry.registerTool(createValidTool());
      expect(() => registry.updateTool('test_tool', { type: 'UNKNOWN' as any })).toThrow();
    });
  });

  describe('validateParameters', () => {
    it('should return valid for tool with no required params', () => {
      registry.registerTool(createValidTool());
      const result = registry.validateParameters('test_tool', {});
      expect(result.valid).toBe(true);
    });

    it('should return invalid for non-existent tool', () => {
      const result = registry.validateParameters('non_existent', {});
      expect(result.valid).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
    });
  });

  describe('execute', () => {
    it('should throw for non-existent tool', async () => {
      await expect(registry.execute('non_existent', {})).rejects.toThrow('not found');
    });
  });

  describe('executeBatch', () => {
    it('should throw for non-existent tools', async () => {
      await expect(
        registry.executeBatch([
          { toolId: 'non_existent_1', parameters: {} },
          { toolId: 'non_existent_2', parameters: {} },
        ]),
      ).rejects.toThrow('not found');
    });
  });

  describe('getAvailableTools', () => {
    it('should include builtin tools and custom tools', () => {
      const customTools = [createValidTool({ id: 'custom_tool' })];
      const available = registry.getAvailableTools(customTools);
      expect(available.length).toBeGreaterThanOrEqual(1);
      expect(available.find(t => t.id === 'custom_tool')).toBeDefined();
    });
  });

  describe('getBuiltinTools', () => {
    it('should return builtin tools', () => {
      const tools = registry.getBuiltinTools();
      expect(tools.length).toBeGreaterThan(0);
    });
  });

  describe('cleanupAll', () => {
    it('should cleanup without throwing', async () => {
      await expect(registry.cleanupAll()).resolves.not.toThrow();
    });
  });

  describe('cleanupWorkflowExecution', () => {
    it('should cleanup without throwing', () => {
      expect(() => registry.cleanupWorkflowExecution('exec-1')).not.toThrow();
    });
  });
});