import type { Terminal } from "./terminal.js";
import type { Renderer } from "./renderer.js";
import { SYNCHRONIZED_OUTPUT_BEGIN, SYNCHRONIZED_OUTPUT_END } from "./renderer.js";

export class RetainedRenderer implements Renderer {
  private previousLines: string[] = [];
  private previousWidth = 0;
  private previousHeight = 0;
  private cursorRow = 0;
  private hardwareCursorRow = 0;
  private maxLinesRendered = 0;
  private fullRedrawCount = 0;

  constructor(private terminal: Terminal) {}

  get stats(): { fullRedraws: number } {
    return { fullRedraws: this.fullRedrawCount };
  }

  reset(): void {
    this.previousLines = [];
    this.previousWidth = -1;
    this.previousHeight = -1;
    this.cursorRow = 0;
    this.hardwareCursorRow = 0;
    this.maxLinesRendered = 0;
  }

  clearScreen(): void {
    this.terminal.clearScreen();
  }

  flush(): void {
    // No buffering in retained mode
  }

  shutdown(): void {
    if (this.previousLines.length > 0) {
      const targetRow = this.previousLines.length;
      const lineDiff = targetRow - this.hardwareCursorRow;
      if (lineDiff > 0) {
        this.terminal.write(`\x1b[${lineDiff}B`);
      } else if (lineDiff < 0) {
        this.terminal.write(`\x1b[${-lineDiff}A`);
      }
      this.terminal.write("\r\n");
    }
    this.terminal.showCursor();
  }

  beginSync(): void {
    this.terminal.write(SYNCHRONIZED_OUTPUT_BEGIN);
  }

  endSync(): void {
    this.terminal.write(SYNCHRONIZED_OUTPUT_END);
  }

  render(lines: string[], width: number, height: number): void {
    const widthChanged = this.previousWidth !== 0 && this.previousWidth !== width;
    const heightChanged = this.previousHeight !== 0 && this.previousHeight !== height;

    // Helper to clear and render all new lines
    const fullRender = (clear: boolean): void => {
      this.fullRedrawCount += 1;
      let buffer = SYNCHRONIZED_OUTPUT_BEGIN;
      if (clear) buffer += "\x1b[2J\x1b[H\x1b[3J";

      for (let i = 0; i < lines.length; i++) {
        if (i > 0) buffer += "\r\n";
        buffer += lines[i];
      }

      buffer += SYNCHRONIZED_OUTPUT_END;
      this.terminal.write(buffer);

      this.cursorRow = Math.max(0, lines.length - 1);
      this.hardwareCursorRow = this.cursorRow;
      this.maxLinesRendered = lines.length;

      this.previousLines = lines;
      this.previousWidth = width;
      this.previousHeight = height;
    };

    // First render
    if (this.previousLines.length === 0 && !widthChanged && !heightChanged) {
      fullRender(false);
      return;
    }

    // Size changes need full re-render
    if (widthChanged || heightChanged) {
      fullRender(true);
      return;
    }

    // Find first and last changed lines
    let firstChanged = -1;
    let lastChanged = -1;
    const maxLines = Math.max(lines.length, this.previousLines.length);

    for (let i = 0; i < maxLines; i++) {
      const oldLine = i < this.previousLines.length ? this.previousLines[i] : "";
      const newLine = i < lines.length ? lines[i] : "";

      if (oldLine !== newLine) {
        if (firstChanged === -1) firstChanged = i;
        lastChanged = i;
      }
    }

    // No changes
    if (firstChanged === -1) {
      return;
    }

    // Differential rendering
    let buffer = SYNCHRONIZED_OUTPUT_BEGIN;

    // Move cursor to first changed line
    const lineDiff = firstChanged - this.hardwareCursorRow;
    if (lineDiff > 0) {
      buffer += `\x1b[${lineDiff}B`;
    } else if (lineDiff < 0) {
      buffer += `\x1b[${-lineDiff}A`;
    }

    buffer += "\r";

    // Render changed lines
    for (let i = firstChanged; i <= lastChanged && i < lines.length; i++) {
      if (i > firstChanged) buffer += "\r\n";
      buffer += "\x1b[2K";
      buffer += lines[i];
    }

    // Clear extra lines if content shrunk
    if (this.previousLines.length > lines.length) {
      const extraLines = this.previousLines.length - lines.length;
      for (let i = 0; i < extraLines; i++) {
        buffer += "\r\n\x1b[2K";
      }
      buffer += `\x1b[${extraLines}A`;
    }

    buffer += SYNCHRONIZED_OUTPUT_END;

    this.terminal.write(buffer);

    this.cursorRow = Math.max(0, lines.length - 1);
    this.hardwareCursorRow = lastChanged;
    this.maxLinesRendered = Math.max(this.maxLinesRendered, lines.length);

    this.previousLines = lines;
    this.previousWidth = width;
    this.previousHeight = height;
  }
}
