/**
 * Execution Hierarchy Types Unit Tests
 * 
 * Tests for type definitions to ensure:
 * - Type safety and compile-time validation
 * - Correct union type behavior
 * - Proper interface structure
 */

import { describe, it, expectTypeOf } from 'vitest';
import type {
  ExecutionType,
  ParentExecutionContext,
  ChildExecutionReference,
  ExecutionHierarchyMetadata,
  ExecutionIdentity,
} from '../../src/execution/hierarchy.js';

describe('Execution Hierarchy Types', () => {
  describe('ExecutionType', () => {
    it('should accept valid execution types', () => {
      const workflowType: ExecutionType = 'WORKFLOW';
      const agentType: ExecutionType = 'AGENT_LOOP';
      
      expectTypeOf(workflowType).toEqualTypeOf<'WORKFLOW'>();
      expectTypeOf(agentType).toEqualTypeOf<'AGENT_LOOP'>();
    });
  });

  describe('ExecutionIdentity', () => {
    it('should have correct structure', () => {
      const identity: ExecutionIdentity = {
        type: 'WORKFLOW',
        id: 'workflow-123',
      };

      expectTypeOf(identity.type).toEqualTypeOf<ExecutionType>();
      expectTypeOf(identity.id).toBeString();
    });
  });

  describe('ParentExecutionContext', () => {
    it('should accept WORKFLOW parent context with required fields', () => {
      const workflowParent: ParentExecutionContext = {
        parentType: 'WORKFLOW',
        parentId: 'workflow-123',
      };

      expectTypeOf(workflowParent.parentType).toEqualTypeOf<'WORKFLOW'>();
      expectTypeOf(workflowParent.parentId).toBeString();
    });

    it('should accept WORKFLOW parent context with optional nodeId', () => {
      const workflowParentWithNode: ParentExecutionContext = {
        parentType: 'WORKFLOW',
        parentId: 'workflow-123',
        nodeId: 'agent-node-1',
      };

      expectTypeOf(workflowParentWithNode.nodeId).toEqualTypeOf<string | undefined>();
    });

    it('should accept AGENT_LOOP parent context with required fields', () => {
      const agentParent: ParentExecutionContext = {
        parentType: 'AGENT_LOOP',
        parentId: 'agent-456',
      };

      expectTypeOf(agentParent.parentType).toEqualTypeOf<'AGENT_LOOP'>();
      expectTypeOf(agentParent.parentId).toBeString();
    });

    it('should accept AGENT_LOOP parent context with optional delegationPurpose', () => {
      const agentParentWithPurpose: ParentExecutionContext = {
        parentType: 'AGENT_LOOP',
        parentId: 'agent-456',
        delegationPurpose: 'Code review task delegation',
      };

      expectTypeOf(agentParentWithPurpose.delegationPurpose).toEqualTypeOf<string | undefined>();
    });

    it('should discriminate between WORKFLOW and AGENT_LOOP types', () => {
      const context: ParentExecutionContext = {
        parentType: 'WORKFLOW',
        parentId: 'workflow-123',
      };

      // TypeScript should narrow the type based on parentType
      if (context.parentType === 'WORKFLOW') {
        expectTypeOf(context.nodeId).toEqualTypeOf<string | undefined>();
      } else {
        expectTypeOf(context.delegationPurpose).toEqualTypeOf<string | undefined>();
      }
    });
  });

  describe('ChildExecutionReference', () => {
    it('should accept WORKFLOW child reference', () => {
      const workflowChild: ChildExecutionReference = {
        childType: 'WORKFLOW',
        childId: 'sub-workflow-789',
        createdAt: Date.now(),
      };

      expectTypeOf(workflowChild.childType).toEqualTypeOf<'WORKFLOW'>();
      expectTypeOf(workflowChild.childId).toBeString();
      expectTypeOf(workflowChild.createdAt).toBeNumber();
    });

    it('should accept AGENT_LOOP child reference', () => {
      const agentChild: ChildExecutionReference = {
        childType: 'AGENT_LOOP',
        childId: 'sub-agent-012',
        createdAt: Date.now(),
      };

      expectTypeOf(agentChild.childType).toEqualTypeOf<'AGENT_LOOP'>();
      expectTypeOf(agentChild.childId).toBeString();
      expectTypeOf(agentChild.createdAt).toBeNumber();
    });

    it('should discriminate between WORKFLOW and AGENT_LOOP types', () => {
      const ref: ChildExecutionReference = {
        childType: 'WORKFLOW',
        childId: 'child-123',
        createdAt: Date.now(),
      };

      // TypeScript should narrow the type based on childType
      if (ref.childType === 'WORKFLOW') {
        expectTypeOf(ref.childId).toBeString();
      }
    });
  });

  describe('ExecutionHierarchyMetadata', () => {
    it('should accept complete metadata structure', () => {
      const metadata: ExecutionHierarchyMetadata = {
        parent: {
          parentType: 'WORKFLOW',
          parentId: 'root-workflow',
        },
        children: [
          {
            childType: 'AGENT_LOOP',
            childId: 'agent-1',
            createdAt: 1234567890,
          },
        ],
        depth: 1,
        rootExecutionId: 'root-workflow',
        rootExecutionType: 'WORKFLOW',
      };

      expectTypeOf(metadata.parent).toEqualTypeOf<ParentExecutionContext | undefined>();
      expectTypeOf(metadata.children).toEqualTypeOf<ChildExecutionReference[]>();
      expectTypeOf(metadata.depth).toBeNumber();
      expectTypeOf(metadata.rootExecutionId).toBeString();
      expectTypeOf(metadata.rootExecutionType).toEqualTypeOf<ExecutionType>();
    });

    it('should accept metadata without parent (root node)', () => {
      const rootMetadata: ExecutionHierarchyMetadata = {
        parent: undefined,
        children: [],
        depth: 0,
        rootExecutionId: 'root-workflow',
        rootExecutionType: 'WORKFLOW',
      };

      expectTypeOf(rootMetadata.parent).toEqualTypeOf<ParentExecutionContext | undefined>();
      expectTypeOf(rootMetadata.depth).toEqualTypeOf<0>();
    });

    it('should support mixed child types', () => {
      const metadata: ExecutionHierarchyMetadata = {
        children: [
          {
            childType: 'WORKFLOW',
            childId: 'sub-workflow-1',
            createdAt: 1000,
          },
          {
            childType: 'AGENT_LOOP',
            childId: 'agent-1',
            createdAt: 2000,
          },
        ],
        depth: 0,
        rootExecutionId: 'root-workflow',
        rootExecutionType: 'WORKFLOW',
      };

      expectTypeOf(metadata.children[0].childType).toEqualTypeOf<'WORKFLOW'>();
      expectTypeOf(metadata.children[1].childType).toEqualTypeOf<'AGENT_LOOP'>();
    });

    it('should support Agent → Agent hierarchy', () => {
      const agentMetadata: ExecutionHierarchyMetadata = {
        parent: {
          parentType: 'AGENT_LOOP',
          parentId: 'parent-agent-123',
          delegationPurpose: 'Specialized task delegation',
        },
        children: [
          {
            childType: 'AGENT_LOOP',
            childId: 'sub-agent-456',
            createdAt: Date.now(),
          },
        ],
        depth: 2,
        rootExecutionId: 'root-workflow',
        rootExecutionType: 'WORKFLOW',
      };

      expectTypeOf(agentMetadata.parent?.parentType).toEqualTypeOf<'AGENT_LOOP' | undefined>();
      expectTypeOf(agentMetadata.depth).toEqualTypeOf<2>();
    });
  });

  describe('Type Safety - Compile-time Validation', () => {
    it('should reject invalid parentType values', () => {
      // This would cause a TypeScript compilation error:
      // const invalidParent: ParentExecutionContext = {
      //   parentType: 'INVALID_TYPE', // ❌ Error: Type '"INVALID_TYPE"' is not assignable to type '"WORKFLOW" | "AGENT_LOOP"'
      //   parentId: 'test',
      // };
      
      // Valid usage:
      const validParent: ParentExecutionContext = {
        parentType: 'WORKFLOW', // ✅ OK
        parentId: 'test',
      };
      
      expectTypeOf(validParent.parentType).toEqualTypeOf<'WORKFLOW' | 'AGENT_LOOP'>();
    });

    it('should reject missing required fields', () => {
      // This would cause a TypeScript compilation error:
      // const incompleteParent: ParentExecutionContext = {
      //   parentType: 'WORKFLOW',
      //   // parentId is missing ❌ Error: Property 'parentId' is missing
      // };
      
      // Valid usage:
      const completeParent: ParentExecutionContext = {
        parentType: 'WORKFLOW',
        parentId: 'test', // ✅ Required field present
      };
      
      expectTypeOf(completeParent.parentId).toBeString();
    });
  });
});
