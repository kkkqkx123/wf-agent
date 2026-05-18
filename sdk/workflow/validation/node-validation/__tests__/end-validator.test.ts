/**
 * END Validator Unit Tests
 * Tests for end-validator.ts functionality
 */

import { describe, it, expect } from 'vitest';
import { validateEndNode } from '../end-validator.js';
import type { StaticNode } from '@wf-agent/types';
import { ConfigurationValidationError } from '@wf-agent/types';

describe('validateEndNode', () => {
  describe('valid END nodes', () => {
    it('should validate END node with empty config', () => {
      // Arrange
      const validNode: StaticNode = {
        id: 'end-1',
        name: 'End Node',
        type: 'END',
        config: {}
      };
      
      // Act
      const result = validateEndNode(validNode);
      
      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });
    
    it('should validate END node with variable outputs', () => {
      // Arrange
      const nodeWithOutputs: StaticNode = {
        id: 'end-2',
        name: 'End Node 2',
        type: 'END',
        config: {
          variableOutputs: [
            { internalName: 'result', externalName: 'output' }
          ]
        }
      } as StaticNode;
      
      // Act
      const result = validateEndNode(nodeWithOutputs);
      
      // Assert
      expect(result.isOk()).toBe(true);
    });
  });
  
  describe('invalid END nodes', () => {
    it('should reject node with wrong type', () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: 'wrong-type',
        name: 'Wrong Type',
        type: 'START',
        config: {}
      };
      
      // Act
      const result = validateEndNode(wrongTypeNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain('Expected END node');
      }
    });
  });
  
  describe('edge cases', () => {
    it('should handle node with null config by converting to empty object', () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: 'null-config',
        name: 'Null Config',
        type: 'END',
        config: null as any
      };
      
      // Act
      const result = validateEndNode(nullConfigNode);
      
      // Assert - null config is converted to {} which is valid for WorkflowEndConfigSchema
      expect(result.isOk()).toBe(true);
    });
    
    it('should handle node with undefined config by converting to empty object', () => {
      // Arrange
      const undefinedConfigNode: StaticNode = {
        id: 'undefined-config',
        name: 'Undefined Config',
        type: 'END',
        config: undefined as any
      };
      
      // Act
      const result = validateEndNode(undefinedConfigNode);
      
      // Assert - undefined config is converted to {} which is valid for WorkflowEndConfigSchema
      expect(result.isOk()).toBe(true);
    });
  });
});
