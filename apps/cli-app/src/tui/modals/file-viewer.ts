import { getKeybindings } from "../core/keybindings.js";
import type { Component } from "../core/tui.js";
import { visibleWidth, truncateToWidth } from "../core/utils.js";
import type { Modal as CoreModal, ModalHandle } from "../core/modal.js";
import { ModalAction } from "./modal.js";

export class FileViewer implements Component, CoreModal {
  readonly component: Component = this;
  private scrollOffset = 0;
  private renderedHeight = 0;

  private onCloseCallback?: () => void;

  constructor(
    private title: string,
    private content: string[],
    onClose?: () => void,
  ) {
    this.onCloseCallback = onClose;
  }

  static view(
    tui: import("../core/tui.js").TUI,
    title: string,
    content: string[],
  ): Promise<void> {
    return new Promise<void>((resolve) => {
      const modal = new FileViewer(title, content, () => resolve());
      const handle: ModalHandle = tui.showModal(modal, { anchor: "center", width: "80%" });
      modal._handle = handle;
    });
  }

  _handle?: ModalHandle;

  capturesAllKeys(): boolean {
    return true;
  }

  handleKey(key: string): ModalAction {
    const kb = getKeybindings();

    if (kb.matches(key, "tui.select.cancel") || key === "\x1b" || key === "q" || key === "Q") {
      this.onCloseCallback?.();
      return ModalAction.Close;
    }

    if (kb.matches(key, "tui.navigate.up") || key === "\x1b[A" || key === "k") {
      this.scrollOffset = Math.max(0, this.scrollOffset - 1);
      return ModalAction.Continue;
    }

    if (kb.matches(key, "tui.navigate.down") || key === "\x1b[B" || key === "j") {
      const maxOffset = Math.max(0, this.content.length - this.renderedHeight + 3);
      this.scrollOffset = Math.min(maxOffset, this.scrollOffset + 1);
      return ModalAction.Continue;
    }

    if (kb.matches(key, "tui.navigate.halfPageUp")) {
      const halfPage = Math.max(1, Math.floor((this.renderedHeight - 3) / 2));
      this.scrollOffset = Math.max(0, this.scrollOffset - halfPage);
      return ModalAction.Continue;
    }

    if (kb.matches(key, "tui.navigate.halfPageDown")) {
      const halfPage = Math.max(1, Math.floor((this.renderedHeight - 3) / 2));
      const maxOffset = Math.max(0, this.content.length - this.renderedHeight + 3);
      this.scrollOffset = Math.min(maxOffset, this.scrollOffset + halfPage);
      return ModalAction.Continue;
    }

    if (kb.matches(key, "tui.navigate.top")) {
      this.scrollOffset = 0;
      return ModalAction.Continue;
    }

    if (kb.matches(key, "tui.navigate.bottom")) {
      const maxOffset = Math.max(0, this.content.length - this.renderedHeight + 3);
      this.scrollOffset = maxOffset;
      return ModalAction.Continue;
    }

    return ModalAction.Continue;
  }

  invalidate(): void {}

  render(width: number): string[] {
    const borderWidth = Math.min(120, Math.max(40, Math.floor(width * 0.85)));
    const innerWidth = borderWidth - 2;
    const lines: string[] = [];

    const truncatedTitle = truncateToWidth(this.title, innerWidth - 4, "\u2026");
    const topBorder = `┌─ ${truncatedTitle} ${"─".repeat(Math.max(0, borderWidth - truncatedTitle.length - 4))}┐`;
    lines.push(topBorder);

    const availableHeight = 25;
    let contentLines: string[];

    const visibleContent = this.content.slice(
      this.scrollOffset,
      this.scrollOffset + availableHeight,
    );

    contentLines = [];
    for (const line of visibleContent) {
      if (visibleWidth(line) > innerWidth) {
        contentLines.push(truncateToWidth(line, innerWidth, ""));
      } else {
        const pad = Math.max(0, innerWidth - visibleWidth(line));
        contentLines.push(`${line}${" ".repeat(pad)}`);
      }
    }

    for (const cl of contentLines) {
      lines.push(`│ ${cl} │`);
    }

    const contentLen = contentLines.length;
    const remaining = Math.max(0, availableHeight - contentLen + 1);
    for (let i = 0; i < remaining; i++) {
      lines.push(`│ ${" ".repeat(innerWidth)} │`);
    }

    const hasMore = this.content.length > this.scrollOffset + availableHeight;
    const scrollInfo = hasMore
      ? `Line ${this.scrollOffset + 1}-${Math.min(this.content.length, this.scrollOffset + availableHeight)} of ${this.content.length} (j/k scroll, q/Esc close)`
      : `All ${this.content.length} lines (q/Esc close)`;
    const footerPad = Math.max(0, innerWidth - visibleWidth(scrollInfo));
    lines.push(`│ ${scrollInfo}${" ".repeat(footerPad)} │`);

    const bottomBorder = `└${"─".repeat(borderWidth)}┘`;
    lines.push(bottomBorder);

    this.renderedHeight = lines.length;
    return lines;
  }
}
