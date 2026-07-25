import type { Component } from "./tui.js";
import type { OverlayOptions } from "./tui.js";

export enum ModalAction {
  Continue = "continue",
  Close = "close",
}

export interface Modal {
  readonly component: Component;
  handleKey(data: string): ModalAction;
  capturesAllKeys(): boolean;
  onOpen?(): void;
  onClose?(): void;
  closeRequested?(): boolean;
}

export interface ModalHandle {
  close(): void;
  isOpen(): boolean;
}

export function isModal(component: Component): component is Component & Modal {
  return "handleKey" in component && "capturesAllKeys" in component;
}

export function modalToOverlayOptions(options?: {
  width?: string | number;
  anchor?: string;
  margin?: number;
  nonCapturing?: boolean;
}): OverlayOptions | undefined {
  if (!options) return undefined;
  const opt: OverlayOptions = {};
  if (options.width !== undefined) opt.width = options.width as any;
  if (options.anchor !== undefined) opt.anchor = options.anchor as any;
  if (options.margin !== undefined) opt.margin = options.margin;
  if (options.nonCapturing !== undefined) opt.nonCapturing = options.nonCapturing;
  return opt;
}
