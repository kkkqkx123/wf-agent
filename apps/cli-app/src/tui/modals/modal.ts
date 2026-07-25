import type { Component } from "../core/tui.js";

export enum ModalAction {
  Continue = "continue",
  Close = "close",
}

export interface Modal {
  handleKey(key: string): ModalAction;
  capturesAllKeys(): boolean;
  onOpen?(): void;
  onClose?(): void;
  closeRequested?(): boolean;
}

export function isModal(component: Component): component is Component & Modal {
  return "handleKey" in component && "capturesAllKeys" in component;
}
