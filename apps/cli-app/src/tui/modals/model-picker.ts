import { getKeybindings } from "../core/keybindings.js";
import type { Component } from "../core/tui.js";
import { visibleWidth, truncateToWidth } from "../core/utils.js";
import type { Modal as CoreModal, ModalHandle } from "../core/modal.js";
import { ModalAction } from "./modal.js";

export interface ModelItem {
  id: string;
  name: string;
  provider: string;
  description?: string;
}

export class ModelPicker implements Component, CoreModal {
  readonly component: Component = this;
  private selectedIndex = 0;

  constructor(
    private models: ModelItem[],
    private onSelect?: (model: ModelItem | null) => void,
  ) {}

  static pick(
    tui: import("../core/tui.js").TUI,
    models: ModelItem[],
  ): Promise<ModelItem | null> {
    return new Promise<ModelItem | null>((resolve) => {
      const modal = new ModelPicker(models, resolve);
      const handle: ModalHandle = tui.showModal(modal, { anchor: "center", width: 70 });
      modal.onClose = () => {
        handle.close();
      };
    });
  }

  capturesAllKeys(): boolean {
    return true;
  }

  handleKey(key: string): ModalAction {
    const kb = getKeybindings();

    if (kb.matches(key, "tui.select.confirm") || key === "\r") {
      const selected = this.models[this.selectedIndex] ?? null;
      this.onSelect?.(selected);
      return ModalAction.Close;
    }

    if (kb.matches(key, "tui.select.cancel") || key === "\x1b") {
      this.onSelect?.(null);
      return ModalAction.Close;
    }

    if (kb.matches(key, "tui.select.up") || key === "\x1b[A") {
      this.selectedIndex = Math.max(0, this.selectedIndex - 1);
      return ModalAction.Continue;
    }

    if (kb.matches(key, "tui.select.down") || key === "\x1b[B") {
      this.selectedIndex = Math.min(this.models.length - 1, this.selectedIndex + 1);
      return ModalAction.Continue;
    }

    return ModalAction.Continue;
  }

  onClose?: () => void;

  invalidate(): void {}

  render(width: number): string[] {
    const borderWidth = Math.min(70, Math.max(30, width - 4));
    const innerWidth = borderWidth - 2;
    const lines: string[] = [];

    const title = "Select Model";
    const topBorder = `┌─ ${title} ${"─".repeat(Math.max(0, borderWidth - title.length - 4))}┐`;
    lines.push(topBorder);

    lines.push(`│ ${" ".repeat(innerWidth)} │`);

    const maxVisible = Math.min(this.models.length, 10);
    const start = Math.max(0, this.selectedIndex - Math.floor(maxVisible / 2));
    const end = Math.min(this.models.length, start + maxVisible);

    for (let i = start; i < end; i++) {
      const model = this.models[i];
      if (!model) continue;
      const isSelected = i === this.selectedIndex;
      const prefix = isSelected ? "\u2192 " : "  ";
      const label = `${model.name} (${model.provider})`;
      const truncated = truncateToWidth(label, innerWidth - visibleWidth(prefix), "");
      const line = `${prefix}${truncated}`;
      const pad = Math.max(0, innerWidth - visibleWidth(line));
      lines.push(`│ ${line}${" ".repeat(pad)} │`);
    }

    lines.push(`│ ${" ".repeat(innerWidth)} │`);

    const footer = `\u2191/\u2193 navigate, Enter select, Esc cancel`;
    const footerPad = Math.max(0, innerWidth - visibleWidth(footer));
    lines.push(`│ ${footer}${" ".repeat(footerPad)} │`);

    const bottomBorder = `└${"─".repeat(borderWidth)}┘`;
    lines.push(bottomBorder);

    return lines;
  }
}
