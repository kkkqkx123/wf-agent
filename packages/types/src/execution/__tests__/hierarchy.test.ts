/**
 * Unit Tests for Execution Hierarchy Types
 * 
 * This test file validates the type definitions and structure of the execution hierarchy system.
 * Since these are TypeScript type definitions, we focus on:
 * - Type compatibility and assignability
 * - Union type discrimination
 * - Interface structure validation
 * - Type safety guarantees
 */

import { describe, it, expect } from 'vitest';
import type {
  ExecutionType,
  ExecutionIdentity,
  ParentExecutionContext,
  ChildExecutionReference,
  ExecutionHierarchyMetadata,
} from '../hierarchy.js';

describe('Execution Hierarchy Types', () => {
  describe('ExecutionType', () => {
    it('should accept WORKFLOW as valid execution type', () => {
      const workflowType: ExecutionType = 'WORKFLOW';
      expect(workflowType).toBe('WORKFLOW');
    });

    it('should accept AGENT_LOOP as valid execution type', () => {
      const agentType: ExecutionType = 'AGENT_LOOP';
      expect(agentType).toBe('AGENT_LOOP');
    });

    it('should not accept invalid execution types (compile-time check)', () => {
      // This test verifies that only 'WORKFLOW' and 'AGENT_LOOP' are valid
      // TypeScript will catch invalid types at compile time
      const validTypes: ExecutionType[] = ['WORKFLOW', 'AGENT_LOOP'];
      expect(validTypes).toHaveLength(2);
    });
  });

  describe('ExecutionIdentity', () => {
    it('should create valid workflow execution identity', () => {
      const identity: ExecutionIdentity = {
        type: 'WORKFLOW',
        id: 'workflow-123',
      };

      expect(identity.type).toBe('WORKFLOW');
      expect(identity.id).toBe('workflow-123');
    });

    it('should create valid agent loop execution identity', () => {
      const identity: ExecutionIdentity = {
        type: 'AGENT_LOOP',
        id: 'agent-456',
      };

      expect(identity.type).toBe('AGENT_LOOP');
      expect(identity.id).toBe('agent-456');
    });

    it('should require both type and id fields', () => {
      // TypeScript enforces required fields at compile time
      const identity: ExecutionIdentity = {
        type: 'WORKFLOW',
        id: 'test-id',
      };

      expect(identity).toHaveProperty('type');
      expect(identity).toHaveProperty('id');
    });
  });

  describe('ParentExecutionContext', () => {
    describe('Workflow Parent Context', () => {
      it('should create valid workflow parent context without nodeId', () => {
        const parent: ParentExecutionContext = {
          parentType: 'WORKFLOW',
          parentId: 'workflow-parent-123',
        };

        expect(parent.parentType).toBe('WORKFLOW');
        expect(parent.parentId).toBe('workflow-parent-123');
        expect(parent.nodeId).toBeUndefined();
      });

      it('should create valid workflow parent context with nodeId', () => {
        const parent: ParentExecutionContext = {
          parentType: 'WORKFLOW',
          parentId: 'workflow-parent-456',
          nodeId: 'agent-node-789',
        };

        expect(parent.parentType).toBe('WORKFLOW');
        expect(parent.parentId).toBe('workflow-parent-456');
        expect(parent.nodeId).toBe('agent-node-789');
      });

      it('should use parentType as discriminant for workflow', () => {
        const parent: ParentExecutionContext = {
          parentType: 'WORKFLOW',
          parentId: 'workflow-123',
        };

        // Type narrowing based on discriminant
        if (parent.parentType === 'WORKFLOW') {
          expect(parent.parentId).toBeDefined();
          // nodeId is optional for WORKFLOW parent
          expect(parent.nodeId).toBeUndefined();
        }
      });
    });

    describe('Agent Loop Parent Context', () => {
      it('should create valid agent loop parent context without delegationPurpose', () => {
        const parent: ParentExecutionContext = {
          parentType: 'AGENT_LOOP',
          parentId: 'agent-parent-123',
        };

        expect(parent.parentType).toBe('AGENT_LOOP');
        expect(parent.parentId).toBe('agent-parent-123');
        expect(parent.delegationPurpose).toBeUndefined();
      });

      it('should create valid agent loop parent context with delegationPurpose', () => {
        const parent: ParentExecutionContext = {
          parentType: 'AGENT_LOOP',
          parentId: 'agent-parent-456',
          delegationPurpose: 'Code review task delegation',
        };

        expect(parent.parentType).toBe('AGENT_LOOP');
        expect(parent.parentId).toBe('agent-parent-456');
        expect(parent.delegationPurpose).toBe('Code review task delegation');
      });

      it('should use parentType as discriminant for agent loop', () => {
        const parent: ParentExecutionContext = {
          parentType: 'AGENT_LOOP',
          parentId: 'agent-123',
        };

        // Type narrowing based on discriminant
        if (parent.parentType === 'AGENT_LOOP') {
          expect(parent.parentId).toBeDefined();
          // delegationPurpose is optional for AGENT_LOOP parent
          expect(parent.delegationPurpose).toBeUndefined();
        }
      });
    });

    describe('Union Type Discrimination', () => {
      it('should correctly discriminate between workflow and agent parents', () => {
        const workflowParent: ParentExecutionContext = {
          parentType: 'WORKFLOW',
          parentId: 'workflow-123',
        };

        const agentParent: ParentExecutionContext = {
          parentType: 'AGENT_LOOP',
          parentId: 'agent-456',
        };

        // Verify discrimination works
        if (workflowParent.parentType === 'WORKFLOW') {
          expect(workflowParent.parentId).toBe('workflow-123');
        }

        if (agentParent.parentType === 'AGENT_LOOP') {
          expect(agentParent.parentId).toBe('agent-456');
        }
      });
    });
  });

  describe('ChildExecutionReference', () => {
    describe('Workflow Child Reference', () => {
      it('should create valid workflow child reference', () => {
        const now = Date.now();
        const child: ChildExecutionReference = {
          childType: 'WORKFLOW',
          childId: 'sub-workflow-789',
          createdAt: now,
        };

        expect(child.childType).toBe('WORKFLOW');
        expect(child.childId).toBe('sub-workflow-789');
        expect(child.createdAt).toBe(now);
      });

      it('should require all fields for workflow child', () => {
        const now = Date.now();
        const child: ChildExecutionReference = {
          childType: 'WORKFLOW',
          childId: 'workflow-child',
          createdAt: now,
        };

        expect(child).toHaveProperty('childType');
        expect(child).toHaveProperty('childId');
        expect(child).toHaveProperty('createdAt');
      });
    });

    describe('Agent Loop Child Reference', () => {
      it('should create valid agent loop child reference', () => {
        const now = Date.now();
        const child: ChildExecutionReference = {
          childType: 'AGENT_LOOP',
          childId: 'sub-agent-012',
          createdAt: now,
        };

        expect(child.childType).toBe('AGENT_LOOP');
        expect(child.childId).toBe('sub-agent-012');
        expect(child.createdAt).toBe(now);
      });

      it('should require all fields for agent child', () => {
        const now = Date.now();
        const child: ChildExecutionReference = {
          childType: 'AGENT_LOOP',
          childId: 'agent-child',
          createdAt: now,
        };

        expect(child).toHaveProperty('childType');
        expect(child).toHaveProperty('childId');
        expect(child).toHaveProperty('createdAt');
      });
    });

    describe('Timestamp Validation', () => {
      it('should accept valid timestamp values', () => {
        const timestamps = [0, 1234567890, Date.now(), Number.MAX_SAFE_INTEGER];

        timestamps.forEach((ts) => {
          const child: ChildExecutionReference = {
            childType: 'WORKFLOW',
            childId: 'test-child',
            createdAt: ts,
          };

          expect(child.createdAt).toBe(ts);
        });
      });
    });
  });

  describe('ExecutionHierarchyMetadata', () => {
    it('should create valid metadata for root execution (no parent)', () => {
      const metadata: ExecutionHierarchyMetadata = {
        parent: undefined,
        children: [],
        depth: 0,
        rootExecutionId: 'root-workflow',
        rootExecutionType: 'WORKFLOW',
      };

      expect(metadata.parent).toBeUndefined();
      expect(metadata.children).toEqual([]);
      expect(metadata.depth).toBe(0);
      expect(metadata.rootExecutionId).toBe('root-workflow');
      expect(metadata.rootExecutionType).toBe('WORKFLOW');
    });

    it('should create valid metadata with workflow parent', () => {
      const metadata: ExecutionHierarchyMetadata = {
        parent: {
          parentType: 'WORKFLOW',
          parentId: 'parent-workflow-123',
          nodeId: 'agent-node-1',
        },
        children: [],
        depth: 1,
        rootExecutionId: 'root-workflow',
        rootExecutionType: 'WORKFLOW',
      };

      expect(metadata.parent).toBeDefined();
      expect(metadata.parent?.parentType).toBe('WORKFLOW');
      expect(metadata.parent?.parentId).toBe('parent-workflow-123');
      expect(metadata.depth).toBe(1);
    });

    it('should create valid metadata with agent loop parent', () => {
      const metadata: ExecutionHierarchyMetadata = {
        parent: {
          parentType: 'AGENT_LOOP',
          parentId: 'parent-agent-456',
          delegationPurpose: 'Subtask delegation',
        },
        children: [],
        depth: 2,
        rootExecutionId: 'root-workflow',
        rootExecutionType: 'WORKFLOW',
      };

      expect(metadata.parent).toBeDefined();
      expect(metadata.parent?.parentType).toBe('AGENT_LOOP');
      expect(metadata.parent?.parentId).toBe('parent-agent-456');
      expect(metadata.depth).toBe(2);
    });

    it('should support multiple children of different types', () => {
      const now = Date.now();
      const metadata: ExecutionHierarchyMetadata = {
        parent: undefined,
        children: [
          {
            childType: 'AGENT_LOOP',
            childId: 'agent-1',
            createdAt: now,
          },
          {
            childType: 'WORKFLOW',
            childId: 'workflow-1',
            createdAt: now + 100,
          },
          {
            childType: 'AGENT_LOOP',
            childId: 'agent-2',
            createdAt: now + 200,
          },
        ],
        depth: 0,
        rootExecutionId: 'root-workflow',
        rootExecutionType: 'WORKFLOW',
      };

      expect(metadata.children).toHaveLength(3);
      expect(metadata.children[0]?.childType).toBe('AGENT_LOOP');
      expect(metadata.children[1]?.childType).toBe('WORKFLOW');
      expect(metadata.children[2]?.childType).toBe('AGENT_LOOP');
    });

    it('should track depth correctly in nested hierarchy', () => {
      // Root level
      const rootMetadata: ExecutionHierarchyMetadata = {
        parent: undefined,
        children: [],
        depth: 0,
        rootExecutionId: 'root-workflow',
        rootExecutionType: 'WORKFLOW',
      };

      // Level 1
      const level1Metadata: ExecutionHierarchyMetadata = {
        parent: {
          parentType: 'WORKFLOW',
          parentId: 'root-workflow',
        },
        children: [],
        depth: 1,
        rootExecutionId: 'root-workflow',
        rootExecutionType: 'WORKFLOW',
      };

      // Level 2
      const level2Metadata: ExecutionHierarchyMetadata = {
        parent: {
          parentType: 'AGENT_LOOP',
          parentId: 'agent-1',
        },
        children: [],
        depth: 2,
        rootExecutionId: 'root-workflow',
        rootExecutionType: 'WORKFLOW',
      };

      expect(rootMetadata.depth).toBe(0);
      expect(level1Metadata.depth).toBe(1);
      expect(level2Metadata.depth).toBe(2);
      expect(level2Metadata.rootExecutionId).toBe('root-workflow');
    });

    it('should maintain root execution info across hierarchy', () => {
      const metadata: ExecutionHierarchyMetadata = {
        parent: {
          parentType: 'AGENT_LOOP',
          parentId: 'agent-123',
        },
        children: [],
        depth: 3,
        rootExecutionId: 'original-root-workflow',
        rootExecutionType: 'WORKFLOW',
      };

      expect(metadata.rootExecutionId).toBe('original-root-workflow');
      expect(metadata.rootExecutionType).toBe('WORKFLOW');
    });

    it('should support empty children array', () => {
      const metadata: ExecutionHierarchyMetadata = {
        parent: undefined,
        children: [],
        depth: 0,
        rootExecutionId: 'root',
        rootExecutionType: 'AGENT_LOOP',
      };

      expect(metadata.children).toEqual([]);
      expect(metadata.children).toHaveLength(0);
    });

    it('should allow adding children to existing metadata', () => {
      const now = Date.now();
      const metadata: ExecutionHierarchyMetadata = {
        parent: undefined,
        children: [],
        depth: 0,
        rootExecutionId: 'root-workflow',
        rootExecutionType: 'WORKFLOW',
      };

      // Simulate adding children
      metadata.children.push({
        childType: 'AGENT_LOOP',
        childId: 'agent-1',
        createdAt: now,
      });

      metadata.children.push({
        childType: 'WORKFLOW',
        childId: 'workflow-1',
        createdAt: now + 100,
      });

      expect(metadata.children).toHaveLength(2);
    });
  });

  describe('Type Safety and Compatibility', () => {
    it('should enforce type safety for execution types', () => {
      // These assignments should work without type errors
      const workflowType: ExecutionType = 'WORKFLOW';
      const agentType: ExecutionType = 'AGENT_LOOP';

      expect(['WORKFLOW', 'AGENT_LOOP']).toContain(workflowType);
      expect(['WORKFLOW', 'AGENT_LOOP']).toContain(agentType);
    });

    it('should support type narrowing with discriminated unions', () => {
      const parent: ParentExecutionContext = {
        parentType: 'WORKFLOW',
        parentId: 'workflow-123',
        nodeId: 'node-456',
      };

      // Type narrowing allows access to specific fields
      if (parent.parentType === 'WORKFLOW') {
        // TypeScript knows nodeId exists on WORKFLOW parent
        expect(parent.nodeId).toBe('node-456');
      }
    });

    it('should maintain type information through assignment', () => {
      const child: ChildExecutionReference = {
        childType: 'AGENT_LOOP',
        childId: 'agent-789',
        createdAt: Date.now(),
      };

      // Type information is preserved
      expect(child.childType).toBe('AGENT_LOOP');
      expect(typeof child.childId).toBe('string');
      expect(typeof child.createdAt).toBe('number');
    });
  });

  describe('Edge Cases', () => {
    it('should handle deeply nested hierarchies', () => {
      const deepMetadata: ExecutionHierarchyMetadata = {
        parent: {
          parentType: 'AGENT_LOOP',
          parentId: 'level-4-agent',
        },
        children: [],
        depth: 10,
        rootExecutionId: 'root-workflow',
        rootExecutionType: 'WORKFLOW',
      };

      expect(deepMetadata.depth).toBeGreaterThan(5);
      expect(deepMetadata.rootExecutionId).toBeDefined();
    });

    it('should handle large numbers of children', () => {
      const now = Date.now();
      const children: ChildExecutionReference[] = Array.from({ length: 100 }, (_, i) => ({
        childType: i % 2 === 0 ? 'WORKFLOW' : 'AGENT_LOOP',
        childId: `child-${i}`,
        createdAt: now + i,
      }));

      const metadata: ExecutionHierarchyMetadata = {
        parent: undefined,
        children,
        depth: 0,
        rootExecutionId: 'root',
        rootExecutionType: 'WORKFLOW',
      };

      expect(metadata.children).toHaveLength(100);
    });

    it('should handle special characters in IDs', () => {
      const metadata: ExecutionHierarchyMetadata = {
        parent: {
          parentType: 'WORKFLOW',
          parentId: 'workflow-with-dashes_and_underscores123',
        },
        children: [
          {
            childType: 'AGENT_LOOP',
            childId: 'agent:with:colons',
            createdAt: Date.now(),
          },
        ],
        depth: 1,
        rootExecutionId: 'root.workflow.dotted',
        rootExecutionType: 'WORKFLOW',
      };

      expect(metadata.parent?.parentId).toContain('-');
      expect(metadata.children[0]?.childId).toContain(':');
      expect(metadata.rootExecutionId).toContain('.');
    });

    it('should handle zero depth for root executions', () => {
      const rootMetadata: ExecutionHierarchyMetadata = {
        parent: undefined,
        children: [],
        depth: 0,
        rootExecutionId: 'root',
        rootExecutionType: 'WORKFLOW',
      };

      expect(rootMetadata.depth).toBe(0);
      expect(rootMetadata.parent).toBeUndefined();
    });
  });
});
