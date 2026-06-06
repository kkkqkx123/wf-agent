/**
 * Tool Call Indicator Component
 * 
 * Visualizes active and completed tool calls during agent execution.
 * Shows tool name, status, duration, and arguments preview.
 */

import type { Component } from "../core/tui.js";

// Local types for tool call data (simplified, avoiding legacy component-message types)
interface AgentToolCallData {
  toolCallId: string;
  toolName: string;
  arguments?: Record<string, unknown>;
}

interface AgentToolEndData {
  toolCallId: string;
  success: boolean;
  duration?: number;
}

interface ToolCallInfo {
  id: string;
  name: string;
  arguments?: Record<string, unknown>;
  startTime: number;
  endTime?: number;
  duration?: number;
  status: "running" | "completed" | "failed";
}

export interface ToolCallIndicatorOptions {
  maxDisplayCalls?: number;
  showArguments?: boolean;
}

export class ToolCallIndicator implements Component {
  private activeCalls: Map<string, ToolCallInfo> = new Map();
  private completedCalls: ToolCallInfo[] = [];
  private maxDisplayCalls: number;
  private showArguments: boolean;

  constructor(options: ToolCallIndicatorOptions = {}) {
    this.maxDisplayCalls = options.maxDisplayCalls ?? 5;
    this.showArguments = options.showArguments ?? false;
  }

  /**
   * Handle tool call start
   */
  handleToolCallStart(data: AgentToolCallData) {
    const call: ToolCallInfo = {
      id: data.toolCallId,
      name: data.toolName,
      arguments: data.arguments,
      startTime: Date.now(),
      status: "running",
    };

    this.activeCalls.set(call.id, call);
  }

  /**
   * Handle tool call end
   */
  handleToolCallEnd(data: AgentToolEndData) {
    const call = this.activeCalls.get(data.toolCallId);
    if (call) {
      call.status = data.success ? "completed" : "failed";
      call.endTime = Date.now();
      call.duration = data.duration ?? (call.endTime - call.startTime);

      // Move to completed list
      this.activeCalls.delete(call.id);
      this.completedCalls.push(call);

      // Keep only recent completed calls
      if (this.completedCalls.length > this.maxDisplayCalls * 2) {
        this.completedCalls = this.completedCalls.slice(-this.maxDisplayCalls);
      }
    }
  }

  /**
   * Clear all tool calls
   */
  clear() {
    this.activeCalls.clear();
    this.completedCalls = [];
  }

  render(width?: number): string[] {
    const lines: string[] = [];

    // Active calls section
    if (this.activeCalls.size > 0) {
      lines.push("=== Active Tool Calls ===");
      lines.push("");

      for (const call of this.activeCalls.values()) {
        const icon = "🔄";
        const elapsed = Math.round((Date.now() - call.startTime) / 1000);
        
        lines.push(`${icon} ${call.name} (${elapsed}s)`);
        
        if (this.showArguments && call.arguments) {
          const argsPreview = this.formatArguments(call.arguments, width);
          if (argsPreview) {
            lines.push(`   Args: ${argsPreview}`);
          }
        }
      }
      
      lines.push("");
    }

    // Recent completed calls
    if (this.completedCalls.length > 0) {
      lines.push("=== Recent Tool Calls ===");
      lines.push("");

      const recentCalls = this.completedCalls.slice(-this.maxDisplayCalls);
      
      for (const call of recentCalls) {
        const icon = call.status === "completed" ? "✓" : "✗";
        const duration = call.duration ? `${Math.round(call.duration)}ms` : "N/A";
        
        lines.push(`${icon} ${call.name} (${duration})`);
        
        if (this.showArguments && call.arguments) {
          const argsPreview = this.formatArguments(call.arguments, width);
          if (argsPreview) {
            lines.push(`   Args: ${argsPreview}`);
          }
        }
      }
    }

    return lines;
  }

  /**
   * Format arguments for display
   */
  private formatArguments(args: Record<string, unknown>, maxWidth?: number): string {
    try {
      const json = JSON.stringify(args);
      
      if (!maxWidth || json.length <= maxWidth - 7) {
        return json;
      }
      
      // Truncate with ellipsis
      return json.substring(0, maxWidth - 10) + "...";
    } catch (_error) {
      return "[Invalid arguments]";
    }
  }

  handleInput?(_data: string): boolean {
    return false;
  }

  invalidate(): void {
    // No caching, re-render on demand
  }
}
