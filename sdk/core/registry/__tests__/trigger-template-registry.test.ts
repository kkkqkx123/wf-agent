import { describe, it, expect, beforeEach } from 'vitest';
import { TriggerTemplateRegistry } from '../trigger-template-registry.js';
import type { TriggerTemplate } from '@wf-agent/types';

function createValidTemplate(overrides: Partial<TriggerTemplate> = {}): TriggerTemplate {
  return {
    name: 'test-trigger',
    description: 'A test trigger template',
    condition: {
      eventType: 'NODE_COMPLETED',
      filter: undefined,
    },
    action: {
      type: 'set_variable',
      parameters: { key: 'status', value: 'done' },
    },
    enabled: true,
    createdAt: Date.now(),
    updatedAt: Date.now(),
    metadata: { category: 'workflow', tags: ['test'] },
    ...overrides,
  } as TriggerTemplate;
}

describe('TriggerTemplateRegistry', () => {
  let registry: TriggerTemplateRegistry;

  beforeEach(() => {
    registry = new TriggerTemplateRegistry();
  });

  describe('register', () => {
    it('should register a valid trigger template', () => {
      registry.register(createValidTemplate());
      expect(registry.has('test-trigger')).toBe(true);
    });

    it('should throw if name already exists', () => {
      registry.register(createValidTemplate());
      expect(() => registry.register(createValidTemplate())).toThrow('already exists');
    });

    it('should skip if skipIfExists option is set', () => {
      registry.register(createValidTemplate());
      expect(() => registry.register(createValidTemplate(), { skipIfExists: true })).not.toThrow();
    });

    it('should throw if name is empty', () => {
      expect(() => registry.register(createValidTemplate({ name: '' }))).toThrow('name is required');
    });

    it('should throw if condition is missing', () => {
      expect(() =>
        registry.register(createValidTemplate({ condition: undefined as any })),
      ).toThrow('condition is required');
    });

    it('should throw if action is missing', () => {
      expect(() =>
        registry.register(createValidTemplate({ action: undefined as any })),
      ).toThrow('action is required');
    });

    it('should throw if eventType is invalid', () => {
      expect(() =>
        registry.register(createValidTemplate({ condition: { eventType: 'INVALID_EVENT' } as any })),
      ).toThrow('Invalid event type');
    });

    it('should throw if action type is invalid', () => {
      expect(() =>
        registry.register(createValidTemplate({ action: { type: 'invalid_action' } as any })),
      ).toThrow('Invalid action type');
    });
  });

  describe('registerBatch', () => {
    it('should register multiple templates', () => {
      const t1 = createValidTemplate({ name: 't1' });
      const t2 = createValidTemplate({ name: 't2' });
      registry.registerBatch([t1, t2]);
      expect(registry.size()).toBe(2);
    });

    it('should continue on error with skipErrors option', () => {
      registry.register(createValidTemplate({ name: 't1' }));
      registry.registerBatch(
        [createValidTemplate({ name: 't1' }), createValidTemplate({ name: 't2' })],
        { skipErrors: true },
      );
      expect(registry.size()).toBe(2);
    });

    it('should throw on first error without skipErrors', () => {
      registry.register(createValidTemplate({ name: 't1' }));
      expect(() =>
        registry.registerBatch([
          createValidTemplate({ name: 't1' }),
          createValidTemplate({ name: 't2' }),
        ]),
      ).toThrow('already exists');
    });
  });

  describe('update', () => {
    it('should update an existing template', () => {
      registry.register(createValidTemplate());
      registry.update('test-trigger', { description: 'Updated' });
      expect(registry.get('test-trigger')?.description).toBe('Updated');
    });

    it('should update updatedAt timestamp', () => {
      registry.register(createValidTemplate());
      const before = registry.get('test-trigger')!.updatedAt;
      registry.update('test-trigger', { description: 'Updated' });
      expect(registry.get('test-trigger')!.updatedAt).toBeGreaterThanOrEqual(before);
    });

    it('should throw if template does not exist', () => {
      expect(() => registry.update('non-existent', { description: 'test' })).toThrow('not found');
    });

    it('should create template with createIfNotExists option', () => {
      registry.update('new-trigger', createValidTemplate({ name: 'new-trigger' }), { createIfNotExists: true });
      expect(registry.has('new-trigger')).toBe(true);
    });
  });

  describe('upsert', () => {
    it('should register new template', () => {
      registry.upsert(createValidTemplate({ name: 'new-trigger' }));
      expect(registry.has('new-trigger')).toBe(true);
    });

    it('should update existing template', () => {
      registry.register(createValidTemplate({ name: 'existing' }));
      registry.upsert(createValidTemplate({ name: 'existing', description: 'Updated via upsert' }));
      expect(registry.get('existing')?.description).toBe('Updated via upsert');
    });
  });

  describe('get', () => {
    it('should return template by name', () => {
      registry.register(createValidTemplate());
      expect(registry.get('test-trigger')?.name).toBe('test-trigger');
    });

    it('should return undefined for non-existent template', () => {
      expect(registry.get('non-existent')).toBeUndefined();
    });
  });

  describe('has', () => {
    it('should return true if template exists', () => {
      registry.register(createValidTemplate());
      expect(registry.has('test-trigger')).toBe(true);
    });

    it('should return false if template does not exist', () => {
      expect(registry.has('non-existent')).toBe(false);
    });
  });

  describe('unregister', () => {
    it('should delete a template', () => {
      registry.register(createValidTemplate());
      registry.unregister('test-trigger');
      expect(registry.has('test-trigger')).toBe(false);
    });

    it('should throw if template does not exist', () => {
      expect(() => registry.unregister('non-existent')).toThrow('not found');
    });
  });

  describe('unregisterBatch', () => {
    it('should delete multiple templates', () => {
      registry.register(createValidTemplate({ name: 't1' }));
      registry.register(createValidTemplate({ name: 't2' }));
      registry.unregisterBatch(['t1', 't2']);
      expect(registry.size()).toBe(0);
    });

    it('should continue on error with skipErrors', () => {
      registry.register(createValidTemplate({ name: 't1' }));
      registry.unregisterBatch(['t1', 'non-existent'], { skipErrors: true });
      expect(registry.size()).toBe(0);
    });

    it('should throw on first error without skipErrors', () => {
      registry.register(createValidTemplate({ name: 't1' }));
      expect(() => registry.unregisterBatch(['t1', 'non-existent'])).toThrow('not found');
    });
  });

  describe('list', () => {
    it('should return all templates', () => {
      registry.register(createValidTemplate({ name: 't1' }));
      registry.register(createValidTemplate({ name: 't2' }));
      expect(registry.list()).toHaveLength(2);
    });

    it('should return empty array when no templates', () => {
      expect(registry.list()).toEqual([]);
    });
  });

  describe('listSummaries', () => {
    it('should return summaries with metadata fields', () => {
      registry.register(createValidTemplate({ name: 't1', metadata: { category: 'workflow', tags: ['test'] } }));
      const summaries = registry.listSummaries();
      expect(summaries).toHaveLength(1);
      expect(summaries[0].name).toBe('t1');
      expect(summaries[0].category).toBe('workflow');
      expect(summaries[0].tags).toEqual(['test']);
    });
  });

  describe('clear', () => {
    it('should remove all templates', () => {
      registry.register(createValidTemplate({ name: 't1' }));
      registry.register(createValidTemplate({ name: 't2' }));
      registry.clear();
      expect(registry.size()).toBe(0);
    });
  });

  describe('size', () => {
    it('should return 0 for empty registry', () => {
      expect(registry.size()).toBe(0);
    });

    it('should return correct count', () => {
      registry.register(createValidTemplate({ name: 't1' }));
      expect(registry.size()).toBe(1);
    });
  });

  describe('search', () => {
    it('should search by name', () => {
      registry.register(createValidTemplate({ name: 'on-node-complete' }));
      expect(registry.search('node')).toHaveLength(1);
    });

    it('should search by description', () => {
      registry.register(createValidTemplate({ name: 't1', description: 'handle workflow completion' }));
      expect(registry.search('completion')).toHaveLength(1);
    });

    it('should search by tags', () => {
      registry.register(createValidTemplate({ name: 't1', metadata: { tags: ['important'] } }));
      expect(registry.search('important')).toHaveLength(1);
    });

    it('should search by category', () => {
      registry.register(createValidTemplate({ name: 't1', metadata: { category: 'notification' } }));
      expect(registry.search('notification')).toHaveLength(1);
    });

    it('should return empty for no match', () => {
      registry.register(createValidTemplate({ name: 't1' }));
      expect(registry.search('nonexistent')).toHaveLength(0);
    });
  });

  describe('export / import', () => {
    it('should export template as JSON string', () => {
      registry.register(createValidTemplate());
      const json = registry.export('test-trigger');
      const parsed = JSON.parse(json);
      expect(parsed.name).toBe('test-trigger');
    });

    it('should throw on export of non-existent template', () => {
      expect(() => registry.export('non-existent')).toThrow('not found');
    });

    it('should import a valid template from JSON', () => {
      const template = createValidTemplate({ name: 'imported' });
      const json = JSON.stringify(template);
      const name = registry.import(json);
      expect(name).toBe('imported');
      expect(registry.has('imported')).toBe(true);
    });

    it('should throw on import of invalid JSON', () => {
      expect(() => registry.import('not json')).toThrow('Failed to import');
    });

    it('should throw on import with duplicate name', () => {
      registry.register(createValidTemplate({ name: 'dup' }));
      const json = JSON.stringify(createValidTemplate({ name: 'dup' }));
      expect(() => registry.import(json)).toThrow('already exists');
    });
  });

  describe('convertToWorkflowTrigger', () => {
    it('should convert template to WorkflowTrigger', () => {
      registry.register(createValidTemplate());
      const trigger = registry.convertToWorkflowTrigger('test-trigger', 'trigger-1');
      expect(trigger.id).toBe('trigger-1');
      expect(trigger.name).toBe('test-trigger');
      expect(trigger.condition.eventType).toBe('NODE_COMPLETED');
      expect(trigger.action.type).toBe('set_variable');
    });

    it('should throw if template does not exist', () => {
      expect(() => registry.convertToWorkflowTrigger('non-existent', 't1')).toThrow('not found');
    });

    it('should apply config overrides', () => {
      registry.register(createValidTemplate());
      const trigger = registry.convertToWorkflowTrigger('test-trigger', 't1', 'Custom Name', {
        enabled: false,
        maxTriggers: 5,
      });
      expect(trigger.name).toBe('Custom Name');
      expect(trigger.enabled).toBe(false);
      expect(trigger.maxTriggers).toBe(5);
    });
  });
});