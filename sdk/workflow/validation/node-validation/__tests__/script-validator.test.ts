/**
 * SCRIPT Validator Unit Tests
 * Tests for script-validator.ts functionality
 */

import { describe, it, expect } from 'vitest';
import { validateScriptNode } from '../script-validator.js';
import type { StaticNode } from '@wf-agent/types';
import { ConfigurationValidationError } from '@wf-agent/types';

describe('validateScriptNode', () => {
  describe('valid SCRIPT nodes', () => {
    it('should validate SCRIPT node with script', () => {
      // Arrange
      const validNode: StaticNode = {
        id: 'script-1',
        name: 'Script Node',
        type: 'SCRIPT',
        config: {
          scriptName: 'test-script',
          risk: 'low'
        }
      } as StaticNode;
      
      // Act
      const result = validateScriptNode(validNode);
      
      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });
  });
  
  describe('invalid SCRIPT nodes', () => {
    it('should reject node with wrong type', () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: 'wrong-type',
        name: 'Wrong Type',
        type: 'LLM',
        config: {}
      } as StaticNode;
      
      // Act
      const result = validateScriptNode(wrongTypeNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain('Expected SCRIPT node');
      }
    });
    
    it('should reject node missing script', () => {
      // Arrange
      const missingScriptNode: StaticNode = {
        id: 'missing-script',
        name: 'Missing Script',
        type: 'SCRIPT',
        config: {} as any
      };
      
      // Act
      const result = validateScriptNode(missingScriptNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
    });
  });
  
  describe('edge cases', () => {
    it('should handle node with null config', () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: 'null-config',
        name: 'Null Config',
        type: 'SCRIPT',
        config: null as any
      };
      
      // Act
      const result = validateScriptNode(nullConfigNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
    });
  });
});
