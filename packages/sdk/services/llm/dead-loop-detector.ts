/**
 * LLM Flow Generation Dead Reckoning Detector
 *
 * Implemented based on streaming-dead-loop-detector.md design specification
 */

export interface DeadLoopDetectionResult {
  detected: boolean;
  type?: "short-sequence" | "paragraph-repeat" | "list-repeat";
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
      enabled: config.enabled ?? true,
      checkpoints: config.checkpoints || [500, 1000, 2000],
      shortSequenceWindow: config.shortSequenceWindow || 200,
      minRepeatUnitLength: config.minRepeatUnitLength || 2,
      minRepeatCount: config.minRepeatCount || 4,
      minPeriodElements: config.minPeriodElements || 6,
      maxPeriodLength: config.maxPeriodLength || 50,
    };
  }

  /**
   * Detecting Dead Loops
   * @param reasoningMessage Current accumulated reasoning content
   * @returns Detection result
   */
  detect(reasoningMessage: string): DeadLoopDetectionResult {
    const charCount = reasoningMessage.length;

    // Traversing checkpoints
    for (const checkpoint of this.config.checkpoints) {
      if (charCount >= checkpoint && !this.checkedCheckpoints.has(checkpoint)) {
        this.checkedCheckpoints.add(checkpoint);

        // Perform different tests based on checkpoints
        const result = this.detectAtCheckpoint(reasoningMessage, checkpoint);
        if (result.detected) {
          return result;
        }
      }
    }

    return { detected: false };
  }

  /**
   * Reset detection state (called at the start of a new API request)
   */
  reset(): void {
    this.checkedCheckpoints.clear();
  }

  /**
   * Perform testing at designated checkpoints
   */
  private detectAtCheckpoint(text: string, checkpoint: number): DeadLoopDetectionResult {
    // Get the text snippet of the detection range
    const previousCheckpoint = this.getPreviousCheckpoint(checkpoint);
    const startIndex = previousCheckpoint || 0;
    const segment = text.slice(startIndex);

    // Type 3: Short Sequence Loop Detection (Checkpoint 1 only)
    if (checkpoint === this.config.checkpoints[0]) {
      const result = this.detectShortSequence(segment);
      if (result.detected) return result;
    }

    // Type 1 and Type 2: executed at checkpoints 2 and 3
    const secondCheckpoint = this.config.checkpoints[1];
    if (secondCheckpoint !== undefined && checkpoint >= secondCheckpoint) {
      // Type 1: Paragraph content duplication detection
      const paragraphResult = this.detectParagraphRepeat(segment);
      if (paragraphResult.detected) return paragraphResult;

      // Type 2: Ordered list duplicate detection
      const listResult = this.detectListRepeat(segment);
      if (listResult.detected) return listResult;
    }

    return { detected: false };
  }

  /**
   * Type 3: Short Sequence Cycle Detection
   */
  private detectShortSequence(text: string): DeadLoopDetectionResult {
    // Fetch the last N characters
    const windowSize = Math.min(this.config.shortSequenceWindow, text.length);
    const recentText = text.slice(-windowSize);

    // Regular match: substring of at least 2 characters repeated at least 4 times in a row
    const pattern = new RegExp(
      `(.{${this.config.minRepeatUnitLength},})\\1{${this.config.minRepeatCount - 1},}`,
      "s",
    );

    const match = recentText.match(pattern);
    if (match) {
      return {
        detected: true,
        type: "short-sequence",
        details: `Detected short sequence loop: "${match[1]}" repeated`,
      };
    }

    return { detected: false };
  }

  /**
   * Type 1: Paragraph content duplication detection
   */
  private detectParagraphRepeat(text: string): DeadLoopDetectionResult {
    // Step 1: Semantic Block Segmentation
    const blocks = this.splitIntoSemanticBlocks(text);

    if (blocks.length < this.config.minPeriodElements) {
      return { detected: false };
    }

    // Step 2: Invoke generic cycle detection
    const periodResult = this.detectPeriod(blocks);
    if (periodResult.detected) {
      return {
        detected: true,
        type: "paragraph-repeat",
        details: `Detected paragraph repeat with period ${periodResult.period}`,
      };
    }

    return { detected: false };
  }

  /**
   * Type 2: Ordered list duplicate detection
   */
  private detectListRepeat(text: string): DeadLoopDetectionResult {
    // Step 1: Split by row
    const lines = text.split("\n");

    if (lines.length < this.config.minPeriodElements) {
      return { detected: false };
    }

    // Step 2: Row normalization (removal of ordered list labels)
    const normalizedLines = lines.map(line => this.normalizeListItem(line));

    // Step 3: Invoke generic cycle detection
    const periodResult = this.detectPeriod(normalizedLines);
    if (periodResult.detected) {
      return {
        detected: true,
        type: "list-repeat",
        details: `Detected list repeat with period ${periodResult.period}`,
      };
    }

    return { detected: false };
  }

  /**
   * Generic cycle detection logic (common to Type 1 and Type 2)
   */
  private detectPeriod(elements: string[]): { detected: boolean; period?: number } {
    const maxPeriod = Math.min(this.config.maxPeriodLength, Math.floor(elements.length / 2));

    for (let p = 1; p <= maxPeriod; p++) {
      let consecutiveCount = 0;

      // Proceed from the end forward.
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
   * semantic chunk segmentation
   */
  private splitIntoSemanticBlocks(text: string): string[] {
    // Segmentation by natural language boundary characters:...! ;; ???? \n
    const separators = /[。.!！;；?？\n]+/;
    const blocks = text.split(separators).filter(block => block.trim().length > 0);
    return blocks;
  }

  /**
   * Normalization of ordered list rows
   */
  private normalizeListItem(line: string): string {
    // Match ordered list labeling patterns: 1. 2. 10. etc.
    const listPattern = /^\d+\.\s*/;
    return line.replace(listPattern, "");
  }

  /**
   * Get previous checkpoint
   */
  private getPreviousCheckpoint(current: number): number | null {
    const index = this.config.checkpoints.indexOf(current);
    if (index <= 0) return null;
    const previousIndex = index - 1;
    return this.config.checkpoints[previousIndex] ?? null;
  }
}
