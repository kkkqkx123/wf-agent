/**
 * VariableManager - SUBGRAPH Variable Passing Unit Tests
 * Tests for importVariables and exportVariables functionality used by SUBGRAPH nodes
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { VariableManager } from '../variable-manager.js';
import { RuntimeValidationError } from '@wf-agent/types';

// Mock structuredClone
const mockStructuredClone = vi.fn();
vi.stubGlobal('structuredClone', mockStructuredClone);

describe('VariableManager - SUBGRAPH Variable Passing', () => {
  let parentManager: VariableManager;
  let childManager: VariableManager;
  
  beforeEach(() => {
    vi.clearAllMocks();
    mockStructuredClone.mockImplementation((value) => {
      // Simple deep clone for testing
      return JSON.parse(JSON.stringify(value));
    });
    
    parentManager = new VariableManager();
    childManager = new VariableManager();
    
    // Setup parent variables - need to register them first
    parentManager.registerVariable({ name: 'parentVar1', type: 'string', value: 'value1', readonly: false });
    parentManager.registerVariable({ name: 'parentVar2', type: 'object', value: { nested: { data: 'test' } }, readonly: false });
    parentManager.registerVariable({ name: 'parentVar3', type: 'array', value: [1, 2, 3], readonly: false });
  });
  
  describe('importVariables', () => {
    it('should import required variable successfully with deep clone', () => {
      // Arrange
      const mappings = [
        { externalName: 'parentVar1', internalName: 'childVar1', required: true }
      ];
      
      // Act
      childManager.importVariables(parentManager, mappings);
      
      // Assert
      expect(childManager.getVariable('childVar1')).toBe('value1');
      expect(mockStructuredClone).toHaveBeenCalledWith('value1');
    });
    
    it('should import object variable with deep clone to prevent mutation', () => {
      // Arrange
      const originalObject = { nested: { data: 'test' } };
      const mappings = [
        { externalName: 'parentVar2', internalName: 'childVar2', required: true }
      ];
      
      // Act
      childManager.importVariables(parentManager, mappings);
      
      // Assert
      const importedValue = childManager.getVariable('childVar2');
      expect(importedValue).toEqual(originalObject);
      expect(importedValue).not.toBe(originalObject); // Should be a clone
      
      // Verify mutation doesn't affect parent
      (importedValue as any).nested.data = 'modified';
      expect(parentManager.getVariable('parentVar2')).toEqual({ nested: { data: 'test' } });
    });
    
    it('should throw error when required variable is missing', () => {
      // Arrange
      const mappings = [
        { externalName: 'nonExistentVar', internalName: 'childVar', required: true }
      ];
      
      // Act & Assert
      expect(() => {
        childManager.importVariables(parentManager, mappings);
      }).toThrow(RuntimeValidationError);
      
      expect(() => {
        childManager.importVariables(parentManager, mappings);
      }).toThrow("Required input variable 'nonExistentVar' not found in parent workflow");
    });
    
    it('should use default value for optional missing variable', () => {
      // Arrange
      const defaultValue = { default: 'value' };
      const mappings = [
        { 
          externalName: 'nonExistentVar', 
          internalName: 'childVar', 
          required: false, 
          defaultValue 
        }
      ];
      
      // Act
      childManager.importVariables(parentManager, mappings);
      
      // Assert
      expect(childManager.getVariable('childVar')).toEqual(defaultValue);
      expect(childManager.getVariable('childVar')).not.toBe(defaultValue); // Should be cloned
    });
    
    it('should skip optional variable without default value', () => {
      // Arrange
      const mappings = [
        { externalName: 'nonExistentVar', internalName: 'childVar', required: false }
      ];
      
      // Act
      childManager.importVariables(parentManager, mappings);
      
      // Assert
      expect(childManager.getVariable('childVar')).toBeUndefined();
    });
    
    it('should handle structuredClone failure gracefully', () => {
      // Arrange
      const circularReference: any = { data: 'test' };
      circularReference.self = circularReference; // Circular reference
      
      parentManager.registerVariable({ name: 'circularVar', type: 'object', value: circularReference, readonly: false });
      
      // Mock structuredClone to fail
      mockStructuredClone.mockImplementation(() => {
        throw new Error('structuredClone failed');
      });
      
      const mappings = [
        { externalName: 'circularVar', internalName: 'childVar', required: true }
      ];
      
      // Act
      childManager.importVariables(parentManager, mappings);
      
      // Assert
      expect(childManager.getVariable('childVar')).toBe(circularReference);
      // Should fall back to shallow copy
    });
    
    it('should import multiple variables with mixed requirements', () => {
      // Arrange
      const mappings = [
        { externalName: 'parentVar1', internalName: 'childVar1', required: true },
        { externalName: 'parentVar2', internalName: 'childVar2', required: false },
        { externalName: 'nonExistentVar', internalName: 'childVar3', required: false, defaultValue: 'default' }
      ];
      
      // Act
      childManager.importVariables(parentManager, mappings);
      
      // Assert
      expect(childManager.getVariable('childVar1')).toBe('value1');
      expect(childManager.getVariable('childVar2')).toEqual({ nested: { data: 'test' } });
      expect(childManager.getVariable('childVar3')).toBe('default');
    });
  });
  
  describe('exportVariables', () => {
    beforeEach(() => {
      // Setup child variables - need to register them first
      childManager.registerVariable({ name: 'childResult', type: 'string', value: 'success', readonly: false });
      childManager.registerVariable({ name: 'childData', type: 'object', value: { result: 'data' }, readonly: false });
      childManager.registerVariable({ name: 'childArray', type: 'array', value: [1, 2, 3], readonly: false });
    });
    
    it('should export variable successfully with deep clone', () => {
      // Arrange
      const mappings = [
        { internalName: 'childResult', externalName: 'parentResult' }
      ];
      
      // Act
      childManager.exportVariables(parentManager, mappings);
      
      // Assert
      expect(parentManager.getVariable('parentResult')).toBe('success');
      expect(mockStructuredClone).toHaveBeenCalledWith('success');
    });
    
    it('should export object variable with deep clone to prevent mutation', () => {
      // Arrange
      const originalObject = { result: 'data' };
      const mappings = [
        { internalName: 'childData', externalName: 'parentData' }
      ];
      
      // Act
      childManager.exportVariables(parentManager, mappings);
      
      // Assert
      const exportedValue = parentManager.getVariable('parentData');
      expect(exportedValue).toEqual(originalObject);
      expect(exportedValue).not.toBe(originalObject); // Should be a clone
      
      // Verify mutation doesn't affect child
      (exportedValue as any).result = 'modified';
      expect(childManager.getVariable('childData')).toEqual({ result: 'data' });
    });
    
    it('should skip undefined output variable (optional output)', () => {
      // Arrange
      const mappings = [
        { internalName: 'nonExistentVar', externalName: 'parentOutput' }
      ];
      
      // Act
      childManager.exportVariables(parentManager, mappings);
      
      // Assert
      expect(parentManager.getVariable('parentOutput')).toBeUndefined();
      expect(mockStructuredClone).not.toHaveBeenCalled();
    });
    
    it('should handle structuredClone failure during export gracefully', () => {
      // Arrange
      const circularReference: any = { data: 'test' };
      circularReference.self = circularReference; // Circular reference
      
      childManager.registerVariable({ name: 'circularResult', type: 'object', value: circularReference, readonly: false });
      
      // Mock structuredClone to fail
      mockStructuredClone.mockImplementation(() => {
        throw new Error('structuredClone failed');
      });
      
      const mappings = [
        { internalName: 'circularResult', externalName: 'parentResult' }
      ];
      
      // Act
      childManager.exportVariables(parentManager, mappings);
      
      // Assert
      expect(parentManager.getVariable('parentResult')).toBe(circularReference);
      // Should fall back to shallow copy
    });
    
    it('should export multiple variables', () => {
      // Arrange
      const mappings = [
        { internalName: 'childResult', externalName: 'parentResult' },
        { internalName: 'childData', externalName: 'parentData' },
        { internalName: 'nonExistentVar', externalName: 'parentOptional' } // Should be skipped
      ];
      
      // Act
      childManager.exportVariables(parentManager, mappings);
      
      // Assert
      expect(parentManager.getVariable('parentResult')).toBe('success');
      expect(parentManager.getVariable('parentData')).toEqual({ result: 'data' });
      expect(parentManager.getVariable('parentOptional')).toBeUndefined();
    });
  });
  
  describe('integration - full SUBGRAPH variable flow', () => {
    it('should complete full import-execute-export cycle with isolation', () => {
      // Arrange
      const inputMappings = [
        { externalName: 'parentInput', internalName: 'childInput', required: true }
      ];
      
      const outputMappings = [
        { internalName: 'childOutput', externalName: 'parentOutput' }
      ];
      
      // Setup parent input
      const originalInput = { data: 'original', nested: { value: 1 } };
      parentManager.registerVariable({ name: 'parentInput', type: 'object', value: originalInput, readonly: false });
      
      // Act - Import phase
      childManager.importVariables(parentManager, inputMappings);
      
      // Simulate child workflow execution (modify the imported data)
      const childInput = childManager.getVariable('childInput') as any;
      childInput.data = 'modified';
      childInput.nested.value = 2;
      
      // Set child output
      const childOutput = { result: 'processed', input: childInput };
      childManager.registerVariable({ name: 'childOutput', type: 'object', value: childOutput, readonly: false });
      
      // Export phase
      childManager.exportVariables(parentManager, outputMappings);
      
      // Assert
      const exportedOutput = parentManager.getVariable('parentOutput') as any;
      
      // Verify parent input was not mutated by child
      expect(parentManager.getVariable('parentInput')).toEqual({ 
        data: 'original', 
        nested: { value: 1 } 
      });
      
      // Verify child output was exported
      expect(exportedOutput.result).toBe('processed');
      
      // Verify exported output is a clone, not the same object
      expect(exportedOutput).not.toBe(childOutput);
      
      // Verify exported output contains modified child data
      expect(exportedOutput.input.data).toBe('modified');
      expect(exportedOutput.input.nested.value).toBe(2);
    });
  });
});
