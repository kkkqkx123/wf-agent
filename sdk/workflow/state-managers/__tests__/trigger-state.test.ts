/**
 * TriggerState - Unit Tests
 * Tests for trigger runtime state management
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { TriggerState } from '../trigger-state.js';
import { RuntimeValidationError, ExecutionError, NotFoundError } from '@wf-agent/types';
import type { TriggerRuntimeState, TriggerStatus } from '@wf-agent/types';

describe('TriggerState', () => {
  let triggerState: TriggerState;
  const executionId = 'exec-123';
  const workflowId = 'workflow-456';

  beforeEach(() => {
    triggerState = new TriggerState(executionId);
    triggerState.setWorkflowId(workflowId);
  });

  describe('constructor', () => {
    it('should initialize with execution ID', () => {
      const state = new TriggerState('test-exec');
      expect(state.getExecutionId()).toBe('test-exec');
      expect(state.getWorkflowId()).toBeNull();
      expect(state.size()).toBe(0);
      expect(state.isEmpty()).toBe(true);
    });
  });

  describe('setWorkflowId and getWorkflowId', () => {
    it('should set workflow ID', () => {
      const state = new TriggerState('exec-1');
      state.setWorkflowId('workflow-1');
      expect(state.getWorkflowId()).toBe('workflow-1');
    });

    it('should allow null workflow ID', () => {
      const state = new TriggerState('exec-1');
      expect(state.getWorkflowId()).toBeNull();
    });
  });

  describe('register', () => {
    it('should register trigger state successfully', () => {
      // Arrange
      const state: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId,
        workflowId,
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      // Act
      triggerState.register(state);

      // Assert
      expect(triggerState.size()).toBe(1);
      expect(triggerState.hasState('trigger-1')).toBe(true);
      
      const registered = triggerState.getState('trigger-1');
      expect(registered).toEqual(state);
    });

    it('should throw error when triggerId is missing', () => {
      // Arrange
      const state: TriggerRuntimeState = {
        triggerId: '',
        executionId,
        workflowId,
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      // Act & Assert
      expect(() => triggerState.register(state)).toThrow(RuntimeValidationError);
      expect(() => triggerState.register(state)).toThrow('Trigger ID cannot be null');
    });

    it('should throw error when executionId is missing', () => {
      // Arrange
      const state: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId: '',
        workflowId,
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      // Act & Assert
      expect(() => triggerState.register(state)).toThrow(RuntimeValidationError);
      expect(() => triggerState.register(state)).toThrow('Execution ID cannot be null');
    });

    it('should throw error when workflowId is missing', () => {
      // Arrange
      const state: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId,
        workflowId: '',
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      // Act & Assert
      expect(() => triggerState.register(state)).toThrow(RuntimeValidationError);
      expect(() => triggerState.register(state)).toThrow('Workflow ID cannot be null');
    });

    it('should throw error when executionId mismatch', () => {
      // Arrange
      const state: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId: 'different-exec',
        workflowId,
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      // Act & Assert
      expect(() => triggerState.register(state)).toThrow(RuntimeValidationError);
      expect(() => triggerState.register(state)).toThrow('Execution ID mismatch');
    });

    it('should throw error when workflowId mismatch', () => {
      // Arrange
      const state: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId,
        workflowId: 'different-workflow',
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      // Act & Assert
      expect(() => triggerState.register(state)).toThrow(RuntimeValidationError);
      expect(() => triggerState.register(state)).toThrow('Workflow ID mismatch');
    });

    it('should allow registration when workflowId is not set', () => {
      // Arrange
      const stateWithoutWorkflow = new TriggerState('exec-1');
      const state: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId: 'exec-1',
        workflowId: 'workflow-1',
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      // Act
      stateWithoutWorkflow.register(state);

      // Assert
      expect(stateWithoutWorkflow.hasState('trigger-1')).toBe(true);
    });

    it('should throw error when registering duplicate trigger', () => {
      // Arrange
      const state: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId,
        workflowId,
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      };

      triggerState.register(state);

      // Act & Assert
      expect(() => triggerState.register(state)).toThrow(ExecutionError);
      expect(() => triggerState.register(state)).toThrow('Trigger state trigger-1 Existing');
    });
  });

  describe('getState', () => {
    it('should return trigger state', () => {
      // Arrange
      const state: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId,
        workflowId,
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      };
      triggerState.register(state);

      // Act
      const result = triggerState.getState('trigger-1');

      // Assert
      expect(result).toEqual(state);
    });

    it('should return undefined when trigger not found', () => {
      expect(triggerState.getState('non-existent')).toBeUndefined();
    });
  });

  describe('updateStatus', () => {
    it('should update trigger status', () => {
      // Arrange
      const state: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId,
        workflowId,
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now() - 1000, // Use past timestamp to ensure update
      };
      triggerState.register(state);

      // Act
      triggerState.updateStatus('trigger-1', 'DISABLED');

      // Assert
      const updated = triggerState.getState('trigger-1');
      expect(updated?.status).toBe('DISABLED');
      expect(updated?.updatedAt).toBeGreaterThan(state.updatedAt);
    });

    it('should throw error when trigger not found', () => {
      // Act & Assert
      expect(() => triggerState.updateStatus('non-existent', 'DISABLED')).toThrow(NotFoundError);
      expect(() => triggerState.updateStatus('non-existent', 'DISABLED')).toThrow('Trigger status non-existent not present');
    });
  });

  describe('incrementTriggerCount', () => {
    it('should increment trigger count', () => {
      // Arrange
      const state: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId,
        workflowId,
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now() - 1000, // Use past timestamp to ensure update
      };
      triggerState.register(state);

      // Act
      triggerState.incrementTriggerCount('trigger-1');

      // Assert
      const updated = triggerState.getState('trigger-1');
      expect(updated?.triggerCount).toBe(1);
      expect(updated?.updatedAt).toBeGreaterThan(state.updatedAt);
    });

    it('should increment multiple times', () => {
      // Arrange
      const state: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId,
        workflowId,
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      };
      triggerState.register(state);

      // Act
      triggerState.incrementTriggerCount('trigger-1');
      triggerState.incrementTriggerCount('trigger-1');
      triggerState.incrementTriggerCount('trigger-1');

      // Assert
      const updated = triggerState.getState('trigger-1');
      expect(updated?.triggerCount).toBe(3);
    });

    it('should throw error when trigger not found', () => {
      // Act & Assert
      expect(() => triggerState.incrementTriggerCount('non-existent')).toThrow(NotFoundError);
    });
  });

  describe('getAllStates', () => {
    it('should return all trigger states', () => {
      // Arrange
      const state1: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId,
        workflowId,
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      };
      const state2: TriggerRuntimeState = {
        triggerId: 'trigger-2',
        executionId,
        workflowId,
        status: 'DISABLED',
        triggerCount: 5,
        updatedAt: Date.now(),
      };
      triggerState.register(state1);
      triggerState.register(state2);

      // Act
      const allStates = triggerState.getAllStates();

      // Assert
      expect(allStates.size).toBe(2);
      expect(allStates.has('trigger-1')).toBe(true);
      expect(allStates.has('trigger-2')).toBe(true);
    });

    it('should return copy of states', () => {
      // Arrange
      const state: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId,
        workflowId,
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      };
      triggerState.register(state);

      // Act
      const allStates = triggerState.getAllStates();
      allStates.delete('trigger-1');

      // Assert
      expect(triggerState.hasState('trigger-1')).toBe(true);
    });
  });

  describe('deleteState', () => {
    it('should delete trigger state', () => {
      // Arrange
      const state: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId,
        workflowId,
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      };
      triggerState.register(state);

      // Act
      triggerState.deleteState('trigger-1');

      // Assert
      expect(triggerState.hasState('trigger-1')).toBe(false);
      expect(triggerState.size()).toBe(0);
    });

    it('should throw error when deleting non-existent trigger', () => {
      // Act & Assert
      expect(() => triggerState.deleteState('non-existent')).toThrow(NotFoundError);
    });
  });

  describe('createSnapshot and restoreFromSnapshot', () => {
    it('should create snapshot', () => {
      // Arrange
      const state1: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId,
        workflowId,
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      };
      const state2: TriggerRuntimeState = {
        triggerId: 'trigger-2',
        executionId,
        workflowId,
        status: 'DISABLED',
        triggerCount: 5,
        updatedAt: Date.now(),
      };
      triggerState.register(state1);
      triggerState.register(state2);

      // Act
      const snapshot = triggerState.createSnapshot();

      // Assert
      expect(snapshot.size).toBe(2);
      expect(snapshot.has('trigger-1')).toBe(true);
      expect(snapshot.has('trigger-2')).toBe(true);
    });

    it('should restore from snapshot', () => {
      // Arrange
      const state: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId,
        workflowId,
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      };
      triggerState.register(state);
      const snapshot = triggerState.createSnapshot();
      
      triggerState.deleteState('trigger-1');

      // Act
      triggerState.restoreFromSnapshot(snapshot);

      // Assert
      expect(triggerState.hasState('trigger-1')).toBe(true);
      expect(triggerState.size()).toBe(1);
    });

    it('should throw error when restoring with mismatched executionId', () => {
      // Arrange
      const snapshot = new Map<string, TriggerRuntimeState>();
      snapshot.set('trigger-1', {
        triggerId: 'trigger-1',
        executionId: 'different-exec',
        workflowId,
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      });

      // Act & Assert
      expect(() => triggerState.restoreFromSnapshot(snapshot)).toThrow(RuntimeValidationError);
      expect(() => triggerState.restoreFromSnapshot(snapshot)).toThrow('Execution ID mismatch');
    });
  });

  describe('cleanup and reset', () => {
    it('should cleanup all states', () => {
      // Arrange
      const state: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId,
        workflowId,
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      };
      triggerState.register(state);

      // Act
      triggerState.cleanup();

      // Assert
      expect(triggerState.size()).toBe(0);
      expect(triggerState.isEmpty()).toBe(true);
    });

    it('should reset to initial state', () => {
      // Arrange
      const state: TriggerRuntimeState = {
        triggerId: 'trigger-1',
        executionId,
        workflowId,
        status: 'ENABLED',
        triggerCount: 0,
        updatedAt: Date.now(),
      };
      triggerState.register(state);

      // Act
      triggerState.reset();

      // Assert
      expect(triggerState.size()).toBe(0);
      expect(triggerState.isEmpty()).toBe(true);
    });
  });
});