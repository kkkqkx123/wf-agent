import { describe, it, expect, vi } from "vitest";
import { ConfirmModal } from "../confirm-modal.js";
import { ModalAction } from "../modal.js";

describe("ConfirmModal", () => {
  it("should capture all keys", () => {
    const modal = new ConfirmModal("Test", "message?");
    expect(modal.capturesAllKeys()).toBe(true);
  });

  it("should confirm on Y", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    const modal = new ConfirmModal("Test", "msg", onConfirm, onCancel);
    const result = modal.handleKey("y");
    expect(result).toBe(ModalAction.Close);
    expect(onConfirm).toHaveBeenCalled();
    expect(onCancel).not.toHaveBeenCalled();
  });

  it("should confirm on Enter", () => {
    const onConfirm = vi.fn();
    const modal = new ConfirmModal("Test", "msg", onConfirm);
    modal.handleKey("\r");
    expect(onConfirm).toHaveBeenCalled();
  });

  it("should cancel on N", () => {
    const onCancel = vi.fn();
    const modal = new ConfirmModal("Test", "msg", undefined, onCancel);
    const result = modal.handleKey("n");
    expect(result).toBe(ModalAction.Close);
    expect(onCancel).toHaveBeenCalled();
  });

  it("should cancel on Esc", () => {
    const onCancel = vi.fn();
    const modal = new ConfirmModal("Test", "msg", undefined, onCancel);
    modal.handleKey("\x1b");
    expect(onCancel).toHaveBeenCalled();
  });

  it("should continue on other keys", () => {
    const modal = new ConfirmModal("Test", "msg");
    const result = modal.handleKey("x");
    expect(result).toBe(ModalAction.Continue);
  });

  it("should ignore keys after resolution", () => {
    const onConfirm = vi.fn();
    const modal = new ConfirmModal("Test", "msg", onConfirm);
    modal.handleKey("y");
    expect(onConfirm).toHaveBeenCalledTimes(1);
    modal.handleKey("y");
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("should render border with title and message", () => {
    const modal = new ConfirmModal("My Title", "Are you sure?");
    const lines = modal.render(80);
    expect(lines.length).toBeGreaterThan(3);
    expect(lines[0]).toContain("My Title");
    expect(lines.some((l) => l.includes("Are you sure?"))).toBe(true);
    expect(lines.some((l) => l.includes("confirm"))).toBe(true);
    expect(lines[lines.length - 1]!.startsWith("└")).toBe(true);
  });

  it("should not throw without callbacks", () => {
    const modal = new ConfirmModal("Test", "msg");
    expect(() => modal.handleKey("y")).not.toThrow();
  });

  it("invalidate should not throw", () => {
    const modal = new ConfirmModal("T", "m");
    expect(() => modal.invalidate()).not.toThrow();
  });
});
