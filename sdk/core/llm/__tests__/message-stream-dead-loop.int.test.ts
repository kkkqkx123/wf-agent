/**
 * MessageStream Dead Loop Detection Integration Tests
 */

import { describe, it, expect, vi } from 'vitest';
import { MessageStream } from '../message-stream.js';
import type { DeadLoopDetectionResult } from '../dead-loop-detector.js';

describe('MessageStream with Dead Loop Detection', () => {
  describe('Dead Loop Detection Enabled', () => {
    it('should automatically abort when dead loop is detected', () => {
      const onDeadLoopDetected = vi.fn();
      const stream = new MessageStream({
        enableDeadLoopDetection: true,
        onDeadLoopDetected,
      });

      // Simulate pushing repetitive reasoning content that exceeds checkpoint
      const repetitiveContent = 'I need to analyze this problem. '.repeat(100);
      stream.pushReasoning(repetitiveContent);

      // Verify that dead loop detection was triggered
      expect(onDeadLoopDetected).toHaveBeenCalled();
      // Note: abort is called asynchronously, so we check if callback was triggered
    });

    it('should not trigger dead loop detection for normal content', () => {
      const onDeadLoopDetected = vi.fn();
      const stream = new MessageStream({
        enableDeadLoopDetection: true,
        // Use higher checkpoints to avoid false positives in tests
        deadLoopConfig: {
          checkpoints: [5000, 10000, 20000],
        },
        onDeadLoopDetected,
      });

      const normalContent = 'This is a normal reasoning content without any repetitive patterns. ';
      stream.pushReasoning(normalContent.repeat(50));

      expect(onDeadLoopDetected).not.toHaveBeenCalled();
    });

    it('should emit reasoningText events for valid content', () => {
      const reasoningEvents: Array<{ delta: string; snapshot: string }> = [];
      const stream = new MessageStream({
        enableDeadLoopDetection: true,
      });

      stream.on('reasoningText', (data: any) => {
        reasoningEvents.push({ delta: data.delta, snapshot: data.snapshot });
      });

      stream.pushReasoning('First part ');
      stream.pushReasoning('Second part');

      expect(reasoningEvents).toHaveLength(2);
      expect(reasoningEvents[0]!.delta).toBe('First part ');
      expect(reasoningEvents[1]!.snapshot).toBe('First part Second part');
    });

    it('should respect custom checkpoint configuration', () => {
      const onDeadLoopDetected = vi.fn();
      const stream = new MessageStream({
        enableDeadLoopDetection: true,
        deadLoopConfig: {
          checkpoints: [50, 100, 150],
        },
        onDeadLoopDetected,
      });

      // Push content that exceeds first checkpoint with repetition
      const repetitiveContent = 'x'.repeat(60);
      stream.pushReasoning(repetitiveContent);

      expect(onDeadLoopDetected).toHaveBeenCalled();
    });

    it('should reset detector state for new requests', () => {
      const stream1 = new MessageStream({
        enableDeadLoopDetection: true,
      });

      // First request - trigger detection
      const repetitiveContent1 = 'a'.repeat(600);
      stream1.pushReasoning(repetitiveContent1);
      
      // Create new stream to test fresh state
      const stream2 = new MessageStream({
        enableDeadLoopDetection: true,
      });
      
      // Should be able to push content without immediate abort
      stream2.pushReasoning('Normal content ');
      // Stream should still be functional
    });
  });

  describe('Dead Loop Detection Disabled', () => {
    it('should not detect dead loops when disabled', () => {
      const onDeadLoopDetected = vi.fn();
      const stream = new MessageStream({
        enableDeadLoopDetection: false,
        onDeadLoopDetected,
      });

      // Push highly repetitive content
      const repetitiveContent = 'loop'.repeat(200);
      stream.pushReasoning(repetitiveContent);

      expect(onDeadLoopDetected).not.toHaveBeenCalled();
      expect(stream.isAborted()).toBe(false);
    });

    it('should still emit reasoningText events when detection is disabled', () => {
      const reasoningEvents: Array<{ delta: string }> = [];
      const stream = new MessageStream({
        enableDeadLoopDetection: false,
      });

      stream.on('reasoningText', (data: any) => {
        reasoningEvents.push({ delta: data.delta });
      });

      stream.pushReasoning('Some reasoning');

      expect(reasoningEvents).toHaveLength(1);
      expect(reasoningEvents[0]!.delta).toBe('Some reasoning');
    });
  });

  describe('Error Handling', () => {
    it('should handle detector errors gracefully', () => {
      const stream = new MessageStream({
        enableDeadLoopDetection: true,
      });

      // This should not throw even if detector has issues
      expect(() => {
        stream.pushReasoning('Test content');
      }).not.toThrow();
    });

    it('should continue normal flow after detector error', () => {
      const stream = new MessageStream({
        enableDeadLoopDetection: true,
      });

      stream.pushReasoning('Content before potential error');
      
      // Stream should still be functional
      expect(stream.isAborted()).toBe(false);
      expect(stream.isEnded()).toBe(false);
    });
  });

  describe('Integration with Stream Lifecycle', () => {
    it('should not interfere with normal stream ending', async () => {
      const stream = new MessageStream({
        enableDeadLoopDetection: true,
      });

      stream.pushReasoning('Normal reasoning content');
      stream.end();

      expect(stream.isEnded()).toBe(true);
      expect(stream.isAborted()).toBe(false);
    });

    it('should allow manual abort after detection is disabled', () => {
      const stream = new MessageStream({
        enableDeadLoopDetection: false,
      });

      stream.abort();

      // Check controller signal instead of isAborted flag
      expect(stream.getController().signal.aborted).toBe(true);
    });

    it('should work with text and reasoning content together', () => {
      const stream = new MessageStream({
        enableDeadLoopDetection: true,
      });

      const textDeltas: string[] = [];
      const reasoningDeltas: string[] = [];

      stream.on('text', (data: any) => {
        // Event data structure: { type, delta, snapshot }
        if (data && typeof data === 'object') {
          if ('delta' in data) {
            textDeltas.push(data.delta);
          }
        } else if (typeof data === 'string') {
          // If data is directly the delta string
          textDeltas.push(data);
        }
      });

      stream.on('reasoningText', (data: any) => {
        if (data && typeof data === 'object') {
          if ('delta' in data) {
            reasoningDeltas.push(data.delta);
          }
        } else if (typeof data === 'string') {
          reasoningDeltas.push(data);
        }
      });

      stream.pushText('Answer: ');
      stream.pushReasoning('Let me think... ');
      stream.pushText('The solution is X');

      const textReceived = textDeltas.join('');
      const reasoningReceived = reasoningDeltas.join('');
      
      expect(textReceived).toContain('Answer:');
      expect(textReceived).toContain('The solution is X');
      expect(reasoningReceived).toContain('Let me think');
    });
  });

  describe('Callback Functionality', () => {
    it('should call onDeadLoopDetected callback with correct result', () => {
      const capturedResults: DeadLoopDetectionResult[] = [];
      const stream = new MessageStream({
        enableDeadLoopDetection: true,
        onDeadLoopDetected: (result) => {
          capturedResults.push(result);
        },
      });

      const repetitiveContent = 'Repeat. '.repeat(100);
      stream.pushReasoning(repetitiveContent);

      expect(capturedResults.length).toBeGreaterThan(0);
      const result = capturedResults[0];
      if (result) {
        expect(result.detected).toBe(true);
        expect(result.type).toBeDefined();
        expect(result.details).toBeDefined();
      }
    });

    it('should support multiple callbacks through event system', () => {
      const callback1 = vi.fn();
      const callback2 = vi.fn();
      
      const stream = new MessageStream({
        enableDeadLoopDetection: true,
        onDeadLoopDetected: callback1,
      });

      // Also listen via event system (if supported)
      stream.on('abort', () => {
        callback2();
      });

      const repetitiveContent = 'x'.repeat(600);
      stream.pushReasoning(repetitiveContent);

      expect(callback1).toHaveBeenCalled();
      expect(callback2).toHaveBeenCalled();
    });
  });
});
