import { describe, it, expect } from "vitest";
import { ModalAction, isModal } from "../modal.js";
import type { Component } from "../../core/tui.js";

describe("ModalAction", () => {
  it("should have Continue and Close values", () => {
    expect(ModalAction.Continue).toBe("continue");
    expect(ModalAction.Close).toBe("close");
  });
});

describe("isModal", () => {
  it("should return true for objects with handleKey and capturesAllKeys", () => {
    const modal = { handleKey: () => ModalAction.Continue, capturesAllKeys: () => true };
    expect(isModal(modal as Component)).toBe(true);
  });

  it("should return false for plain components", () => {
    const component: Component = {
      render: () => [],
      invalidate: () => {},
    };
    expect(isModal(component)).toBe(false);
  });

  it("should return false for objects missing handleKey", () => {
    const obj = { capturesAllKeys: () => true };
    expect(isModal(obj as any)).toBe(false);
  });

  it("should return false for objects missing capturesAllKeys", () => {
    const obj = { handleKey: () => ModalAction.Continue };
    expect(isModal(obj as any)).toBe(false);
  });
});
