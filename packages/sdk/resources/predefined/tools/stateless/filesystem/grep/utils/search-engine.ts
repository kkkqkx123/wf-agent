/**
 * Grep Search Engine
 *
 * Internal wrapper for RipgrepExecutor, providing singleton instance management
 * and search functionality specific to the grep tool.
 *
 * This replaces the previous SearchService dependency, moving the search logic
 * directly into the grep tool where it is exclusively used.
 */

import { RipgrepExecutor } from "@wf-agent/sdk/services";

/**
 * Search content options
 */
export interface SearchContentOptions {
  cwd: string;
  directoryPath: string;
  pattern: string;
  filePattern?: string;
  contextLines?: number;
  maxResults?: number;
}

/**
 * Grep Search Engine class - manages ripgrep executor as singleton
 */
export class GrepSearchEngine {
  private static executor: RipgrepExecutor | null = null;

  /**
   * Get or create the singleton RipgrepExecutor instance
   */
  private static getExecutor(): RipgrepExecutor {
    if (!this.executor) {
      this.executor = new RipgrepExecutor();
    }
    return this.executor;
  }

  /**
   * Initialize the executor (checks for ripgrep availability)
   */
  static async initialize(): Promise<void> {
    const executor = this.getExecutor();
    await executor.initialize();
  }

  /**
   * Search content using ripgrep
   */
  static async searchContent(options: SearchContentOptions): Promise<string> {
    const executor = this.getExecutor();

    return executor.searchContent({
      cwd: options.cwd,
      directoryPath: options.directoryPath,
      pattern: options.pattern,
      filePattern: options.filePattern,
      contextLines: options.contextLines ?? 1,
      maxResults: options.maxResults ?? 300,
    });
  }

  /**
   * Reset singleton instance (mainly for testing)
   */
  static reset(): void {
    this.executor = null;
  }
}
