/**
 * VARIABLE Validator Unit Tests
 * Tests for variable-validator.ts functionality
 */

import { describe, it, expect } from 'vitest';
import { validateVariableNode } from '../variable-validator.js';
import type { StaticNode } from '@wf-agent/types';
import { ConfigurationValidationError } from '@wf-agent/types';

describe('validateVariableNode', () => {
  describe('valid VARIABLE nodes', () => {
    it('should validate VARIABLE node with operations', () => {
      // Arrange
      const validNode: StaticNode = {
        id: 'variable-1',
        name: 'Variable Node',
        type: 'VARIABLE',
        config: {
          variableName: 'var1',
          variableType: 'number' as const,
          expression: '42'
        }
      } as StaticNode;
      
      // Act
      const result = validateVariableNode(validNode);
      
      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });
  });
  
  describe('invalid VARIABLE nodes', () => {
    it('should reject node with wrong type', () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: 'wrong-type',
        name: 'Wrong Type',
        type: 'LLM',
        config: {}
      } as StaticNode;
      
      // Act
      const result = validateVariableNode(wrongTypeNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain('Expected VARIABLE node');
      }
    });
  });
  
  describe('edge cases', () => {
    it('should handle node with null config', () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: 'null-config',
        name: 'Null Config',
        type: 'VARIABLE',
        config: null as any
      };
      
      // Act
      const result = validateVariableNode(nullConfigNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
    });
  });
});
