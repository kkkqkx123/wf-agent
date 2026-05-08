/**
 * File IO Service
 * 
 * Manages functional and display file operations for CLI-App.
 * Follows file-io-prd.md specification.
 */

import * as fs from "fs/promises";
import * as path from "path";
import { FSWatcher } from "fs";

/**
 * Display section for output.md aggregation
 */
export interface DisplaySection {
  title: string;
  content: string;
}

/**
 * Human Relay paths for a session
 */
export interface HumanRelayPaths {
  functional: {
    humanRelayOutput: string;
    humanRelayInput: string;
  };
  display: {
    output: string;
  };
}

/**
 * Session metadata for output.md frontmatter
 */
export interface SessionMetadata {
  instanceId: string;
  type: "agent" | "graph" | "workflow-execution";
  parentId?: string | null;
  startedAt: number;
  name?: string;
}

/**
 * File IO Service Options
 */
export interface FileIOServiceOptions {
  /** Base directory for all IO files (default: ".wf-agent") */
  baseDir?: string;
  /** Functional files directory (default: "{baseDir}/function") */
  functionalDir?: string;
  /** Display files directory (default: "{baseDir}/display") */
  displayDir?: string;
  /** Auto cleanup old sessions (default: true) */
  autoCleanup?: boolean;
  /** Retention days for old sessions (default: 7) */
  retentionDays?: number;
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
 * File IO Service
 * 
 * Handles both functional (program-to-program) and display (human-readable) file operations.
 */
export class FileIOService {
  private baseDir: string;
  private functionalDir: string;
  private displayDir: string;
  private autoCleanup: boolean;
  private retentionDays: number;
  private watchers: Map<string, NodeJS.Timeout | FSWatcher> = new Map();

  constructor(options: FileIOServiceOptions = {}) {
    this.baseDir = options.baseDir ?? ".wf-agent";
    this.functionalDir = options.functionalDir ?? path.join(this.baseDir, "function");
    this.displayDir = options.displayDir ?? path.join(this.baseDir, "display");
    this.autoCleanup = options.autoCleanup ?? true;
    this.retentionDays = options.retentionDays ?? 7;
  }

  /**
   * Initialize directories
   */
  async initialize(): Promise<void> {
    await fs.mkdir(this.functionalDir, { recursive: true });
    await fs.mkdir(this.displayDir, { recursive: true });
  }

  /**
   * Generate session paths
   * @param sessionId Session identifier
   * @returns Paths for functional and display files
   */
  getSessionPaths(sessionId: string): HumanRelayPaths {
    const functionDir = path.join(this.functionalDir, sessionId);
    const displayDir = path.join(this.displayDir, sessionId);

    return {
      functional: {
        humanRelayOutput: path.join(functionDir, "human-relay-output.txt"),
        humanRelayInput: path.join(functionDir, "human-relay-input.txt"),
      },
      display: {
        output: path.join(displayDir, "output.md"),
      },
    };
  }

  /**
   * Write Human Relay output (prompt to be copied to web LLM)
   * Pure text format, no formatting
   * @param params Session ID and content
   */
  async writeHumanRelayOutput(params: {
    sessionId: string;
    content: string;
  }): Promise<void> {
    const paths = this.getSessionPaths(params.sessionId);
    const dir = path.dirname(paths.functional.humanRelayOutput);

    // Ensure directory exists
    await fs.mkdir(dir, { recursive: true });

    // Write pure text (no formatting)
    await fs.writeFile(paths.functional.humanRelayOutput, params.content, "utf-8");
  }

  /**
   * Read Human Relay input (user response from web LLM)
   * @param sessionId Session identifier
   * @returns Content of the input file
   */
  async readHumanRelayInput(sessionId: string): Promise<string> {
    const paths = this.getSessionPaths(sessionId);
    
    try {
      const content = await fs.readFile(paths.functional.humanRelayInput, "utf-8");
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
  watchHumanRelayInput(params: HumanRelayWatchOptions): void {
    const paths = this.getSessionPaths(params.sessionId);
    const inputFile = paths.functional.humanRelayInput;
    const dir = path.dirname(inputFile);

    // Create empty file if doesn't exist
    fs.writeFile(inputFile, "", "utf-8").catch(() => {});

    let responded = false;
    let lastSize = 0;

    // Close existing watcher if any
    this.unwatchHumanRelayInput(params.sessionId);

    // Watch for file changes using polling (more reliable across platforms)
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
    this.watchers.set(params.sessionId, { close: () => clearInterval(pollTimer) } as any);

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
  unwatchHumanRelayInput(sessionId: string): void {
    const watcher = this.watchers.get(sessionId);
    if (watcher) {
      watcher.close();
      this.watchers.delete(sessionId);
    }
  }

  /**
   * Update display output (output.md) with aggregated sections
   * Incrementally updates the file
   * @param params Session ID and sections to append
   */
  async updateDisplayOutput(params: {
    sessionId: string;
    sections: DisplaySection[];
    metadata?: SessionMetadata;
    append?: boolean;
  }): Promise<void> {
    const paths = this.getSessionPaths(params.sessionId);
    const outputFile = paths.display.output;
    const dir = path.dirname(outputFile);

    await fs.mkdir(dir, { recursive: true });

    let content: string;

    if (params.append) {
      // Read existing content
      try {
        content = await fs.readFile(outputFile, "utf-8");
      } catch (error) {
        content = "";
      }
    } else {
      // Start fresh with metadata frontmatter
      content = this.buildFrontmatter(params.metadata);
      content += "\n# Workflow Execution Output\n\n======\n\n";
    }

    // Append new sections
    for (const section of params.sections) {
      content += `## ${section.title}\n\n`;
      content += `${section.content}\n\n`;
      content += "══════════════════════════════\n\n";
    }

    await fs.writeFile(outputFile, content, "utf-8");
  }

  /**
   * Build YAML-like frontmatter for output.md
   * @param metadata Session metadata
   * @returns Frontmatter string
   */
  private buildFrontmatter(metadata?: SessionMetadata): string {
    if (!metadata) {
      return "";
    }

    let frontmatter = "---\n";
    frontmatter += `instanceId: ${metadata.instanceId}\n`;
    frontmatter += `type: ${metadata.type}\n`;
    frontmatter += `parentId: ${metadata.parentId ?? "null"}\n`;
    frontmatter += `startedAt: ${metadata.startedAt}\n`;
    if (metadata.name) {
      frontmatter += `name: ${metadata.name}\n`;
    }
    frontmatter += "---\n\n";

    return frontmatter;
  }

  /**
   * Initialize output.md with basic info
   * @param params Session ID and metadata
   */
  async initializeOutput(params: {
    sessionId: string;
    metadata: SessionMetadata;
    initialSections?: DisplaySection[];
  }): Promise<void> {
    const sections: DisplaySection[] = params.initialSections ?? [];

    // Add basic info section
    sections.unshift({
      title: "基本信息",
      content: this.buildBasicInfoContent(params.metadata),
    });

    await this.updateDisplayOutput({
      sessionId: params.sessionId,
      sections,
      metadata: params.metadata,
      append: false,
    });
  }

  /**
   * Build basic info content for output.md
   * @param metadata Session metadata
   * @returns Formatted basic info string
   */
  private buildBasicInfoContent(metadata: SessionMetadata): string {
    const startTime = new Date(metadata.startedAt).toLocaleString("zh-CN");
    
    let content = `- **实例名称**: ${metadata.name ?? metadata.instanceId}\n`;
    content += `- **实例 ID**: ${metadata.instanceId}\n`;
    content += `- **类型**: ${metadata.type}\n`;
    content += `- **开始时间**: ${startTime}\n`;
    content += `- **状态**: 运行中\n`;

    if (metadata.parentId) {
      content += `- **父实例**: ${metadata.parentId}\n`;
    }

    return content;
  }

  /**
   * Append execution log entry to output.md
   * @param params Session ID and log entry
   */
  async appendExecutionLog(params: {
    sessionId: string;
    timestamp: number;
    nodeId?: string;
    nodeType?: string;
    status: "start" | "end" | "error" | "waiting";
    message: string;
    duration?: number;
  }): Promise<void> {
    const timeStr = new Date(params.timestamp).toLocaleTimeString("zh-CN");
    const statusIcon = this.getStatusIcon(params.status);
    
    let content = `### [${timeStr}]`;
    if (params.nodeId) {
      content += ` Node: ${params.nodeId}`;
      if (params.nodeType) {
        content += ` (${params.nodeType})`;
      }
    }
    content += `\n`;
    content += `状态: ${statusIcon} ${this.getStatusText(params.status)}\n`;
    
    if (params.duration) {
      content += `耗时: ${params.duration}ms\n`;
    }
    
    content += `${params.message}\n`;

    await this.updateDisplayOutput({
      sessionId: params.sessionId,
      sections: [{
        title: "执行日志",
        content,
      }],
      append: true,
    });
  }

  /**
   * Get status icon for output.md
   * @param status Status string
   * @returns Icon character
   */
  private getStatusIcon(status: string): string {
    switch (status) {
      case "start":
        return "▶";
      case "end":
        return "✓";
      case "error":
        return "✗";
      case "waiting":
        return "⏳";
      default:
        return "•";
    }
  }

  /**
   * Get status text in Chinese
   * @param status Status string
   * @returns Status text
   */
  private getStatusText(status: string): string {
    switch (status) {
      case "start":
        return "开始";
      case "end":
        return "完成";
      case "error":
        return "错误";
      case "waiting":
        return "等待中";
      default:
        return status;
    }
  }

  /**
   * Cleanup old sessions based on retention policy
   */
  async cleanupOldSessions(): Promise<void> {
    if (!this.autoCleanup) {
      return;
    }

    const cutoffTime = Date.now() - this.retentionDays * 24 * 60 * 60 * 1000;

    try {
      // Cleanup functional directory
      await this.cleanupDirectory(this.functionalDir, cutoffTime);
      
      // Cleanup display directory
      await this.cleanupDirectory(this.displayDir, cutoffTime);
    } catch (error) {
      // Log error but don't fail
      console.warn("Failed to cleanup old sessions:", error);
    }
  }

  /**
   * Cleanup a directory recursively
   * @param dirPath Directory path
   * @param cutoffTime Cutoff timestamp
   */
  private async cleanupDirectory(dirPath: string, cutoffTime: number): Promise<void> {
    try {
      const entries = await fs.readdir(dirPath, { withFileTypes: true });

      for (const entry of entries) {
        if (entry.isDirectory()) {
          const fullPath = path.join(dirPath, entry.name);
          const stats = await fs.stat(fullPath);

          if (stats.mtimeMs < cutoffTime) {
            await fs.rm(fullPath, { recursive: true, force: true });
          }
        }
      }
    } catch (error) {
      // Ignore errors for non-existent directories
    }
  }

  /**
   * Close all watchers and cleanup resources
   */
  async close(): Promise<void> {
    // Close all active watchers
    for (const [sessionId, watcher] of this.watchers.entries()) {
      watcher.close();
      this.watchers.delete(sessionId);
    }

    // Perform cleanup if enabled
    await this.cleanupOldSessions();
  }
}
