import type { Component } from "../core/tui.js";
import { visibleWidth, wrapTextWithAnsi } from "../core/utils.js";
import type { Modal as CoreModal, ModalHandle } from "../core/modal.js";
import { ModalAction } from "./modal.js";

export class ConfirmModal implements Component, CoreModal {
  readonly component: Component = this;
  private resolved = false;

  constructor(
    private title: string,
    private message: string,
    private onConfirm?: () => void,
    private onCancel?: () => void,
  ) {}

  static show(
    tui: import("../core/tui.js").TUI,
    title: string,
    message: string,
  ): Promise<boolean> {
    return new Promise<boolean>((resolve) => {
      const modal = new ConfirmModal(
        title,
        message,
        () => resolve(true),
        () => resolve(false),
      );
      const handle: ModalHandle = tui.showModal(modal, { anchor: "center", width: 60 });
      modal.onClose = () => {
        if (!modal.resolved) {
          resolve(false);
        }
        handle.close();
      };
    });
  }

  capturesAllKeys(): boolean {
    return true;
  }

  handleKey(key: string): ModalAction {
    if (this.resolved) return ModalAction.Close;

    if (key === "y" || key === "Y" || key === "\r") {
      this.resolved = true;
      this.onConfirm?.();
      return ModalAction.Close;
    }

    if (key === "n" || key === "N" || key === "\x1b") {
      this.resolved = true;
      this.onCancel?.();
      return ModalAction.Close;
    }

    return ModalAction.Continue;
  }

  onClose?: () => void;

  invalidate(): void {}

  render(width: number): string[] {
    const borderWidth = Math.min(60, Math.max(30, width - 4));
    const innerWidth = borderWidth - 2;
    const lines: string[] = [];

    const topBorder = `┌─ ${this.title} ${"─".repeat(Math.max(0, borderWidth - this.title.length - 4))}┐`;
    lines.push(topBorder);

    lines.push(`│ ${" ".repeat(innerWidth)} │`);

    const wrappedMessage = wrapTextWithAnsi(this.message, innerWidth);
    for (const line of wrappedMessage) {
      const pad = Math.max(0, innerWidth - visibleWidth(line));
      lines.push(`│ ${line}${" ".repeat(pad)} │`);
    }

    lines.push(`│ ${" ".repeat(innerWidth)} │`);

    const footer = "Y/Enter to confirm, N/Esc to cancel";
    const footerPad = Math.max(0, innerWidth - visibleWidth(footer));
    lines.push(`│ ${footer}${" ".repeat(footerPad)} │`);

    const bottomBorder = `└${"─".repeat(borderWidth)}┘`;
    lines.push(bottomBorder);

    return lines;
  }
}
