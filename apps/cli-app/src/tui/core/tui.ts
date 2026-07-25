import { isKeyRelease } from "./keys/index.js";
import type { InputContext } from "./keybindings.js";
import type { Terminal } from "./terminal.js";
import { visibleWidth } from "./utils.js";
import type { Renderer } from "./renderer.js";
import { RetainedRenderer } from "./retained-renderer.js";
import { PhaseManager, InteractionPhase } from "./interaction-phase.js";
import { EventScheduler, Priority } from "./event-scheduler.js";
import { TerminalGuard } from "./terminal-guard.js";
import type { Modal, ModalHandle } from "./modal.js";
import { ModalAction, modalToOverlayOptions } from "./modal.js";

export enum InputMode {
  Chat = "chat",
  Normal = "normal",
}

export interface Component {
  render(width: number): string[];
  handleInput?(data: string): void;
  wantsKeyRelease?: boolean;
  invalidate(): void;
}

export interface Focusable {
  focused: boolean;
}

export function isFocusable(component: Component | null): component is Component & Focusable {
  return component !== null && "focused" in component;
}

export const CURSOR_MARKER = "\x1b_pi:c\x07";

export { visibleWidth };

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

export interface OverlayMargin {
  top?: number;
  right?: number;
  bottom?: number;
  left?: number;
}

export type SizeValue = number | `${number}%`;

function parseSizeValue(value: SizeValue | undefined, referenceSize: number): number | undefined {
  if (value === undefined) return undefined;
  if (typeof value === "number") return value;
  const match = value.match(/^(\d+(?:\.\d+)?)%$/);
  if (match && match[1]) {
    return Math.floor((referenceSize * parseFloat(match[1])) / 100);
  }
  return undefined;
}

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

export interface OverlayHandle {
  hide(): void;
  setHidden(hidden: boolean): void;
  isHidden(): boolean;
  focus(): void;
  unfocus(): void;
  isFocused(): boolean;
}

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

export class TUI extends Container {
  public terminal: Terminal;
  public renderer: Renderer;
  private focusedComponent: Component | null = null;
  private renderRequested = false;
  private renderTimer: NodeJS.Timeout | undefined;
  private lastRenderAt = 0;
  private static readonly MIN_RENDER_INTERVAL_MS = 16;
  private stopped = false;

  private focusOrderCounter = 0;
  private overlayStack: {
    component: Component;
    options?: OverlayOptions;
    preFocus: Component | null;
    hidden: boolean;
    focusOrder: number;
  }[] = [];

  readonly phaseManager = new PhaseManager();
  readonly schedule = new EventScheduler();
  readonly modalStack: { modal: Modal; handle: OverlayHandle }[] = [];

  constructor(terminal: Terminal, renderer?: Renderer) {
    super();
    this.terminal = terminal;
    this.renderer = renderer ?? new RetainedRenderer(terminal);
    this.setupPhaseManager();
    this.setupScheduler();
  }

  currentContext: InputContext = "global";
  inputMode: InputMode = InputMode.Chat;

  setContext(context: InputContext): void {
    this.currentContext = context;
  }

  setInputMode(mode: InputMode): void {
    this.inputMode = mode;
  }

  private setupPhaseManager(): void {
    this.phaseManager.onPhaseChange = (_from, to) => {
      if (to === InteractionPhase.Idle && this.modalStack.length > 0) {
        const top = this.getTopModal();
        if (top && top.capturesAllKeys()) {
          this.closeModal(top);
        }
      }

      const contextMap: Record<string, InputContext> = {
        [InteractionPhase.Idle]: "chat",
        [InteractionPhase.Streaming]: "chat",
        [InteractionPhase.Approval]: "modal",
        [InteractionPhase.Normal]: "chat",
      };
      this.currentContext = (contextMap[to] ?? "global") as InputContext;

      const modeMap: Record<string, InputMode> = {
        [InteractionPhase.Idle]: InputMode.Chat,
        [InteractionPhase.Streaming]: InputMode.Chat,
        [InteractionPhase.Approval]: InputMode.Chat,
        [InteractionPhase.Normal]: InputMode.Normal,
      };
      this.inputMode = modeMap[to] ?? InputMode.Chat;

      this.requestRender();
    };

    this.phaseManager.onInputRelease = (entries) => {
      for (const entry of entries) {
        this.handleInput(entry);
      }
    };
  }

  private setupScheduler(): void {
    this.schedule.addSource("spinner", {
      interval: 80,
      handler: () => this.requestRender(),
      priority: Priority.Animation,
      enabled: () => !this.stopped && this.phaseManager.phase === InteractionPhase.Streaming,
    });
    this.schedule.addSource("configPoll", {
      interval: 500,
      handler: () => {
        if (this._configPollHandler) this._configPollHandler();
      },
      priority: Priority.Poll,
      enabled: () => !this.stopped && !!this._configPollHandler,
    });
    this.schedule.addSource("renderTick", {
      interval: 50,
      handler: () => this.requestRender(),
      priority: Priority.Render,
      enabled: () => !this.stopped,
    });
  }

  private _configPollHandler?: () => void;
  set onConfigPoll(handler: (() => void) | undefined) {
    this._configPollHandler = handler;
  }

  get phase(): InteractionPhase {
    return this.phaseManager.phase;
  }

  get fullRedraws(): number {
    if (this.renderer instanceof RetainedRenderer) {
      return this.renderer.stats.fullRedraws;
    }
    return 0;
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

  hasOverlay(): boolean {
    return this.overlayStack.some((o) => this.isOverlayVisible(o));
  }

  private isOverlayVisible(entry: (typeof this.overlayStack)[number]): boolean {
    if (entry.hidden) return false;
    if (entry.options?.visible) {
      return entry.options.visible(this.terminal.columns, this.terminal.rows);
    }
    return true;
  }

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
    const onInput = (data: string) => this.handleInput(data);
    const onResize = () => {
      this.renderer.reset();
      this.requestRender();
    };
    TerminalGuard.getInstance().arm(this.terminal, onInput, onResize);
    this.terminal.start(onInput, onResize);
    this.schedule.start();
    this.terminal.hideCursor();
    this.requestRender();
  }

  stop(): void {
    this.stopped = true;
    if (this.renderTimer) {
      clearTimeout(this.renderTimer);
      this.renderTimer = undefined;
    }

    this.schedule.stop();
    TerminalGuard.getInstance().disarm();
    this.renderer.shutdown();
    this.terminal.showCursor();
    this.terminal.stop();
  }

  requestRender(force = false): void {
    if (force) {
      this.renderer.reset();
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
    if (isKeyRelease(data) && this.focusedComponent && !this.focusedComponent.wantsKeyRelease) {
      return;
    }

    // Phase manager gets first look (buffers printable input during streaming)
    const phaseResult = this.phaseManager.handleInput(data);
    if (phaseResult.consumed) {
      return;
    }

    // Active modal with capturesAllKeys intercepts all input
    const activeModal = this.getTopModal();
    if (activeModal && activeModal.capturesAllKeys()) {
      const action = activeModal.handleKey(data);
      if (action === ModalAction.Close) {
        this.closeModal(activeModal);
      }
      this.requestRender();
      return;
    }

    // App-level onInput (screen routing, global keybindings)
    if (this.onInput?.(data, this.currentContext)) {
      this.requestRender();
      return;
    }

    // Non-capturing modal still gets a chance
    if (activeModal) {
      const action = activeModal.handleKey(data);
      if (action === ModalAction.Close) {
        this.closeModal(activeModal);
      }
      this.requestRender();
      return;
    }

    if (this.focusedComponent?.handleInput) {
      this.focusedComponent.handleInput(data);
      this.requestRender();
    }
  }

  onInput?: (data: string, context: InputContext) => boolean;

  showModal(modal: Modal, options?: { width?: string | number; anchor?: string; margin?: number }): ModalHandle {
    const overlayOptions = modalToOverlayOptions({
      ...options,
      nonCapturing: !modal.capturesAllKeys(),
    });
    if (modal.capturesAllKeys()) {
      this.setFocus(modal.component);
    }
    const handle = this.showOverlay(modal.component, overlayOptions);
    modal.onOpen?.();
    this.modalStack.push({ modal, handle });
    this.requestRender();
    return {
      close: () => {
        this.closeModal(modal);
      },
      isOpen: () => this.modalStack.some((m) => m.modal === modal),
    };
  }

  closeModal(modal: Modal): void {
    const idx = this.modalStack.findIndex((m) => m.modal === modal);
    if (idx === -1) return;
    const entry = this.modalStack[idx]!;
    entry.handle.hide();
    this.modalStack.splice(idx, 1);
    modal.onClose?.();
    this.requestRender();
  }

  private getTopModal(): Modal | undefined {
    for (let i = this.modalStack.length - 1; i >= 0; i--) {
      const entry = this.modalStack[i];
      if (!entry) continue;
      const handle = entry.handle;
      if (!handle.isHidden()) {
        return entry.modal;
      }
    }
    return undefined;
  }

  get hasModal(): boolean {
    return this.modalStack.length > 0;
  }

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
    const before = baseLine.slice(0, startCol);
    const after = baseLine.slice(startCol + overlayWidth);
    const result = before + overlayLine + after;

    if (visibleWidth(result) > totalWidth) {
      return result.slice(0, totalWidth);
    }

    return result;
  }

  private doRender(): void {
    if (this.stopped) return;

    const width = this.terminal.columns;
    const height = this.terminal.rows;

    // Render all components
    let newLines = this.render(width);

    // Composite overlays
    if (this.overlayStack.length > 0) {
      newLines = this.compositeOverlays(newLines, width, height);
    }

    // Delegate to renderer
    this.renderer.render(newLines, width, height);
  }
}
