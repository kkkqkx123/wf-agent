/**
 * Minimal TUI implementation with differential rendering
 */

import { isKeyRelease, matchesKey } from "./keys.js";
import type { Terminal } from "./terminal.js";
import { visibleWidth } from "./utils.js";

/**
 * Component interface - all components must implement this
 */
export interface Component {
  /**
   * Render the component to lines for the given viewport width
   */
  render(width: number): string[];

  /**
   * Optional handler for keyboard input when component has focus
   */
  handleInput?(data: string): void;

  /**
   * If true, component receives key release events (Kitty protocol).
   */
  wantsKeyRelease?: boolean;

  /**
   * Invalidate any cached rendering state.
   */
  invalidate(): void;
}

/**
 * Interface for components that can receive focus and display a hardware cursor.
 */
export interface Focusable {
  /** Set by TUI when focus changes. Component should emit CURSOR_MARKER when true. */
  focused: boolean;
}

/** Type guard to check if a component implements Focusable */
export function isFocusable(component: Component | null): component is Component & Focusable {
  return component !== null && "focused" in component;
}

/**
 * Cursor position marker - APC sequence.
 */
export const CURSOR_MARKER = "\x1b_pi:c\x07";

export { visibleWidth };

/**
 * Anchor position for overlays
 */
export type OverlayAnchor =
  | "center"
  | "top-left"
  | "top-right"
  | "bottom-left"
  | "bottom-right"
  | "top-center"
  | "bottom-center"
  | "left-center"
  | "right-center";

/**
 * Margin configuration for overlays
 */
export interface OverlayMargin {
  top?: number;
  right?: number;
  bottom?: number;
  left?: number;
}

/** Value that can be absolute (number) or percentage (string like "50%") */
export type SizeValue = number | `${number}%`;

/** Parse a SizeValue into absolute value given a reference size */
function parseSizeValue(value: SizeValue | undefined, referenceSize: number): number | undefined {
  if (value === undefined) return undefined;
  if (typeof value === "number") return value;
  const match = value.match(/^(\d+(?:\.\d+)?)%$/);
  if (match && match[1]) {
    return Math.floor((referenceSize * parseFloat(match[1])) / 100);
  }
  return undefined;
}

/**
 * Options for overlay positioning and sizing.
 */
export interface OverlayOptions {
  width?: SizeValue;
  minWidth?: number;
  maxHeight?: SizeValue;
  anchor?: OverlayAnchor;
  offsetX?: number;
  offsetY?: number;
  row?: SizeValue;
  col?: SizeValue;
  margin?: OverlayMargin | number;
  visible?: (termWidth: number, termHeight: number) => boolean;
  nonCapturing?: boolean;
}

/**
 * Handle returned by showOverlay for controlling the overlay
 */
export interface OverlayHandle {
  hide(): void;
  setHidden(hidden: boolean): void;
  isHidden(): boolean;
  focus(): void;
  unfocus(): void;
  isFocused(): boolean;
}

/**
 * Container - a component that contains other components
 */
export class Container implements Component {
  children: Component[] = [];

  addChild(component: Component): void {
    this.children.push(component);
  }

  removeChild(component: Component): void {
    const index = this.children.indexOf(component);
    if (index !== -1) {
      this.children.splice(index, 1);
    }
  }

  clear(): void {
    this.children = [];
  }

  invalidate(): void {
    for (const child of this.children) {
      child.invalidate?.();
    }
  }

  render(width: number): string[] {
    const lines: string[] = [];
    for (const child of this.children) {
      const childLines = child.render(width);
      for (const line of childLines) {
        lines.push(line);
      }
    }
    return lines;
  }
}

/**
 * TUI - Main class for managing terminal UI with differential rendering
 */
export class TUI extends Container {
  public terminal: Terminal;
  private previousLines: string[] = [];
  private previousWidth = 0;
  private previousHeight = 0;
  private focusedComponent: Component | null = null;
  private renderRequested = false;
  private renderTimer: NodeJS.Timeout | undefined;
  private lastRenderAt = 0;
  private static readonly MIN_RENDER_INTERVAL_MS = 16;
  private cursorRow = 0;
  private hardwareCursorRow = 0;
  private showHardwareCursor = process.env["PI_HARDWARE_CURSOR"] === "1";
  private clearOnShrink = process.env["PI_CLEAR_ON_SHRINK"] === "1";
  private maxLinesRendered = 0;
  private previousViewportTop = 0;
  private fullRedrawCount = 0;
  private stopped = false;

  // Overlay stack
  private focusOrderCounter = 0;
  private overlayStack: {
    component: Component;
    options?: OverlayOptions;
    preFocus: Component | null;
    hidden: boolean;
    focusOrder: number;
  }[] = [];

  constructor(terminal: Terminal, showHardwareCursor?: boolean) {
    super();
    this.terminal = terminal;
    if (showHardwareCursor !== undefined) {
      this.showHardwareCursor = showHardwareCursor;
    }
  }

  get fullRedraws(): number {
    return this.fullRedrawCount;
  }

  setFocus(component: Component | null): void {
    if (isFocusable(this.focusedComponent)) {
      this.focusedComponent.focused = false;
    }

    this.focusedComponent = component;

    if (isFocusable(component)) {
      component.focused = true;
    }
  }

  /**
   * Show an overlay component with configurable positioning and sizing.
   */
  showOverlay(component: Component, options?: OverlayOptions): OverlayHandle {
    const entry = {
      component,
      options,
      preFocus: this.focusedComponent,
      hidden: false,
      focusOrder: ++this.focusOrderCounter,
    };
    this.overlayStack.push(entry);
    
    if (!options?.nonCapturing && this.isOverlayVisible(entry)) {
      this.setFocus(component);
    }
    this.terminal.hideCursor();
    this.requestRender();

    return {
      hide: () => {
        const index = this.overlayStack.indexOf(entry);
        if (index !== -1) {
          this.overlayStack.splice(index, 1);
          if (this.focusedComponent === component) {
            const topVisible = this.getTopmostVisibleOverlay();
            this.setFocus(topVisible?.component ?? entry.preFocus);
          }
          if (this.overlayStack.length === 0) this.terminal.hideCursor();
          this.requestRender();
        }
      },
      setHidden: (hidden: boolean) => {
        if (entry.hidden === hidden) return;
        entry.hidden = hidden;
        if (hidden) {
          if (this.focusedComponent === component) {
            const topVisible = this.getTopmostVisibleOverlay();
            this.setFocus(topVisible?.component ?? entry.preFocus);
          }
        } else {
          if (!options?.nonCapturing && this.isOverlayVisible(entry)) {
            entry.focusOrder = ++this.focusOrderCounter;
            this.setFocus(component);
          }
        }
        this.requestRender();
      },
      isHidden: () => entry.hidden,
      focus: () => {
        if (!this.overlayStack.includes(entry) || !this.isOverlayVisible(entry)) return;
        if (this.focusedComponent !== component) {
          this.setFocus(component);
        }
        entry.focusOrder = ++this.focusOrderCounter;
        this.requestRender();
      },
      unfocus: () => {
        if (this.focusedComponent !== component) return;
        const topVisible = this.getTopmostVisibleOverlay();
        this.setFocus(topVisible && topVisible !== entry ? topVisible.component : entry.preFocus);
        this.requestRender();
      },
      isFocused: () => this.focusedComponent === component,
    };
  }

  /** Hide the topmost overlay and restore previous focus. */
  hideOverlay(): void {
    const overlay = this.overlayStack.pop();
    if (!overlay) return;
    if (this.focusedComponent === overlay.component) {
      const topVisible = this.getTopmostVisibleOverlay();
      this.setFocus(topVisible?.component ?? overlay.preFocus);
    }
    if (this.overlayStack.length === 0) this.terminal.hideCursor();
    this.requestRender();
  }

  /** Check if there are any visible overlays */
  hasOverlay(): boolean {
    return this.overlayStack.some((o) => this.isOverlayVisible(o));
  }

  /** Check if an overlay entry is currently visible */
  private isOverlayVisible(entry: (typeof this.overlayStack)[number]): boolean {
    if (entry.hidden) return false;
    if (entry.options?.visible) {
      return entry.options.visible(this.terminal.columns, this.terminal.rows);
    }
    return true;
  }

  /** Find the topmost visible capturing overlay, if any */
  private getTopmostVisibleOverlay(): (typeof this.overlayStack)[number] | undefined {
    for (let i = this.overlayStack.length - 1; i >= 0; i--) {
      const entry = this.overlayStack[i];
      if (!entry) continue;
      if (entry.options?.nonCapturing) continue;
      if (this.isOverlayVisible(entry)) {
        return entry;
      }
    }
    return undefined;
  }

  override invalidate(): void {
    super.invalidate();
    for (const overlay of this.overlayStack) overlay.component.invalidate?.();
  }

  start(): void {
    this.stopped = false;
    this.terminal.start(
      (data) => this.handleInput(data),
      () => this.requestRender(),
    );
    this.terminal.hideCursor();
    this.requestRender();
  }

  stop(): void {
    this.stopped = true;
    if (this.renderTimer) {
      clearTimeout(this.renderTimer);
      this.renderTimer = undefined;
    }
    
    // Move cursor to end of content
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
    this.terminal.stop();
  }

  requestRender(force = false): void {
    if (force) {
      this.previousLines = [];
      this.previousWidth = -1;
      this.previousHeight = -1;
      this.cursorRow = 0;
      this.hardwareCursorRow = 0;
      this.maxLinesRendered = 0;
      this.previousViewportTop = 0;
      if (this.renderTimer) {
        clearTimeout(this.renderTimer);
        this.renderTimer = undefined;
      }
      this.renderRequested = true;
      process.nextTick(() => {
        if (this.stopped || !this.renderRequested) return;
        this.renderRequested = false;
        this.lastRenderAt = performance.now();
        this.doRender();
      });
      return;
    }
    
    if (this.renderRequested) return;
    this.renderRequested = true;
    process.nextTick(() => this.scheduleRender());
  }

  private scheduleRender(): void {
    if (this.stopped || this.renderTimer || !this.renderRequested) return;
    
    const elapsed = performance.now() - this.lastRenderAt;
    const delay = Math.max(0, TUI.MIN_RENDER_INTERVAL_MS - elapsed);
    
    this.renderTimer = setTimeout(() => {
      this.renderTimer = undefined;
      if (this.stopped || !this.renderRequested) return;
      
      this.renderRequested = false;
      this.lastRenderAt = performance.now();
      this.doRender();
      
      if (this.renderRequested) {
        this.scheduleRender();
      }
    }, delay);
  }

  private handleInput(data: string): void {
    // Filter out key release events unless component opts in
    if (isKeyRelease(data) && this.focusedComponent && !this.focusedComponent.wantsKeyRelease) {
      return;
    }

    // Pass input to focused component
    if (this.focusedComponent?.handleInput) {
      this.focusedComponent.handleInput(data);
      this.requestRender();
    }
  }

  /**
   * Resolve overlay layout from options.
   */
  private resolveOverlayLayout(
    options: OverlayOptions | undefined,
    overlayHeight: number,
    termWidth: number,
    termHeight: number,
  ): { width: number; row: number; col: number; maxHeight: number | undefined } {
    const opt = options ?? {};

    const margin =
      typeof opt.margin === "number"
        ? { top: opt.margin, right: opt.margin, bottom: opt.margin, left: opt.margin }
        : (opt.margin ?? {});
    const marginTop = Math.max(0, margin.top ?? 0);
    const marginRight = Math.max(0, margin.right ?? 0);
    const marginBottom = Math.max(0, margin.bottom ?? 0);
    const marginLeft = Math.max(0, margin.left ?? 0);

    const availWidth = Math.max(1, termWidth - marginLeft - marginRight);
    const availHeight = Math.max(1, termHeight - marginTop - marginBottom);

    let width = parseSizeValue(opt.width, termWidth) ?? Math.min(80, availWidth);
    if (opt.minWidth !== undefined) {
      width = Math.max(width, opt.minWidth);
    }
    width = Math.max(1, Math.min(width, availWidth));

    let maxHeight = parseSizeValue(opt.maxHeight, termHeight);
    if (maxHeight !== undefined) {
      maxHeight = Math.max(1, Math.min(maxHeight, availHeight));
    }

    const effectiveHeight = maxHeight !== undefined ? Math.min(overlayHeight, maxHeight) : overlayHeight;

    let row: number;
    let col: number;

    if (opt.row !== undefined) {
      if (typeof opt.row === "string") {
        const match = opt.row.match(/^(\d+(?:\.\d+)?)%$/);
        if (match && match[1]) {
          const maxRow = Math.max(0, availHeight - effectiveHeight);
          const percent = parseFloat(match[1]) / 100;
          row = marginTop + Math.floor(maxRow * percent);
        } else {
          row = this.resolveAnchorRow("center", effectiveHeight, availHeight, marginTop);
        }
      } else {
        row = opt.row;
      }
    } else {
      const anchor = opt.anchor ?? "center";
      row = this.resolveAnchorRow(anchor, effectiveHeight, availHeight, marginTop);
    }

    if (opt.col !== undefined) {
      if (typeof opt.col === "string") {
        const match = opt.col.match(/^(\d+(?:\.\d+)?)%$/);
        if (match && match[1]) {
          const maxCol = Math.max(0, availWidth - width);
          const percent = parseFloat(match[1]) / 100;
          col = marginLeft + Math.floor(maxCol * percent);
        } else {
          col = this.resolveAnchorCol("center", width, availWidth, marginLeft);
        }
      } else {
        col = opt.col;
      }
    } else {
      const anchor = opt.anchor ?? "center";
      col = this.resolveAnchorCol(anchor, width, availWidth, marginLeft);
    }

    if (opt.offsetY !== undefined) row += opt.offsetY;
    if (opt.offsetX !== undefined) col += opt.offsetX;

    row = Math.max(marginTop, Math.min(row, termHeight - marginBottom - effectiveHeight));
    col = Math.max(marginLeft, Math.min(col, termWidth - marginRight - width));

    return { width, row, col, maxHeight };
  }

  private resolveAnchorRow(anchor: OverlayAnchor, height: number, availHeight: number, marginTop: number): number {
    switch (anchor) {
      case "top-left":
      case "top-center":
      case "top-right":
        return marginTop;
      case "bottom-left":
      case "bottom-center":
      case "bottom-right":
        return marginTop + availHeight - height;
      case "left-center":
      case "center":
      case "right-center":
        return marginTop + Math.floor((availHeight - height) / 2);
    }
  }

  private resolveAnchorCol(anchor: OverlayAnchor, width: number, availWidth: number, marginLeft: number): number {
    switch (anchor) {
      case "top-left":
      case "left-center":
      case "bottom-left":
        return marginLeft;
      case "top-right":
      case "right-center":
      case "bottom-right":
        return marginLeft + availWidth - width;
      case "top-center":
      case "center":
      case "bottom-center":
        return marginLeft + Math.floor((availWidth - width) / 2);
    }
  }

  /** Composite all overlays into content lines */
  private compositeOverlays(lines: string[], termWidth: number, termHeight: number): string[] {
    if (this.overlayStack.length === 0) return lines;
    const result = [...lines];

    const rendered: { overlayLines: string[]; row: number; col: number; w: number }[] = [];
    let minLinesNeeded = result.length;

    const visibleEntries = this.overlayStack.filter((e) => this.isOverlayVisible(e));
    visibleEntries.sort((a, b) => a.focusOrder - b.focusOrder);
    
    for (const entry of visibleEntries) {
      const { component, options } = entry;
      const { width, maxHeight } = this.resolveOverlayLayout(options, 0, termWidth, termHeight);

      let overlayLines = component.render(width);

      if (maxHeight !== undefined && overlayLines.length > maxHeight) {
        overlayLines = overlayLines.slice(0, maxHeight);
      }

      const { row, col } = this.resolveOverlayLayout(options, overlayLines.length, termWidth, termHeight);

      rendered.push({ overlayLines, row, col, w: width });
      minLinesNeeded = Math.max(minLinesNeeded, row + overlayLines.length);
    }

    const workingHeight = Math.max(result.length, termHeight, minLinesNeeded);

    while (result.length < workingHeight) {
      result.push("");
    }

    const viewportStart = Math.max(0, workingHeight - termHeight);

    for (const { overlayLines, row, col, w } of rendered) {
      for (let i = 0; i < overlayLines.length; i++) {
        const idx = viewportStart + row + i;
        if (idx >= 0 && idx < result.length) {
          const overlayLine = overlayLines[i];
          if (overlayLine && visibleWidth(overlayLine) <= w) {
            const base = result[idx];
            if (base !== undefined) {
              result[idx] = this.compositeLineAt(base, overlayLine, col, w, termWidth);
            }
          }
        }
      }
    }

    return result;
  }

  private compositeLineAt(
    baseLine: string,
    overlayLine: string,
    startCol: number,
    overlayWidth: number,
    totalWidth: number,
  ): string {
    // Simple compositing: replace section of base line with overlay
    const before = baseLine.slice(0, startCol);
    const after = baseLine.slice(startCol + overlayWidth);
    const result = before + overlayLine + after;
    
    // Ensure we don't exceed terminal width
    if (visibleWidth(result) > totalWidth) {
      return result.slice(0, totalWidth);
    }
    
    return result;
  }

  private doRender(): void {
    if (this.stopped) return;
    
    const width = this.terminal.columns;
    const height = this.terminal.rows;
    const widthChanged = this.previousWidth !== 0 && this.previousWidth !== width;
    const heightChanged = this.previousHeight !== 0 && this.previousHeight !== height;

    // Render all components
    let newLines = this.render(width);

    // Composite overlays
    if (this.overlayStack.length > 0) {
      newLines = this.compositeOverlays(newLines, width, height);
    }

    // Helper to clear and render all new lines
    const fullRender = (clear: boolean): void => {
      this.fullRedrawCount += 1;
      let buffer = "\x1b[?2026h"; // Begin synchronized output
      if (clear) buffer += "\x1b[2J\x1b[H\x1b[3J";
      
      for (let i = 0; i < newLines.length; i++) {
        if (i > 0) buffer += "\r\n";
        buffer += newLines[i];
      }
      
      buffer += "\x1b[?2026l"; // End synchronized output
      this.terminal.write(buffer);
      
      this.cursorRow = Math.max(0, newLines.length - 1);
      this.hardwareCursorRow = this.cursorRow;
      this.maxLinesRendered = newLines.length;
      
      this.previousLines = newLines;
      this.previousWidth = width;
      this.previousHeight = height;
    };

    // First render
    if (this.previousLines.length === 0 && !widthChanged && !heightChanged) {
      fullRender(false);
      return;
    }

    // Width changes need full re-render
    if (widthChanged) {
      fullRender(true);
      return;
    }

    // Height changes need full re-render
    if (heightChanged) {
      fullRender(true);
      return;
    }

    // Find first and last changed lines
    let firstChanged = -1;
    let lastChanged = -1;
    const maxLines = Math.max(newLines.length, this.previousLines.length);
    
    for (let i = 0; i < maxLines; i++) {
      const oldLine = i < this.previousLines.length ? this.previousLines[i] : "";
      const newLine = i < newLines.length ? newLines[i] : "";

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
    let buffer = "\x1b[?2026h"; // Begin synchronized output

    // Move cursor to first changed line
    const lineDiff = firstChanged - this.hardwareCursorRow;
    if (lineDiff > 0) {
      buffer += `\x1b[${lineDiff}B`;
    } else if (lineDiff < 0) {
      buffer += `\x1b[${-lineDiff}A`;
    }

    buffer += "\r"; // Move to column 0

    // Render changed lines
    for (let i = firstChanged; i <= lastChanged && i < newLines.length; i++) {
      if (i > firstChanged) buffer += "\r\n";
      buffer += "\x1b[2K"; // Clear current line
      buffer += newLines[i];
    }

    // Clear extra lines if content shrunk
    if (this.previousLines.length > newLines.length) {
      const extraLines = this.previousLines.length - newLines.length;
      for (let i = 0; i < extraLines; i++) {
        buffer += "\r\n\x1b[2K";
      }
      buffer += `\x1b[${extraLines}A`;
    }

    buffer += "\x1b[?2026l"; // End synchronized output

    this.terminal.write(buffer);

    this.cursorRow = Math.max(0, newLines.length - 1);
    this.hardwareCursorRow = lastChanged;
    this.maxLinesRendered = Math.max(this.maxLinesRendered, newLines.length);

    this.previousLines = newLines;
    this.previousWidth = width;
    this.previousHeight = height;
  }
}
