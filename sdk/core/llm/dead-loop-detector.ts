/**
 * LLM流式生成死循环检测器
 * 
 * 基于streaming-dead-loop-detector.md设计规范实现
 */

export interface DeadLoopDetectionResult {
  detected: boolean;
  type?: 'short-sequence' | 'paragraph-repeat' | 'list-repeat';
  details?: string;
}

// Import config type from types package
import type { DeadLoopDetectionConfig as TypesDeadLoopDetectionConfig } from "@wf-agent/types";

// Adapter type to match the internal naming
export type DeadLoopDetectorConfig = TypesDeadLoopDetectionConfig;

export class DeadLoopDetector {
  private config: Required<DeadLoopDetectorConfig>;
  private checkedCheckpoints: Set<number> = new Set();

  constructor(config: DeadLoopDetectorConfig = {}) {
    this.config = {
      checkpoints: config.checkpoints || [500, 1000, 2000],
      shortSequenceWindow: config.shortSequenceWindow || 200,
      minRepeatUnitLength: config.minRepeatUnitLength || 2,
      minRepeatCount: config.minRepeatCount || 4,
      minPeriodElements: config.minPeriodElements || 6,
      maxPeriodLength: config.maxPeriodLength || 50,
    };
  }

  /**
   * 检测死循环
   * @param reasoningMessage 当前累积的推理内容
   * @returns 检测结果
   */
  detect(reasoningMessage: string): DeadLoopDetectionResult {
    const charCount = reasoningMessage.length;

    // 遍历检查点
    for (const checkpoint of this.config.checkpoints) {
      if (charCount >= checkpoint && !this.checkedCheckpoints.has(checkpoint)) {
        this.checkedCheckpoints.add(checkpoint);

        // 根据检查点执行不同的检测
        const result = this.detectAtCheckpoint(reasoningMessage, checkpoint);
        if (result.detected) {
          return result;
        }
      }
    }

    return { detected: false };
  }

  /**
   * 重置检测状态（新的API请求开始时调用）
   */
  reset(): void {
    this.checkedCheckpoints.clear();
  }

  /**
   * 在指定检查点执行检测
   */
  private detectAtCheckpoint(
    text: string,
    checkpoint: number
  ): DeadLoopDetectionResult {
    // 获取检测范围的文本片段
    const previousCheckpoint = this.getPreviousCheckpoint(checkpoint);
    const startIndex = previousCheckpoint || 0;
    const segment = text.slice(startIndex);

    // 类型3：短序列循环检测（仅在第1检查点）
    if (checkpoint === this.config.checkpoints[0]) {
      const result = this.detectShortSequence(segment);
      if (result.detected) return result;
    }

    // 类型1和类型2：在第2和第3检查点执行
    const secondCheckpoint = this.config.checkpoints[1];
    if (secondCheckpoint !== undefined && checkpoint >= secondCheckpoint) {
      // 类型1：段落内容重复检测
      const paragraphResult = this.detectParagraphRepeat(segment);
      if (paragraphResult.detected) return paragraphResult;

      // 类型2：有序列表重复检测
      const listResult = this.detectListRepeat(segment);
      if (listResult.detected) return listResult;
    }

    return { detected: false };
  }

  /**
   * 类型3：短序列循环检测
   */
  private detectShortSequence(text: string): DeadLoopDetectionResult {
    // 取最近N个字符
    const windowSize = Math.min(this.config.shortSequenceWindow, text.length);
    const recentText = text.slice(-windowSize);

    // 正则匹配：至少2个字符的子串连续重复至少4次
    const pattern = new RegExp(
      `(.{${this.config.minRepeatUnitLength},})\\1{${this.config.minRepeatCount - 1},}`,
      's'
    );

    const match = recentText.match(pattern);
    if (match) {
      return {
        detected: true,
        type: 'short-sequence',
        details: `Detected short sequence loop: "${match[1]}" repeated`,
      };
    }

    return { detected: false };
  }

  /**
   * 类型1：段落内容重复检测
   */
  private detectParagraphRepeat(text: string): DeadLoopDetectionResult {
    // 步骤1：语义块分割
    const blocks = this.splitIntoSemanticBlocks(text);

    if (blocks.length < this.config.minPeriodElements) {
      return { detected: false };
    }

    // 步骤2：调用通用周期检测
    const periodResult = this.detectPeriod(blocks);
    if (periodResult.detected) {
      return {
        detected: true,
        type: 'paragraph-repeat',
        details: `Detected paragraph repeat with period ${periodResult.period}`,
      };
    }

    return { detected: false };
  }

  /**
   * 类型2：有序列表重复检测
   */
  private detectListRepeat(text: string): DeadLoopDetectionResult {
    // 步骤1：按行分割
    const lines = text.split('\n');

    if (lines.length < this.config.minPeriodElements) {
      return { detected: false };
    }

    // 步骤2：行标准化（去除有序列表标号）
    const normalizedLines = lines.map(line => this.normalizeListItem(line));

    // 步骤3：调用通用周期检测
    const periodResult = this.detectPeriod(normalizedLines);
    if (periodResult.detected) {
      return {
        detected: true,
        type: 'list-repeat',
        details: `Detected list repeat with period ${periodResult.period}`,
      };
    }

    return { detected: false };
  }

  /**
   * 通用周期检测逻辑（类型1和类型2共用）
   */
  private detectPeriod(elements: string[]): { detected: boolean; period?: number } {
    const maxPeriod = Math.min(
      this.config.maxPeriodLength,
      Math.floor(elements.length / 2)
    );

    for (let p = 1; p <= maxPeriod; p++) {
      let consecutiveCount = 0;

      // 从末尾向前检查
      for (let i = elements.length - 1; i >= p; i--) {
        if (elements[i] === elements[i - p]) {
          consecutiveCount++;
        } else {
          break;
        }
      }

      if (consecutiveCount >= this.config.minPeriodElements) {
        return { detected: true, period: p };
      }
    }

    return { detected: false };
  }

  /**
   * 语义块分割
   */
  private splitIntoSemanticBlocks(text: string): string[] {
    // 以自然语言边界符分割：。.!！;；?？\n
    const separators = /[。.!！;；?？\n]+/;
    const blocks = text.split(separators).filter(block => block.trim().length > 0);
    return blocks;
  }

  /**
   * 有序列表行标准化
   */
  private normalizeListItem(line: string): string {
    // 匹配有序列表标号模式：1. 2. 10. 等
    const listPattern = /^\d+\.\s*/;
    return line.replace(listPattern, '');
  }

  /**
   * 获取上一个检查点
   */
  private getPreviousCheckpoint(current: number): number | null {
    const index = this.config.checkpoints.indexOf(current);
    if (index <= 0) return null;
    const previousIndex = index - 1;
    return this.config.checkpoints[previousIndex] ?? null;
  }
}
