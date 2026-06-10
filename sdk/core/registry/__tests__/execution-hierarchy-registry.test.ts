import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ExecutionHierarchyRegistry } from '../execution-hierarchy-registry.js';
import type { ChildExecutionReference, ParentExecutionContext } from '@wf-agent/types';

function createMockWorkflowEntity(
  id: string,
  children: ChildExecutionReference[] = [],
  parentContext?: ParentExecutionContext,
) {
  return {
    id,
    getChildren: () => children,
    getParentContext: () => parentContext,
    getWorkflowId: () => `wf-${id}`,
    stop: vi.fn(),
    cleanup: vi.fn(),
  } as any;
}

function createMockAgentEntity(
  id: string,
  children: ChildExecutionReference[] = [],
  parentContext?: ParentExecutionContext,
) {
  return {
    id,
    getChildren: () => children,
    getParentContext: () => parentContext,
    conversationManager: { id: `cm-${id}` },
    stop: vi.fn(),
    cleanup: vi.fn(),
  } as any;
}

function createChildRef(childId: string, childType: 'WORKFLOW' | 'AGENT_LOOP' = 'WORKFLOW'): ChildExecutionReference {
  return {
    childType,
    childId,
    createdAt: Date.now(),
  } as ChildExecutionReference;
}

describe('ExecutionHierarchyRegistry', () => {
  let registry: ExecutionHierarchyRegistry;

  beforeEach(() => {
    registry = new ExecutionHierarchyRegistry();
  });

  describe('basic CRUD', () => {
    it('should register and get an execution', () => {
      const entity = createMockWorkflowEntity('wf-1');
      registry.register(entity);
      expect(registry.get('wf-1')).toBe(entity);
    });

    it('should check existence with has()', () => {
      registry.register(createMockWorkflowEntity('wf-1'));
      expect(registry.has('wf-1')).toBe(true);
      expect(registry.has('non-existent')).toBe(false);
    });

    it('should unregister an execution', () => {
      registry.register(createMockWorkflowEntity('wf-1'));
      const result = registry.unregister('wf-1');
      expect(result).toBe(true);
      expect(registry.has('wf-1')).toBe(false);
    });

    it('should return false when unregistering non-existent', () => {
      expect(registry.unregister('non-existent')).toBe(false);
    });

    it('should return all executions', () => {
      const e1 = createMockWorkflowEntity('wf-1');
      const e2 = createMockAgentEntity('agent-1');
      registry.register(e1);
      registry.register(e2);
      expect(registry.getAll()).toEqual([e1, e2]);
    });

    it('should return all IDs', () => {
      registry.register(createMockWorkflowEntity('wf-1'));
      registry.register(createMockAgentEntity('agent-1'));
      expect(registry.getAllIds()).toEqual(['wf-1', 'agent-1']);
    });

    it('should return correct size', () => {
      expect(registry.size()).toBe(0);
      registry.register(createMockWorkflowEntity('wf-1'));
      expect(registry.size()).toBe(1);
    });

    it('should clear all executions', () => {
      registry.register(createMockWorkflowEntity('wf-1'));
      registry.register(createMockAgentEntity('agent-1'));
      registry.clear();
      expect(registry.size()).toBe(0);
    });
  });

  describe('getAllDescendants', () => {
    it('should return empty array for unknown execution', () => {
      expect(registry.getAllDescendants('unknown')).toEqual([]);
    });

    it('should include self when includeSelf is true', () => {
      const parent = createMockWorkflowEntity('wf-1');
      registry.register(parent);
      const descendants = registry.getAllDescendants('wf-1', true);
      expect(descendants).toContain(parent);
    });

    it('should return direct and indirect children', () => {
      const childRef = createChildRef('agent-1', 'AGENT_LOOP');
      const parent = createMockWorkflowEntity('wf-1', [childRef]);
      const child = createMockAgentEntity('agent-1', [], {
        parentType: 'WORKFLOW',
        parentId: 'wf-1',
      });

      registry.register(parent);
      registry.register(child);

      const descendants = registry.getAllDescendants('wf-1');
      expect(descendants).toHaveLength(1);
      expect(descendants[0]!.id).toBe('agent-1');
    });

    it('should handle nested hierarchies recursively', () => {
      const grandChildRef = createChildRef('agent-2', 'AGENT_LOOP');
      const childRef = createChildRef('agent-1', 'AGENT_LOOP');
      const parent = createMockWorkflowEntity('wf-1', [childRef]);
      const child = createMockAgentEntity('agent-1', [grandChildRef], {
        parentType: 'WORKFLOW',
        parentId: 'wf-1',
      });
      const grandChild = createMockAgentEntity('agent-2', [], {
        parentType: 'AGENT_LOOP',
        parentId: 'agent-1',
      });

      registry.register(parent);
      registry.register(child);
      registry.register(grandChild);

      const descendants = registry.getAllDescendants('wf-1', true);
      expect(descendants).toHaveLength(3);
    });
  });

  describe('getDirectChildren', () => {
    it('should return empty array for unknown execution', () => {
      expect(registry.getDirectChildren('unknown')).toEqual([]);
    });

    it('should return only direct children', () => {
      const childRef = createChildRef('agent-1', 'AGENT_LOOP');
      const grandChildRef = createChildRef('agent-2', 'AGENT_LOOP');
      const parent = createMockWorkflowEntity('wf-1', [childRef]);
      const child = createMockAgentEntity('agent-1', [grandChildRef], {
        parentType: 'WORKFLOW',
        parentId: 'wf-1',
      });
      const grandChild = createMockAgentEntity('agent-2', [], {
        parentType: 'AGENT_LOOP',
        parentId: 'agent-1',
      });

      registry.register(parent);
      registry.register(child);
      registry.register(grandChild);

      const children = registry.getDirectChildren('wf-1');
      expect(children).toHaveLength(1);
      expect(children[0]!.id).toBe('agent-1');
    });
  });

  describe('cleanupHierarchy', () => {
    it('should cleanup all descendants including self', () => {
      const childRef = createChildRef('agent-1', 'AGENT_LOOP');
      const parent = createMockWorkflowEntity('wf-1', [childRef]);
      const child = createMockAgentEntity('agent-1', [], {
        parentType: 'WORKFLOW',
        parentId: 'wf-1',
      });
      registry.register(parent);
      registry.register(child);

      const count = registry.cleanupHierarchy('wf-1');
      expect(count).toBe(2);
      expect(parent.stop).toHaveBeenCalledTimes(1);
      expect(parent.cleanup).toHaveBeenCalledTimes(1);
      expect(child.stop).toHaveBeenCalledTimes(1);
      expect(child.cleanup).toHaveBeenCalledTimes(1);
      expect(registry.has('wf-1')).toBe(false);
      expect(registry.has('agent-1')).toBe(false);
    });

    it('should continue cleanup even if stop throws', () => {
      const parent = createMockWorkflowEntity('wf-1');
      (parent as any).stop = vi.fn(() => { throw new Error('stop error'); });
      registry.register(parent);

      const count = registry.cleanupHierarchy('wf-1');
      expect(count).toBe(1);
      expect(registry.has('wf-1')).toBe(false);
    });

    it('should handle entities without stop/cleanup methods', () => {
      const entity = {
        id: 'no-methods',
        getChildren: () => [],
        getParentContext: () => undefined,
      };
      registry.register(entity as any);

      const count = registry.cleanupHierarchy('no-methods');
      expect(count).toBe(1);
    });
  });

  describe('getExecutionsByRoot', () => {
    it('should group descendants by type', () => {
      const childRef = createChildRef('agent-1', 'AGENT_LOOP');
      const parent = createMockWorkflowEntity('wf-1', [childRef]);
      const child = createMockAgentEntity('agent-1', [], {
        parentType: 'WORKFLOW',
        parentId: 'wf-1',
      });

      registry.register(parent);
      registry.register(child);

      const { workflows, agents } = registry.getExecutionsByRoot('wf-1');
      expect(workflows).toHaveLength(1);
      expect(agents).toHaveLength(1);
      expect(workflows[0]!.id).toBe('wf-1');
      expect(agents[0]!.id).toBe('agent-1');
    });
  });

  describe('getRootExecutions', () => {
    it('should return executions without parent context', () => {
      const root = createMockWorkflowEntity('wf-1');
      const child = createMockAgentEntity('agent-1', [], {
        parentType: 'WORKFLOW',
        parentId: 'wf-1',
      });
      registry.register(root);
      registry.register(child);

      const roots = registry.getRootExecutions();
      expect(roots).toHaveLength(1);
      expect(roots[0]!.id).toBe('wf-1');
    });

    it('should handle entities without getParentContext', () => {
      const entity = { id: 'no-parent-ctx', getChildren: () => [] };
      registry.register(entity as any);
      const roots = registry.getRootExecutions();
      expect(roots).toHaveLength(0);
    });
  });

  describe('getChildrenOf', () => {
    it('should return all children of a parent', () => {
      const child1 = createMockAgentEntity('agent-1', [], {
        parentType: 'WORKFLOW',
        parentId: 'wf-1',
      });
      const child2 = createMockAgentEntity('agent-2', [], {
        parentType: 'WORKFLOW',
        parentId: 'wf-1',
      });
      registry.register(child1);
      registry.register(child2);

      const children = registry.getChildrenOf('wf-1');
      expect(children).toHaveLength(2);
    });

    it('should return empty array for parent with no children', () => {
      expect(registry.getChildrenOf('wf-1')).toEqual([]);
    });
  });

  describe('getByType', () => {
    it('should filter by WORKFLOW type', () => {
      registry.register(createMockWorkflowEntity('wf-1'));
      registry.register(createMockAgentEntity('agent-1'));

      const workflows = registry.getByType('WORKFLOW');
      expect(workflows).toHaveLength(1);
      expect(workflows[0]!.id).toBe('wf-1');
    });

    it('should filter by AGENT_LOOP type', () => {
      registry.register(createMockWorkflowEntity('wf-1'));
      registry.register(createMockAgentEntity('agent-1'));

      const agents = registry.getByType('AGENT_LOOP');
      expect(agents).toHaveLength(1);
      expect(agents[0]!.id).toBe('agent-1');
    });
  });

  describe('isInHierarchy', () => {
    it('should return true if execution is the root', () => {
      registry.register(createMockWorkflowEntity('wf-1'));
      expect(registry.isInHierarchy('wf-1', 'wf-1')).toBe(true);
    });

    it('should return true if execution is descendant of root', () => {
      const child = createMockAgentEntity('agent-1', [], {
        parentType: 'WORKFLOW',
        parentId: 'wf-1',
      });
      registry.register(createMockWorkflowEntity('wf-1'));
      registry.register(child);

      expect(registry.isInHierarchy('agent-1', 'wf-1')).toBe(true);
    });

    it('should return false if execution not in hierarchy', () => {
      registry.register(createMockWorkflowEntity('wf-1'));
      registry.register(createMockAgentEntity('agent-1'));

      expect(registry.isInHierarchy('agent-1', 'wf-1')).toBe(false);
    });

    it('should return false for unknown execution', () => {
      expect(registry.isInHierarchy('unknown', 'wf-1')).toBe(false);
    });
  });
});