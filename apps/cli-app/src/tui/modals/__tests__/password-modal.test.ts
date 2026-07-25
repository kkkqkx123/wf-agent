import { describe, it, expect, vi } from "vitest";
import { PasswordModal } from "../password-modal.js";
import { ModalAction } from "../modal.js";

describe("PasswordModal", () => {
  it("should capture all keys", () => {
    const modal = new PasswordModal("Enter password:");
    expect(modal.capturesAllKeys()).toBe(true);
  });

  it("should submit input on Enter", () => {
    const onSubmit = vi.fn();
    const modal = new PasswordModal("pwd:", onSubmit);
    modal.handleKey("h");
    modal.handleKey("i");
    modal.handleKey("\r");
    expect(onSubmit).toHaveBeenCalledWith("hi");
  });

  it("should cancel with null on Esc", () => {
    const onSubmit = vi.fn();
    const modal = new PasswordModal("pwd:", onSubmit);
    modal.handleKey("\x1b");
    expect(onSubmit).toHaveBeenCalledWith(null);
  });

  it("should cancel with null on Ctrl+C", () => {
    const onSubmit = vi.fn();
    const modal = new PasswordModal("pwd:", onSubmit);
    modal.handleKey("\x03");
    expect(onSubmit).toHaveBeenCalledWith(null);
  });

  it("should handle backspace", () => {
    const onSubmit = vi.fn();
    const modal = new PasswordModal("pwd:", onSubmit);
    modal.handleKey("a");
    modal.handleKey("b");
    modal.handleKey("\x7f");
    modal.handleKey("\r");
    expect(onSubmit).toHaveBeenCalledWith("a");
  });

  it("should handle \\b backspace", () => {
    const onSubmit = vi.fn();
    const modal = new PasswordModal("pwd:", onSubmit);
    modal.handleKey("x");
    modal.handleKey("\b");
    modal.handleKey("\r");
    expect(onSubmit).toHaveBeenCalledWith("");
  });

  it("should ignore non-printable keys", () => {
    const modal = new PasswordModal("pwd:");
    const result = modal.handleKey("\x01");
    expect(result).toBe(ModalAction.Continue);
    modal.handleKey("\r");
    expect(modal["input"]).toBe("");
  });

  it("should ignore keys after resolution", () => {
    const onSubmit = vi.fn();
    const modal = new PasswordModal("pwd:", onSubmit);
    modal.handleKey("\r");
    expect(onSubmit).toHaveBeenCalledTimes(1);
    modal.handleKey("\r");
    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  it("should render masked input and border", () => {
    const modal = new PasswordModal("Enter password:");
    modal.handleKey("a");
    modal.handleKey("b");
    modal.handleKey("c");
    const lines = modal.render(80);
    expect(lines[0]).toContain("Password Required");
    expect(lines.some((l) => l.includes("\u2022"))).toBe(true);
    expect(lines.some((l) => l.includes("Enter password"))).toBe(true);
    expect(lines[lines.length - 1]!.startsWith("└")).toBe(true);
  });

  it("should not throw without callback", () => {
    const modal = new PasswordModal("pwd:");
    expect(() => modal.handleKey("x")).not.toThrow();
    expect(() => modal.handleKey("\r")).not.toThrow();
  });

  it("invalidate should not throw", () => {
    const modal = new PasswordModal("pwd:");
    expect(() => modal.invalidate()).not.toThrow();
  });
});
