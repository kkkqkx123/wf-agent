/**
 * Execution Hierarchy Registry Tests
 * 
 * Comprehensive tests for the ExecutionHierarchyRegistry class, covering:
 * - Basic CRUD operations
 * - Hierarchical queries (descendants, children)
 * - Cleanup operations
 * - Complex hierarchy scenarios
 * - Type filtering and grouping
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { ExecutionHierarchyRegistry } from '../../registry/execution-hierarchy-registry.js';
import type { AnyExecutionEntity } from '../../registry/execution-hierarchy-registry.js';

// Mock execution entity for testing
class MockExecutionEntity {
  id: string;
  private parentContext?: any;
  private children: any[] = [];
  stopped: boolean = false;
  cleanedUp: boolean = false;
  isWorkflow: boolean;

  cleanup() {
    this.cleanedUp = true;
  }

  // Distinguish workflow from agent
  getWorkflowId() {
    return this.isWorkflow ? this.id : undefined;
  }

  conversationManager: any;

  constructor(id: string, type: 'workflow' | 'agent' = 'workflow') {
    this.id = id;
    this.isWorkflow = type === 'workflow';
    this.conversationManager = type === 'agent' ? {} : undefined;
  }

  getParentContext() {
    return this.parentContext;
  }

  setParentContext(context: any) {
    this.parentContext = context;
  }

  getChildren() {
    return this.children;
  }

  addChild(childRef: any) {
    this.children.push(childRef);
  }

  stop() {
    this.stopped = true;
  }
}

describe('ExecutionHierarchyRegistry', () => {
  let registry: ExecutionHierarchyRegistry;

  beforeEach(() => {
    registry = new ExecutionHierarchyRegistry();
  });

  describe('Basic Operations', () => {
    it('should register and retrieve executions', () => {
      const execution = new MockExecutionEntity('exec-1') as any;
      registry.register(execution);

      expect(registry.get('exec-1')).toBe(execution);
      expect(registry.has('exec-1')).toBe(true);
    });

    it('should unregister executions', () => {
      const execution = new MockExecutionEntity('exec-1') as any;
      registry.register(execution);
      
      const result = registry.unregister('exec-1');
      expect(result).toBe(true);
      expect(registry.get('exec-1')).toBeUndefined();
      expect(registry.has('exec-1')).toBe(false);
    });

    it('should return false when unregistering non-existent execution', () => {
      const result = registry.unregister('non-existent');
      expect(result).toBe(false);
    });

    it('should get all executions', () => {
      const exec1 = new MockExecutionEntity('exec-1') as any;
      const exec2 = new MockExecutionEntity('exec-2') as any;
      registry.register(exec1);
      registry.register(exec2);

      const all = registry.getAll();
      expect(all).toHaveLength(2);
      expect(all).toContain(exec1);
      expect(all).toContain(exec2);
    });

    it('should get all execution IDs', () => {
      registry.register(new MockExecutionEntity('exec-1') as any);
      registry.register(new MockExecutionEntity('exec-2') as any);

      const ids = registry.getAllIds();
      expect(ids).toEqual(['exec-1', 'exec-2']);
    });

    it('should return correct size', () => {
      expect(registry.size()).toBe(0);
      registry.register(new MockExecutionEntity('exec-1') as any);
      expect(registry.size()).toBe(1);
      registry.register(new MockExecutionEntity('exec-2') as any);
      expect(registry.size()).toBe(2);
    });

    it('should clear all executions', () => {
      registry.register(new MockExecutionEntity('exec-1') as any);
      registry.register(new MockExecutionEntity('exec-2') as any);
      
      registry.clear();
      expect(registry.size()).toBe(0);
    });
  });

  describe('Direct Children Queries', () => {
    it('should get direct children of an execution', () => {
      const parent = new MockExecutionEntity('parent');
      const child1 = new MockExecutionEntity('child-1');
      const child2 = new MockExecutionEntity('child-2');

      parent.addChild({ childType: 'WORKFLOW', childId: 'child-1', createdAt: Date.now() });
      parent.addChild({ childType: 'AGENT_LOOP', childId: 'child-2', createdAt: Date.now() });

      registry.register(parent);
      registry.register(child1);
      registry.register(child2);

      const children = registry.getDirectChildren('parent');
      expect(children).toHaveLength(2);
      expect(children).toContain(child1);
      expect(children).toContain(child2);
    });

    it('should return empty array for non-existent parent', () => {
      const children = registry.getDirectChildren('non-existent');
      expect(children).toHaveLength(0);
    });

    it('should return empty array for parent with no children', () => {
      const parent = new MockExecutionEntity('parent');
      registry.register(parent);

      const children = registry.getDirectChildren('parent');
      expect(children).toHaveLength(0);
    });
  });

  describe('Recursive Descendant Queries', () => {
    it('should get all descendants excluding self', () => {
      // Create hierarchy: root -> child1 -> grandchild1
      const root = new MockExecutionEntity('root');
      const child1 = new MockExecutionEntity('child-1');
      const grandchild1 = new MockExecutionEntity('grandchild-1');

      root.addChild({ childType: 'WORKFLOW', childId: 'child-1', createdAt: Date.now() });
      child1.addChild({ childType: 'WORKFLOW', childId: 'grandchild-1', createdAt: Date.now() });

      registry.register(root);
      registry.register(child1);
      registry.register(grandchild1);

      const descendants = registry.getAllDescendants('root', false);
      expect(descendants).toHaveLength(2);
      expect(descendants).toContain(child1);
      expect(descendants).toContain(grandchild1);
    });

    it('should get all descendants including self', () => {
      const root = new MockExecutionEntity('root');
      const child1 = new MockExecutionEntity('child-1');

      root.addChild({ childType: 'WORKFLOW', childId: 'child-1', createdAt: Date.now() });

      registry.register(root);
      registry.register(child1);

      const descendants = registry.getAllDescendants('root', true);
      expect(descendants).toHaveLength(2);
      expect(descendants).toContain(root);
      expect(descendants).toContain(child1);
    });

    it('should handle complex nested hierarchies', () => {
      // Create complex hierarchy:
      // root
      // ├── child1
      // │   ├── grandchild1
      // │   └── grandchild2
      // └── child2
      const root = new MockExecutionEntity('root');
      const child1 = new MockExecutionEntity('child-1');
      const child2 = new MockExecutionEntity('child-2');
      const grandchild1 = new MockExecutionEntity('grandchild-1');
      const grandchild2 = new MockExecutionEntity('grandchild-2');

      root.addChild({ childType: 'WORKFLOW', childId: 'child-1', createdAt: Date.now() });
      root.addChild({ childType: 'WORKFLOW', childId: 'child-2', createdAt: Date.now() });
      child1.addChild({ childType: 'WORKFLOW', childId: 'grandchild-1', createdAt: Date.now() });
      child1.addChild({ childType: 'AGENT_LOOP', childId: 'grandchild-2', createdAt: Date.now() });

      registry.register(root);
      registry.register(child1);
      registry.register(child2);
      registry.register(grandchild1);
      registry.register(grandchild2);

      const descendants = registry.getAllDescendants('root', false);
      expect(descendants).toHaveLength(4);
      expect(descendants).toContain(child1);
      expect(descendants).toContain(child2);
      expect(descendants).toContain(grandchild1);
      expect(descendants).toContain(grandchild2);
    });

    it('should return empty array for non-existent execution', () => {
      const descendants = registry.getAllDescendants('non-existent', false);
      expect(descendants).toHaveLength(0);
    });
  });

  describe('Cleanup Operations', () => {
    it('should cleanup entire hierarchy', () => {
      const root = new MockExecutionEntity('root');
      const child1 = new MockExecutionEntity('child-1');
      const child2 = new MockExecutionEntity('child-2');

      root.addChild({ childType: 'WORKFLOW', childId: 'child-1', createdAt: Date.now() });
      root.addChild({ childType: 'WORKFLOW', childId: 'child-2', createdAt: Date.now() });

      registry.register(root);
      registry.register(child1);
      registry.register(child2);

      const count = registry.cleanupHierarchy('root');
      
      expect(count).toBe(3);
      expect(registry.size()).toBe(0);
      expect(root.stopped).toBe(true);
      expect(root.cleanedUp).toBe(true);
      expect(child1.stopped).toBe(true);
      expect(child2.stopped).toBe(true);
    });

    it('should cleanup only specified hierarchy', () => {
      const root1 = new MockExecutionEntity('root-1');
      const root2 = new MockExecutionEntity('root-2');
      const child1 = new MockExecutionEntity('child-1');

      root1.addChild({ childType: 'WORKFLOW', childId: 'child-1', createdAt: Date.now() });

      registry.register(root1);
      registry.register(root2);
      registry.register(child1);

      const count = registry.cleanupHierarchy('root-1');
      
      expect(count).toBe(2);
      expect(registry.size()).toBe(1);
      expect(registry.get('root-2')).toBe(root2);
    });

    it('should handle cleanup of non-existent execution', () => {
      const count = registry.cleanupHierarchy('non-existent');
      expect(count).toBe(0);
    });
  });

  describe('Query by Root', () => {
    it('should get executions grouped by root', () => {
      const workflow = new MockExecutionEntity('workflow-1', 'workflow') as any;
      const agent1 = new MockExecutionEntity('agent-1', 'agent') as any;
      const agent2 = new MockExecutionEntity('agent-2', 'agent') as any;

      workflow.addChild({ childType: 'AGENT_LOOP', childId: 'agent-1', createdAt: Date.now() });
      workflow.addChild({ childType: 'AGENT_LOOP', childId: 'agent-2', createdAt: Date.now() });

      registry.register(workflow);
      registry.register(agent1);
      registry.register(agent2);

      const result = registry.getExecutionsByRoot('workflow-1');
      
      expect(result.workflows).toHaveLength(1);
      expect(result.workflows).toContain(workflow);
      expect(result.agents).toHaveLength(2);
      expect(result.agents).toContain(agent1);
      expect(result.agents).toContain(agent2);
    });

    it('should handle mixed hierarchy types', () => {
      const workflow1 = new MockExecutionEntity('workflow-1', 'workflow') as any;
      const workflow2 = new MockExecutionEntity('workflow-2', 'workflow') as any;
      const agent1 = new MockExecutionEntity('agent-1', 'agent') as any;

      workflow1.addChild({ childType: 'WORKFLOW', childId: 'workflow-2', createdAt: Date.now() });
      workflow1.addChild({ childType: 'AGENT_LOOP', childId: 'agent-1', createdAt: Date.now() });

      registry.register(workflow1);
      registry.register(workflow2);
      registry.register(agent1);

      const result = registry.getExecutionsByRoot('workflow-1');
      
      expect(result.workflows).toHaveLength(2);
      expect(result.agents).toHaveLength(1);
    });
  });

  describe('Root Executions', () => {
    it('should get all root executions', () => {
      const root1 = new MockExecutionEntity('root-1');
      const root2 = new MockExecutionEntity('root-2');
      const child = new MockExecutionEntity('child');

      child.setParentContext({ parentType: 'WORKFLOW', parentId: 'root-1' });

      registry.register(root1);
      registry.register(root2);
      registry.register(child);

      const roots = registry.getRootExecutions();
      
      expect(roots).toHaveLength(2);
      expect(roots).toContain(root1);
      expect(roots).toContain(root2);
    });
  });

  describe('Children of Parent', () => {
    it('should get all children of a specific parent', () => {
      const parent = new MockExecutionEntity('parent');
      const child1 = new MockExecutionEntity('child-1');
      const child2 = new MockExecutionEntity('child-2');
      const unrelated = new MockExecutionEntity('unrelated');

      child1.setParentContext({ parentType: 'WORKFLOW', parentId: 'parent' });
      child2.setParentContext({ parentType: 'AGENT_LOOP', parentId: 'parent' });

      registry.register(parent);
      registry.register(child1);
      registry.register(child2);
      registry.register(unrelated);

      const children = registry.getChildrenOf('parent');
      
      expect(children).toHaveLength(2);
      expect(children).toContain(child1);
      expect(children).toContain(child2);
    });
  });

  describe('Type Filtering', () => {
    it('should filter executions by type WORKFLOW', () => {
      const workflow1 = new MockExecutionEntity('workflow-1', 'workflow') as any;
      const workflow2 = new MockExecutionEntity('workflow-2', 'workflow') as any;
      const agent1 = new MockExecutionEntity('agent-1', 'agent') as any;

      registry.register(workflow1);
      registry.register(workflow2);
      registry.register(agent1);

      const workflows = registry.getByType('WORKFLOW');
      
      expect(workflows).toHaveLength(2);
      expect(workflows).toContain(workflow1);
      expect(workflows).toContain(workflow2);
    });

    it('should filter executions by type AGENT_LOOP', () => {
      const workflow1 = new MockExecutionEntity('workflow-1', 'workflow') as any;
      const agent1 = new MockExecutionEntity('agent-1', 'agent') as any;
      const agent2 = new MockExecutionEntity('agent-2', 'agent') as any;

      registry.register(workflow1);
      registry.register(agent1);
      registry.register(agent2);

      const agents = registry.getByType('AGENT_LOOP');
      
      expect(agents).toHaveLength(2);
      expect(agents).toContain(agent1);
      expect(agents).toContain(agent2);
    });
  });

  describe('Hierarchy Membership Check', () => {
    it('should check if execution is in hierarchy tree', () => {
      const root = new MockExecutionEntity('root');
      const child = new MockExecutionEntity('child');
      const grandchild = new MockExecutionEntity('grandchild');
      const unrelated = new MockExecutionEntity('unrelated');

      child.setParentContext({ parentType: 'WORKFLOW', parentId: 'root' });
      grandchild.setParentContext({ parentType: 'WORKFLOW', parentId: 'child' });

      registry.register(root);
      registry.register(child);
      registry.register(grandchild);
      registry.register(unrelated);

      expect(registry.isInHierarchy('root', 'root')).toBe(true);
      expect(registry.isInHierarchy('child', 'root')).toBe(true);
      expect(registry.isInHierarchy('grandchild', 'root')).toBe(true);
      expect(registry.isInHierarchy('unrelated', 'root')).toBe(false);
    });

    it('should return false for non-existent execution', () => {
      const root = new MockExecutionEntity('root');
      registry.register(root);

      expect(registry.isInHierarchy('non-existent', 'root')).toBe(false);
    });
  });

  describe('Complex Scenarios', () => {
    it('should handle Agent → Agent hierarchy', () => {
      const mainAgent = new MockExecutionEntity('main-agent');
      const subAgent1 = new MockExecutionEntity('sub-agent-1');
      const subAgent2 = new MockExecutionEntity('sub-agent-2');

      mainAgent.addChild({ childType: 'AGENT_LOOP', childId: 'sub-agent-1', createdAt: Date.now() });
      mainAgent.addChild({ childType: 'AGENT_LOOP', childId: 'sub-agent-2', createdAt: Date.now() });
      subAgent1.setParentContext({ parentType: 'AGENT_LOOP', parentId: 'main-agent' });
      subAgent2.setParentContext({ parentType: 'AGENT_LOOP', parentId: 'main-agent' });

      registry.register(mainAgent);
      registry.register(subAgent1);
      registry.register(subAgent2);

      const descendants = registry.getAllDescendants('main-agent', false);
      expect(descendants).toHaveLength(2);
      
      const children = registry.getDirectChildren('main-agent');
      expect(children).toHaveLength(2);
    });

    it('should handle Workflow → Agent → Agent hierarchy', () => {
      const workflow = new MockExecutionEntity('workflow', 'workflow') as any;
      const agent1 = new MockExecutionEntity('agent-1', 'agent') as any;
      const agent2 = new MockExecutionEntity('agent-2', 'agent') as any;

      workflow.addChild({ childType: 'AGENT_LOOP', childId: 'agent-1', createdAt: Date.now() });
      agent1.addChild({ childType: 'AGENT_LOOP', childId: 'agent-2', createdAt: Date.now() });
      agent1.setParentContext({ parentType: 'WORKFLOW', parentId: 'workflow' });
      agent2.setParentContext({ parentType: 'AGENT_LOOP', parentId: 'agent-1' });

      registry.register(workflow);
      registry.register(agent1);
      registry.register(agent2);

      const descendants = registry.getAllDescendants('workflow', false);
      expect(descendants).toHaveLength(2);
      expect(descendants).toContain(agent1);
      expect(descendants).toContain(agent2);

      const { workflows, agents } = registry.getExecutionsByRoot('workflow');
      expect(workflows).toHaveLength(1);
      expect(agents).toHaveLength(2);
    });

    it('should handle deep nesting within depth limits', () => {
      const entities: MockExecutionEntity[] = [];
      for (let i = 0; i < 5; i++) {
        entities.push(new MockExecutionEntity(`level-${i}`));
      }

      // Create chain: level-0 -> level-1 -> level-2 -> level-3 -> level-4
      for (let i = 0; i < entities.length - 1; i++) {
        entities[i].addChild({ 
          childType: 'WORKFLOW', 
          childId: `level-${i + 1}`, 
          createdAt: Date.now() 
        });
        entities[i + 1].setParentContext({ 
          parentType: 'WORKFLOW', 
          parentId: `level-${i}` 
        });
        registry.register(entities[i]);
      }
      registry.register(entities[entities.length - 1]);

      const descendants = registry.getAllDescendants('level-0', false);
      expect(descendants).toHaveLength(4);
    });
  });
});
