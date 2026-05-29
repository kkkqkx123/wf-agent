/**
 * Message Array Manager Tests
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { MessageArrayManager } from '../message-array-manager.js';
import type { Message, LLMMessage } from '@wf-agent/types';

describe('MessageArrayManager', () => {
  let manager: MessageArrayManager;

  beforeEach(() => {
    manager = new MessageArrayManager();
  });

  describe('constructor', () => {
    it('should initialize with empty messages', () => {
      const state = manager.getState();
      expect(state.messages).toEqual([]);
      expect(state.currentBatchIndex).toBe(0);
      expect(state.totalMessageCount).toBe(0);
    });

    it('should initialize with initial messages', () => {
      const initialMessages: Message[] = [
        { role: 'user', content: 'Hello' },
        { role: 'assistant', content: 'Hi' },
      ];
      manager = new MessageArrayManager(initialMessages);
      const state = manager.getState();
      expect(state.messages).toHaveLength(2);
      expect(state.messages[0]!.content).toBe('Hello');
      expect(state.messages[1]!.content).toBe('Hi');
      expect(state.totalMessageCount).toBe(2);
    });
  });

  describe('execute - APPEND', () => {
    it('should append messages to empty array', () => {
      const result = manager.execute({
        operation: 'APPEND',
        messages: [{ role: 'user', content: 'Hello' }],
      });
      expect(result.messages).toHaveLength(1);
      expect(result.messages[0]!.content).toBe('Hello');
      expect(result.stats.visibleMessageCount).toBe(1);
    });

    it('should append messages multiple times', () => {
      manager.execute({
        operation: 'APPEND',
        messages: [{ role: 'user', content: 'First' }],
      });
      const result = manager.execute({
        operation: 'APPEND',
        messages: [{ role: 'assistant', content: 'Second' }],
      });
      expect(result.messages).toHaveLength(2);
      expect(result.messages[0]!.content).toBe('First');
      expect(result.messages[1]!.content).toBe('Second');
    });

    it('should not create new batch on append', () => {
      manager.execute({
        operation: 'APPEND',
        messages: [{ role: 'user', content: 'A' }],
      });
      const stateAfterAppend = manager.getState();
      expect(stateAfterAppend.currentBatchIndex).toBe(0);
    });
  });

  describe('execute - INSERT', () => {
    it('should insert messages at position 0', () => {
      manager = new MessageArrayManager([
        { role: 'user', content: 'World' },
      ]);
      const result = manager.execute({
        operation: 'INSERT',
        messages: [{ role: 'user', content: 'Hello' }],
        position: 0,
      });
      expect(result.messages).toHaveLength(2);
      expect(result.messages[0]!.content).toBe('Hello');
      expect(result.messages[1]!.content).toBe('World');
    });

    it('should insert messages at the end', () => {
      manager = new MessageArrayManager([
        { role: 'user', content: 'Hello' },
      ]);
      const result = manager.execute({
        operation: 'INSERT',
        messages: [{ role: 'assistant', content: 'World' }],
        position: 1,
      });
      expect(result.messages).toHaveLength(2);
      expect(result.messages[0]!.content).toBe('Hello');
      expect(result.messages[1]!.content).toBe('World');
    });

    it('should create a new batch on insert', () => {
      manager = new MessageArrayManager([
        { role: 'user', content: 'Hello' },
      ]);
      const stateBefore = manager.getState();
      expect(stateBefore.currentBatchIndex).toBe(0);

      manager.execute({
        operation: 'INSERT',
        messages: [{ role: 'assistant', content: 'World' }],
        position: 1,
      });
      const stateAfter = manager.getState();
      expect(stateAfter.currentBatchIndex).toBe(1);
      expect(stateAfter.batchSnapshots).toHaveLength(1);
    });

    it('should throw error for invalid position (negative)', () => {
      expect(() => {
        manager.execute({
          operation: 'INSERT',
          messages: [{ role: 'user', content: 'X' }],
          position: -1,
        });
      }).toThrow('Invalid insert position');
    });

    it('should throw error for invalid position (too large)', () => {
      expect(() => {
        manager.execute({
          operation: 'INSERT',
          messages: [{ role: 'user', content: 'X' }],
          position: 5,
        });
      }).toThrow('Invalid insert position');
    });
  });

  describe('execute - REPLACE', () => {
    it('should replace message at valid index', () => {
      manager = new MessageArrayManager([
        { role: 'user', content: 'Original' },
      ]);
      const result = manager.execute({
        operation: 'REPLACE',
        message: { role: 'user', content: 'Replaced' },
        index: 0,
      });
      expect(result.messages).toHaveLength(1);
      expect(result.messages[0]!.content).toBe('Replaced');
    });

    it('should create a new batch on replace', () => {
      manager = new MessageArrayManager([
        { role: 'user', content: 'A' },
      ]);
      manager.execute({
        operation: 'REPLACE',
        message: { role: 'user', content: 'B' },
        index: 0,
      });
      expect(manager.getState().currentBatchIndex).toBe(1);
    });

    it('should throw error for invalid index (negative)', () => {
      expect(() => {
        manager.execute({
          operation: 'REPLACE',
          message: { role: 'user', content: 'X' },
          index: -1,
        });
      }).toThrow('Invalid replace index');
    });

    it('should throw error for out of range index', () => {
      expect(() => {
        manager.execute({
          operation: 'REPLACE',
          message: { role: 'user', content: 'X' },
          index: 0,
        });
      }).toThrow('Invalid replace index');
    });
  });

  describe('execute - TRUNCATE', () => {
    it('should truncate from the beginning', () => {
      manager = new MessageArrayManager([
        { role: 'user', content: 'A' },
        { role: 'user', content: 'B' },
        { role: 'user', content: 'C' },
      ]);
      const result = manager.execute({
        operation: 'TRUNCATE',
        strategy: { type: 'KEEP_LAST', count: 2 },
      });
      expect(result.messages).toHaveLength(2);
      expect(result.messages[0]!.content).toBe('B');
      expect(result.messages[1]!.content).toBe('C');
    });

    it('should truncate from the end', () => {
      manager = new MessageArrayManager([
        { role: 'user', content: 'A' },
        { role: 'user', content: 'B' },
        { role: 'user', content: 'C' },
      ]);
      const result = manager.execute({
        operation: 'TRUNCATE',
        strategy: { type: 'KEEP_FIRST', count: 2 },
      });
      expect(result.messages).toHaveLength(2);
      expect(result.messages[0]!.content).toBe('A');
      expect(result.messages[1]!.content).toBe('B');
    });

    it('should create a new batch on truncate', () => {
      manager = new MessageArrayManager([
        { role: 'user', content: 'A' },
        { role: 'user', content: 'B' },
      ]);
      manager.execute({
        operation: 'TRUNCATE',
        strategy: { type: 'KEEP_FIRST', count: 1 },
      });
      expect(manager.getState().currentBatchIndex).toBe(1);
    });
  });

  describe('execute - CLEAR', () => {
    it('should clear all messages', () => {
      manager = new MessageArrayManager([
        { role: 'user', content: 'A' },
        { role: 'assistant', content: 'B' },
      ]);
      const result = manager.execute({ operation: 'CLEAR' });
      expect(result.messages).toHaveLength(0);
    });

    it('should create a new batch on clear', () => {
      manager = new MessageArrayManager([
        { role: 'user', content: 'A' },
      ]);
      manager.execute({ operation: 'CLEAR' });
      expect(manager.getState().currentBatchIndex).toBe(1);
    });

    it('should clear empty array', () => {
      const result = manager.execute({ operation: 'CLEAR' });
      expect(result.messages).toHaveLength(0);
      expect(manager.getState().currentBatchIndex).toBe(1);
    });
  });

  describe('execute - FILTER', () => {
    it('should filter messages by role', () => {
      manager = new MessageArrayManager([
        { role: 'user', content: 'A' },
        { role: 'assistant', content: 'B' },
        { role: 'system', content: 'C' },
        { role: 'user', content: 'D' },
      ]);
      const result = manager.execute({
        operation: 'FILTER',
        roles: ['user'],
      });
      expect(result.messages).toHaveLength(2);
      expect(result.messages[0]!.content).toBe('A');
      expect(result.messages[1]!.content).toBe('D');
    });

    it('should filter by multiple roles', () => {
      manager = new MessageArrayManager([
        { role: 'user', content: 'A' },
        { role: 'tool', toolCallId: 't1', content: 'R1' } as LLMMessage,
        { role: 'assistant', content: 'B' },
      ]);
      const result = manager.execute({
        operation: 'FILTER',
        roles: ['user', 'assistant'],
      });
      expect(result.messages).toHaveLength(2);
    });
  });

  describe('execute - ROLLBACK', () => {
    it('should rollback to initial state (batch 0)', () => {
      manager = new MessageArrayManager([
        { role: 'user', content: 'Initial' },
      ]);
      manager.execute({
        operation: 'INSERT',
        messages: [{ role: 'assistant', content: 'Added' }],
        position: 1,
      });
      expect(manager.getCurrentMessages()).toHaveLength(2);

      const result = manager.execute({
        operation: 'ROLLBACK',
        targetBatchIndex: 0,
      });
      expect(result.messages).toHaveLength(1);
      expect(result.messages[0]!.content).toBe('Initial');
    });

    it('should throw error for invalid batch index', () => {
      expect(() => {
        manager.execute({
          operation: 'ROLLBACK',
          targetBatchIndex: 5,
        });
      }).toThrow('Invalid batch index');
    });
  });

  describe('execute - unsupported operation', () => {
    it('should throw error for unsupported operation type', () => {
      expect(() => {
        (manager as any).execute({ operation: 'UNKNOWN' });
      }).toThrow('Unsupported operation type');
    });
  });

  describe('getCurrentMessages', () => {
    it('should return a copy of current messages', () => {
      manager = new MessageArrayManager([
        { role: 'user', content: 'Test' },
      ]);
      const messages = manager.getCurrentMessages();
      expect(messages).toHaveLength(1);
      // Verify it's a copy
      (messages[0] as any).content = 'Modified';
      const messagesAgain = manager.getCurrentMessages();
      expect(messagesAgain[0]!.content).toBe('Test');
    });
  });

  describe('getStats', () => {
    it('should return correct stats for empty manager', () => {
      const stats = manager.getStats();
      expect(stats.totalMessages).toBe(0);
      expect(stats.totalBatches).toBe(1);
      expect(stats.currentBatchIndex).toBe(0);
    });

    it('should return correct stats after insert', () => {
      manager = new MessageArrayManager([
        { role: 'user', content: 'A' },
      ]);
      manager.execute({
        operation: 'INSERT',
        messages: [{ role: 'assistant', content: 'B' }],
        position: 1,
      });
      const stats = manager.getStats();
      expect(stats.totalMessages).toBe(2);
      expect(stats.totalBatches).toBe(2);
      expect(stats.currentBatchIndex).toBe(1);
    });
  });

  describe('rollback', () => {
    it('should rollback via convenience method', () => {
      manager = new MessageArrayManager([
        { role: 'user', content: 'A' },
      ]);
      manager.execute({
        operation: 'INSERT',
        messages: [{ role: 'assistant', content: 'B' }],
        position: 1,
      });
      const result = manager.rollback(0);
      expect(result.messages).toHaveLength(1);
    });
  });

  describe('getBatchSnapshot', () => {
    it('should return null for non-existent batch', () => {
      const snapshot = manager.getBatchSnapshot(0);
      expect(snapshot).toBeNull();
    });

    it('should return snapshot after batch operation', () => {
      manager = new MessageArrayManager([
        { role: 'user', content: 'A' },
      ]);
      manager.execute({
        operation: 'INSERT',
        messages: [{ role: 'assistant', content: 'B' }],
        position: 1,
      });
      const snapshot = manager.getBatchSnapshot(0);
      expect(snapshot).not.toBeNull();
      expect(snapshot!.batchIndex).toBe(0);
      expect(snapshot!.messages).toHaveLength(1);
    });
  });
});
