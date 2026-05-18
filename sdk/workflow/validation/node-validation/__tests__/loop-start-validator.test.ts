/**
 * LOOP_START Validator Unit Tests
 * Tests for loop-start-validator.ts functionality
 */

import { describe, it, expect } from 'vitest';
import { validateLoopStartNode } from '../loop-start-validator.js';
import type { StaticNode } from '@wf-agent/types';
import { ConfigurationValidationError } from '@wf-agent/types';

describe('validateLoopStartNode', () => {
  describe('valid LOOP_START nodes', () => {
    it('should validate LOOP_START node with loopId', () => {
      // Arrange
      const validNode: StaticNode = {
        id: 'loop-start-1',
        name: 'Loop Start Node',
        type: 'LOOP_START',
        config: {
          loopId: 'test-loop',
          maxIterations: 10
        }
      } as StaticNode;
      
      // Act
      const result = validateLoopStartNode(validNode);
      
      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });
  });
  
  describe('invalid LOOP_START nodes', () => {
    it('should reject node with wrong type', () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: 'wrong-type',
        name: 'Wrong Type',
        type: 'LOOP_END',
        config: {}
      } as StaticNode;
      
      // Act
      const result = validateLoopStartNode(wrongTypeNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain('Expected LOOP_START node');
      }
    });
  });
  
  describe('edge cases', () => {
    it('should handle node with null config', () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: 'null-config',
        name: 'Null Config',
        type: 'LOOP_START',
        config: null as any
      };
      
      // Act
      const result = validateLoopStartNode(nullConfigNode);
      
      // Assert
      expect(result.isErr()).toBe(true);
    });
  });
});
