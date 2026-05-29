/**
 * Message History Tests
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { MessageHistory } from '../message-history.js';
import type { LLMMessage, LLMToolCall } from '@wf-agent/types';

describe('MessageHistory', () => {
  let history: MessageHistory;

  beforeEach(() => {
    history = new MessageHistory();
  });

  describe('constructor', () => {
    it('should initialize with empty messages', () => {
      expect(history.getAllMessages()).toEqual([]);
      expect(history.getMessageCount()).toBe(0);
      expect(history.getTotalMessageCount()).toBe(0);
    });

    it('should initialize with initial messages', () => {
      const initialMessages: LLMMessage[] = [
        { role: 'system', content: 'You are a helpful assistant.' },
        { role: 'user', content: 'Hello' },
      ];
      history = new MessageHistory({ initialMessages });
      expect(history.getAllMessages()).toHaveLength(2);
      expect(history.getTotalMessageCount()).toBe(2);
    });

    it('should not share reference with initial messages', () => {
      const initialMessages: LLMMessage[] = [
        { role: 'user', content: 'Hello' },
      ];
      history = new MessageHistory({ initialMessages });
      initialMessages[0]!.content = 'Modified';
      expect(history.getMessages()[0]!.content).toBe('Hello');
    });
  });

  describe('addMessage', () => {
    it('should add a single message', () => {
      const result = history.addMessage({ role: 'user', content: 'Hello' });
      expect(result).toBe(1);
      expect(history.getTotalMessageCount()).toBe(1);
    });

    it('should add multiple messages sequentially', () => {
      history.addMessage({ role: 'user', content: 'Hello' });
      history.addMessage({ role: 'assistant', content: 'Hi' });
      expect(history.getTotalMessageCount()).toBe(2);
    });

    it('should not share reference with added message', () => {
      const msg: LLMMessage = { role: 'user', content: 'Test' };
      history.addMessage(msg);
      msg.content = 'Modified';
      expect(history.getMessages()[0]!.content).toBe('Test');
    });
  });

  describe('addMessages', () => {
    it('should add multiple messages at once', () => {
      const result = history.addMessages(
        { role: 'user', content: 'Q1' },
        { role: 'assistant', content: 'A1' },
      );
      expect(result).toBe(2);
      expect(history.getTotalMessageCount()).toBe(2);
    });

    it('should handle empty spread', () => {
      const result = history.addMessages();
      expect(result).toBe(0);
    });
  });

  describe('getMessages', () => {
    it('should return visible messages', () => {
      history.addMessage({ role: 'user', content: 'Hello' });
      const messages = history.getMessages();
      expect(messages).toHaveLength(1);
      expect(messages[0]!.content).toBe('Hello');
    });

    it('should return a copy of messages', () => {
      history.addMessage({ role: 'user', content: 'Test' });
      const messages = history.getMessages();
      (messages[0] as any).content = 'Modified';
      expect(history.getMessages()[0]!.content).toBe('Test');
    });
  });

  describe('getAllMessages', () => {
    it('should return all messages including invisible ones', () => {
      history.addMessage({ role: 'user', content: 'A' });
      history.addMessage({ role: 'user', content: 'B' });
      history.startNewBatch(1);
      const allMessages = history.getAllMessages();
      expect(allMessages).toHaveLength(2);
    });

    it('should return a copy', () => {
      history.addMessage({ role: 'user', content: 'Test' });
      const all = history.getAllMessages();
      (all[0] as any).content = 'Modified';
      expect(history.getAllMessages()[0]!.content).toBe('Test');
    });
  });

  describe('getMessageCount / getTotalMessageCount', () => {
    it('should return correct counts before batch boundary', () => {
      history.addMessage({ role: 'user', content: 'A' });
      history.addMessage({ role: 'user', content: 'B' });
      // Visible count = total count before any batch boundary
      expect(history.getMessageCount()).toBe(2);
      expect(history.getTotalMessageCount()).toBe(2);
    });

    it('should return different counts after batch boundary', () => {
      history.addMessage({ role: 'user', content: 'A' });
      history.addMessage({ role: 'user', content: 'B' });
      history.addMessage({ role: 'user', content: 'C' });

      // Mark first message as boundary (hidden)
      history.startNewBatch(1);
      expect(history.getMessageCount()).toBe(2); // visible: B, C
      expect(history.getTotalMessageCount()).toBe(3); // total: A, B, C
    });
  });

  describe('clear', () => {
    it('should clear all messages', () => {
      history.addMessage({ role: 'user', content: 'Hello' });
      history.clear();
      expect(history.getAllMessages()).toEqual([]);
      expect(history.getMessageCount()).toBe(0);
    });

    it('should reset markMap', () => {
      history.addMessage({ role: 'user', content: 'A' });
      history.startNewBatch(0);
      history.clear();
      expect(history.getCurrentBatch()).toBe(0);
    });
  });

  describe('startNewBatch', () => {
    it('should create a new batch', () => {
      history.addMessage({ role: 'user', content: 'A' });
      const newBatch = history.startNewBatch();
      expect(newBatch).toBe(1);
      expect(history.getCurrentBatch()).toBe(1);
    });

    it('should mark previous messages as invisible', () => {
      history.addMessage({ role: 'user', content: 'Old' });
      history.startNewBatch(); // boundary at end of messages (index 1), making 'Old' invisible
      expect(history.getMessageCount()).toBe(0);
    });

    it('should use specified boundary index', () => {
      history.addMessage({ role: 'user', content: 'A' });
      history.addMessage({ role: 'user', content: 'B' });
      history.startNewBatch(1);
      expect(history.getMessageCount()).toBe(1); // only B visible
      expect(history.getCurrentBoundary()).toBe(1);
    });
  });

  describe('rollbackToBatch', () => {
    it('should rollback to a previous batch', () => {
      history.addMessage({ role: 'user', content: 'First' });
      history.startNewBatch();
      history.addMessage({ role: 'user', content: 'Second' });
      expect(history.getMessageCount()).toBe(1);

      history.rollbackToBatch(0);
      expect(history.getCurrentBatch()).toBe(0);
      // After rollbackToBatch(0), boundary is at index 0, so all 2 messages are visible
      expect(history.getMessageCount()).toBe(2);
      expect(history.getMessages()[0]!.content).toBe('First');
    });
  });

  describe('getCurrentBatch', () => {
    it('should return 0 initially', () => {
      expect(history.getCurrentBatch()).toBe(0);
    });

    it('should return updated batch after startNewBatch', () => {
      history.startNewBatch();
      expect(history.getCurrentBatch()).toBe(1);
    });
  });

  describe('getBatchInfo / getAllBatchesInfo', () => {
    it('should return batch info', () => {
      history.addMessage({ role: 'user', content: 'A' });
      history.startNewBatch(0);
      const info = history.getBatchInfo(0);
      expect(info).toBeDefined();
      expect(info?.batchId).toBe(0);
    });

    it('should return all batches info', () => {
      history.addMessage({ role: 'user', content: 'A' });
      history.startNewBatch();
      const allInfo = history.getAllBatchesInfo();
      expect(allInfo.length).toBeGreaterThanOrEqual(2);
    });
  });

  describe('getCurrentBoundary', () => {
    it('should return current boundary index', () => {
      history.addMessage({ role: 'user', content: 'A' });
      history.addMessage({ role: 'user', content: 'B' });
      history.startNewBatch(1);
      expect(history.getCurrentBoundary()).toBe(1);
    });
  });

  describe('getMarkMap / setMarkMap', () => {
    it('should return a copy of markMap', () => {
      const markMap = history.getMarkMap();
      expect(markMap.currentBatch).toBe(0);
      expect(markMap.originalIndices).toEqual([]);
    });

    it('should set markMap', () => {
      const newMap = {
        originalIndices: [0, 1],
        batchBoundaries: [0, 2],
        boundaryToBatch: [0, 0],
        currentBatch: 0,
      };
      history.setMarkMap(newMap as any);
      expect(history.getCurrentBatch()).toBe(0);
    });
  });

  describe('Convenience message methods', () => {
    describe('addSystemMessage', () => {
      it('should add a system message', () => {
        history.addSystemMessage('System instruction');
        const msg = history.getMessages()[0]!;
        expect(msg.role).toBe('system');
        expect(msg.content).toBe('System instruction');
      });
    });

    describe('addUserMessage', () => {
      it('should add a user message', () => {
        history.addUserMessage('User input');
        const msg = history.getMessages()[0]!;
        expect(msg.role).toBe('user');
        expect(msg.content).toBe('User input');
      });
    });

    describe('addAssistantMessage', () => {
      it('should add an assistant message', () => {
        history.addAssistantMessage('Assistant response');
        const msg = history.getMessages()[0]!;
        expect(msg.role).toBe('assistant');
        expect(msg.content).toBe('Assistant response');
      });

      it('should add assistant message with tool calls', () => {
        const toolCalls = [
          { id: 'call_1', name: 'test_tool', arguments: { arg1: 'val1' } },
        ];
        history.addAssistantMessage('Using tool', toolCalls as unknown as LLMToolCall[]);
        const msg = history.getMessages()[0]!;
        expect(msg.role).toBe('assistant');
        expect(msg.toolCalls).toEqual(toolCalls);
      });

      it('should add assistant message with thinking', () => {
        history.addAssistantMessage('Thinking...', undefined, 'deep thought');
        const msg = history.getMessages()[0]!;
        expect(msg.thinking).toBe('deep thought');
      });
    });

    describe('addToolResultMessage', () => {
      it('should add a tool result message', () => {
        history.addToolResultMessage('call_1', 'Result content');
        const msg = history.getMessages()[0]!;
        expect(msg.role).toBe('tool');
        expect(msg.toolCallId).toBe('call_1');
        expect(msg.content).toBe('Result content');
      });
    });
  });

  describe('Query methods', () => {
    beforeEach(() => {
      history.addSystemMessage('System prompt');
      history.addUserMessage('Hello');
      history.addAssistantMessage('Hi');
      history.addUserMessage('How are you?');
      history.addAssistantMessage('I am fine');
    });

    it('getRecentMessages should return last N messages', () => {
      const recent = history.getRecentMessages(2);
      expect(recent).toHaveLength(2);
      expect(recent[0]!.role).toBe('user');
      expect(recent[0]!.content).toBe('How are you?');
      expect(recent[1]!.role).toBe('assistant');
    });

    it('getRecentMessages should return all if n exceeds count', () => {
      const all = history.getRecentMessages(10);
      expect(all).toHaveLength(5);
    });

    it('filterMessagesByRole should filter by roles', () => {
      const userMessages = history.filterMessagesByRole(['user']);
      expect(userMessages).toHaveLength(2);
      expect(userMessages.every(m => m.role === 'user')).toBe(true);
    });

    it('getRecentMessagesByRole should return recent messages for role', () => {
      const recentUser = history.getRecentMessagesByRole('user', 1);
      expect(recentUser).toHaveLength(1);
      expect(recentUser[0]!.content).toBe('How are you?');
    });

    it('getMessagesByRole should return all visible messages for role', () => {
      const assistantMsgs = history.getMessagesByRole('assistant');
      expect(assistantMsgs).toHaveLength(2);
    });

    it('getMessageCountByRole should count visible messages by role', () => {
      const count = history.getMessageCountByRole('user');
      expect(count).toBe(2);
    });

    it('searchMessages should find matching content', () => {
      const results = history.searchMessages('How');
      expect(results).toHaveLength(1);
      expect(results[0]!.content).toBe('How are you?');
    });

    it('searchMessages should return empty array for no match', () => {
      const results = history.searchMessages('NonExistent');
      expect(results).toEqual([]);
    });
  });

  describe('All-messages query methods', () => {
    it('getAllMessagesByRole should return all messages for role', () => {
      history.addSystemMessage('Sys');
      history.addUserMessage('User1');
      history.addAssistantMessage('Asst1');
      history.addUserMessage('User2');

      const allUsers = history.getAllMessagesByRole('user');
      expect(allUsers).toHaveLength(2);
    });

    it('getTotalMessageCountByRole should count all messages by role', () => {
      history.addSystemMessage('Sys');
      history.addUserMessage('User1');
      const count = history.getTotalMessageCountByRole('user');
      expect(count).toBe(1);
    });
  });

  describe('Operation methods', () => {
    it('truncateMessages should truncate', () => {
      history.addUserMessage('A');
      history.addUserMessage('B');
      history.addUserMessage('C');
      history.truncateMessages({ keepLast: 2 });
      expect(history.getTotalMessageCount()).toBe(2);
    });

    it('insertMessages should insert at position', () => {
      history.addUserMessage('A');
      history.addUserMessage('C');
      history.insertMessages(1, [{ role: 'user', content: 'B' }]);
      expect(history.getTotalMessageCount()).toBe(3);
      expect(history.getMessages()[1]!.content).toBe('B');
    });

    it('insertMessages at -1 should append', () => {
      history.addUserMessage('A');
      history.insertMessages(-1, [{ role: 'user', content: 'B' }]);
      expect(history.getTotalMessageCount()).toBe(2);
    });

    it('replaceMessage should replace at index', () => {
      history.addUserMessage('Original');
      history.replaceMessage(0, { role: 'user', content: 'Replaced' });
      expect(history.getMessages()[0]!.content).toBe('Replaced');
    });

    it('clearMessages should clear with system retention', () => {
      history.addSystemMessage('Keep me');
      history.addUserMessage('Delete me');
      history.clearMessages(true);
      expect(history.getTotalMessageCount()).toBe(1);
      expect(history.getMessages()[0]!.role).toBe('system');
    });

    it('clearMessages should clear everything when keepSystemMessage is false', () => {
      history.addSystemMessage('Sys');
      history.addUserMessage('User');
      history.clearMessages(false);
      expect(history.getTotalMessageCount()).toBe(0);
    });

    it('deduplicateMessages should remove duplicates', () => {
      history.addUserMessage('Hello');
      history.addUserMessage('Hello');
      history.deduplicateMessages(msg => msg.content as string);
      expect(history.getTotalMessageCount()).toBe(1);
    });
  });

  describe('initializeHistory', () => {
    it('should replace all messages', () => {
      history.addUserMessage('Old');
      history.initializeHistory([{ role: 'user', content: 'New' }]);
      expect(history.getTotalMessageCount()).toBe(1);
      expect(history.getMessages()[0]!.content).toBe('New');
    });
  });

  describe('clone', () => {
    it('should create an independent clone', () => {
      history.addUserMessage('Hello');
      const cloned = history.clone();
      expect(cloned.getTotalMessageCount()).toBe(1);
      cloned.addUserMessage('World');
      expect(history.getTotalMessageCount()).toBe(1);
      expect(cloned.getTotalMessageCount()).toBe(2);
    });
  });

  describe('validate', () => {
    it('should return valid for proper messages', () => {
      history.addUserMessage('Hello');
      const result = history.validate();
      expect(result.valid).toBe(true);
    });
  });

  describe('createSnapshot / restoreFromSnapshot', () => {
    it('should create a snapshot and restore from it', () => {
      history.addUserMessage('Original');
      history.addAssistantMessage('Response');
      const snapshot = history.createSnapshot();

      history.addUserMessage('Extra');
      expect(history.getTotalMessageCount()).toBe(3);

      history.restoreFromSnapshot(snapshot);
      expect(history.getTotalMessageCount()).toBe(2);
      expect(history.getMessages()[0]!.content).toBe('Original');
    });
  });

  describe('Visibility methods', () => {
    it('getInvisibleMessages should return messages before batch boundary', () => {
      history.addUserMessage('A');
      history.addUserMessage('B');
      history.startNewBatch(1);
      const invisible = history.getInvisibleMessages();
      expect(invisible).toHaveLength(1);
      expect(invisible[0]!.content).toBe('A');
    });

    it('getInvisibleMessageCount should count invisible messages', () => {
      history.addUserMessage('A');
      history.addUserMessage('B');
      history.addUserMessage('C');
      history.startNewBatch(2);
      expect(history.getInvisibleMessageCount()).toBe(2);
    });

    it('isMessageVisible should check visibility by original index', () => {
      history.addUserMessage('A');
      history.addUserMessage('B');
      history.startNewBatch(1);
      expect(history.isMessageVisible(0)).toBe(false);
      expect(history.isMessageVisible(1)).toBe(true);
    });
  });
});
