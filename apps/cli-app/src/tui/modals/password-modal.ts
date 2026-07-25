import type { Component } from "../core/tui.js";
import { visibleWidth } from "../core/utils.js";
import type { Modal as CoreModal, ModalHandle } from "../core/modal.js";
import { ModalAction } from "./modal.js";

export class PasswordModal implements Component, CoreModal {
  readonly component: Component = this;
  private input = "";
  private resolved = false;

  constructor(
    private prompt: string,
    private onSubmit?: (value: string | null) => void,
  ) {}

  static ask(
    tui: import("../core/tui.js").TUI,
    prompt: string,
  ): Promise<string | null> {
    return new Promise<string | null>((resolve) => {
      const modal = new PasswordModal(prompt, resolve);
      const handle: ModalHandle = tui.showModal(modal, { anchor: "center", width: 60 });
      modal.onClose = () => {
        if (!modal.resolved) {
          resolve(null);
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

    if (key === "\r") {
      this.resolved = true;
      this.onSubmit?.(this.input);
      return ModalAction.Close;
    }

    if (key === "\x1b" || key === "\x03") {
      this.resolved = true;
      this.onSubmit?.(null);
      return ModalAction.Close;
    }

    if (key === "\x7f" || key === "\b") {
      this.input = this.input.slice(0, -1);
      return ModalAction.Continue;
    }

    if (key.length === 1) {
      const code = key.charCodeAt(0);
      if (code >= 0x20 && code !== 0x7f) {
        this.input += key;
        return ModalAction.Continue;
      }
    }

    return ModalAction.Continue;
  }

  onClose?: () => void;

  invalidate(): void {}

  render(width: number): string[] {
    const borderWidth = Math.min(50, Math.max(30, width - 4));
    const innerWidth = borderWidth - 2;
    const lines: string[] = [];

    const title = "Password Required";
    const topBorder = `┌─ ${title} ${"─".repeat(Math.max(0, borderWidth - title.length - 4))}┐`;
    lines.push(topBorder);

    lines.push(`│ ${" ".repeat(innerWidth)} │`);

    const promptLines = this.prompt.split("\n");
    for (const pl of promptLines) {
      const pad = Math.max(0, innerWidth - visibleWidth(pl));
      lines.push(`│ ${pl}${" ".repeat(pad)} │`);
    }

    lines.push(`│ ${" ".repeat(innerWidth)} │`);

    const masked = "\u2022".repeat(this.input.length);
    const maskedWidth = visibleWidth(masked);
    const inputLine = masked || (this.input.length === 0 ? " " : "");
    const inputPad = Math.max(0, innerWidth - maskedWidth);
    lines.push(`│ ${inputLine}${" ".repeat(inputPad)} │`);

    lines.push(`│ ${" ".repeat(innerWidth)} │`);

    const footer = "Enter to submit, Esc to cancel";
    const footerPad = Math.max(0, innerWidth - visibleWidth(footer));
    lines.push(`│ ${footer}${" ".repeat(footerPad)} │`);

    const bottomBorder = `└${"─".repeat(borderWidth)}┘`;
    lines.push(bottomBorder);

    return lines;
  }
}
