/**
 * USER_INTERACTION Validator Unit Tests
 * Tests for user-interaction-validator.ts functionality
 */

import { describe, it, expect } from 'vitest';
import { validateUserInteractionNode } from '../user-interaction-validator.js';
import type { StaticNode } from '@wf-agent/types';
import { ConfigurationValidationError } from '@wf-agent/types';

describe('validateUserInteractionNode', () => {
  describe('valid USER_INTERACTION nodes', () => {
    it('should validate USER_INTERACTION node with interactionType', () => {
      // Arrange
      const validNode: StaticNode = {
        id: 'user-interaction-1',
        name: 'User Interaction Node',
        type: 'USER_INTERACTION',
        config: {
          operationType: 'UPDATE_VARIABLES' as const,
          variables: [
            {
              variableName: 'var1',
              expression: '42',
              scope: 'execution' as const
            }
          ],
          prompt: 'Please approve'
        }
      };
      
      // Act
      const result = validateUserInteractionNode(validNode);
      
      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });
  });
  
  describe('invalid USER_INTERACTION nodes', () => {
    it('should reject node with wrong type', () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: 'wrong-type',
        name: 'Wrong Type',
        type: 'LLM',
        config: {}
      } as StaticNode;
      
      // Act
      const result = validateUserInteractionNode(wrongTypeNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain('Expected USER_INTERACTION node');
      }
    });
  });
  
  describe('edge cases', () => {
    it('should handle node with null config', () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: 'null-config',
        name: 'Null Config',
        type: 'USER_INTERACTION',
        config: null as any
      };
      
      // Act
      const result = validateUserInteractionNode(nullConfigNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
    });
  });
});
