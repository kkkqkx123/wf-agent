import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ScriptRegistry } from '../script-registry.js';
import type { Script, ScriptFlow } from '@wf-agent/types';

vi.mock('../../executors/script-executor.js', () => {
  const mockExecute = vi.fn().mockResolvedValue({
    success: true,
    scriptName: 'test-script',
    stdout: 'executed',
    executionTime: 10,
  });
  return {
    ScriptExecutor: vi.fn().mockImplementation(function () {
      return {
        execute: mockExecute,
        cleanup: vi.fn().mockResolvedValue(undefined),
      };
    }),
  };
});

function createValidScript(overrides: Partial<Script> = {}): Script {
  return {
    name: 'test-script',
    description: 'A test script',
    content: 'console.log("hello")',
    options: {
      timeout: 5000,
      retries: 0,
      retryDelay: 0,
    },
    enabled: true,
    ...overrides,
  } as Script;
}

describe('ScriptRegistry', () => {
  let registry: ScriptRegistry;

  beforeEach(() => {
    registry = new ScriptRegistry();
  });

  describe('registerScript', () => {
    it('should register a valid script', () => {
      registry.registerScript(createValidScript());
      expect(registry.hasScript('test-script')).toBe(true);
    });

    it('should throw if name already exists', () => {
      registry.registerScript(createValidScript());
      expect(() => registry.registerScript(createValidScript())).toThrow('already exists');
    });

    it('should set enabled to true by default', () => {
      const script = createValidScript({ enabled: undefined });
      registry.registerScript(script);
      expect(registry.getScript('test-script').enabled).toBe(true);
    });
  });

  describe('registerScripts', () => {
    it('should register multiple scripts', () => {
      const s1 = createValidScript({ name: 's1' });
      const s2 = createValidScript({ name: 's2' });
      registry.registerScripts([s1, s2]);
      expect(registry.scriptCount()).toBe(2);
    });

    it('should throw on duplicate in batch', () => {
      const s1 = createValidScript({ name: 's1' });
      const dup = createValidScript({ name: 's1' });
      expect(() => registry.registerScripts([s1, dup])).toThrow('already exists');
    });
  });

  describe('unregisterScript', () => {
    it('should delete a script', () => {
      registry.registerScript(createValidScript());
      registry.unregisterScript('test-script');
      expect(registry.hasScript('test-script')).toBe(false);
    });

    it('should throw if script does not exist', () => {
      expect(() => registry.unregisterScript('non-existent')).toThrow('not found');
    });
  });

  describe('getScript', () => {
    it('should return script by name', () => {
      const script = createValidScript();
      registry.registerScript(script);
      expect(registry.getScript('test-script').name).toBe('test-script');
    });

    it('should throw if script does not exist', () => {
      expect(() => registry.getScript('non-existent')).toThrow('not found');
    });
  });

  describe('findScript', () => {
    it('should return script or undefined', () => {
      registry.registerScript(createValidScript());
      expect(registry.findScript('test-script')).toBeDefined();
      expect(registry.findScript('non-existent')).toBeUndefined();
    });
  });

  describe('listScripts', () => {
    it('should return all scripts', () => {
      registry.registerScript(createValidScript({ name: 's1' }));
      registry.registerScript(createValidScript({ name: 's2' }));
      expect(registry.listScripts()).toHaveLength(2);
    });
  });

  describe('listScriptsByCategory', () => {
    it('should filter by category', () => {
      registry.registerScript(createValidScript({ name: 's1', metadata: { category: 'data' } }));
      registry.registerScript(createValidScript({ name: 's2', metadata: { category: 'util' } }));
      expect(registry.listScriptsByCategory('data')).toHaveLength(1);
    });
  });

  describe('searchScripts', () => {
    it('should search by name', () => {
      registry.registerScript(createValidScript({ name: 'my-processor' }));
      expect(registry.searchScripts('processor')).toHaveLength(1);
    });

    it('should search by description', () => {
      registry.registerScript(createValidScript({ name: 's1', description: 'data transformation' }));
      expect(registry.searchScripts('transformation')).toHaveLength(1);
    });

    it('should search by tag', () => {
      registry.registerScript(createValidScript({ name: 's1', metadata: { tags: ['important'] } }));
      expect(registry.searchScripts('important')).toHaveLength(1);
    });

    it('should search by category', () => {
      registry.registerScript(createValidScript({ name: 's1', metadata: { category: 'machine-learning' } }));
      expect(registry.searchScripts('machine')).toHaveLength(1);
    });

    it('should return empty for no match', () => {
      registry.registerScript(createValidScript({ name: 's1' }));
      expect(registry.searchScripts('nonexistent')).toHaveLength(0);
    });
  });

  describe('hasScript', () => {
    it('should return true if exists', () => {
      registry.registerScript(createValidScript());
      expect(registry.hasScript('test-script')).toBe(true);
    });

    it('should return false if not exists', () => {
      expect(registry.hasScript('non-existent')).toBe(false);
    });
  });

  describe('clearScripts', () => {
    it('should remove all scripts', () => {
      registry.registerScript(createValidScript({ name: 's1' }));
      registry.registerScript(createValidScript({ name: 's2' }));
      registry.clearScripts();
      expect(registry.scriptCount()).toBe(0);
    });
  });

  describe('scriptCount', () => {
    it('should return correct count', () => {
      expect(registry.scriptCount()).toBe(0);
      registry.registerScript(createValidScript());
      expect(registry.scriptCount()).toBe(1);
    });
  });

  describe('updateScript', () => {
    it('should update existing script fields', () => {
      registry.registerScript(createValidScript());
      registry.updateScript('test-script', { description: 'Updated description' });
      expect(registry.getScript('test-script').description).toBe('Updated description');
    });

    it('should throw if script does not exist', () => {
      expect(() => registry.updateScript('non-existent', { description: 'test' })).toThrow('not found');
    });
  });

  describe('enableScript / disableScript / isScriptEnabled', () => {
    it('should enable a script', () => {
      registry.registerScript(createValidScript({ enabled: false }));
      registry.enableScript('test-script');
      expect(registry.isScriptEnabled('test-script')).toBe(true);
    });

    it('should disable a script', () => {
      registry.registerScript(createValidScript({ enabled: true }));
      registry.disableScript('test-script');
      expect(registry.isScriptEnabled('test-script')).toBe(false);
    });

    it('should throw isScriptEnabled for non-existent script', () => {
      expect(() => registry.isScriptEnabled('non-existent')).toThrow('not found');
    });
  });

  describe('validateScript', () => {
    it('should throw on empty name', () => {
      expect(() => registry.registerScript(createValidScript({ name: '' }))).toThrow('name is required');
    });

    it('should throw on missing name', () => {
      expect(() => registry.registerScript(createValidScript({ name: undefined as any }))).toThrow('name is required');
    });

    it('should throw on missing description', () => {
      expect(() => registry.registerScript(createValidScript({ description: '' }))).toThrow('description is required');
    });

    it('should throw when content, filePath, and template are all missing', () => {
      expect(() =>
        registry.registerScript(createValidScript({ content: undefined, filePath: undefined, template: undefined })),
      ).toThrow('must have either content, filePath, or template');
    });

    it('should accept script with filePath instead of content', () => {
      registry.registerScript(createValidScript({ content: undefined, filePath: '/path/to/script.js' }));
      expect(registry.hasScript('test-script')).toBe(true);
    });

    it('should accept script with template instead of content', () => {
      registry.registerScript(createValidScript({ content: undefined, template: 'echo {{input}}' }));
      expect(registry.hasScript('test-script')).toBe(true);
    });

    it('should throw on missing options', () => {
      expect(() =>
        registry.registerScript(createValidScript({ options: undefined as any })),
      ).toThrow('options are required');
    });

    it('should throw on negative timeout', () => {
      expect(() =>
        registry.registerScript(createValidScript({ options: { timeout: -1, retries: 0, retryDelay: 0 } as any })),
      ).toThrow('timeout');
    });

    it('should throw on negative retries', () => {
      expect(() =>
        registry.registerScript(createValidScript({ options: { timeout: 1000, retries: -1, retryDelay: 0 } as any })),
      ).toThrow('retries');
    });

    it('should throw on negative retryDelay', () => {
      expect(() =>
        registry.registerScript(createValidScript({ options: { timeout: 1000, retries: 0, retryDelay: -1 } as any })),
      ).toThrow('retryDelay');
    });

    it('should throw on invalid enabled type', () => {
      expect(() =>
        registry.registerScript(createValidScript({ enabled: 'yes' as any })),
      ).toThrow('enabled must be a boolean');
    });
  });

  describe('execute', () => {
    it('should throw ScriptNotFoundError for non-existent script', async () => {
      await expect(registry.execute('non-existent')).rejects.toThrow('not found');
    });
  });

  describe('executeWithEngine', () => {
    it('should throw ScriptNotFoundError for non-existent script', async () => {
      await expect(registry.executeWithEngine('non-existent')).rejects.toThrow('not found');
    });
  });

  describe('registerFlow', () => {
    it('should register a flow', () => {
      const flow: ScriptFlow = { name: 'test-flow', steps: [] };
      registry.registerFlow(flow);
      expect(registry.getFlow('test-flow')).toBeDefined();
    });

    it('should throw on duplicate flow name', () => {
      registry.registerFlow({ name: 'dup', steps: [] });
      expect(() => registry.registerFlow({ name: 'dup', steps: [] })).toThrow('already exists');
    });
  });

  describe('getFlow', () => {
    it('should return registered flow', () => {
      const flow: ScriptFlow = { name: 'my-flow', steps: [] };
      registry.registerFlow(flow);
      expect(registry.getFlow('my-flow').name).toBe('my-flow');
    });

    it('should throw if flow not found', () => {
      expect(() => registry.getFlow('non-existent')).toThrow('not found');
    });
  });

  describe('listFlows', () => {
    it('should list all flows', () => {
      registry.registerFlow({ name: 'f1', steps: [] });
      registry.registerFlow({ name: 'f2', steps: [] });
      expect(registry.listFlows()).toHaveLength(2);
    });
  });

  describe('executeFlow', () => {
    it('should throw for non-existent flow', async () => {
      await expect(registry.executeFlow('non-existent')).rejects.toThrow('not found');
    });
  });

  describe('executeBatch', () => {
    it('should return combined results', async () => {
      registry.registerScript(createValidScript({ name: 's1' }));
      registry.registerScript(createValidScript({ name: 's2' }));

      const result = await registry.executeBatch([
        { scriptName: 's1' },
        { scriptName: 's2' },
      ]);

      expect(result.isOk()).toBe(true);
    });
  });
});