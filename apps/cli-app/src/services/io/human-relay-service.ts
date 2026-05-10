/**
 * Human Relay Service
 * 
 * Manages human relay file operations for CLI-App.
 * Handles program-to-program data exchange via files.
 */

import * as fs from "fs/promises";
import * as path from "path";
import { FSWatcher } from "fs";

/**
 * Human Relay file paths
 */
export interface HumanRelayPaths {
  output: string;
  input: string;
}

/**
 * Watch options for Human Relay input
 */
export interface HumanRelayWatchOptions {
  sessionId: string;
  timeout: number;
  onResponse: (content: string) => void;
  onTimeout?: () => void;
  pollInterval?: number;
}

/**
 * Human Relay Service Options
 */
export interface HumanRelayServiceOptions {
  /** Base directory for functional files (default: ".wf-agent/function") */
  baseDir?: string;
}

/**
 * Human Relay Service
 * 
 * Handles Human Relay request/response flow via file system.
 * Writes prompts to output files and watches for user responses in input files.
 */
export class HumanRelayService {
  private baseDir: string;
  private watchers: Map<string, NodeJS.Timeout | FSWatcher> = new Map();

  constructor(options: HumanRelayServiceOptions = {}) {
    this.baseDir = options.baseDir ?? path.join(".wf-agent", "function");
  }

  /**
   * Initialize directories
   */
  async initialize(): Promise<void> {
    await fs.mkdir(this.baseDir, { recursive: true });
  }

  /**
   * Get Human Relay file paths for a session
   * @param sessionId Session identifier
   * @returns Paths for output and input files
   */
  getSessionPaths(sessionId: string): HumanRelayPaths {
    const sessionDir = path.join(this.baseDir, sessionId);

    return {
      output: path.join(sessionDir, "human-relay-output.txt"),
      input: path.join(sessionDir, "human-relay-input.txt"),
    };
  }

  /**
   * Write Human Relay output (prompt to be copied to web LLM)
   * Pure text format, no formatting
   * @param params Session ID and content
   */
  async writeOutput(params: {
    sessionId: string;
    content: string;
  }): Promise<void> {
    const paths = this.getSessionPaths(params.sessionId);
    const dir = path.dirname(paths.output);

    // Ensure directory exists
    await fs.mkdir(dir, { recursive: true });

    // Write pure text (no formatting)
    await fs.writeFile(paths.output, params.content, "utf-8");
  }

  /**
   * Read Human Relay input (user response from web LLM)
   * @param sessionId Session identifier
   * @returns Content of the input file
   */
  async readInput(sessionId: string): Promise<string> {
    const paths = this.getSessionPaths(sessionId);
    
    try {
      const content = await fs.readFile(paths.input, "utf-8");
      return content;
    } catch (error) {
      // Return empty string if file doesn't exist yet
      return "";
    }
  }

  /**
   * Watch Human Relay input file for changes
   * Monitors for user pasting LLM response
   * @param params Watch configuration
   */
  watchInput(params: HumanRelayWatchOptions): void {
    const paths = this.getSessionPaths(params.sessionId);
    const inputFile = paths.input;
    const dir = path.dirname(inputFile);

    // Create empty file if doesn't exist
    fs.writeFile(inputFile, "", "utf-8").catch(() => {});

    let responded = false;
    let lastSize = 0;

    // Close existing watcher if any
    this.unwatch(params.sessionId);

    // Watch for file change using polling (more reliable across platforms)
    const pollInterval = params.pollInterval ?? 500;
    const pollTimer = setInterval(async () => {
      if (responded) {
        clearInterval(pollTimer);
        return;
      }

      try {
        const stats = await fs.stat(inputFile);
        const currentSize = stats.size;

        // Check if file has content and size changed
        if (currentSize > 0 && currentSize !== lastSize) {
          const content = await fs.readFile(inputFile, "utf-8");
          
          // Only respond if content is not empty after trim
          if (content.trim().length > 0) {
            responded = true;
            clearInterval(pollTimer);
            this.watchers.delete(params.sessionId);
            params.onResponse(content);
          }
        }

        lastSize = currentSize;
      } catch (error) {
        // Ignore read errors during polling
      }
    }, pollInterval);

    // Store timer reference for cleanup
    const watcher: NodeJS.Timeout = pollTimer as unknown as NodeJS.Timeout;
    this.watchers.set(params.sessionId, watcher);

    // Timeout handler
    setTimeout(() => {
      if (!responded) {
        responded = true;
        clearInterval(pollTimer);
        this.watchers.delete(params.sessionId);
        params.onTimeout?.();
      }
    }, params.timeout);
  }

  /**
   * Stop watching Human Relay input
   * @param sessionId Session identifier
   */
  unwatch(sessionId: string): void {
    const watcher = this.watchers.get(sessionId);
    if (watcher) {
      watcher.close();
      this.watchers.delete(sessionId);
    }
  }

  /**
   * Close all watchers and cleanup resources
   */
  async dispose(): Promise<void> {
    // Close all active watchers
    const watchers = Array.from(this.watchers.entries());
    for (const [sessionId, watcher] of watchers) {
      watcher.close();
      this.watchers.delete(sessionId);
    }
  }
}
