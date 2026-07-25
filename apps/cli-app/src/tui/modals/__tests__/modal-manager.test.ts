import { describe, it, expect, vi, beforeEach } from "vitest";
import { ModalManager } from "../modal-manager.js";
import { ModalAction } from "../../core/modal.js";
import type { Modal, ModalHandle } from "../../core/modal.js";
import type { TUI } from "../../core/tui.js";
import { PhaseManager } from "../../core/interaction-phase.js";

function createModalStub(capturesAllKeys = false): Modal {
  return {
    handleKey: vi.fn(() => ModalAction.Continue),
    capturesAllKeys: () => capturesAllKeys,
  };
}

function createMockTUI(): TUI {
  const phaseManager = new PhaseManager();
  const modalStack: { modal: Modal; handle: ModalHandle }[] = [];
  return {
    phaseManager,
    modalStack: modalStack as any,
    get hasModal() { return modalStack.length > 0; },
    showModal: vi.fn((modal: Modal, _options?: any): ModalHandle => {
      const handle: ModalHandle = { close: vi.fn(), isOpen: () => true };
      modalStack.push({ modal, handle });
      return handle;
    }),
    closeModal: vi.fn((modal: Modal) => {
      const idx = modalStack.findIndex((e) => e.modal === modal);
      if (idx >= 0) modalStack.splice(idx, 1);
    }),
  } as unknown as TUI;
}

describe("ModalManager", () => {
  let tui: TUI;
  let manager: ModalManager;

  beforeEach(() => {
    tui = createMockTUI();
    manager = new ModalManager(tui);
  });

  it("should have no active modal initially", () => {
    expect(manager.activeModal).toBeNull();
    expect(manager.hasOpen).toBe(false);
    expect(manager.isCapturing).toBe(false);
  });

  it("should show a modal", () => {
    const modal = createModalStub();
    const handle = manager.show(modal);
    expect(manager.hasOpen).toBe(true);
    expect(manager.activeModal).toBe(modal);
    expect(handle).toBeDefined();
  });

  it("should close the active modal", () => {
    const modal = createModalStub();
    manager.show(modal);
    expect(manager.hasOpen).toBe(true);
    manager.close();
    expect(manager.hasOpen).toBe(false);
    expect(manager.activeModal).toBeNull();
  });

  it("should close all modals", () => {
    manager.show(createModalStub());
    manager.show(createModalStub());
    expect(manager.hasOpen).toBe(true);
    manager.closeAll();
    expect(manager.hasOpen).toBe(false);
  });

  it("isCapturing should be true only when modal capturesAllKeys", () => {
    expect(manager.isCapturing).toBe(false);
    manager.show(createModalStub(false));
    expect(manager.isCapturing).toBe(false);
    manager.close();

    manager.show(createModalStub(true));
    expect(manager.isCapturing).toBe(true);
  });

  it("should close capturing modals on transition to Idle (orphan cleanup)", () => {
    manager.show(createModalStub(true));
    expect(manager.hasOpen).toBe(true);

    const pm = tui.phaseManager;
    pm.transition("streaming" as any);
    pm.transition("idle" as any);

    expect(manager.hasOpen).toBe(false);
  });

  it("should not close non-capturing modals on transition to Idle", () => {
    manager.show(createModalStub(false));
    const pm = tui.phaseManager;
    pm.transition("streaming" as any);
    pm.transition("idle" as any);
    expect(manager.hasOpen).toBe(true);
  });

  it("should close gracefully when no modal is open", () => {
    expect(() => manager.close()).not.toThrow();
    expect(() => manager.closeAll()).not.toThrow();
  });
});
