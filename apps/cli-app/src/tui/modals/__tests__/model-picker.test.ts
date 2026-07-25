import { describe, it, expect, vi } from "vitest";
import { ModelPicker } from "../model-picker.js";
import { ModalAction } from "../modal.js";

const MODELS = [
  { id: "gpt-4", name: "GPT-4", provider: "openai" },
  { id: "claude-3", name: "Claude 3", provider: "anthropic" },
  { id: "gemini-pro", name: "Gemini Pro", provider: "google" },
];

describe("ModelPicker", () => {
  it("should capture all keys", () => {
    const picker = new ModelPicker(MODELS);
    expect(picker.capturesAllKeys()).toBe(true);
  });

  it("should select on Enter", () => {
    const onSelect = vi.fn();
    const picker = new ModelPicker(MODELS, onSelect);
    picker.handleKey("\r");
    expect(onSelect).toHaveBeenCalledWith(MODELS[0]);
  });

  it("should cancel on Esc", () => {
    const onSelect = vi.fn();
    const picker = new ModelPicker(MODELS, onSelect);
    picker.handleKey("\x1b");
    expect(onSelect).toHaveBeenCalledWith(null);
  });

  it("should navigate down", () => {
    const picker = new ModelPicker(MODELS);
    const result = picker.handleKey("\x1b[B");
    expect(result).toBe(ModalAction.Continue);
    picker.handleKey("\r");
    expect(picker["selectedIndex"]).toBe(1);
  });

  it("should navigate up", () => {
    const picker = new ModelPicker(MODELS);
    picker.handleKey("\x1b[B");
    picker.handleKey("\x1b[B");
    picker.handleKey("\x1b[A");
    picker.handleKey("\r");
    expect(picker["selectedIndex"]).toBe(1);
  });

  it("should not go above first item", () => {
    const picker = new ModelPicker(MODELS);
    picker.handleKey("\x1b[A");
    picker.handleKey("\r");
    expect(picker["selectedIndex"]).toBe(0);
  });

  it("should not go below last item", () => {
    const picker = new ModelPicker(MODELS);
    picker.handleKey("\x1b[B");
    picker.handleKey("\x1b[B");
    picker.handleKey("\x1b[B");
    picker.handleKey("\x1b[B");
    picker.handleKey("\r");
    expect(picker["selectedIndex"]).toBe(2);
  });

  it("should render list with selection indicator", () => {
    const picker = new ModelPicker(MODELS);
    const lines = picker.render(80);
    expect(lines[0]).toContain("Select Model");
    expect(lines.some((l) => l.includes(MODELS[0].name))).toBe(true);
    expect(lines.some((l) => l.includes("openai"))).toBe(true);
    expect(lines.some((l) => l.includes("\u2192"))).toBe(true);
    expect(lines[lines.length - 1]!.startsWith("└")).toBe(true);
  });

  it("should handle empty model list", () => {
    const picker = new ModelPicker([]);
    const result = picker.handleKey("\r");
    expect(result).toBe(ModalAction.Close);
  });

  it("should not throw without callback", () => {
    const picker = new ModelPicker(MODELS);
    expect(() => picker.handleKey("\x1b[B")).not.toThrow();
    expect(() => picker.handleKey("\r")).not.toThrow();
  });

  it("invalidate should not throw", () => {
    const picker = new ModelPicker(MODELS);
    expect(() => picker.invalidate()).not.toThrow();
  });
});
