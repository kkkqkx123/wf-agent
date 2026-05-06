/**
 * Conversation Session Context Caching Tests
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { ConversationSession } from '../conversation-session.js';
import type { LLMMessage } from '@wf-agent/types';

describe('ConversationSession - Context Caching', () => {
  let session: ConversationSession;

  beforeEach(() => {
    session = new ConversationSession();
  });

  describe('getTurnDynamicContext', () => {
    it('should return undefined for uncached turn', () => {
      const retrieved = session.getTurnDynamicContext(0);
      expect(retrieved).toBeUndefined();
    });

    it('should return cached context', () => {
      session.setTurnDynamicContext(0, 'cached context for turn 0');
      
      const retrieved = session.getTurnDynamicContext(0);
      expect(retrieved).toBe('cached context for turn 0');
    });

    it('should handle multiple cached turns independently', () => {
      session.setTurnDynamicContext(0, 'context 0');
      session.setTurnDynamicContext(5, 'context 5');
      session.setTurnDynamicContext(10, 'context 10');

      expect(session.getTurnDynamicContext(0)).toBe('context 0');
      expect(session.getTurnDynamicContext(5)).toBe('context 5');
      expect(session.getTurnDynamicContext(10)).toBe('context 10');
      expect(session.getTurnDynamicContext(3)).toBeUndefined();
    });
  });

  describe('setTurnDynamicContext', () => {
    it('should cache and retrieve context', () => {
      const context = 'This is dynamic context for turn 1';
      session.setTurnDynamicContext(1, context);

      const retrieved = session.getTurnDynamicContext(1);
      expect(retrieved).toBe(context);
    });

    it('should overwrite existing cached context', () => {
      session.setTurnDynamicContext(2, 'original context');
      session.setTurnDynamicContext(2, 'updated context');

      const retrieved = session.getTurnDynamicContext(2);
      expect(retrieved).toBe('updated context');
    });

    it('should handle empty string context', () => {
      session.setTurnDynamicContext(3, '');
      
      const retrieved = session.getTurnDynamicContext(3);
      expect(retrieved).toBe('');
    });

    it('should handle large context strings', () => {
      const largeContext = 'x'.repeat(10000);
      session.setTurnDynamicContext(4, largeContext);

      const retrieved = session.getTurnDynamicContext(4);
      expect(retrieved).toBe(largeContext);
      expect(retrieved?.length).toBe(10000);
    });
  });

  describe('clearTurnContextFromIndex', () => {
    it('should clear cache from specified index onwards', () => {
      session.setTurnDynamicContext(0, 'context 0');
      session.setTurnDynamicContext(1, 'context 1');
      session.setTurnDynamicContext(2, 'context 2');
      session.setTurnDynamicContext(3, 'context 3');

      session.clearTurnContextFromIndex(2);

      expect(session.getTurnDynamicContext(0)).toBe('context 0');
      expect(session.getTurnDynamicContext(1)).toBe('context 1');
      expect(session.getTurnDynamicContext(2)).toBeUndefined();
      expect(session.getTurnDynamicContext(3)).toBeUndefined();
    });

    it('should not clear cache before specified index', () => {
      session.setTurnDynamicContext(0, 'context 0');
      session.setTurnDynamicContext(1, 'context 1');
      session.setTurnDynamicContext(2, 'context 2');

      session.clearTurnContextFromIndex(1);

      expect(session.getTurnDynamicContext(0)).toBe('context 0');
      expect(session.getTurnDynamicContext(1)).toBeUndefined();
      expect(session.getTurnDynamicContext(2)).toBeUndefined();
    });

    it('should handle clearing from index 0 (clear all)', () => {
      session.setTurnDynamicContext(0, 'context 0');
      session.setTurnDynamicContext(5, 'context 5');
      session.setTurnDynamicContext(10, 'context 10');

      session.clearTurnContextFromIndex(0);

      expect(session.getTurnDynamicContext(0)).toBeUndefined();
      expect(session.getTurnDynamicContext(5)).toBeUndefined();
      expect(session.getTurnDynamicContext(10)).toBeUndefined();
    });

    it('should handle clearing from non-existent index', () => {
      session.setTurnDynamicContext(0, 'context 0');
      session.setTurnDynamicContext(1, 'context 1');

      session.clearTurnContextFromIndex(100);

      expect(session.getTurnDynamicContext(0)).toBe('context 0');
      expect(session.getTurnDynamicContext(1)).toBe('context 1');
    });

    it('should handle negative index gracefully', () => {
      session.setTurnDynamicContext(0, 'context 0');
      session.setTurnDynamicContext(1, 'context 1');

      session.clearTurnContextFromIndex(-1);

      expect(session.getTurnDynamicContext(0)).toBeUndefined();
      expect(session.getTurnDynamicContext(1)).toBeUndefined();
    });
  });

  describe('clearAllTurnContexts', () => {
    it('should clear all cached contexts', () => {
      session.setTurnDynamicContext(0, 'context 0');
      session.setTurnDynamicContext(1, 'context 1');
      session.setTurnDynamicContext(2, 'context 2');

      session.clearAllTurnContexts();

      expect(session.getTurnDynamicContext(0)).toBeUndefined();
      expect(session.getTurnDynamicContext(1)).toBeUndefined();
      expect(session.getTurnDynamicContext(2)).toBeUndefined();
    });

    it('should handle clearing when cache is empty', () => {
      expect(() => {
        session.clearAllTurnContexts();
      }).not.toThrow();
    });

    it('should allow re-caching after clearing', () => {
      session.setTurnDynamicContext(0, 'old context');
      session.clearAllTurnContexts();
      session.setTurnDynamicContext(0, 'new context');

      expect(session.getTurnDynamicContext(0)).toBe('new context');
    });
  });

  describe('Integration with Message Operations', () => {
    it('should clear all contexts on cleanup', () => {
      session.setTurnDynamicContext(0, 'context 0');
      session.setTurnDynamicContext(1, 'context 1');

      session.cleanup();

      expect(session.getTurnDynamicContext(0)).toBeUndefined();
      expect(session.getTurnDynamicContext(1)).toBeUndefined();
    });
  });

  describe('Edge Cases', () => {
    it('should handle very large turn indices', () => {
      session.setTurnDynamicContext(999999, 'large index context');
      
      expect(session.getTurnDynamicContext(999999)).toBe('large index context');
    });

    it('should maintain cache across multiple operations', () => {
      // Add initial messages
      session.addMessage({ role: 'user', content: 'msg 0' });
      
      // Cache context for turn 0
      session.setTurnDynamicContext(0, 'initial context');
      
      // Add more messages
      session.addMessage({ role: 'assistant', content: 'resp 1' });
      session.addMessage({ role: 'user', content: 'msg 2' });
      
      // Cache context for turn 2
      session.setTurnDynamicContext(2, 'later context');
      
      // Both cached contexts should still be available
      expect(session.getTurnDynamicContext(0)).toBe('initial context');
      expect(session.getTurnDynamicContext(2)).toBe('later context');
    });

    it('should handle special characters in context', () => {
      const specialContext = 'Context with\nnewlines\tand\ttabs\nand "quotes" and \'apostrophes\'';
      session.setTurnDynamicContext(0, specialContext);

      const retrieved = session.getTurnDynamicContext(0);
      expect(retrieved).toBe(specialContext);
    });

    it('should handle unicode characters', () => {
      const unicodeContext = '上下文缓存 🚀 测试 ñ 中文';
      session.setTurnDynamicContext(0, unicodeContext);

      const retrieved = session.getTurnDynamicContext(0);
      expect(retrieved).toBe(unicodeContext);
    });
  });
});
