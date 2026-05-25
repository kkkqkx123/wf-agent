/**
 * AgentLoop Node Validator Unit Tests
 * Tests for agent-loop-validator.ts functionality
 */

import { describe, it, expect } from 'vitest';
import { validateAgentLoopNode } from '../agent-loop-validator.js';
import type { StaticNode } from '@wf-agent/types';
import { ConfigurationValidationError } from '@wf-agent/types';

describe('validateAgentLoopNode', () => {
  describe('valid AGENT_LOOP nodes', () => {
    it('should validate AGENT_LOOP node with agentLoopId only', () => {
      // Arrange
      const validNode: StaticNode = {
        id: 'agent-1',
        name: 'Agent Loop 1',
        type: 'AGENT_LOOP',
        config: {
          agentLoopId: 'my-agent-config',
        },
      };

      // Act
      const result = validateAgentLoopNode(validNode);

      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });

    it('should validate AGENT_LOOP node with inlineConfig only', () => {
      // Arrange
      const validNode: StaticNode = {
        id: 'agent-2',
        name: 'Agent Loop 2',
        type: 'AGENT_LOOP',
        config: {
          inlineConfig: {
            profileId: 'DEFAULT',
            maxIterations: 10,
          },
        },
      };

      // Act
      const result = validateAgentLoopNode(validNode);

      // Assert
      expect(result.isOk()).toBe(true);
    });

    it('should validate AGENT_LOOP node with both agentLoopId and inlineConfig', () => {
      // Arrange
      const validNode: StaticNode = {
        id: 'agent-3',
        name: 'Agent Loop 3',
        type: 'AGENT_LOOP',
        config: {
          agentLoopId: 'base-agent',
          inlineConfig: {
            profileId: 'OVERRIDE_PROFILE',
            maxIterations: 5,
          },
        },
      };

      // Act
      const result = validateAgentLoopNode(validNode);

      // Assert
      expect(result.isOk()).toBe(true);
    });

    it('should validate AGENT_LOOP node with all optional inlineConfig fields', () => {
      // Arrange
      const validNode: StaticNode = {
        id: 'agent-4',
        name: 'Full Agent Loop',
        type: 'AGENT_LOOP',
        config: {
          agentLoopId: 'full-agent',
          inlineConfig: {
            profileId: 'CUSTOM',
            maxIterations: 100,
            availableTools: {
              tools: ['search_web', 'calculator'],
              requireApproval: ['deploy'],
            },
            workingContext: 'thread-1',
            dataInputs: [
              { parentField: 'query', internalName: 'query_text' },
            ],
            messageInputs: [
              { externalName: 'system-context', internalName: 'sys', required: true },
            ],
            messageOutputs: [
              { internalName: 'agent-chat', externalName: 'updated-conversation' },
            ],
          },
        },
      };

      // Act
      const result = validateAgentLoopNode(validNode);

      // Assert
      expect(result.isOk()).toBe(true);
    });

    it('should validate AGENT_LOOP node with agentLoopId and empty inlineConfig for overrides', () => {
      // Arrange — inlineConfig with profileId alone is enough to signal intent
      const validNode: StaticNode = {
        id: 'agent-5',
        name: 'Override Only',
        type: 'AGENT_LOOP',
        config: {
          agentLoopId: 'base-agent',
          inlineConfig: {
            maxIterations: 15,
          },
        },
      };

      // Act
      const result = validateAgentLoopNode(validNode);

      // Assert
      expect(result.isOk()).toBe(true);
    });
  });

  describe('invalid AGENT_LOOP nodes', () => {
    it('should reject node with wrong type', () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: 'wrong-type',
        name: 'Wrong Type',
        type: 'LLM',
        config: {
          profileId: 'test',
        },
      };

      // Act
      const result = validateAgentLoopNode(wrongTypeNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain('Expected AGENT_LOOP node');
      }
    });

    it('should reject node missing both agentLoopId and inlineConfig', () => {
      // Arrange
      const missingBothNode: StaticNode = {
        id: 'invalid-loop',
        name: 'Invalid',
        type: 'AGENT_LOOP',
        config: {} as any,
      };

      // Act
      const result = validateAgentLoopNode(missingBothNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
      }
    });

    it('should reject node with empty agentLoopId', () => {
      // Arrange
      const emptyIdNode: StaticNode = {
        id: 'empty-id',
        name: 'Empty ID',
        type: 'AGENT_LOOP',
        config: {
          agentLoopId: '', // Empty string — should fail min(1)
        },
      };

      // Act
      const result = validateAgentLoopNode(emptyIdNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toMatch(/empty|min/);
      }
    });

    it('should reject inlineConfig without profileId when agentLoopId is absent', () => {
      // Arrange
      const noProfileIdNode: StaticNode = {
        id: 'no-profile',
        name: 'No Profile',
        type: 'AGENT_LOOP',
        config: {
          inlineConfig: {
            maxIterations: 10,
            // Missing profileId
          },
        },
      };

      // Act
      const result = validateAgentLoopNode(noProfileIdNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain('profileId is required');
      }
    });

    it('should reject inlineConfig with empty profileId when agentLoopId is absent', () => {
      // Arrange
      const emptyProfileNode: StaticNode = {
        id: 'empty-profile',
        name: 'Empty Profile',
        type: 'AGENT_LOOP',
        config: {
          inlineConfig: {
            profileId: '', // Empty string — rejected by min(1) on profileId
            maxIterations: 10,
          },
        },
      };

      // Act
      const result = validateAgentLoopNode(emptyProfileNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        // Two errors: min(1) on profileId field + refine cross-field check
        expect(result.error.length).toBeGreaterThanOrEqual(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
      }
    });

    it('should reject inlineConfig with non-positive maxIterations', () => {
      // Arrange
      const invalidMaxNode: StaticNode = {
        id: 'bad-max',
        name: 'Bad Max',
        type: 'AGENT_LOOP',
        config: {
          inlineConfig: {
            profileId: 'DEFAULT',
            maxIterations: 0, // Non-positive — should fail positive()
          },
        },
      };

      // Act
      const result = validateAgentLoopNode(invalidMaxNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
      }
    });

    it('should reject inlineConfig with negative maxIterations', () => {
      // Arrange
      const negativeMaxNode: StaticNode = {
        id: 'neg-max',
        name: 'Negative Max',
        type: 'AGENT_LOOP',
        config: {
          inlineConfig: {
            profileId: 'DEFAULT',
            maxIterations: -1, // Negative — should fail positive()
          },
        },
      };

      // Act
      const result = validateAgentLoopNode(negativeMaxNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
      }
    });

    it('should reject inlineConfig with invalid dataInputs format', () => {
      // Arrange
      const invalidDataInputsNode: StaticNode = {
        id: 'bad-inputs',
        name: 'Bad Inputs',
        type: 'AGENT_LOOP',
        config: {
          inlineConfig: {
            profileId: 'DEFAULT',
            dataInputs: [
              { internalName: 'missingParent' } as any, // Missing parentField
            ],
          },
        },
      };

      // Act
      const result = validateAgentLoopNode(invalidDataInputsNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
      }
    });

    it('should reject inlineConfig with invalid messageInputs format', () => {
      // Arrange
      const invalidMessageInputsNode: StaticNode = {
        id: 'bad-msg-inputs',
        name: 'Bad Msg Inputs',
        type: 'AGENT_LOOP',
        config: {
          inlineConfig: {
            profileId: 'DEFAULT',
            messageInputs: [
              { externalName: 'ctx' } as any, // Missing internalName
            ],
          },
        },
      };

      // Act
      const result = validateAgentLoopNode(invalidMessageInputsNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
      }
    });

    it('should reject inlineConfig with invalid messageOutputs format', () => {
      // Arrange
      const invalidMessageOutputsNode: StaticNode = {
        id: 'bad-msg-outputs',
        name: 'Bad Msg Outputs',
        type: 'AGENT_LOOP',
        config: {
          inlineConfig: {
            profileId: 'DEFAULT',
            messageOutputs: [
              { externalName: 'ctx' } as any, // Missing internalName
            ],
          },
        },
      };

      // Act
      const result = validateAgentLoopNode(invalidMessageOutputsNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
      }
    });

    it('should reject inlineConfig with invalid availableTools.tools type', () => {
      // Arrange
      const invalidToolsNode: StaticNode = {
        id: 'bad-tools',
        name: 'Bad Tools',
        type: 'AGENT_LOOP',
        config: {
          inlineConfig: {
            profileId: 'DEFAULT',
            availableTools: {
              tools: 'not-an-array' as any, // Should be string[]
            },
          },
        },
      };

      // Act
      const result = validateAgentLoopNode(invalidToolsNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
      }
    });
  });

  describe('edge cases', () => {
    it('should handle node with null config', () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: 'null-config',
        name: 'Null Config',
        type: 'AGENT_LOOP',
        config: null as any,
      };

      // Act
      const result = validateAgentLoopNode(nullConfigNode);

      // Assert
      expect(result.isErr()).toBe(true);
    });

    it('should handle node with undefined config', () => {
      // Arrange
      const undefinedConfigNode: StaticNode = {
        id: 'undefined-config',
        name: 'Undefined Config',
        type: 'AGENT_LOOP',
        config: undefined as any,
      };

      // Act
      const result = validateAgentLoopNode(undefinedConfigNode);

      // Assert
      expect(result.isErr()).toBe(true);
    });

    it('should handle node with empty inlineConfig and no agentLoopId', () => {
      // Arrange — inlineConfig object exists but is empty
      const emptyInlineNode: StaticNode = {
        id: 'empty-inline',
        name: 'Empty Inline',
        type: 'AGENT_LOOP',
        config: {
          inlineConfig: {} as any, // Empty inlineConfig — no profileId
        },
      };

      // Act
      const result = validateAgentLoopNode(emptyInlineNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error[0]?.message).toContain('profileId is required');
      }
    });

    it('should validate node with empty dataInputs array', () => {
      // Arrange
      const emptyArraysNode: StaticNode = {
        id: 'empty-arrays',
        name: 'Empty Arrays',
        type: 'AGENT_LOOP',
        config: {
          agentLoopId: 'base-agent',
          inlineConfig: {
            dataInputs: [],
            messageInputs: [],
            messageOutputs: [],
          },
        },
      };

      // Act
      const result = validateAgentLoopNode(emptyArraysNode);

      // Assert
      expect(result.isOk()).toBe(true);
    });

    it('should validate node with agentLoopId and empty inlineConfig override', () => {
      // Arrange — inlineConfig with maxIterations only is valid override
      const overrideNode: StaticNode = {
        id: 'override-only',
        name: 'Override Only',
        type: 'AGENT_LOOP',
        config: {
          agentLoopId: 'base-agent',
          inlineConfig: {
            maxIterations: 30,
          },
        },
      };

      // Act
      const result = validateAgentLoopNode(overrideNode);

      // Assert
      expect(result.isOk()).toBe(true);
    });

    it('should validate node with extra properties in config (stripped by Zod)', () => {
      // Arrange
      const nodeWithExtraProps: StaticNode = {
        id: 'extra-props',
        name: 'Extra Props',
        type: 'AGENT_LOOP',
        config: {
          agentLoopId: 'my-agent',
          // Zod strips extra properties by default
          extraField: 'should-be-ignored',
        } as any,
      };

      // Act
      const result = validateAgentLoopNode(nodeWithExtraProps);

      // Assert
      expect(result.isOk()).toBe(true);
    });
  });
});
