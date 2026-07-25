import type { TUI } from "../core/tui.js";
import { InteractionPhase } from "../core/interaction-phase.js";
import type { Modal, ModalHandle } from "../core/modal.js";

export class ModalManager {
  private tui: TUI;
  private orphanCleanupRegistered = false;

  constructor(tui: TUI) {
    this.tui = tui;
  }

  get activeModal(): Modal | null {
    if (this.tui.modalStack.length === 0) return null;
    const top = this.tui.modalStack[this.tui.modalStack.length - 1];
    if (!top) return null;
    return top.modal;
  }

  get isCapturing(): boolean {
    const modal = this.activeModal;
    return modal !== null && modal.capturesAllKeys();
  }

  get hasOpen(): boolean {
    return this.tui.hasModal;
  }

  show(modal: Modal, options?: { width?: string | number; anchor?: string; margin?: number }): ModalHandle {
    this.ensureOrphanCleanup();
    return this.tui.showModal(modal, options);
  }

  close(): void {
    if (this.tui.modalStack.length > 0) {
      const entry = this.tui.modalStack[this.tui.modalStack.length - 1];
      if (entry) {
        this.tui.closeModal(entry.modal);
      }
    }
  }

  closeAll(): void {
    while (this.tui.modalStack.length > 0) {
      this.close();
    }
  }

  private ensureOrphanCleanup(): void {
    if (this.orphanCleanupRegistered) return;
    this.orphanCleanupRegistered = true;

    this.tui.phaseManager.onPhaseChange = ((originalHandler) => {
      return (from: InteractionPhase, to: InteractionPhase, data?: string) => {
        if (to === InteractionPhase.Idle && this.hasOpen && this.isCapturing) {
          this.closeAll();
        }
        if (originalHandler) {
          originalHandler(from, to, data);
        }
      };
    })(this.tui.phaseManager.onPhaseChange);
  }
}
