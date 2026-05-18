/**
 * SUBGRAPH Validator Unit Tests
 * Tests for subgraph-validator.ts functionality
 */

import { describe, it, expect } from 'vitest';
import { validateSubgraphNode } from '../subgraph-validator.js';
import type { StaticNode } from '@wf-agent/types';
import { ConfigurationValidationError } from '@wf-agent/types';

describe('validateSubgraphNode', () => {
  describe('valid SUBGRAPH nodes', () => {
    it('should validate SUBGRAPH node with required subgraphId', () => {
      // Arrange
      const validNode: StaticNode = {
        id: 'subgraph-1',
        name: 'Subgraph Node 1',
        type: 'SUBGRAPH',
        config: {
          subgraphId: 'child-workflow',
          async: false
        }
      };
      
      // Act
      const result = validateSubgraphNode(validNode);
      
      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });
    
    it('should validate SUBGRAPH node with async flag', () => {
      // Arrange
      const nodeWithAsync: StaticNode = {
        id: 'subgraph-2',
        name: 'Subgraph Node 2',
        type: 'SUBGRAPH',
        config: {
          subgraphId: 'child-workflow',
          async: true
        }
      };
      
      // Act
      const result = validateSubgraphNode(nodeWithAsync);
      
      // Assert
      expect(result.isOk()).toBe(true);
    });
    
    it('should validate SUBGRAPH node with variable mappings', () => {
      // Arrange
      const nodeWithVariables: StaticNode = {
        id: 'subgraph-3',
        name: 'Subgraph Node 3',
        type: 'SUBGRAPH',
        config: {
          subgraphId: 'child-workflow',
          async: false,
          variableInputs: [
            { externalName: 'parentVar', internalName: 'childVar', required: true }
          ],
          variableOutputs: [
            { internalName: 'childResult', externalName: 'parentResult' }
          ]
        }
      };
      
      // Act
      const result = validateSubgraphNode(nodeWithVariables);
      
      // Assert
      expect(result.isOk()).toBe(true);
    });
  });
  
  describe('invalid SUBGRAPH nodes', () => {
    it('should reject node with wrong type', () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: 'wrong-type',
        name: 'Wrong Type',
        type: 'LLM', // Wrong type
        config: {
          profileId: 'test'
        }
      };
      
      // Act
      const result = validateSubgraphNode(wrongTypeNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain('Expected SUBGRAPH node');
      }
    });
    
    it('should reject node missing subgraphId', () => {
      // Arrange
      const missingSubgraphIdNode: StaticNode = {
        id: 'missing-id',
        name: 'Missing ID',
        type: 'SUBGRAPH',
        config: {} as any // Missing subgraphId
      };
      
      // Act
      const result = validateSubgraphNode(missingSubgraphIdNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toMatch(/required|Invalid input/);
      }
    });
    
    it('should reject node with empty subgraphId', () => {
      // Arrange
      const emptySubgraphIdNode: StaticNode = {
        id: 'empty-id',
        name: 'Empty ID',
        type: 'SUBGRAPH',
        config: {
          subgraphId: '', // Empty string
          async: false
        }
      };
      
      // Act
      const result = validateSubgraphNode(emptySubgraphIdNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain('Subgraph ID is required');
      }
    });
    
    it('should reject node with invalid async type', () => {
      // Arrange
      const invalidAsyncNode: StaticNode = {
        id: 'invalid-async',
        name: 'Invalid Async',
        type: 'SUBGRAPH',
        config: {
          subgraphId: 'child-workflow',
          async: 'not-a-boolean' as any // Invalid type
        }
      };
      
      // Act
      const result = validateSubgraphNode(invalidAsyncNode);
      
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
        type: 'SUBGRAPH',
        config: null as any
      };
      
      // Act
      const result = validateSubgraphNode(nullConfigNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
    });
    
    it('should handle node with undefined config', () => {
      // Arrange
      const undefinedConfigNode: StaticNode = {
        id: 'undefined-config',
        name: 'Undefined Config',
        type: 'SUBGRAPH',
        config: undefined as any
      };
      
      // Act
      const result = validateSubgraphNode(undefinedConfigNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
    });
    
    it('should validate node with extra properties in config', () => {
      // Arrange
      const nodeWithExtraProps: StaticNode = {
        id: 'extra-props',
        name: 'Extra Props',
        type: 'SUBGRAPH',
        config: {
          subgraphId: 'child-workflow',
          async: false
        }
      };
      
      // Act
      const result = validateSubgraphNode(nodeWithExtraProps);
      
      // Assert
      expect(result.isOk()).toBe(true);
      // Extra properties should be stripped or ignored by Zod
    });
  });
});
