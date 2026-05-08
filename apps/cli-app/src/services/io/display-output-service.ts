/**
 * Display Output Service
 * 
 * Manages display file operations for CLI-App.
 * Handles human-readable output aggregation into output.md files.
 */

import * as fs from "fs/promises";
import * as path from "path";

/**
 * Display section for output.md aggregation
 */
export interface DisplaySection {
  title: string;
  content: string;
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
 * Display Output Service Options
 */
export interface DisplayOutputServiceOptions {
  /** Base directory for display files (default: ".wf-agent/display") */
  baseDir?: string;
  /** Auto cleanup old sessions (default: true) */
  autoCleanup?: boolean;
  /** Retention days for old sessions (default: 7) */
  retentionDays?: number;
}

/**
 * Display Output Service
 * 
 * Manages output.md files for human-readable workflow execution logs.
 * Provides incremental updates and session metadata management.
 */
export class DisplayOutputService {
  private baseDir: string;
  private autoCleanup: boolean;
  private retentionDays: number;

  constructor(options: DisplayOutputServiceOptions = {}) {
    this.baseDir = options.baseDir ?? path.join(".wf-agent", "display");
    this.autoCleanup = options.autoCleanup ?? true;
    this.retentionDays = options.retentionDays ?? 7;
  }

  /**
   * Initialize directories
   */
  async initialize(): Promise<void> {
    await fs.mkdir(this.baseDir, { recursive: true });
  }

  /**
   * Get display output file path for a session
   * @param sessionId Session identifier
   * @returns Path to output.md file
   */
  getOutputPath(sessionId: string): string {
    const sessionDir = path.join(this.baseDir, sessionId);
    return path.join(sessionDir, "output.md");
  }

  /**
   * Update display output (output.md) with aggregated sections
   * Incrementally updates the file
   * @param params Session ID and sections to append
   */
  async updateOutput(params: {
    sessionId: string;
    sections: DisplaySection[];
    metadata?: SessionMetadata;
    append?: boolean;
  }): Promise<void> {
    const outputFile = this.getOutputPath(params.sessionId);
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

    await this.updateOutput({
      sessionId: params.sessionId,
      sections,
      metadata: params.metadata,
      append: false,
    });
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

    await this.updateOutput({
      sessionId: params.sessionId,
      sections: [{
        title: "执行日志",
        content,
      }],
      append: true,
    });
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
      await this.cleanupDirectory(this.baseDir, cutoffTime);
    } catch (error) {
      // Log error but don't fail
      console.warn("Failed to cleanup old sessions:", error);
    }
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
   * Dispose service and perform cleanup
   */
  async dispose(): Promise<void> {
    await this.cleanupOldSessions();
  }
}
