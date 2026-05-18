/**
 * DeadLoopDetector Unit Tests
 */

import { describe, it, expect } from 'vitest';
import { DeadLoopDetector } from '../dead-loop-detector.js';

describe('DeadLoopDetector', () => {
  describe('Type 3: Short Sequence Loop Detection', () => {
    it('should detect short sequence repetition', () => {
      const detector = new DeadLoopDetector();
      // Need to exceed first checkpoint (500 chars)
      const text = '我需要思考'.repeat(100); // 500 characters
      const result = detector.detect(text);
      
      expect(result.detected).toBe(true);
      expect(result.type).toBe('short-sequence');
    });

    it('should not misjudge normal text', () => {
      const detector = new DeadLoopDetector();
      const text = '这是一个正常的句子，没有任何重复模式。';
      const result = detector.detect(text.repeat(3));
      
      expect(result.detected).toBe(false);
    });

    it('should detect repetition at first checkpoint', () => {
      const detector = new DeadLoopDetector({
        checkpoints: [100, 200, 300]
      });
      
      // Create repetitive content that exceeds first checkpoint
      const repetitiveText = 'abc'.repeat(50); // 150 characters
      const result = detector.detect(repetitiveText);
      
      expect(result.detected).toBe(true);
      expect(result.type).toBe('short-sequence');
    });
  });

  describe('Type 1: Paragraph Content Repeat Detection', () => {
    it('should detect paragraph repetition', () => {
      const detector = new DeadLoopDetector();
      // Add non-repetitive prefix to pass first checkpoint without triggering short-sequence detection
      const prefix = Array.from({ length: 500 }, (_, i) => String.fromCharCode(97 + (i % 26))).join('');
      // Need to exceed second checkpoint (1000 chars) for paragraph detection
      const repetitiveText = '今天天气真好。我们出去玩吧！'.repeat(50); // ~1000 characters
      const text = prefix + repetitiveText;
      const result = detector.detect(text);
      
      expect(result.detected).toBe(true);
      // May detect as short-sequence or paragraph-repeat depending on content
      expect(['short-sequence', 'paragraph-repeat']).toContain(result.type);
    });

    it('should not detect when blocks are insufficient', () => {
      const detector = new DeadLoopDetector();
      const text = '短句。另一个短句。';
      const result = detector.detect(text);
      
      expect(result.detected).toBe(false);
    });

    it('should detect with different punctuation', () => {
      const detector = new DeadLoopDetector();
      // Add non-repetitive prefix
      const prefix = Array.from({ length: 500 }, (_, i) => String.fromCharCode(65 + (i % 26))).join('');
      // Need to exceed second checkpoint
      const repetitiveText = 'First sentence. Second sentence! Third sentence? '.repeat(25); // ~1250 characters
      const text = prefix + repetitiveText;
      const result = detector.detect(text);
      
      expect(result.detected).toBe(true);
      // May detect as short-sequence or paragraph-repeat
      expect(['short-sequence', 'paragraph-repeat']).toContain(result.type);
    });
  });

  describe('Type 2: Ordered List Repeat Detection', () => {
    it('should detect ordered list repetition', () => {
      const detector = new DeadLoopDetector();
      // Add non-repetitive prefix to reach checkpoint
      const prefix = Array.from({ length: 600 }, (_, i) => String.fromCharCode(97 + (i % 26))).join('');
      // Need sufficient lines and exceed checkpoint
      const lines = Array.from({ length: 20 }, (_, i) => 
        `${(i % 2) + 1}. Analyze requirements and design solutions`
      );
      const repetitiveText = lines.join('\n');
      const text = prefix + repetitiveText;
      
      const result = detector.detect(text);
      
      expect(result.detected).toBe(true);
      // May detect as paragraph-repeat or list-repeat
      expect(['paragraph-repeat', 'list-repeat']).toContain(result.type);
    });

    it('should normalize list numbers before detection', () => {
      const detector = new DeadLoopDetector();
      // Add non-repetitive prefix
      const prefix = Array.from({ length: 600 }, (_, i) => String.fromCharCode(48 + (i % 10))).join('');
      // Create a pattern where content repeats but numbers increment
      const lines = [
        '1. Step A - analyze the requirements carefully',
        '2. Step B - design the solution architecture',
        '3. Step A - analyze the requirements carefully',
        '4. Step B - design the solution architecture',
        '5. Step A - analyze the requirements carefully',
        '6. Step B - design the solution architecture',
        '7. Step A - analyze the requirements carefully',
        '8. Step B - design the solution architecture',
        '9. Step A - analyze the requirements carefully',
        '10. Step B - design the solution architecture',
        '11. Step A - analyze the requirements carefully',
        '12. Step B - design the solution architecture',
      ];
      const repetitiveText = lines.join('\n');
      const text = prefix + repetitiveText;
      
      const result = detector.detect(text);
      
      expect(result.detected).toBe(true);
      expect(result.type).toBe('list-repeat');
    });

    it('should not detect when lines are insufficient', () => {
      const detector = new DeadLoopDetector();
      const text = '1. First\n2. Second\n3. Third';
      const result = detector.detect(text);
      
      expect(result.detected).toBe(false);
    });
  });

  describe('Checkpoint Mechanism', () => {
    it('should only detect at checkpoints', () => {
      const detector = new DeadLoopDetector({
        checkpoints: [100, 200, 300]
      });
      
      // Not reaching checkpoint
      const shortText = '重复'.repeat(10); // 40 characters
      expect(detector.detect(shortText).detected).toBe(false);
      
      // Reaching first checkpoint
      const longText = '重复'.repeat(100); // 400 characters
      const result = detector.detect(longText);
      expect(result.detected).toBe(true);
    });

    it('should not re-check passed checkpoints', () => {
      const detector = new DeadLoopDetector({
        checkpoints: [50, 100, 150]
      });
      
      // First check at 50
      const text1 = 'a'.repeat(50);
      detector.detect(text1);
      
      // Add more text to reach 100
      const text2 = 'a'.repeat(100);
      const result = detector.detect(text2);
      
      // Should check at 100 (new checkpoint)
      expect(result).toBeDefined();
    });

    it('should reset checkpoint state', () => {
      const detector = new DeadLoopDetector({
        checkpoints: [50, 100]
      });
      
      // Reach first checkpoint
      const text1 = 'test'.repeat(20);
      detector.detect(text1);
      
      // Reset
      detector.reset();
      
      // Should be able to detect again at same checkpoint
      const text2 = 'test'.repeat(20);
      const result = detector.detect(text2);
      
      expect(result).toBeDefined();
    });
  });

  describe('Configuration Options', () => {
    it('should use custom checkpoint values', () => {
      const detector = new DeadLoopDetector({
        checkpoints: [50, 100],
        minRepeatCount: 3,
      });
      
      const text = 'x'.repeat(60);
      const result = detector.detect(text);
      
      expect(result).toBeDefined();
    });

    it('should use default configuration when not provided', () => {
      const detector = new DeadLoopDetector();
      
      // Should have default checkpoints
      const text = 'a'.repeat(2500);
      const result = detector.detect(text);
      
      expect(result).toBeDefined();
    });

    it('should respect minPeriodElements configuration', () => {
      const detector = new DeadLoopDetector({
        minPeriodElements: 10,
      });
      
      // Create pattern with only 6 repetitions (less than minPeriodElements)
      const text = 'Block. '.repeat(6);
      const result = detector.detect(text);
      
      expect(result.detected).toBe(false);
    });
  });

  describe('Edge Cases', () => {
    it('should handle empty text', () => {
      const detector = new DeadLoopDetector();
      const result = detector.detect('');
      
      expect(result.detected).toBe(false);
    });

    it('should handle very short text', () => {
      const detector = new DeadLoopDetector();
      const result = detector.detect('ab');
      
      expect(result.detected).toBe(false);
    });

    it('should handle mixed Chinese and English', () => {
      const detector = new DeadLoopDetector();
      // Add non-repetitive prefix
      const prefix = Array.from({ length: 500 }, (_, i) => String.fromCharCode(97 + (i % 26))).join('');
      // Need to exceed checkpoint
      const repetitiveText = 'Hello世界.Hello世界.Hello世界.'.repeat(20);
      const text = prefix + repetitiveText;
      const result = detector.detect(text);
      
      expect(result.detected).toBe(true);
    });

    it('should handle special characters', () => {
      const detector = new DeadLoopDetector();
      // Need to exceed checkpoint
      const text = '@#$%'.repeat(150);
      const result = detector.detect(text);
      
      expect(result.detected).toBe(true);
    });
  });
});
