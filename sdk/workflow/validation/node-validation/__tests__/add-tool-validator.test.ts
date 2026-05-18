/**
 * ADD_TOOL Validator Unit Tests
 * Tests for add-tool-validator.ts functionality
 */

import { describe, it, expect } from 'vitest';
import { validateAddToolNode } from '../add-tool-validator.js';
import type { StaticNode } from '@wf-agent/types';
import { ConfigurationValidationError } from '@wf-agent/types';

describe('validateAddToolNode', () => {
  describe('valid ADD_TOOL nodes', () => {
    it('should validate ADD_TOOL node with required toolIds', () => {
      // Arrange
      const validNode: StaticNode = {
        id: 'add-tool-1',
        name: 'Add Tool Node',
        type: 'ADD_TOOL',
        config: {
          toolIds: ['tool-1', 'tool-2']
        }
      };
      
      // Act
      const result = validateAddToolNode(validNode);
      
      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });
    
    it('should reject ADD_TOOL node with empty toolIds array', () => {
      // Arrange
      const nodeWithEmptyTools: StaticNode = {
        id: 'add-tool-2',
        name: 'Add Tool Node 2',
        type: 'ADD_TOOL',
        config: {
          toolIds: []
        }
      };
      
      // Act
      const result = validateAddToolNode(nodeWithEmptyTools);
      
      // Assert
      expect(result.isErr()).toBe(true);
    });
  });
  
  describe('invalid ADD_TOOL nodes', () => {
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
      const result = validateAddToolNode(wrongTypeNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain('Expected ADD_TOOL node');
      }
    });
    
    it('should reject node missing toolIds', () => {
      // Arrange
      const missingToolIdsNode: StaticNode = {
        id: 'missing-toolids',
        name: 'Missing ToolIds',
        type: 'ADD_TOOL',
        config: {} as any // Missing toolIds
      };
      
      // Act
      const result = validateAddToolNode(missingToolIdsNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
      }
    });
  });
  
  describe('tool checker functionality', () => {
    it('should validate when all tools exist in registry', () => {
      // Arrange
      const node: StaticNode = {
        id: 'add-tool-3',
        name: 'Add Tool Node 3',
        type: 'ADD_TOOL',
        config: {
          toolIds: ['tool-1', 'tool-2']
        }
      };
      
      const mockToolChecker = {
        hasTool: (toolId: string) => ['tool-1', 'tool-2'].includes(toolId)
      };
      
      // Act
      const result = validateAddToolNode(node, mockToolChecker);
      
      // Assert
      expect(result.isOk()).toBe(true);
    });
    
    it('should reject when some tools do not exist in registry', () => {
      // Arrange
      const node: StaticNode = {
        id: 'add-tool-4',
        name: 'Add Tool Node 4',
        type: 'ADD_TOOL',
        config: {
          toolIds: ['tool-1', 'non-existent-tool']
        }
      };
      
      const mockToolChecker = {
        hasTool: (toolId: string) => toolId === 'tool-1'
      };
      
      // Act
      const result = validateAddToolNode(node, mockToolChecker);
      
      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain('Unknown tools');
        expect(result.error[0]?.message).toContain('non-existent-tool');
      }
    });
    
    it('should pass validation when no tool checker is provided', () => {
      // Arrange
      const node: StaticNode = {
        id: 'add-tool-5',
        name: 'Add Tool Node 5',
        type: 'ADD_TOOL',
        config: {
          toolIds: ['any-tool-id']
        }
      };
      
      // Act
      const result = validateAddToolNode(node);
      
      // Assert
      expect(result.isOk()).toBe(true);
    });
  });
  
  describe('edge cases', () => {
    it('should handle node with null config', () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: 'null-config',
        name: 'Null Config',
        type: 'ADD_TOOL',
        config: null as any
      };
      
      // Act
      const result = validateAddToolNode(nullConfigNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
    });
    
    it('should handle node with undefined config', () => {
      // Arrange
      const undefinedConfigNode: StaticNode = {
        id: 'undefined-config',
        name: 'Undefined Config',
        type: 'ADD_TOOL',
        config: undefined as any
      };
      
      // Act
      const result = validateAddToolNode(undefinedConfigNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
    });
  });
});
