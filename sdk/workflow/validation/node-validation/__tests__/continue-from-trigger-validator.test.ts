/**
 * CONTINUE_FROM_TRIGGER Validator Unit Tests
 * Tests for continue-from-trigger-validator.ts functionality
 */

import { describe, it, expect } from 'vitest';
import { validateContinueFromTriggerNode } from '../continue-from-trigger-validator.js';
import type { StaticNode } from '@wf-agent/types';
import { ConfigurationValidationError } from '@wf-agent/types';

describe('validateContinueFromTriggerNode', () => {
  describe('valid CONTINUE_FROM_TRIGGER nodes', () => {
    it('should validate CONTINUE_FROM_TRIGGER node with empty config', () => {
      // Arrange
      const validNode: StaticNode = {
        id: 'continue-1',
        name: 'Continue From Trigger Node',
        type: 'CONTINUE_FROM_TRIGGER',
        config: {}
      };
      
      // Act
      const result = validateContinueFromTriggerNode(validNode);
      
      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });
    
    it('should validate CONTINUE_FROM_TRIGGER node with variableCallback includeAll', () => {
      // Arrange
      const nodeWithIncludeAll: StaticNode = {
        id: 'continue-2',
        name: 'Continue From Trigger Node 2',
        type: 'CONTINUE_FROM_TRIGGER',
        config: {
          variableCallback: {
            includeAll: true
          }
        }
      } as StaticNode;
      
      // Act
      const result = validateContinueFromTriggerNode(nodeWithIncludeAll);
      
      // Assert
      expect(result.isOk()).toBe(true);
    });
    
    it('should validate CONTINUE_FROM_TRIGGER node with variableCallback includeVariables', () => {
      // Arrange
      const nodeWithIncludeVars: StaticNode = {
        id: 'continue-3',
        name: 'Continue From Trigger Node 3',
        type: 'CONTINUE_FROM_TRIGGER',
        config: {
          variableCallback: {
            includeVariables: ['var1', 'var2']
          }
        }
      } as StaticNode;
      
      // Act
      const result = validateContinueFromTriggerNode(nodeWithIncludeVars);
      
      // Assert
      expect(result.isOk()).toBe(true);
    });
  });
  
  describe('invalid CONTINUE_FROM_TRIGGER nodes', () => {
    it('should reject node with wrong type', () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: 'wrong-type',
        name: 'Wrong Type',
        type: 'LLM',
        config: {
          profileId: 'test'
        }
      };
      
      // Act
      const result = validateContinueFromTriggerNode(wrongTypeNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain('Expected CONTINUE_FROM_TRIGGER node');
      }
    });
    
    it('should reject node with both includeAll and includeVariables', () => {
      // Arrange
      const invalidNode: StaticNode = {
        id: 'invalid-config',
        name: 'Invalid Config',
        type: 'CONTINUE_FROM_TRIGGER',
        config: {
          variableCallback: {
            includeAll: true,
            includeVariables: ['var1']
          }
        }
      } as StaticNode;
      
      // Act
      const result = validateContinueFromTriggerNode(invalidNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain('Cannot specify both includeAll and includeVariables');
      }
    });
  });
  
  describe('edge cases', () => {
    it('should handle node with null config by converting to empty object', () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: 'null-config',
        name: 'Null Config',
        type: 'CONTINUE_FROM_TRIGGER',
        config: null as any
      };
      
      // Act
      const result = validateContinueFromTriggerNode(nullConfigNode);
      
      // Assert - null config is converted to {} which is valid
      expect(result.isOk()).toBe(true);
    });
    
    it('should handle node with undefined config by converting to empty object', () => {
      // Arrange
      const undefinedConfigNode: StaticNode = {
        id: 'undefined-config',
        name: 'Undefined Config',
        type: 'CONTINUE_FROM_TRIGGER',
        config: undefined as any
      };
      
      // Act
      const result = validateContinueFromTriggerNode(undefinedConfigNode);
      
      // Assert - undefined config is converted to {} which is valid
      expect(result.isOk()).toBe(true);
    });
  });
});
