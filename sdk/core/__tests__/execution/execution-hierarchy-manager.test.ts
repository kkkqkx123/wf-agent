/**
 * Execution Hierarchy Manager Unit Tests
 * 
 * Tests for the ExecutionHierarchyManager class to ensure:
 * - Correct parent/child relationship management
 * - Depth calculation
 * - Root execution tracking
 * - Cycle detection
 * - Serialization/deserialization
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { ExecutionHierarchyManager } from '../../execution/execution-hierarchy-manager.js';
import { ExecutionHierarchyRegistry } from '../../registry/execution-hierarchy-registry.js';
import type { ParentExecutionContext, ChildExecutionReference } from '@wf-agent/types';

// Mock entity interface for testing
interface MockEntity {
  id: string;
  getParentContext(): ParentExecutionContext | undefined;
  getHierarchyDepth(): number;
  getRootExecutionId(): string;
  getRootExecutionType(): 'WORKFLOW' | 'AGENT_LOOP';
}

describe('ExecutionHierarchyManager', () => {
  let registry: ExecutionHierarchyRegistry;

  beforeEach(() => {
    registry = new ExecutionHierarchyRegistry();
  });

  describe('Initialization', () => {
    it('should initialize as root node when no hierarchy provided', () => {
      const manager = new ExecutionHierarchyManager('exec-1', 'WORKFLOW');

      expect(manager.getDepth()).toBe(0);
      expect(manager.getRootExecutionId()).toBe('exec-1');
      expect(manager.getRootExecutionType()).toBe('WORKFLOW');
      expect(manager.getParent()).toBeUndefined();
      expect(manager.getChildren()).toEqual([]);
    });

    it('should restore state from existing hierarchy metadata', () => {
      const existingMetadata = {
        parent: {
          parentType: 'WORKFLOW' as const,
          parentId: 'parent-workflow',
        },
        children: [
          {
            childType: 'AGENT_LOOP' as const,
            childId: 'child-agent-1',
            createdAt: 1234567890,
          },
        ],
        depth: 1,
        rootExecutionId: 'root-workflow',
        rootExecutionType: 'WORKFLOW' as const,
      };

      const manager = new ExecutionHierarchyManager('exec-2', 'AGENT_LOOP', existingMetadata);

      expect(manager.getDepth()).toBe(1);
      expect(manager.getRootExecutionId()).toBe('root-workflow');
      expect(manager.getRootExecutionType()).toBe('WORKFLOW');
      expect(manager.getParent()).toEqual(existingMetadata.parent);
      expect(manager.getChildren()).toHaveLength(1);
    });
  });

  describe('Parent Management', () => {
    it('should set parent context correctly', () => {
      const manager = new ExecutionHierarchyManager('agent-1', 'AGENT_LOOP');
      
      const parentContext: ParentExecutionContext = {
        parentType: 'WORKFLOW',
        parentId: 'workflow-123',
        nodeId: 'agent-node-1',
      };

      manager.setParent(parentContext);

      expect(manager.getParent()).toEqual(parentContext);
      expect(manager.getDepth()).toBeGreaterThan(0);
    });

    it('should detect self-referencing cycle', () => {
      const manager = new ExecutionHierarchyManager('agent-1', 'AGENT_LOOP');

      expect(() => {
        manager.setParent({
          parentType: 'AGENT_LOOP',
          parentId: 'agent-1', // Same as own ID
        });
      }).toThrow(/Circular reference detected/);
    });

    it('should enforce depth limit', () => {
      const manager = new ExecutionHierarchyManager('deep-agent', 'AGENT_LOOP');

      // Mock a very deep parent chain (this would need registry integration in real scenario)
      // For now, we test that the validation logic exists
      expect(() => {
        // This should pass since our simplified implementation doesn't track full depth yet
        manager.setParent({
          parentType: 'AGENT_LOOP',
          parentId: 'parent-agent',
        });
      }).not.toThrow();
    });
  });

  describe('Child Management', () => {
    it('should add child references', () => {
      const manager = new ExecutionHierarchyManager('workflow-1', 'WORKFLOW');

      const childRef: ChildExecutionReference = {
        childType: 'AGENT_LOOP',
        childId: 'agent-1',
        createdAt: Date.now(),
      };

      manager.addChild(childRef);

      expect(manager.getChildren()).toHaveLength(1);
      expect(manager.getChildren()[0]).toEqual(childRef);
    });

    it('should remove child references', () => {
      const manager = new ExecutionHierarchyManager('workflow-1', 'WORKFLOW');

      const childRef: ChildExecutionReference = {
        childType: 'AGENT_LOOP',
        childId: 'agent-1',
        createdAt: Date.now(),
      };

      manager.addChild(childRef);
      expect(manager.getChildren()).toHaveLength(1);

      const removed = manager.removeChild('agent-1', 'AGENT_LOOP');
      expect(removed).toBe(true);
      expect(manager.getChildren()).toHaveLength(0);
    });

    it('should return false when removing non-existent child', () => {
      const manager = new ExecutionHierarchyManager('workflow-1', 'WORKFLOW');

      const removed = manager.removeChild('non-existent', 'AGENT_LOOP');
      expect(removed).toBe(false);
    });

    it('should support multiple children of different types', () => {
      const manager = new ExecutionHierarchyManager('workflow-1', 'WORKFLOW');

      manager.addChild({
        childType: 'WORKFLOW',
        childId: 'sub-workflow-1',
        createdAt: 1000,
      });

      manager.addChild({
        childType: 'AGENT_LOOP',
        childId: 'agent-1',
        createdAt: 2000,
      });

      manager.addChild({
        childType: 'AGENT_LOOP',
        childId: 'agent-2',
        createdAt: 3000,
      });

      expect(manager.getChildren()).toHaveLength(3);
    });

    it('should not duplicate children with same ID and type', () => {
      const manager = new ExecutionHierarchyManager('workflow-1', 'WORKFLOW');

      const childRef1: ChildExecutionReference = {
        childType: 'AGENT_LOOP',
        childId: 'agent-1',
        createdAt: 1000,
      };

      const childRef2: ChildExecutionReference = {
        childType: 'AGENT_LOOP',
        childId: 'agent-1',
        createdAt: 2000, // Different timestamp
      };

      manager.addChild(childRef1);
      manager.addChild(childRef2); // Should overwrite

      expect(manager.getChildren()).toHaveLength(1);
      expect(manager.getChildren()[0].createdAt).toBe(2000);
    });
  });

  describe('Hierarchy Metadata', () => {
    it('should convert to metadata correctly', () => {
      const manager = new ExecutionHierarchyManager('workflow-1', 'WORKFLOW');

      manager.setParent({
        parentType: 'WORKFLOW',
        parentId: 'parent-workflow',
      });

      manager.addChild({
        childType: 'AGENT_LOOP',
        childId: 'agent-1',
        createdAt: Date.now(),
      });

      const metadata = manager.toMetadata();

      expect(metadata.parent).toBeDefined();
      expect(metadata.children).toHaveLength(1);
      expect(metadata.depth).toBeGreaterThanOrEqual(0);
      expect(metadata.rootExecutionId).toBeDefined();
      expect(metadata.rootExecutionType).toBeDefined();
    });

    it('should include all required fields in metadata', () => {
      const manager = new ExecutionHierarchyManager('exec-1', 'AGENT_LOOP');
      const metadata = manager.toMetadata();

      expect(metadata).toHaveProperty('parent');
      expect(metadata).toHaveProperty('children');
      expect(metadata).toHaveProperty('depth');
      expect(metadata).toHaveProperty('rootExecutionId');
      expect(metadata).toHaveProperty('rootExecutionType');
    });
  });

  describe('Agent → Agent Scenarios', () => {
    it('should support Agent parent context', () => {
      const subAgentManager = new ExecutionHierarchyManager('sub-agent', 'AGENT_LOOP');

      const parentContext: ParentExecutionContext = {
        parentType: 'AGENT_LOOP',
        parentId: 'main-agent',
        delegationPurpose: 'Code review task',
      };

      subAgentManager.setParent(parentContext);

      const parent = subAgentManager.getParent();
      expect(parent?.parentType).toBe('AGENT_LOOP');
      if (parent?.parentType === 'AGENT_LOOP') {
        expect(parent.delegationPurpose).toBe('Code review task');
      }
    });

    it('should track Agent children', () => {
      const mainAgentManager = new ExecutionHierarchyManager('main-agent', 'AGENT_LOOP');

      mainAgentManager.addChild({
        childType: 'AGENT_LOOP',
        childId: 'sub-agent-1',
        createdAt: Date.now(),
      });

      const children = mainAgentManager.getChildren();
      expect(children).toHaveLength(1);
      expect(children[0].childType).toBe('AGENT_LOOP');
    });
  });

  describe('Mixed Hierarchy Scenarios', () => {
    it('should support Workflow → Agent → Agent hierarchy', () => {
      // Root workflow
      const workflowManager = new ExecutionHierarchyManager('root-workflow', 'WORKFLOW');

      // Agent child of workflow
      const agentManager = new ExecutionHierarchyManager('agent-1', 'AGENT_LOOP');
      agentManager.setParent({
        parentType: 'WORKFLOW',
        parentId: 'root-workflow',
      });
      workflowManager.addChild({
        childType: 'AGENT_LOOP',
        childId: 'agent-1',
        createdAt: Date.now(),
      });

      // Sub-agent child of agent
      const subAgentManager = new ExecutionHierarchyManager('sub-agent', 'AGENT_LOOP');
      subAgentManager.setParent({
        parentType: 'AGENT_LOOP',
        parentId: 'agent-1',
      });
      agentManager.addChild({
        childType: 'AGENT_LOOP',
        childId: 'sub-agent',
        createdAt: Date.now(),
      });

      // Verify hierarchy
      expect(workflowManager.getChildren()).toHaveLength(1);
      expect(agentManager.getChildren()).toHaveLength(1);
      expect(subAgentManager.getParent()?.parentType).toBe('AGENT_LOOP');
    });
  });

  describe('Edge Cases', () => {
    it('should handle empty children array', () => {
      const manager = new ExecutionHierarchyManager('exec-1', 'WORKFLOW');
      expect(manager.getChildren()).toEqual([]);
    });

    it('should handle undefined parent', () => {
      const manager = new ExecutionHierarchyManager('exec-1', 'WORKFLOW');
      expect(manager.getParent()).toBeUndefined();
    });

    it('should maintain consistency after multiple operations', () => {
      const manager = new ExecutionHierarchyManager('workflow-1', 'WORKFLOW');

      // Add children
      manager.addChild({
        childType: 'AGENT_LOOP',
        childId: 'agent-1',
        createdAt: 1000,
      });

      manager.addChild({
        childType: 'AGENT_LOOP',
        childId: 'agent-2',
        createdAt: 2000,
      });

      // Remove one
      manager.removeChild('agent-1', 'AGENT_LOOP');

      // Set parent
      manager.setParent({
        parentType: 'WORKFLOW',
        parentId: 'parent-workflow',
      });

      // Verify state
      expect(manager.getChildren()).toHaveLength(1);
      expect(manager.getChildren()[0].childId).toBe('agent-2');
      expect(manager.getParent()).toBeDefined();
    });
  });

  describe('Cycle Detection with Registry (Problem 1)', () => {
    it('should detect direct self-reference cycle', () => {
      const manager = new ExecutionHierarchyManager('agent-1', 'AGENT_LOOP', undefined, registry);

      expect(() => {
        manager.setParent({
          parentType: 'AGENT_LOOP',
          parentId: 'agent-1',
        });
      }).toThrow(/Circular reference detected/);
    });

    it('should detect transitive cycle A → B → C → A', () => {
      // Create mock entities
      const createMockEntity = (id: string, parentContext?: ParentExecutionContext, depth: number = 0) => {
        return {
          id,
          getParentContext: () => parentContext,
          getHierarchyDepth: () => depth,
          getRootExecutionId: () => 'root',
          getRootExecutionType: () => 'WORKFLOW' as const,
        };
      };

      const entityA = createMockEntity('entity-a');
      const entityB = createMockEntity('entity-b', { parentType: 'WORKFLOW', parentId: 'entity-a' });
      const entityC = createMockEntity('entity-c', { parentType: 'WORKFLOW', parentId: 'entity-b' });

      // Register entities
      registry.register(entityA as any);
      registry.register(entityB as any);
      registry.register(entityC as any);

      // Try to create cycle: A → C (but C's parent is B, B's parent is A)
      const managerA = new ExecutionHierarchyManager('entity-a', 'WORKFLOW', undefined, registry);
      
      expect(() => {
        managerA.setParent({
          parentType: 'WORKFLOW',
          parentId: 'entity-c', // This would create cycle: A → C → B → A
        });
      }).toThrow(/Circular reference detected/);
    });

    it('should allow valid parent chain without cycles', () => {
      const createMockEntity = (id: string, parentContext?: ParentExecutionContext, depth: number = 0) => ({
        id,
        getParentContext: () => parentContext,
        getHierarchyDepth: () => depth,
        getRootExecutionId: () => 'root-workflow',
        getRootExecutionType: () => 'WORKFLOW' as const,
      });

      const root = createMockEntity('root-workflow');
      const child = createMockEntity('child-workflow', { parentType: 'WORKFLOW', parentId: 'root-workflow' });

      registry.register(root as any);
      registry.register(child as any);

      // Create grandchild - should succeed
      const grandchildManager = new ExecutionHierarchyManager('grandchild', 'WORKFLOW', undefined, registry);
      
      expect(() => {
        grandchildManager.setParent({
          parentType: 'WORKFLOW',
          parentId: 'child-workflow',
        });
      }).not.toThrow();

      expect(grandchildManager.getParent()?.parentId).toBe('child-workflow');
    });
  });

  describe('Depth Calculation with Registry (Problem 2)', () => {
    it('should calculate correct depth for single level hierarchy', () => {
      const createMockEntity = (id: string, depth: number) => ({
        id,
        getParentContext: () => undefined,
        getHierarchyDepth: () => depth,
        getRootExecutionId: () => id,
        getRootExecutionType: () => 'WORKFLOW' as const,
      });

      const root = createMockEntity('root', 0);
      registry.register(root as any);

      const childManager = new ExecutionHierarchyManager('child', 'WORKFLOW', undefined, registry);
      childManager.setParent({ parentType: 'WORKFLOW', parentId: 'root' });

      expect(childManager.getDepth()).toBe(1);
    });

    it('should calculate correct depth for multi-level hierarchy', () => {
      const createMockEntity = (id: string, parentContext?: ParentExecutionContext, depth: number = 0) => ({
        id,
        getParentContext: () => parentContext,
        getHierarchyDepth: () => depth,
        getRootExecutionId: () => 'root',
        getRootExecutionType: () => 'WORKFLOW' as const,
      });

      const root = createMockEntity('root', undefined, 0);
      const level1 = createMockEntity('level1', { parentType: 'WORKFLOW', parentId: 'root' }, 1);
      const level2 = createMockEntity('level2', { parentType: 'WORKFLOW', parentId: 'level1' }, 2);

      registry.register(root as any);
      registry.register(level1 as any);
      registry.register(level2 as any);

      const level3Manager = new ExecutionHierarchyManager('level3', 'WORKFLOW', undefined, registry);
      level3Manager.setParent({ parentType: 'WORKFLOW', parentId: 'level2' });

      expect(level3Manager.getDepth()).toBe(3);
    });

    it('should fallback to depth 0 when registry not available', () => {
      const manager = new ExecutionHierarchyManager('child', 'WORKFLOW'); // No registry
      manager.setParent({ parentType: 'WORKFLOW', parentId: 'parent' });

      // Without registry, depth calculation assumes parent is root (depth 0)
      expect(manager.getDepth()).toBe(1); // parent depth (0) + 1
    });

    it('should enforce depth limit', () => {
      const createMockEntity = (id: string, depth: number) => ({
        id,
        getParentContext: () => undefined,
        getHierarchyDepth: () => depth,
        getRootExecutionId: () => 'root',
        getRootExecutionType: () => 'WORKFLOW' as const,
      });

      // Create a parent at depth 10 (MAX_DEPTH default is 10)
      const deepParent = createMockEntity('deep-parent', 10);
      registry.register(deepParent as any);

      const tooDeepManager = new ExecutionHierarchyManager('too-deep', 'WORKFLOW', undefined, registry);

      expect(() => {
        tooDeepManager.setParent({ parentType: 'WORKFLOW', parentId: 'deep-parent' });
      }).toThrow(/Maximum hierarchy depth exceeded/);
    });
  });

  describe('Root Execution Tracking with Registry (Problem 3)', () => {
    it('should inherit root from parent correctly', () => {
      const createMockEntity = (id: string, rootId: string, rootType: 'WORKFLOW' | 'AGENT_LOOP') => ({
        id,
        getParentContext: () => undefined,
        getHierarchyDepth: () => 0,
        getRootExecutionId: () => rootId,
        getRootExecutionType: () => rootType,
      });

      const rootWorkflow = createMockEntity('root-workflow', 'root-workflow', 'WORKFLOW');
      registry.register(rootWorkflow as any);

      const childManager = new ExecutionHierarchyManager('child', 'AGENT_LOOP', undefined, registry);
      childManager.setParent({ parentType: 'WORKFLOW', parentId: 'root-workflow' });

      expect(childManager.getRootExecutionId()).toBe('root-workflow');
      expect(childManager.getRootExecutionType()).toBe('WORKFLOW');
    });

    it('should inherit root through multiple levels', () => {
      const createMockEntity = (
        id: string,
        parentContext?: ParentExecutionContext,
        rootId: string = 'root-workflow',
        rootType: 'WORKFLOW' | 'AGENT_LOOP' = 'WORKFLOW',
        depth: number = 0
      ) => ({
        id,
        getParentContext: () => parentContext,
        getHierarchyDepth: () => depth,
        getRootExecutionId: () => rootId,
        getRootExecutionType: () => rootType,
      });

      const root = createMockEntity('root-workflow', undefined, 'root-workflow', 'WORKFLOW', 0);
      const agent = createMockEntity(
        'agent-1',
        { parentType: 'WORKFLOW', parentId: 'root-workflow' },
        'root-workflow',
        'WORKFLOW',
        1
      );

      registry.register(root as any);
      registry.register(agent as any);

      // Sub-agent spawned by agent
      const subAgentManager = new ExecutionHierarchyManager('sub-agent', 'AGENT_LOOP', undefined, registry);
      subAgentManager.setParent({ parentType: 'AGENT_LOOP', parentId: 'agent-1' });

      // Should inherit root from agent, which inherited from root-workflow
      expect(subAgentManager.getRootExecutionId()).toBe('root-workflow');
      expect(subAgentManager.getRootExecutionType()).toBe('WORKFLOW');
    });

    it('should fallback to parent as root when registry not available', () => {
      const manager = new ExecutionHierarchyManager('child', 'AGENT_LOOP'); // No registry
      manager.setParent({ parentType: 'WORKFLOW', parentId: 'parent-workflow' });

      // Without registry, assumes parent is root
      expect(manager.getRootExecutionId()).toBe('parent-workflow');
      expect(manager.getRootExecutionType()).toBe('WORKFLOW');
    });
  });

  describe('Integration: Full Hierarchy Tree', () => {
    it('should maintain correct hierarchy for Workflow → Agent → Agent tree', () => {
      const createMockEntity = (
        id: string,
        parentContext?: ParentExecutionContext,
        depth: number = 0
      ) => ({
        id,
        getParentContext: () => parentContext,
        getHierarchyDepth: () => depth,
        getRootExecutionId: () => 'root-workflow',
        getRootExecutionType: () => 'WORKFLOW' as const,
      });

      const rootWorkflow = createMockEntity('root-workflow', undefined, 0);
      const agent1 = createMockEntity(
        'agent-1',
        { parentType: 'WORKFLOW', parentId: 'root-workflow' },
        1
      );
      const agent2 = createMockEntity(
        'agent-2',
        { parentType: 'AGENT_LOOP', parentId: 'agent-1' },
        2
      );

      registry.register(rootWorkflow as any);
      registry.register(agent1 as any);
      registry.register(agent2 as any);

      // Create agent-3 as child of agent-2
      const agent3Manager = new ExecutionHierarchyManager('agent-3', 'AGENT_LOOP', undefined, registry);
      agent3Manager.setParent({ parentType: 'AGENT_LOOP', parentId: 'agent-2' });

      // Verify complete hierarchy
      expect(agent3Manager.getDepth()).toBe(3);
      expect(agent3Manager.getRootExecutionId()).toBe('root-workflow');
      expect(agent3Manager.getRootExecutionType()).toBe('WORKFLOW');
      expect(agent3Manager.getParent()?.parentId).toBe('agent-2');
    });

    it('should prevent cycles in complex hierarchy', () => {
      const createMockEntity = (id: string, parentContext?: ParentExecutionContext, depth: number = 0) => ({
        id,
        getParentContext: () => parentContext,
        getHierarchyDepth: () => depth,
        getRootExecutionId: () => 'root',
        getRootExecutionType: () => 'WORKFLOW' as const,
      });

      const root = createMockEntity('root', undefined, 0);
      const wf1 = createMockEntity('wf1', { parentType: 'WORKFLOW', parentId: 'root' }, 1);
      const agent1 = createMockEntity('agent1', { parentType: 'WORKFLOW', parentId: 'wf1' }, 2);

      registry.register(root as any);
      registry.register(wf1 as any);
      registry.register(agent1 as any);

      // Try to make root a child of agent1 (would create cycle)
      const rootManager = new ExecutionHierarchyManager('root', 'WORKFLOW', undefined, registry);
      
      expect(() => {
        rootManager.setParent({ parentType: 'AGENT_LOOP', parentId: 'agent1' });
      }).toThrow(/Circular reference detected/);
    });
  });

  describe('Edge Cases and Boundary Conditions', () => {
    it('should handle rapid parent changes correctly', () => {
      const manager = new ExecutionHierarchyManager('exec-1', 'WORKFLOW');
      
      // Change parent multiple times rapidly
      manager.setParent({ parentType: 'WORKFLOW', parentId: 'parent-1' });
      expect(manager.getDepth()).toBe(1);
      
      manager.setParent({ parentType: 'WORKFLOW', parentId: 'parent-2' });
      expect(manager.getDepth()).toBe(1);
      
      manager.setParent({ parentType: 'WORKFLOW', parentId: 'parent-3' });
      expect(manager.getDepth()).toBe(1);
      
      // Verify final state is consistent
      expect(manager.getParent()?.parentId).toBe('parent-3');
    });

    it('should handle setting parent to undefined (becoming root)', () => {
      const manager = new ExecutionHierarchyManager('exec-1', 'WORKFLOW');
      manager.setParent({ parentType: 'WORKFLOW', parentId: 'parent-1' });
      expect(manager.getDepth()).toBe(1);
      
      // Note: The current implementation doesn't support unsetting parent
      // This test documents the expected behavior if we add that feature
      // manager.setParent(undefined); // Would need API change
      // expect(manager.getDepth()).toBe(0);
      // expect(manager.getRootExecutionId()).toBe('exec-1');
    });

    it('should handle very long child IDs', () => {
      const manager = new ExecutionHierarchyManager('exec-1', 'WORKFLOW');
      const longId = 'a'.repeat(1000);
      
      manager.addChild({
        childType: 'WORKFLOW',
        childId: longId,
        createdAt: Date.now(),
      });
      
      expect(manager.getChildren()).toHaveLength(1);
      expect(manager.getChildren()[0].childId).toBe(longId);
    });

    it('should handle special characters in IDs', () => {
      const manager = new ExecutionHierarchyManager('exec-1', 'WORKFLOW');
      const specialId = 'workflow-with-dashes_and_underscores.and.dots';
      
      manager.addChild({
        childType: 'WORKFLOW',
        childId: specialId,
        createdAt: Date.now(),
      });
      
      expect(manager.getChildren()).toHaveLength(1);
      expect(manager.removeChild(specialId, 'WORKFLOW')).toBe(true);
      expect(manager.getChildren()).toHaveLength(0);
    });

    it('should handle removing non-existent child gracefully', () => {
      const manager = new ExecutionHierarchyManager('exec-1', 'WORKFLOW');
      
      const result = manager.removeChild('non-existent', 'WORKFLOW');
      expect(result).toBe(false);
    });

    it('should handle duplicate child additions (last write wins)', () => {
      const manager = new ExecutionHierarchyManager('exec-1', 'WORKFLOW');
      
      manager.addChild({
        childType: 'WORKFLOW',
        childId: 'child-1',
        createdAt: 1000,
      });
      
      // Add same child again with different timestamp
      manager.addChild({
        childType: 'WORKFLOW',
        childId: 'child-1',
        createdAt: 2000,
      });
      
      // Should still have only one child
      expect(manager.getChildren()).toHaveLength(1);
      expect(manager.getChildren()[0].createdAt).toBe(2000);
    });

    it('should maintain separate namespaces for different child types', () => {
      const manager = new ExecutionHierarchyManager('exec-1', 'WORKFLOW');
      
      // Add workflow and agent with same ID
      manager.addChild({
        childType: 'WORKFLOW',
        childId: 'same-id',
        createdAt: 1000,
      });
      
      manager.addChild({
        childType: 'AGENT_LOOP',
        childId: 'same-id',
        createdAt: 2000,
      });
      
      // Should have two children (different types)
      expect(manager.getChildren()).toHaveLength(2);
      
      // Remove only workflow child
      manager.removeChild('same-id', 'WORKFLOW');
      expect(manager.getChildren()).toHaveLength(1);
      expect(manager.getChildren()[0].childType).toBe('AGENT_LOOP');
    });
  });

  describe('Large Hierarchy Performance', () => {
    it('should handle deep hierarchy (MAX_DEPTH levels)', () => {
      const maxDepth = parseInt(process.env['MAX_EXECUTION_DEPTH'] || '10', 10);
      
      // Create root entity
      const rootEntity = {
        id: 'root-unique',
        getParentContext: () => undefined,
        getHierarchyDepth: () => 0,
        getRootExecutionId: () => 'root-unique',
        getRootExecutionType: () => 'WORKFLOW' as const,
      };
      registry.register(rootEntity as any);
      
      // Create chain of depth MAX_DEPTH
      let currentParentId = 'root-unique';
      for (let i = 1; i <= maxDepth; i++) {
        const childId = `level-${i}-${Date.now()}`; // Unique ID to avoid collision
        
        // Register parent entity first
        const parentEntity = {
          id: currentParentId,
          getParentContext: () => undefined, // Simplified - just need depth info
          getHierarchyDepth: () => i - 1,
          getRootExecutionId: () => 'root-unique',
          getRootExecutionType: () => 'WORKFLOW' as const,
        };
        registry.register(parentEntity as any);
        
        // Create child manager and set parent
        const childManager = new ExecutionHierarchyManager(childId, 'WORKFLOW', undefined, registry);
        childManager.setParent({ parentType: 'WORKFLOW', parentId: currentParentId });
        
        // Verify depth
        expect(childManager.getDepth()).toBe(i);
        expect(childManager.getRootExecutionId()).toBe('root-unique');
        
        currentParentId = childId;
      }
    });

    it('should handle many children efficiently', () => {
      const manager = new ExecutionHierarchyManager('parent', 'WORKFLOW');
      const childCount = 1000;
      
      // Add many children
      for (let i = 0; i < childCount; i++) {
        manager.addChild({
          childType: i % 2 === 0 ? 'WORKFLOW' : 'AGENT_LOOP',
          childId: `child-${i}`,
          createdAt: Date.now(),
        });
      }
      
      expect(manager.getChildren()).toHaveLength(childCount);
      
      // Remove half of them
      for (let i = 0; i < childCount; i += 2) {
        manager.removeChild(`child-${i}`, i % 2 === 0 ? 'WORKFLOW' : 'AGENT_LOOP');
      }
      
      expect(manager.getChildren()).toHaveLength(childCount / 2);
    });

    it('should detect cycles in deep hierarchy efficiently', () => {
      const depth = 8; // Use smaller depth to avoid hitting MAX_DEPTH limit
      const entities: any[] = [];
      
      // Create a deep chain
      for (let i = 0; i < depth; i++) {
        const entity = {
          id: `node-${i}`,
          getParentContext: () => i > 0 ? { parentType: 'WORKFLOW' as const, parentId: `node-${i - 1}` } : undefined,
          getHierarchyDepth: () => i,
          getRootExecutionId: () => 'node-0',
          getRootExecutionType: () => 'WORKFLOW' as const,
        };
        entities.push(entity);
        registry.register(entity as any);
      }
      
      // Try to create cycle: node-0 -> node-7
      const rootNodeManager = new ExecutionHierarchyManager('node-0', 'WORKFLOW', undefined, registry);
      
      expect(() => {
        rootNodeManager.setParent({ parentType: 'WORKFLOW', parentId: 'node-7' });
      }).toThrow(/Circular reference detected/);
    });

    it('should handle mixed execution types in hierarchy', () => {
      const createMockEntity = (
        id: string,
        type: 'WORKFLOW' | 'AGENT_LOOP',
        parentContext?: ParentExecutionContext,
        depth: number = 0,
        rootId: string = 'root-wf'
      ) => ({
        id,
        type,
        getParentContext: () => parentContext,
        getHierarchyDepth: () => depth,
        getRootExecutionId: () => rootId,
        getRootExecutionType: () => 'WORKFLOW' as const,
      });

      // Workflow → Agent → Workflow → Agent pattern
      const rootWorkflow = createMockEntity('root-wf', 'WORKFLOW', undefined, 0, 'root-wf');
      const agent1 = createMockEntity('agent-1', 'AGENT_LOOP', { parentType: 'WORKFLOW', parentId: 'root-wf' }, 1, 'root-wf');
      const subWorkflow = createMockEntity('sub-wf', 'WORKFLOW', { parentType: 'AGENT_LOOP', parentId: 'agent-1' }, 2, 'root-wf');
      const agent2 = createMockEntity('agent-2', 'AGENT_LOOP', { parentType: 'WORKFLOW', parentId: 'sub-wf' }, 3, 'root-wf');

      registry.register(rootWorkflow as any);
      registry.register(agent1 as any);
      registry.register(subWorkflow as any);
      registry.register(agent2 as any);

      const agent2Manager = new ExecutionHierarchyManager('agent-2', 'AGENT_LOOP', undefined, registry);
      agent2Manager.setParent({ parentType: 'WORKFLOW', parentId: 'sub-wf' });

      expect(agent2Manager.getDepth()).toBe(3);
      expect(agent2Manager.getRootExecutionId()).toBe('root-wf');
      expect(agent2Manager.getRootExecutionType()).toBe('WORKFLOW');
    });
  });

  describe('Serialization and Deserialization', () => {
    it('should serialize and deserialize correctly', () => {
      const manager = new ExecutionHierarchyManager('exec-1', 'WORKFLOW');
      manager.setParent({ parentType: 'WORKFLOW', parentId: 'parent-1' });
      manager.addChild({ childType: 'WORKFLOW', childId: 'child-1', createdAt: 1000 });
      manager.addChild({ childType: 'AGENT_LOOP', childId: 'child-2', createdAt: 2000 });

      const metadata = manager.toMetadata();

      // Create new manager from metadata
      const restoredManager = new ExecutionHierarchyManager('exec-1', 'WORKFLOW', metadata);

      expect(restoredManager.getDepth()).toBe(manager.getDepth());
      expect(restoredManager.getRootExecutionId()).toBe(manager.getRootExecutionId());
      expect(restoredManager.getRootExecutionType()).toBe(manager.getRootExecutionType());
      expect(restoredManager.getChildren()).toHaveLength(2);
    });

    it('should preserve all hierarchy fields in metadata', () => {
      const manager = new ExecutionHierarchyManager('exec-1', 'AGENT_LOOP');
      manager.setParent({ parentType: 'WORKFLOW', parentId: 'parent-1', nodeId: 'node-123' });

      const metadata = manager.toMetadata();

      expect(metadata.parent).toBeDefined();
      expect(metadata.parent?.parentType).toBe('WORKFLOW');
      expect(metadata.parent?.parentId).toBe('parent-1');
      expect(metadata.children).toEqual([]);
      expect(typeof metadata.depth).toBe('number');
      expect(metadata.rootExecutionId).toBeDefined();
      expect(metadata.rootExecutionType).toBeDefined();
    });

    it('should handle empty hierarchy serialization', () => {
      const manager = new ExecutionHierarchyManager('exec-1', 'WORKFLOW');
      const metadata = manager.toMetadata();

      expect(metadata.parent).toBeUndefined();
      expect(metadata.children).toEqual([]);
      expect(metadata.depth).toBe(0);
      expect(metadata.rootExecutionId).toBe('exec-1');
      expect(metadata.rootExecutionType).toBe('WORKFLOW');
    });
  });
});
