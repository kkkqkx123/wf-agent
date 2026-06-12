/**
 * WorkflowStateCoordinator - Unit Tests
 * Tests for workflow state coordination between WorkflowExecutionEntity and ConversationSession
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { WorkflowStateCoordinator } from '../workflow-state-coordinator.js';
import { RuntimeValidationError } from '@wf-agent/types';
import type { LLMMessage, TokenUsageStats, MessageMarkMap } from '@wf-agent/types';

// Mock MessageHistory
const createMockMessageHistory = () => ({
  addMessage: vi.fn(),
  addMessages: vi.fn(),
  getMessages: vi.fn().mockReturnValue([]),
  getRecentMessages: vi.fn().mockReturnValue([]),
  setMessages: vi.fn(),
  clearMessages: vi.fn(),
  normalizeHistory: vi.fn(),
  cleanup: vi.fn(),
});

// Mock ConversationSession
const createMockConversationSession = () => ({
  addMessage: vi.fn(),
  addMessages: vi.fn(),
  getAllMessages: vi.fn().mockReturnValue([]),
  clear: vi.fn(),
  getTokenUsage: vi.fn().mockReturnValue({ totalTokens: 0, promptTokens: 0, completionTokens: 0 }),
  getCurrentRequestUsage: vi.fn().mockReturnValue({ totalTokens: 0, promptTokens: 0, completionTokens: 0 }),
  setTokenUsageState: vi.fn(),
  checkTokenUsage: vi.fn().mockResolvedValue(undefined),
  getMarkMap: vi.fn().mockReturnValue({
    originalIndices: [],
    batchBoundaries: [0],
    boundaryToBatch: [0],
    currentBatch: 0,
  }),
  setMarkMap: vi.fn(),
  startNewBatch: vi.fn().mockReturnValue(0),
  startNewBatchWithAutoCheckpoint: vi.fn().mockResolvedValue(0),
  cleanup: vi.fn(),
});

// Mock WorkflowExecutionEntity
const createMockWorkflowExecutionEntity = () => ({
  id: 'test-execution-id',
  messageHistoryManager: createMockMessageHistory(),
});

describe('WorkflowStateCoordinator', () => {
  let coordinator: WorkflowStateCoordinator;
  let mockWorkflowExecutionEntity: ReturnType<typeof createMockWorkflowExecutionEntity>;
  let mockConversationSession: ReturnType<typeof createMockConversationSession>;

  beforeEach(() => {
    vi.clearAllMocks();
    mockWorkflowExecutionEntity = createMockWorkflowExecutionEntity();
    mockConversationSession = createMockConversationSession();
    
    coordinator = new WorkflowStateCoordinator({
      workflowExecutionEntity: mockWorkflowExecutionEntity as any,
      conversationManager: mockConversationSession as any,
    });
  });

  describe('constructor', () => {
    it('should initialize with workflow execution entity and conversation session', () => {
      expect(coordinator.getWorkflowExecutionEntity()).toBe(mockWorkflowExecutionEntity);
      expect(coordinator.getConversationManager()).toBe(mockConversationSession);
    });
  });

  describe('addMessage', () => {
    it('should add message to both managers', () => {
      // Arrange
      const message: LLMMessage = {
        role: 'user',
        content: 'test message',
      };

      // Act
      coordinator.addMessage(message);

      // Assert
      expect(mockWorkflowExecutionEntity.messageHistoryManager.addMessage).toHaveBeenCalledWith(message);
      expect(mockConversationSession.addMessage).toHaveBeenCalledWith(message);
    });

    it('should throw error when message is null', () => {
      // Act & Assert
      expect(() => coordinator.addMessage(null as any)).toThrow(RuntimeValidationError);
      expect(() => coordinator.addMessage(null as any)).toThrow('Message cannot be null');
    });

    it('should throw error when message has no role', () => {
      // Arrange
      const message: LLMMessage = {
        role: '' as any,
        content: 'test',
      };

      // Act & Assert
      expect(() => coordinator.addMessage(message)).toThrow(RuntimeValidationError);
      expect(() => coordinator.addMessage(message)).toThrow('Message must have role and content');
    });

    it('should throw error when message has no content', () => {
      // Arrange
      const message: LLMMessage = {
        role: 'user',
        content: '' as any,
      };

      // Act & Assert
      expect(() => coordinator.addMessage(message)).toThrow(RuntimeValidationError);
      expect(() => coordinator.addMessage(message)).toThrow('Message must have role and content');
    });
  });

  describe('addMessages', () => {
    it('should add multiple messages', () => {
      // Arrange
      const messages: LLMMessage[] = [
        { role: 'user', content: 'message 1' },
        { role: 'assistant', content: 'message 2' },
      ];

      // Act
      coordinator.addMessages(...messages);

      // Assert
      expect(mockWorkflowExecutionEntity.messageHistoryManager.addMessage).toHaveBeenCalledTimes(2);
      expect(mockConversationSession.addMessage).toHaveBeenCalledTimes(2);
    });
  });

  describe('getMessages', () => {
    it('should get visible messages from workflow execution entity', () => {
      // Arrange
      const messages: LLMMessage[] = [
        { role: 'user', content: 'test' },
      ];
      mockWorkflowExecutionEntity.messageHistoryManager.getMessages.mockReturnValue(messages);

      // Act
      const result = coordinator.getMessages();

      // Assert
      expect(result).toEqual(messages);
    });
  });

  describe('getAllMessages', () => {
    it('should get all messages from conversation session', () => {
      // Arrange
      const messages: LLMMessage[] = [
        { role: 'user', content: 'test' },
      ];
      mockConversationSession.getAllMessages.mockReturnValue(messages);

      // Act
      const result = coordinator.getAllMessages();

      // Assert
      expect(result).toEqual(messages);
    });
  });

  describe('getRecentMessages', () => {
    it('should get recent messages from workflow execution entity', () => {
      // Arrange
      const messages: LLMMessage[] = [
        { role: 'user', content: 'test' },
      ];
      mockWorkflowExecutionEntity.messageHistoryManager.getRecentMessages.mockReturnValue(messages);

      // Act
      const result = coordinator.getRecentMessages(5);

      // Assert
      expect(mockWorkflowExecutionEntity.messageHistoryManager.getRecentMessages).toHaveBeenCalledWith(5);
      expect(result).toEqual(messages);
    });
  });

  describe('setMessages', () => {
    it('should set messages to both managers', () => {
      // Arrange
      const messages: LLMMessage[] = [
        { role: 'user', content: 'test' },
      ];

      // Act
      coordinator.setMessages(messages);

      // Assert
      expect(mockWorkflowExecutionEntity.messageHistoryManager.setMessages).toHaveBeenCalledWith(messages);
      expect(mockConversationSession.clear).toHaveBeenCalled();
      expect(mockConversationSession.addMessages).toHaveBeenCalledWith(...messages);
    });
  });

  describe('clearMessages', () => {
    it('should clear messages from both managers', () => {
      // Act
      coordinator.clearMessages();

      // Assert
      expect(mockWorkflowExecutionEntity.messageHistoryManager.clearMessages).toHaveBeenCalled();
      expect(mockConversationSession.clear).toHaveBeenCalled();
    });
  });

  describe('normalizeHistory', () => {
    it('should normalize history', () => {
      // Act
      coordinator.normalizeHistory();

      // Assert
      expect(mockWorkflowExecutionEntity.messageHistoryManager.normalizeHistory).toHaveBeenCalled();
    });
  });

  describe('token management', () => {
    describe('getTokenUsage', () => {
      it('should get token usage from conversation session', () => {
        // Arrange
        const usage: TokenUsageStats = { totalTokens: 100, promptTokens: 50, completionTokens: 50 };
        mockConversationSession.getTokenUsage.mockReturnValue(usage);

        // Act
        const result = coordinator.getTokenUsage();

        // Assert
        expect(result).toEqual(usage);
      });
    });

    describe('getCurrentRequestUsage', () => {
      it('should get current request usage from conversation session', () => {
        // Arrange
        const usage: TokenUsageStats = { totalTokens: 10, promptTokens: 5, completionTokens: 5 };
        mockConversationSession.getCurrentRequestUsage.mockReturnValue(usage);

        // Act
        const result = coordinator.getCurrentRequestUsage();

        // Assert
        expect(result).toEqual(usage);
      });
    });

    describe('setTokenUsageState', () => {
      it('should set token usage state', () => {
        // Arrange
        const cumulativeUsage: TokenUsageStats = { totalTokens: 100, promptTokens: 50, completionTokens: 50 };
        const currentRequestUsage: TokenUsageStats = { totalTokens: 10, promptTokens: 5, completionTokens: 5 };

        // Act
        coordinator.setTokenUsageState(cumulativeUsage, currentRequestUsage);

        // Assert
        expect(mockConversationSession.setTokenUsageState).toHaveBeenCalledWith(cumulativeUsage, currentRequestUsage);
      });
    });

    describe('checkTokenUsage', () => {
      it('should check token usage', async () => {
        // Act
        await coordinator.checkTokenUsage();

        // Assert
        expect(mockConversationSession.checkTokenUsage).toHaveBeenCalled();
      });
    });
  });

  describe('batch management', () => {
    describe('getMarkMap', () => {
      it('should get mark map from conversation session', () => {
        // Arrange
        const markMap: MessageMarkMap = {
          originalIndices: [0, 1],
          batchBoundaries: [0],
          boundaryToBatch: [0],
          currentBatch: 0,
        };
        mockConversationSession.getMarkMap.mockReturnValue(markMap);

        // Act
        const result = coordinator.getMarkMap();

        // Assert
        expect(result).toEqual(markMap);
      });
    });

    describe('setMarkMap', () => {
      it('should set mark map', () => {
        // Arrange
        const markMap: MessageMarkMap = {
          originalIndices: [0, 1],
          batchBoundaries: [0],
          boundaryToBatch: [0],
          currentBatch: 0,
        };

        // Act
        coordinator.setMarkMap(markMap);

        // Assert
        expect(mockConversationSession.setMarkMap).toHaveBeenCalledWith(markMap);
      });
    });

    describe('startNewBatch', () => {
      it('should start new batch', () => {
        // Arrange
        mockConversationSession.startNewBatch.mockReturnValue(1);

        // Act
        const result = coordinator.startNewBatch(5);

        // Assert
        expect(mockConversationSession.startNewBatch).toHaveBeenCalledWith(5);
        expect(result).toBe(1);
      });
    });

    describe('startNewBatchWithAutoCheckpoint', () => {
      it('should start new batch with auto checkpoint', async () => {
        // Arrange
        mockConversationSession.startNewBatchWithAutoCheckpoint.mockResolvedValue(1);

        // Act
        const result = await coordinator.startNewBatchWithAutoCheckpoint(5, 3);

        // Assert
        expect(mockConversationSession.startNewBatchWithAutoCheckpoint).toHaveBeenCalledWith(5, 3);
        expect(result).toBe(1);
      });
    });
  });

  describe('createSnapshot', () => {
    it('should create snapshot', () => {
      // Arrange
      const messages: LLMMessage[] = [
        { role: 'user', content: 'test' },
      ];
      const markMap: MessageMarkMap = {
        originalIndices: [0],
        batchBoundaries: [0],
        boundaryToBatch: [0],
        currentBatch: 0,
      };
      const tokenUsage: TokenUsageStats = { totalTokens: 100, promptTokens: 50, completionTokens: 50 };
      
      mockConversationSession.getAllMessages.mockReturnValue(messages);
      mockConversationSession.getMarkMap.mockReturnValue(markMap);
      mockConversationSession.getTokenUsage.mockReturnValue(tokenUsage);
      mockConversationSession.getCurrentRequestUsage.mockReturnValue(tokenUsage);

      // Act
      const snapshot = coordinator.createSnapshot();

      // Assert
      expect(snapshot.messages).toEqual(messages);
      expect(snapshot.markMap).toEqual(markMap);
      expect(snapshot.tokenUsage).toEqual(tokenUsage);
      expect(snapshot.currentRequestUsage).toEqual(tokenUsage);
    });
  });

  describe('restoreFromSnapshot', () => {
    it('should restore from snapshot', () => {
      // Arrange
      const messages: LLMMessage[] = [
        { role: 'user', content: 'test' },
      ];
      const markMap: MessageMarkMap = {
        originalIndices: [0],
        batchBoundaries: [0],
        boundaryToBatch: [0],
        currentBatch: 0,
      };
      const tokenUsage: TokenUsageStats = { totalTokens: 100, promptTokens: 50, completionTokens: 50 };
      
      const snapshot = {
        messages,
        markMap,
        tokenUsage,
        currentRequestUsage: tokenUsage,
      };

      // Act
      coordinator.restoreFromSnapshot(snapshot);

      // Assert
      expect(mockWorkflowExecutionEntity.messageHistoryManager.clearMessages).toHaveBeenCalled();
      expect(mockConversationSession.clear).toHaveBeenCalled();
      expect(mockWorkflowExecutionEntity.messageHistoryManager.addMessage).toHaveBeenCalledWith(messages[0]);
      expect(mockConversationSession.addMessage).toHaveBeenCalledWith(messages[0]);
      expect(mockConversationSession.setMarkMap).toHaveBeenCalledWith(markMap);
      expect(mockConversationSession.setTokenUsageState).toHaveBeenCalledWith(tokenUsage, tokenUsage);
    });

    it('should handle empty snapshot', () => {
      // Arrange
      const snapshot = {
        messages: [],
        markMap: {
          originalIndices: [],
          batchBoundaries: [0],
          boundaryToBatch: [0],
          currentBatch: 0,
        },
      };

      // Act
      coordinator.restoreFromSnapshot(snapshot as any);

      // Assert
      expect(mockWorkflowExecutionEntity.messageHistoryManager.clearMessages).toHaveBeenCalled();
      expect(mockConversationSession.clear).toHaveBeenCalled();
      expect(mockWorkflowExecutionEntity.messageHistoryManager.addMessage).not.toHaveBeenCalled();
    });
  });

  describe('cleanup', () => {
    it('should cleanup both managers', () => {
      // Act
      coordinator.cleanup();

      // Assert
      expect(mockWorkflowExecutionEntity.messageHistoryManager.cleanup).toHaveBeenCalled();
      expect(mockConversationSession.cleanup).toHaveBeenCalled();
    });
  });
});