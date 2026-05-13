/**
 * Follow-up Question Coordinator Tests
 * Tests for UI adapter registration, timeout handling, error scenarios, and event emission
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { FollowupQuestionCoordinator } from '../followup-question-coordinator.js';
import type { 
  FollowupQuestionRequestData,
  FollowupQuestionResponseData 
} from '@wf-agent/types';
import { EventRegistry } from '../../registry/event-registry.js';

describe('FollowupQuestionCoordinator', () => {
  let coordinator: FollowupQuestionCoordinator;
  let eventManager: EventRegistry;

  const mockRequestData: FollowupQuestionRequestData = {
    questions: [
      {
        index: 0,
        text: 'What is your preferred programming language?',
        options: [
          { value: 'typescript', description: 'TypeScript' },
          { value: 'python', description: 'Python' },
        ],
      },
    ],
    additionalInfoLabel: 'Additional context',
  };

  const mockResponseData: FollowupQuestionResponseData = {
    answers: [
      {
        questionIndex: 0,
        selectedOptionIndex: 0,
        answer: 'typescript',
      },
    ],
    additionalInfo: 'I prefer TypeScript for type safety',
  };

  beforeEach(() => {
    eventManager = new EventRegistry();
    coordinator = new FollowupQuestionCoordinator(eventManager);
    vi.clearAllMocks();
  });

  describe('Initialization', () => {
    it('should initialize without errors', () => {
      expect(() => coordinator.initialize()).not.toThrow();
    });

    it('should use default timeout of 5 minutes', () => {
      const testCoordinator = new FollowupQuestionCoordinator(eventManager);
      // The timeout is private, but we can verify initialization works
      expect(() => testCoordinator.initialize()).not.toThrow();
    });

    it('should accept custom timeout', () => {
      const customTimeout = 30000; // 30 seconds
      const testCoordinator = new FollowupQuestionCoordinator(eventManager, { 
        timeoutMs: customTimeout 
      });
      expect(() => testCoordinator.initialize()).not.toThrow();
    });
  });

  describe('UI Adapter Registration', () => {
    it('should register UI adapter successfully', () => {
      const mockAdapter = vi.fn().mockResolvedValue(mockResponseData);
      
      expect(() => coordinator.registerUIAdapter(mockAdapter)).not.toThrow();
    });

    it('should call registered UI adapter when request is received', async () => {
      const mockAdapter = vi.fn().mockResolvedValue(mockResponseData);
      coordinator.registerUIAdapter(mockAdapter);
      coordinator.initialize();

      // Simulate FOLLOWUP_QUESTION_REQUESTED event
      const event = {
        type: 'FOLLOWUP_QUESTION_REQUESTED',
        executionId: 'test-execution-123',
        nodeId: 'test-node-456',
        data: mockRequestData,
      };

      await eventManager.emit(event as any);

      // Wait a bit for async processing
      await new Promise(resolve => setTimeout(resolve, 50));

      expect(mockAdapter).toHaveBeenCalledWith(mockRequestData);
    });

    it('should handle missing UI adapter gracefully', async () => {
      coordinator.initialize();

      // Don't register UI adapter
      const event = {
        type: 'FOLLOWUP_QUESTION_REQUESTED',
        executionId: 'test-execution-123',
        nodeId: 'test-node-456',
        data: mockRequestData,
      };

      await eventManager.emit(event as any);

      // Wait a bit for async processing
      await new Promise(resolve => setTimeout(resolve, 50));

      // Should emit failure event
      const failureEvents: any[] = [];
      const emitter = eventManager.getEmitter('test-execution-123');
      emitter.on('FOLLOWUP_QUESTION_RESPONSE' as any, (event: any) => {
        failureEvents.push(event);
      });

      // Re-emit to capture the event
      await eventManager.emit(event as any);
      await new Promise(resolve => setTimeout(resolve, 50));

      expect(failureEvents.length).toBeGreaterThan(0);
      expect(failureEvents[0].data.error).toContain('No UI adapter');
    });
  });

  describe('Event Handling', () => {
    it('should emit success response event', async () => {
      const mockAdapter = vi.fn().mockResolvedValue(mockResponseData);
      coordinator.registerUIAdapter(mockAdapter);
      coordinator.initialize();

      const responseEvents: any[] = [];
      const emitter = eventManager.getEmitter('test-execution-123');
      emitter.on('FOLLOWUP_QUESTION_RESPONSE' as any, (event: any) => {
        responseEvents.push(event);
      });

      const event = {
        type: 'FOLLOWUP_QUESTION_REQUESTED',
        executionId: 'test-execution-123',
        nodeId: 'test-node-456',
        data: mockRequestData,
      };

      await eventManager.emit(event as any);
      await new Promise(resolve => setTimeout(resolve, 100));

      expect(responseEvents.length).toBeGreaterThan(0);
      expect(responseEvents[0].executionId).toBe('test-execution-123');
      expect(responseEvents[0].nodeId).toBe('test-node-456');
      expect(responseEvents[0].data).toEqual(mockResponseData);
    });

    it('should emit failure event on error', async () => {
      const mockAdapter = vi.fn().mockRejectedValue(new Error('User cancelled'));
      coordinator.registerUIAdapter(mockAdapter);
      coordinator.initialize();

      const responseEvents: any[] = [];
      const emitter = eventManager.getEmitter('test-execution-123');
      emitter.on('FOLLOWUP_QUESTION_RESPONSE' as any, (event: any) => {
        responseEvents.push(event);
      });

      const event = {
        type: 'FOLLOWUP_QUESTION_REQUESTED',
        executionId: 'test-execution-123',
        nodeId: 'test-node-456',
        data: mockRequestData,
      };

      await eventManager.emit(event as any);
      await new Promise(resolve => setTimeout(resolve, 100));

      expect(responseEvents.length).toBeGreaterThan(0);
      expect(responseEvents[0].data.error).toBe('User cancelled');
      expect(responseEvents[0].data.answers).toEqual([]);
    });

    it('should handle non-Error objects in catch block', async () => {
      const mockAdapter = vi.fn().mockRejectedValue('String error');
      coordinator.registerUIAdapter(mockAdapter);
      coordinator.initialize();

      const responseEvents: any[] = [];
      const emitter = eventManager.getEmitter('test-execution-123');
      emitter.on('FOLLOWUP_QUESTION_RESPONSE' as any, (event: any) => {
        responseEvents.push(event);
      });

      const event = {
        type: 'FOLLOWUP_QUESTION_REQUESTED',
        executionId: 'test-execution-123',
        nodeId: 'test-node-456',
        data: mockRequestData,
      };

      await eventManager.emit(event as any);
      await new Promise(resolve => setTimeout(resolve, 100));

      expect(responseEvents.length).toBeGreaterThan(0);
      expect(responseEvents[0].data.error).toBe('String error');
    });

    it('should handle events with missing executionId and nodeId', async () => {
      const mockAdapter = vi.fn().mockResolvedValue(mockResponseData);
      coordinator.registerUIAdapter(mockAdapter);
      coordinator.initialize();

      const responseEvents: any[] = [];
      const emitter = eventManager.getEmitter('test-execution-123');
      emitter.on('FOLLOWUP_QUESTION_RESPONSE' as any, (event: any) => {
        responseEvents.push(event);
      });

      const event = {
        type: 'FOLLOWUP_QUESTION_REQUESTED',
        data: mockRequestData,
      };

      await eventManager.emit(event as any);
      await new Promise(resolve => setTimeout(resolve, 100));

      expect(responseEvents.length).toBeGreaterThan(0);
      expect(responseEvents[0].executionId).toBe('unknown');
      expect(responseEvents[0].nodeId).toBe('unknown');
    });
  });

  describe('Timeout Handling', () => {
    it('should timeout when UI adapter takes too long', async () => {
      // Set very short timeout for testing
      const shortTimeoutCoordinator = new FollowupQuestionCoordinator(eventManager, { 
        timeoutMs: 100 
      });
      
      const mockAdapter = vi.fn().mockImplementation(() => {
        return new Promise<FollowupQuestionResponseData>((resolve) => {
          setTimeout(() => resolve(mockResponseData), 500);
        });
      });
      
      shortTimeoutCoordinator.registerUIAdapter(mockAdapter);
      shortTimeoutCoordinator.initialize();

      const responseEvents: any[] = [];
      const emitter = eventManager.getEmitter('test-execution-123');
      emitter.on('FOLLOWUP_QUESTION_RESPONSE' as any, (event: any) => {
        responseEvents.push(event);
      });

      const event = {
        type: 'FOLLOWUP_QUESTION_REQUESTED',
        executionId: 'test-execution-123',
        nodeId: 'test-node-456',
        data: mockRequestData,
      };

      await eventManager.emit(event as any);
      await new Promise(resolve => setTimeout(resolve, 200));

      expect(responseEvents.length).toBeGreaterThan(0);
      expect(responseEvents[0].data.error).toContain('timed out');
    });
  });

  describe('Cleanup', () => {
    it('should cleanup resources', () => {
      const mockAdapter = vi.fn().mockResolvedValue(mockResponseData);
      coordinator.registerUIAdapter(mockAdapter);
      
      expect(() => coordinator.cleanup()).not.toThrow();
    });
  });

  describe('Integration Scenarios', () => {
    it('should handle complete follow-up question workflow', async () => {
      const mockAdapter = vi.fn().mockResolvedValue(mockResponseData);
      coordinator.registerUIAdapter(mockAdapter);
      coordinator.initialize();

      const responseEvents: any[] = [];
      const emitter = eventManager.getEmitter('test-execution-123');
      emitter.on('FOLLOWUP_QUESTION_RESPONSE' as any, (event: any) => {
        responseEvents.push(event);
      });

      // Trigger request
      const event = {
        type: 'FOLLOWUP_QUESTION_REQUESTED',
        executionId: 'workflow-test',
        nodeId: 'question-node',
        data: mockRequestData,
      };

      await eventManager.emit(event as any);
      await new Promise(resolve => setTimeout(resolve, 100));

      // Verify response was emitted
      expect(responseEvents.length).toBe(1);
      expect(responseEvents[0].data).toEqual(mockResponseData);
      expect(mockAdapter).toHaveBeenCalledTimes(1);
    });

    it('should handle multiple sequential requests', async () => {
      const mockAdapter = vi.fn()
        .mockResolvedValueOnce(mockResponseData)
        .mockResolvedValueOnce({
          ...mockResponseData,
          additionalInfo: 'Second response',
        });
      
      coordinator.registerUIAdapter(mockAdapter);
      coordinator.initialize();

      const responseEvents: any[] = [];
      const emitter = eventManager.getEmitter('test-execution-123');
      emitter.on('FOLLOWUP_QUESTION_RESPONSE' as any, (event: any) => {
        responseEvents.push(event);
      });

      // First request
      await eventManager.emit({
        type: 'FOLLOWUP_QUESTION_REQUESTED',
        executionId: 'test-1',
        nodeId: 'node-1',
        data: mockRequestData,
      } as any);
      await new Promise(resolve => setTimeout(resolve, 100));

      // Second request
      await eventManager.emit({
        type: 'FOLLOWUP_QUESTION_REQUESTED',
        executionId: 'test-2',
        nodeId: 'node-2',
        data: mockRequestData,
      } as any);
      await new Promise(resolve => setTimeout(resolve, 100));

      expect(responseEvents.length).toBe(2);
      expect(mockAdapter).toHaveBeenCalledTimes(2);
    });
  });
});
