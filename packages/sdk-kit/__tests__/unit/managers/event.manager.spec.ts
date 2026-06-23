/**
 * Event Manager Tests
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  EventManager,
  ExecutionEventType,
  type ExecutionEventPayload,
} from '@/managers/event.manager.js';

describe('EventManager', () => {
  let manager: EventManager;
  const mockPayload: ExecutionEventPayload = {
    executionId: 'exec-123',
    workflowId: 'wf-123',
    timestamp: Date.now(),
    type: ExecutionEventType.EXECUTION_START,
  };

  beforeEach(() => {
    manager = new EventManager();
  });

  describe('emit', () => {
    it('should emit and record events', () => {
      const handler = vi.fn();
      manager.subscribe(ExecutionEventType.EXECUTION_START, handler);

      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);

      expect(handler).toHaveBeenCalledWith(mockPayload);
    });

    it('should add events to history', () => {
      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);

      const history = manager.getHistory();
      expect(history).toHaveLength(1);
      expect(history[0]).toEqual(mockPayload);
    });

    it('should emit wildcard events', () => {
      const handler = vi.fn();
      manager.subscribeAll(handler);

      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);

      expect(handler).toHaveBeenCalled();
    });
  });

  describe('subscribe', () => {
    it('should subscribe to single event type', () => {
      const handler = vi.fn();
      manager.subscribe(ExecutionEventType.EXECUTION_START, handler);

      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      manager.emit(ExecutionEventType.EXECUTION_COMPLETED, mockPayload);

      expect(handler).toHaveBeenCalledTimes(1);
    });

    it('should subscribe to multiple event types', () => {
      const handler = vi.fn();
      manager.subscribe(
        [ExecutionEventType.EXECUTION_START, ExecutionEventType.EXECUTION_COMPLETED],
        handler
      );

      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      manager.emit(ExecutionEventType.EXECUTION_COMPLETED, mockPayload);
      manager.emit(ExecutionEventType.EXECUTION_FAILED, mockPayload);

      expect(handler).toHaveBeenCalledTimes(2);
    });

    it('should return unsubscribe function', () => {
      const handler = vi.fn();
      const unsubscribe = manager.subscribe(ExecutionEventType.EXECUTION_START, handler);

      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      expect(handler).toHaveBeenCalledTimes(1);

      unsubscribe();
      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      expect(handler).toHaveBeenCalledTimes(1);
    });

    it('should handle errors in handler gracefully', () => {
      const successHandler = vi.fn();
      const errorHandler = vi.fn(() => {
        throw new Error('Handler error');
      });

      manager.subscribe(ExecutionEventType.EXECUTION_START, successHandler);
      manager.subscribe(ExecutionEventType.EXECUTION_COMPLETED, errorHandler);

      // Emit success event - should work fine
      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      expect(successHandler).toHaveBeenCalled();

      // Emit error event - handler will throw but that's expected
      expect(() => {
        manager.emit(ExecutionEventType.EXECUTION_COMPLETED, {
          ...mockPayload,
          type: ExecutionEventType.EXECUTION_COMPLETED,
        });
      }).toThrow();

      expect(errorHandler).toHaveBeenCalled();
    });
  });

  describe('subscribeOnce', () => {
    it('should subscribe once and auto-unsubscribe', () => {
      const handler = vi.fn();
      manager.subscribeOnce(ExecutionEventType.EXECUTION_START, handler);

      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);

      expect(handler).toHaveBeenCalledTimes(1);
    });
  });

  describe('subscribeAll', () => {
    it('should subscribe to all events', () => {
      const handler = vi.fn();
      manager.subscribeAll(handler);

      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      manager.emit(ExecutionEventType.NODE_COMPLETED, mockPayload);
      manager.emit(ExecutionEventType.EXECUTION_FAILED, mockPayload);

      expect(handler).toHaveBeenCalledTimes(3);
    });

    it('should return unsubscribe function', () => {
      const handler = vi.fn();
      const unsubscribe = manager.subscribeAll(handler);

      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      expect(handler).toHaveBeenCalledTimes(1);

      unsubscribe();
      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      expect(handler).toHaveBeenCalledTimes(1);
    });
  });

  describe('getHistory', () => {
    it('should return all events by default', () => {
      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      manager.emit(ExecutionEventType.NODE_COMPLETED, mockPayload);

      const history = manager.getHistory();
      expect(history).toHaveLength(2);
    });

    it('should filter by execution ID', () => {
      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      manager.emit(ExecutionEventType.EXECUTION_START, {
        ...mockPayload,
        executionId: 'exec-456',
      });

      const history = manager.getHistory({ executionId: 'exec-123' });
      expect(history).toHaveLength(1);
      expect(history[0].executionId).toBe('exec-123');
    });

    it('should filter by workflow ID', () => {
      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      manager.emit(ExecutionEventType.EXECUTION_START, {
        ...mockPayload,
        workflowId: 'wf-456',
      });

      const history = manager.getHistory({ workflowId: 'wf-123' });
      expect(history).toHaveLength(1);
      expect(history[0].workflowId).toBe('wf-123');
    });

    it('should filter by event type', () => {
      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      manager.emit(ExecutionEventType.EXECUTION_COMPLETED, {
        ...mockPayload,
        type: ExecutionEventType.EXECUTION_COMPLETED,
      });

      const history = manager.getHistory({ type: ExecutionEventType.EXECUTION_START });
      expect(history).toHaveLength(1);
      expect(history[0].type).toBe(ExecutionEventType.EXECUTION_START);
    });

    it('should filter by multiple event types', () => {
      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      manager.emit(ExecutionEventType.EXECUTION_COMPLETED, {
        ...mockPayload,
        type: ExecutionEventType.EXECUTION_COMPLETED,
      });
      manager.emit(ExecutionEventType.EXECUTION_FAILED, {
        ...mockPayload,
        type: ExecutionEventType.EXECUTION_FAILED,
      });

      const history = manager.getHistory({
        type: [ExecutionEventType.EXECUTION_START, ExecutionEventType.EXECUTION_COMPLETED],
      });
      expect(history).toHaveLength(2);
    });

    it('should filter by time range', () => {
      const manager2 = new EventManager();
      const now = Date.now();
      manager2.emit(ExecutionEventType.EXECUTION_START, {
        ...mockPayload,
        timestamp: now - 1000,
      });
      manager2.emit(ExecutionEventType.EXECUTION_COMPLETED, {
        ...mockPayload,
        type: ExecutionEventType.EXECUTION_COMPLETED,
        timestamp: now,
      });

      const history = manager2.getHistory({ startTime: now - 500 });
      expect(history).toHaveLength(1);
      expect(history[0].type).toBe(ExecutionEventType.EXECUTION_COMPLETED);
    });
  });

  describe('getExecutionEvents', () => {
    it('should get events for specific execution', () => {
      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      manager.emit(ExecutionEventType.EXECUTION_START, {
        ...mockPayload,
        executionId: 'exec-456',
      });

      const events = manager.getExecutionEvents('exec-123');
      expect(events).toHaveLength(1);
      expect(events[0].executionId).toBe('exec-123');
    });
  });

  describe('getWorkflowEvents', () => {
    it('should get events for specific workflow', () => {
      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      manager.emit(ExecutionEventType.EXECUTION_START, {
        ...mockPayload,
        workflowId: 'wf-456',
      });

      const events = manager.getWorkflowEvents('wf-123');
      expect(events).toHaveLength(1);
      expect(events[0].workflowId).toBe('wf-123');
    });
  });

  describe('getEventsByType', () => {
    it('should get events by type', () => {
      const manager2 = new EventManager();
      manager2.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      manager2.emit(ExecutionEventType.EXECUTION_COMPLETED, {
        ...mockPayload,
        type: ExecutionEventType.EXECUTION_COMPLETED,
      });

      const events = manager2.getEventsByType(ExecutionEventType.EXECUTION_START);
      expect(events).toHaveLength(1);
      expect(events[0].type).toBe(ExecutionEventType.EXECUTION_START);
    });

    it('should get events by multiple types', () => {
      const manager2 = new EventManager();
      manager2.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      manager2.emit(ExecutionEventType.EXECUTION_COMPLETED, {
        ...mockPayload,
        type: ExecutionEventType.EXECUTION_COMPLETED,
      });
      manager2.emit(ExecutionEventType.EXECUTION_FAILED, {
        ...mockPayload,
        type: ExecutionEventType.EXECUTION_FAILED,
      });

      const events = manager2.getEventsByType([
        ExecutionEventType.EXECUTION_START,
        ExecutionEventType.EXECUTION_COMPLETED,
      ]);
      expect(events).toHaveLength(2);
    });
  });

  describe('getEventCount', () => {
    it('should return total event count', () => {
      const manager2 = new EventManager();
      manager2.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      manager2.emit(ExecutionEventType.EXECUTION_COMPLETED, {
        ...mockPayload,
        type: ExecutionEventType.EXECUTION_COMPLETED,
      });

      expect(manager2.getEventCount()).toBe(2);
    });

    it('should return filtered event count', () => {
      const manager2 = new EventManager();
      manager2.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      manager2.emit(ExecutionEventType.EXECUTION_COMPLETED, {
        ...mockPayload,
        type: ExecutionEventType.EXECUTION_COMPLETED,
      });

      expect(
        manager2.getEventCount({ type: ExecutionEventType.EXECUTION_START })
      ).toBe(1);
    });
  });

  describe('clear', () => {
    it('should clear all history', () => {
      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      manager.emit(ExecutionEventType.EXECUTION_COMPLETED, mockPayload);

      expect(manager.getEventCount()).toBe(2);

      manager.clear();

      expect(manager.getEventCount()).toBe(0);
    });
  });

  describe('clearExecution', () => {
    it('should clear events for specific execution', () => {
      manager.emit(ExecutionEventType.EXECUTION_START, mockPayload);
      manager.emit(ExecutionEventType.EXECUTION_START, {
        ...mockPayload,
        executionId: 'exec-456',
      });

      manager.clearExecution('exec-123');

      expect(manager.getEventCount()).toBe(1);
      expect(manager.getExecutionEvents('exec-456')).toHaveLength(1);
    });
  });

  describe('history management', () => {
    it('should trim history when exceeding max size', () => {
      const manager2 = new EventManager();
      manager2.setMaxHistorySize(15);

      for (let i = 0; i < 20; i++) {
        manager2.emit(ExecutionEventType.EXECUTION_START, {
          ...mockPayload,
          timestamp: Date.now() + i,
        });
      }

      expect(manager2.getHistorySize()).toBe(15);
    });

    it('should set max history size', () => {
      const manager2 = new EventManager();
      manager2.setMaxHistorySize(1000);
      expect(manager2.getHistorySize()).toBe(0);
    });

    it('should throw on invalid max size', () => {
      const manager2 = new EventManager();
      expect(() => manager2.setMaxHistorySize(5)).toThrow('at least 10');
    });
  });
});
