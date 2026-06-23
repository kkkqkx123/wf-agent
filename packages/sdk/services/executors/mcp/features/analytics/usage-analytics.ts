/**
 * MCP Tools Usage Analytics
 *
 * Tracks and analyzes usage statistics of MCP tools and servers.
 * Provides insights about tool hotness, performance, error rates, and user behavior.
 */

import { createContextualLogger } from "../../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "McpToolsAnalytics" });

/**
 * Tool execution statistics
 */
export interface ToolExecutionStats {
  /** Total number of calls */
  callCount: number;
  /** Number of successful calls */
  successCount: number;
  /** Number of failed calls */
  failureCount: number;
  /** Average execution time in milliseconds */
  avgExecutionTime: number;
  /** Minimum execution time */
  minExecutionTime: number;
  /** Maximum execution time */
  maxExecutionTime: number;
  /** Last execution timestamp */
  lastExecutedAt?: number;
  /** First execution timestamp */
  firstExecutedAt?: number;
}

/**
 * Tool analytics entry
 */
export interface ToolAnalyticsEntry extends ToolExecutionStats {
  /** Tool identifier */
  toolId: string;
  /** Server name */
  serverName: string;
  /** Tool name */
  toolName: string;
  /** Success rate (0-100) */
  successRate: number;
  /** Error messages and their counts */
  errorCounts: Record<string, number>;
  /** Most recent error message */
  lastError?: string;
  /** User identifiers and their call counts */
  userCallCounts: Record<string, number>;
  /** Parameter usage patterns */
  parameterPatterns: Record<string, number>;
}

/**
 * Overall analytics report
 */
export interface AnalyticsReport {
  /** Total tools tracked */
  totalTools: number;
  /** Tools with at least one call */
  activeTools: number;
  /** Total calls across all tools */
  totalCalls: number;
  /** Overall success rate */
  overallSuccessRate: number;
  /** Most used tools (top N) */
  hotTools: ToolAnalyticsEntry[];
  /** Least used tools (bottom N) */
  coldTools: ToolAnalyticsEntry[];
  /** Tools with highest error rates */
  problematicTools: ToolAnalyticsEntry[];
  /** Per-server statistics */
  serverStats: Record<string, { toolCount: number; callCount: number; successRate: number }>;
  /** Report generation timestamp */
  generatedAt: number;
}

/**
 * Tool execution record for tracking
 */
interface ExecutionRecord {
  toolId: string;
  executionTime: number;
  success: boolean;
  error?: string;
  userId?: string;
  parameters?: Record<string, unknown>;
  timestamp: number;
}

/**
 * MCP Tools Usage Analytics
 *
 * Tracks detailed statistics about MCP tool usage and performance.
 */
export class McpToolsUsageAnalytics {
  private stats = new Map<string, ToolAnalyticsEntry>();
  private executionHistory: ExecutionRecord[] = [];
  private maxHistorySize: number;

  constructor(maxHistorySize: number = 10000) {
    this.maxHistorySize = maxHistorySize;
  }

  /**
   * Record a tool execution
   *
   * @param serverName - Server name
   * @param toolName - Tool name
   * @param executionTime - Execution time in milliseconds
   * @param success - Whether execution was successful
   * @param error - Error message if execution failed
   * @param userId - User identifier (optional)
   * @param parameters - Tool parameters (optional)
   */
  recordExecution(
    serverName: string,
    toolName: string,
    executionTime: number,
    success: boolean,
    error?: string,
    userId?: string,
    parameters?: Record<string, unknown>,
  ): void {
    const toolId = `${serverName}/${toolName}`;
    const now = Date.now();

    // Get or create stats entry
    if (!this.stats.has(toolId)) {
      this.stats.set(toolId, {
        toolId,
        serverName,
        toolName,
        callCount: 0,
        successCount: 0,
        failureCount: 0,
        avgExecutionTime: 0,
        minExecutionTime: Infinity,
        maxExecutionTime: -Infinity,
        successRate: 0,
        errorCounts: {},
        userCallCounts: {},
        parameterPatterns: {},
      });
    }

    const entry = this.stats.get(toolId)!;

    // Update basic stats
    entry.callCount++;
    if (success) {
      entry.successCount++;
    } else {
      entry.failureCount++;
    }

    // Update execution time stats
    entry.avgExecutionTime = (entry.avgExecutionTime * (entry.callCount - 1) + executionTime) / entry.callCount;
    entry.minExecutionTime = Math.min(entry.minExecutionTime, executionTime);
    entry.maxExecutionTime = Math.max(entry.maxExecutionTime, executionTime);

    // Update success rate
    entry.successRate = (entry.successCount / entry.callCount) * 100;

    // Track errors
    if (error) {
      entry.errorCounts[error] = (entry.errorCounts[error] || 0) + 1;
      entry.lastError = error;
    }

    // Track user calls
    if (userId) {
      entry.userCallCounts[userId] = (entry.userCallCounts[userId] || 0) + 1;
    }

    // Track parameter patterns
    if (parameters) {
      for (const [key] of Object.entries(parameters)) {
        entry.parameterPatterns[key] = (entry.parameterPatterns[key] || 0) + 1;
      }
    }

    // Update timestamps
    entry.lastExecutedAt = now;
    if (!entry.firstExecutedAt) {
      entry.firstExecutedAt = now;
    }

    // Add to execution history
    this.executionHistory.push({
      toolId,
      executionTime,
      success,
      error,
      userId,
      parameters,
      timestamp: now,
    });

    // Trim history if too large
    if (this.executionHistory.length > this.maxHistorySize) {
      this.executionHistory = this.executionHistory.slice(-this.maxHistorySize);
    }

    logger.debug("Execution recorded", {
      toolId,
      executionTime,
      success,
      totalCalls: entry.callCount,
    });
  }

  /**
   * Get statistics for a specific tool
   *
   * @param toolId - Tool identifier (serverName/toolName)
   * @returns Tool statistics or undefined if not found
   */
  getToolStats(toolId: string): ToolAnalyticsEntry | undefined {
    return this.stats.get(toolId);
  }

  /**
   * Get all tool statistics
   *
   * @returns Map of tool IDs to statistics
   */
  getAllToolStats(): Map<string, ToolAnalyticsEntry> {
    return new Map(this.stats);
  }

  /**
   * Get hot tools (most frequently called)
   *
   * @param limit - Maximum number of tools to return
   * @returns Top tools by call count
   */
  getHotTools(limit: number = 10): ToolAnalyticsEntry[] {
    return Array.from(this.stats.values())
      .sort((a, b) => b.callCount - a.callCount)
      .slice(0, limit);
  }

  /**
   * Get cold tools (least frequently called)
   *
   * @param limit - Maximum number of tools to return
   * @returns Bottom tools by call count
   */
  getColdTools(limit: number = 10): ToolAnalyticsEntry[] {
    return Array.from(this.stats.values())
      .filter(tool => tool.callCount > 0)
      .sort((a, b) => a.callCount - b.callCount)
      .slice(0, limit);
  }

  /**
   * Get problematic tools (highest error rates)
   *
   * @param limit - Maximum number of tools to return
   * @returns Tools with highest error rates
   */
  getProblematicTools(limit: number = 10): ToolAnalyticsEntry[] {
    return Array.from(this.stats.values())
      .filter(tool => tool.callCount > 0)
      .sort((a, b) => a.successRate - b.successRate)
      .slice(0, limit);
  }

  /**
   * Get server statistics
   *
   * @param serverName - Server name (optional, if not provided returns all servers)
   * @returns Server statistics
   */
  getServerStats(serverName?: string): Record<string, { toolCount: number; callCount: number; successRate: number }> {
    const stats: Record<string, { toolCount: number; callCount: number; successCount: number }> = {};

    for (const entry of this.stats.values()) {
      if (serverName && entry.serverName !== serverName) {
        continue;
      }

      if (!stats[entry.serverName]) {
        stats[entry.serverName] = {
          toolCount: 0,
          callCount: 0,
          successCount: 0,
        };
      }

      const serverStats = stats[entry.serverName]!;
      serverStats.toolCount++;
      serverStats.callCount += entry.callCount;
      serverStats.successCount += entry.successCount;
    }

    // Convert to final format
    const result: Record<string, { toolCount: number; callCount: number; successRate: number }> = {};
    for (const [server, data] of Object.entries(stats)) {
      const serverData = data as { toolCount: number; callCount: number; successCount: number };
      result[server] = {
        toolCount: serverData.toolCount,
        callCount: serverData.callCount,
        successRate: serverData.callCount > 0 ? (serverData.successCount / serverData.callCount) * 100 : 0,
      };
    }

    return result;
  }

  /**
   * Generate comprehensive analytics report
   *
   * @param limit - Limit for hot/cold/problematic tools
   * @returns Analytics report
   */
  generateReport(limit: number = 10): AnalyticsReport {
    const allStats = Array.from(this.stats.values());
    const activeTools = allStats.filter(t => t.callCount > 0);
    const totalCalls = allStats.reduce((sum, t) => sum + t.callCount, 0);
    const totalSuccesses = allStats.reduce((sum, t) => sum + t.successCount, 0);

    return {
      totalTools: this.stats.size,
      activeTools: activeTools.length,
      totalCalls,
      overallSuccessRate: totalCalls > 0 ? (totalSuccesses / totalCalls) * 100 : 0,
      hotTools: this.getHotTools(limit),
      coldTools: this.getColdTools(limit),
      problematicTools: this.getProblematicTools(limit),
      serverStats: this.getServerStats(),
      generatedAt: Date.now(),
    };
  }

  /**
   * Get execution history
   *
   * @param toolId - Filter by tool ID (optional)
   * @param limit - Maximum number of records to return
   * @returns Execution history records
   */
  getExecutionHistory(toolId?: string, limit: number = 100): ExecutionRecord[] {
    let history = this.executionHistory;

    if (toolId) {
      history = history.filter(r => r.toolId === toolId);
    }

    return history.slice(-limit);
  }

  /**
   * Clear all analytics data
   *
   * Useful for testing or reset scenarios.
   */
  clear(): void {
    this.stats.clear();
    this.executionHistory = [];
    logger.debug("Analytics data cleared");
  }

  /**
   * Export analytics data as JSON
   *
   * @returns JSON string of all analytics data
   */
  exportJSON(): string {
    const report = this.generateReport(20);
    return JSON.stringify(report, null, 2);
  }

  /**
   * Get analytics summary string
   *
   * @returns Human-readable summary
   */
  getSummary(): string {
    const report = this.generateReport();
    const lines: string[] = [];

    lines.push("# MCP Tools Usage Analytics Summary");
    lines.push("");
    lines.push(`**Generated**: ${new Date(report.generatedAt).toISOString()}`);
    lines.push("");
    lines.push(`## Overview`);
    lines.push(`- Total Tools: ${report.totalTools}`);
    lines.push(`- Active Tools: ${report.activeTools}`);
    lines.push(`- Total Calls: ${report.totalCalls}`);
    lines.push(`- Overall Success Rate: ${report.overallSuccessRate.toFixed(2)}%`);
    lines.push("");

    lines.push(`## Top 5 Hot Tools`);
    for (const tool of report.hotTools.slice(0, 5)) {
      lines.push(`- \`${tool.toolId}\`: ${tool.callCount} calls (${tool.successRate.toFixed(2)}% success)`);
    }
    lines.push("");

    lines.push(`## Top 5 Problematic Tools`);
    for (const tool of report.problematicTools.slice(0, 5)) {
      lines.push(`- \`${tool.toolId}\`: ${tool.successRate.toFixed(2)}% success rate`);
    }
    lines.push("");

    lines.push(`## Per-Server Stats`);
    for (const [server, stats] of Object.entries(report.serverStats)) {
      lines.push(
        `- \`${server}\`: ${stats.toolCount} tools, ${stats.callCount} calls (${stats.successRate.toFixed(2)}% success)`,
      );
    }

    return lines.join("\n");
  }
}
