import type { Component } from "../core/tui.js";
import { truncateToWidth, visibleWidth } from "../core/utils.js";

const DIFF_GREEN = "\x1b[32m";
const DIFF_RED = "\x1b[31m";
const DIFF_CYAN = "\x1b[36m";
const DIFF_GRAY = "\x1b[90m";
const DIFF_RESET = "\x1b[0m";

export interface DiffPanelOptions {
  maxVisibleLines?: number;
  lineNumbers?: boolean;
  title?: string;
}

export class DiffPanel implements Component {
  private lines: string[] = [];
  private scrollOffset = 0;
  private maxVisibleLines: number;
  private title?: string;

  constructor(options: DiffPanelOptions = {}) {
    this.maxVisibleLines = options.maxVisibleLines ?? 20;
    this.title = options.title;
  }

  setDiff(lines: string[]): void {
    this.lines = lines;
    this.scrollOffset = 0;
  }

  scrollUp(n: number = 1): void {
    this.scrollOffset = Math.max(0, this.scrollOffset - n);
  }

  scrollDown(n: number = 1): void {
    const maxOffset = Math.max(0, this.lines.length - this.maxVisibleLines);
    this.scrollOffset = Math.min(maxOffset, this.scrollOffset + n);
  }

  scrollToTop(): void {
    this.scrollOffset = 0;
  }

  scrollToBottom(): void {
    this.scrollOffset = Math.max(0, this.lines.length - this.maxVisibleLines);
  }

  invalidate(): void {}

  render(width: number): string[] {
    const result: string[] = [];

    if (this.title) {
      const titleLine = `${DIFF_GRAY}--- ${this.title} ---${DIFF_RESET}`;
      result.push(titleLine);
    }

    const visibleLines = this.lines.slice(
      this.scrollOffset,
      this.scrollOffset + this.maxVisibleLines,
    );

    for (const line of visibleLines) {
      let colored: string;
      if (line.startsWith("+") && !line.startsWith("+++")) {
        colored = `${DIFF_GREEN}${line}${DIFF_RESET}`;
      } else if (line.startsWith("-") && !line.startsWith("---")) {
        colored = `${DIFF_RED}${line}${DIFF_RESET}`;
      } else if (line.startsWith("@@")) {
        colored = `${DIFF_CYAN}${line}${DIFF_RESET}`;
      } else if (line.startsWith("diff --git") || line.startsWith("index ") || line.startsWith("---") || line.startsWith("+++")) {
        colored = `${DIFF_GRAY}${line}${DIFF_RESET}`;
      } else {
        colored = line;
      }

      if (visibleWidth(colored) > width) {
        result.push(truncateToWidth(colored, width, ""));
      } else {
        result.push(colored);
      }
    }

    const hasMoreBefore = this.scrollOffset > 0;
    const hasMoreAfter = this.scrollOffset + this.maxVisibleLines < this.lines.length;

    if (hasMoreBefore || hasMoreAfter) {
      const info = `${hasMoreBefore ? "↑" : " "} ${this.scrollOffset + 1}-${Math.min(this.lines.length, this.scrollOffset + visibleLines.length)}/${this.lines.length} ${hasMoreAfter ? "↓" : " "}`;
      result.push(`${DIFF_GRAY}${info}${DIFF_RESET}`);
    }

    return result;
  }
}
