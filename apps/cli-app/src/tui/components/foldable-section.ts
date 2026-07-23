/**
 * FoldableSection Component
 *
 * A collapsible UI section that wraps a child component with a foldable header.
 * In collapsed state only the summary/title line is shown; in expanded state
 * the full content is displayed. Used to keep long agent logs navigable.
 *
 * Space (in Normal mode) toggles between collapsed / expanded.
 */

import type { Component } from "../core/tui.js";
import { visibleWidth } from "../core/utils.js";

export interface FoldableSectionOptions {
  /** Whether the section starts collapsed (default: false) */
  collapsed?: boolean;
  /** Max chars for the summary preview when collapsed */
  summaryMaxLength?: number;
}

export class FoldableSection implements Component {
  private title: string;
  private content: Component;
  private collapsed: boolean;

  /** Unique id so AgentScreen can identify which section to toggle */
  public readonly id: string;

  constructor(id: string, title: string, content: Component, options: FoldableSectionOptions = {}) {
    this.id = id;
    this.title = title;
    this.content = content;
    this.collapsed = options.collapsed ?? false;
  }

  /** Get the current collapsed state */
  isCollapsed(): boolean {
    return this.collapsed;
  }

  /** Toggle between collapsed and expanded */
  toggleFold(): void {
    this.collapsed = !this.collapsed;
  }

  /** Set collapsed state directly */
  setCollapsed(collapsed: boolean): void {
    this.collapsed = collapsed;
  }

  invalidate(): void {
    this.content.invalidate?.();
  }

  render(width: number): string[] {
    const lines: string[] = [];

    if (this.collapsed) {
      // Render content temporarily to count lines for summary
      const contentLines = this.content.render(width);
      const lineCount = contentLines.length;

      // Build summary line: "[+] Title (N lines)"
      let summary = `[+] ${this.title} (${lineCount} lines)`;
      if (visibleWidth(summary) > width && width > 10) {
        summary = `[+] ${this.title}`;
        if (visibleWidth(summary) > width - 5) {
          summary = `[+] ${this.title.substring(0, Math.max(10, width - 15))}...`;
        }
        summary += ` (${lineCount} lines)`;
      }
      lines.push(summary);
    } else {
      // Expanded: header line with collapse hint, then content
      lines.push(`[-] ${this.title}`);
      const rendered = this.content.render(width);
      for (const line of rendered) {
        lines.push(line);
      }
    }

    return lines;
  }
}
