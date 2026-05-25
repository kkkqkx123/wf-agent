/**
 * Message Context Registry Tests
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { InMemoryMessageContextRegistry } from '../message-context-registry.js';
import type { NamedMessageContext } from '@wf-agent/types';

describe('InMemoryMessageContextRegistry', () => {
  let registry: InMemoryMessageContextRegistry;

  beforeEach(() => {
    registry = new InMemoryMessageContextRegistry();
  });

  function createContext(id: string, overrides: Partial<NamedMessageContext> = {}): NamedMessageContext {
    return {
      id,
      messages: [],
      createdAt: Date.now(),
      updatedAt: Date.now(),
      metadata: {},
      ...overrides,
    };
  }

  describe('register', () => {
    it('should register a new context', () => {
      const context = createContext('ctx-1');
      registry.register(context);

      expect(registry.has('ctx-1')).toBe(true);
      expect(registry.size()).toBe(1);
    });

    it('should overwrite existing context with warning', () => {
      const ctx1 = createContext('same-id', { messages: [{ role: 'user', content: 'First' }] });
      const ctx2 = createContext('same-id', { messages: [{ role: 'user', content: 'Second' }] });

      registry.register(ctx1);
      registry.register(ctx2);

      const retrieved = registry.get('same-id');
      expect(retrieved?.messages[0]?.content).toBe('Second');
    });

    it('should set createdAt and updatedAt if not provided', () => {
      const context: NamedMessageContext = {
        id: 'ctx-auto',
        messages: [],
      } as any;

      registry.register(context);
      const retrieved = registry.get('ctx-auto');
      expect(retrieved?.createdAt).toBeDefined();
      expect(retrieved?.updatedAt).toBeDefined();
    });
  });

  describe('get', () => {
    it('should retrieve a registered context', () => {
      const context = createContext('ctx-1', { messages: [{ role: 'user', content: 'Hello' }] });
      registry.register(context);

      const retrieved = registry.get('ctx-1');
      expect(retrieved).toBeDefined();
      expect(retrieved?.id).toBe('ctx-1');
      expect(retrieved?.messages).toHaveLength(1);
    });

    it('should return undefined for non-existent context', () => {
      const retrieved = registry.get('non-existent');
      expect(retrieved).toBeUndefined();
    });
  });

  describe('update', () => {
    it('should update messages in an existing context', () => {
      const context = createContext('ctx-1');
      registry.register(context);

      const newMessages = [{ role: 'user', content: 'Updated' }];
      registry.update('ctx-1', newMessages);

      const retrieved = registry.get('ctx-1');
      expect(retrieved?.messages).toHaveLength(1);
      expect(retrieved?.messages[0]?.content).toBe('Updated');
    });

    it('should throw for non-existent context', () => {
      expect(() => {
        registry.update('non-existent', []);
      }).toThrow('Context \'non-existent\' not found');
    });
  });

  describe('delete', () => {
    it('should delete an existing context', () => {
      const context = createContext('ctx-1');
      registry.register(context);

      const result = registry.delete('ctx-1');
      expect(result).toBe(true);
      expect(registry.has('ctx-1')).toBe(false);
    });

    it('should return false for non-existent context', () => {
      const result = registry.delete('non-existent');
      expect(result).toBe(false);
    });
  });

  describe('listIds', () => {
    it('should return empty array for empty registry', () => {
      expect(registry.listIds()).toEqual([]);
    });

    it('should return all context IDs', () => {
      registry.register(createContext('a'));
      registry.register(createContext('b'));
      registry.register(createContext('c'));

      const ids = registry.listIds();
      expect(ids).toHaveLength(3);
      expect(ids).toContain('a');
      expect(ids).toContain('b');
      expect(ids).toContain('c');
    });
  });

  describe('has', () => {
    it('should return true for existing context', () => {
      registry.register(createContext('exists'));
      expect(registry.has('exists')).toBe(true);
    });

    it('should return false for non-existent context', () => {
      expect(registry.has('nope')).toBe(false);
    });
  });

  describe('size', () => {
    it('should return 0 for empty registry', () => {
      expect(registry.size()).toBe(0);
    });

    it('should return correct count', () => {
      registry.register(createContext('a'));
      registry.register(createContext('b'));
      expect(registry.size()).toBe(2);
    });
  });

  describe('clear', () => {
    it('should clear all contexts', () => {
      registry.register(createContext('a'));
      registry.register(createContext('b'));
      registry.clear();

      expect(registry.size()).toBe(0);
      expect(registry.listIds()).toEqual([]);
    });

    it('should handle clearing empty registry', () => {
      expect(() => registry.clear()).not.toThrow();
    });
  });

  describe('getAll', () => {
    it('should return empty array for empty registry', () => {
      expect(registry.getAll()).toEqual([]);
    });

    it('should return all contexts', () => {
      registry.register(createContext('a', { metadata: { key: 'val1' } as Record<string, unknown> }));
      registry.register(createContext('b', { metadata: { key: 'val2' } as Record<string, unknown> }));

      const all = registry.getAll();
      expect(all).toHaveLength(2);
      expect(all.map(c => c.id)).toContain('a');
      expect(all.map(c => c.id)).toContain('b');
    });
  });
});
