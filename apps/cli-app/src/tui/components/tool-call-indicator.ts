/**
 * Tool Call Indicator Component
 *
 * Visualizes active and completed tool calls during agent execution.
 * Supports expand/collapse per completed call: press Enter on a title
 * line to toggle details (arguments, duration). Active calls always
 * show a compact title line.
 */

import { getKeybindings } from "../core/keybindings.js";
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
}

export class ToolCallIndicator implements Component {
  private activeCalls: Map<string, ToolCallInfo> = new Map();
  private completedCalls: ToolCallInfo[] = [];
  private maxDisplayCalls: number;

  /** Index into completedCalls of the currently expanded entry, or -1 if none. */
  private expandedIndex: number = -1;

  constructor(options: ToolCallIndicatorOptions = {}) {
    this.maxDisplayCalls = options.maxDisplayCalls ?? 5;
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

      // Reset expansion if the expanded call was culled
      if (this.expandedIndex >= this.completedCalls.length) {
        this.expandedIndex = -1;
      }
    }
  }

  /**
   * Clear all tool calls
   */
  clear() {
    this.activeCalls.clear();
    this.completedCalls = [];
    this.expandedIndex = -1;
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

        lines.push(`  ${icon} ${call.name} (${elapsed}s)`);
      }

      lines.push("");
    }

    // Recent completed calls
    if (this.completedCalls.length > 0) {
      lines.push("=== Recent Tool Calls (Enter to expand/collapse) ===");
      lines.push("");

      const recentCalls = this.completedCalls.slice(-this.maxDisplayCalls);
      const offset = this.completedCalls.length - recentCalls.length;

      for (let i = 0; i < recentCalls.length; i++) {
        const call = recentCalls[i]!;
        const globalIndex = offset + i;
        const isExpanded = globalIndex === this.expandedIndex;
        const icon = call.status === "completed" ? "✓" : "✗";
        const duration = call.duration ? `${Math.round(call.duration)}ms` : "N/A";
        const expandMarker = isExpanded ? "▼" : "▶";

        lines.push(`  ${expandMarker} ${icon} ${call.name} (${duration})`);

        // Show details when expanded
        if (isExpanded && call.arguments) {
          const argsStr = this.formatArguments(call.arguments, width);
          if (argsStr) {
            lines.push(`     Args: ${argsStr}`);
          }
        }
      }
    }

    return lines;
  }

  /**
   * Toggle expand/collapse of the completed call at the given index.
   * Returns true if the input was consumed (Enter on a completed call line).
   */
  handleInput(data: string): boolean {
    const kb = getKeybindings();

    // Enter or Space toggles the last touched completed call
    if (kb.matches(data, "tui.select.confirm") || data === " ") {
      const recentCalls = this.completedCalls.slice(-this.maxDisplayCalls);
      if (recentCalls.length === 0) return false;

      const offset = this.completedCalls.length - recentCalls.length;
      const lastGlobalIndex = offset + recentCalls.length - 1;

      // Toggle: if already expanded, collapse; otherwise expand the newest
      if (this.expandedIndex === lastGlobalIndex) {
        this.expandedIndex = -1;
      } else {
        this.expandedIndex = lastGlobalIndex;
      }
      return true;
    }

    return false;
  }

  /**
   * Format arguments for display
   */
  private formatArguments(args: Record<string, unknown>, maxWidth?: number): string {
    try {
      const json = JSON.stringify(args, null, 2);

      if (!maxWidth || json.length <= maxWidth - 7) {
        return json;
      }

      // Truncate with ellipsis
      return json.substring(0, maxWidth - 10) + "...";
    } catch (_error) {
      return "[Invalid arguments]";
    }
  }

  invalidate(): void {
    // No caching, re-render on demand
  }
}
