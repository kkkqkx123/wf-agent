/**
 * Iteration Panel Component
 * 
 * Displays iteration progress for agent loop execution.
 * Shows iteration count, tool calls, messages, and status.
 */

import type { Component } from "../core/tui.js";

// Local type for agent iteration data (simplified, avoiding legacy component-message types)
interface AgentIterationData {
  iteration: number;
  toolCallCount: number;
  duration?: number;
}

interface IterationInfo {
  number: number;
  toolCallCount: number;
  messageCount: number;
  status: "running" | "waiting" | "completed" | "error";
  startTime: number;
  endTime?: number;
  duration?: number;
}

export interface IterationPanelOptions {
  maxHeight?: number;
}

export class IterationPanel implements Component {
  private iterations: Map<number, IterationInfo> = new Map();
  private maxHeight: number;

  constructor(options: IterationPanelOptions = {}) {
    this.maxHeight = options.maxHeight ?? 10;
  }

  /**
   * Update or add iteration information
   */
  updateIteration(data: AgentIterationData) {
    const existing = this.iterations.get(data.iteration);
    
    if (existing) {
      // Update existing iteration
      existing.toolCallCount = data.toolCallCount ?? 0;
      
      if (data.duration !== undefined) {
        existing.status = "completed";
        existing.endTime = Date.now();
        existing.duration = data.duration;
      }
    } else {
      // Create new iteration
      this.iterations.set(data.iteration, {
        number: data.iteration,
        toolCallCount: data.toolCallCount ?? 0,
        messageCount: 0,
        status: "running",
        startTime: Date.now(),
      });
    }

    this.render();
  }

  /**
   * Mark iteration as completed
   */
  completeIteration(iterationNumber: number, duration?: number) {
    const iteration = this.iterations.get(iterationNumber);
    if (iteration) {
      iteration.status = "completed";
      iteration.endTime = Date.now();
      iteration.duration = duration ?? (iteration.endTime - iteration.startTime);
      this.render();
    }
  }

  /**
   * Mark iteration as error
   */
  errorIteration(iterationNumber: number) {
    const iteration = this.iterations.get(iterationNumber);
    if (iteration) {
      iteration.status = "error";
      iteration.endTime = Date.now();
      iteration.duration = iteration.endTime - iteration.startTime;
      this.render();
    }
  }

  /**
   * Clear all iterations
   */
  clear() {
    this.iterations.clear();
  }

  render(width?: number): string[] {
    const lines: string[] = [];
    
    // Header
    lines.push("=== Iteration Progress ===");
    lines.push("");

    // Sort iterations by number
    const sortedIterations = Array.from(this.iterations.entries())
      .sort((a, b) => a[0] - b[0]);

    // Limit to max height
    const displayIterations = sortedIterations.slice(-this.maxHeight);

    for (const [_, info] of displayIterations) {
      const icon = this.getStatusIcon(info.status);
      const duration = info.duration ? `${Math.round(info.duration / 1000)}s` : "...";
      
      let line = `${icon} Iteration ${info.number}: ` +
        `${info.toolCallCount} tools, ` +
        `${duration}`;
      
      // If width is provided, truncate line if needed
      if (width !== undefined && line.length > width) {
        line = line.substring(0, width - 3) + "...";
      }
      
      lines.push(line);
    }

    // If we have more iterations than displayed, show indicator
    if (sortedIterations.length > this.maxHeight) {
      const hidden = sortedIterations.length - this.maxHeight;
      let summaryLine = `... and ${hidden} more iterations`;
      
      // Truncate summary line if width is provided
      if (width !== undefined && summaryLine.length > width) {
        summaryLine = summaryLine.substring(0, width - 3) + "...";
      }
      
      lines.push(summaryLine);
    }

    return lines;
  }

  private getStatusIcon(status: IterationInfo["status"]): string {
    switch (status) {
      case "running":
        return "▶️";
      case "waiting":
        return "⏸️";
      case "completed":
        return "✓";
      case "error":
        return "✗";
      default:
        return "•";
    }
  }

  handleInput?(_data: string): boolean {
    return false;
  }

  invalidate(): void {
    // No-op: IterationPanel doesn't maintain complex state that needs invalidation
    // The render method will always reflect the current state of iterations
  }
}
