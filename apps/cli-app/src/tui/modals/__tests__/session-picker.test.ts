import { describe, it, expect, vi } from "vitest";
import { SessionPicker } from "../session-picker.js";
import { ModalAction } from "../modal.js";

const SESSIONS = [
  { id: "s1", label: "Session 1", timestamp: Date.now() - 86400000 },
  { id: "s2", label: "Session 2", timestamp: Date.now() },
  { id: "s3", label: "Session 3" },
];

describe("SessionPicker", () => {
  it("should capture all keys", () => {
    const picker = new SessionPicker(SESSIONS);
    expect(picker.capturesAllKeys()).toBe(true);
  });

  it("should select on Enter", () => {
    const onSelect = vi.fn();
    const picker = new SessionPicker(SESSIONS, onSelect);
    picker.handleKey("\r");
    expect(onSelect).toHaveBeenCalledWith(SESSIONS[0]);
  });

  it("should cancel on Esc", () => {
    const onSelect = vi.fn();
    const picker = new SessionPicker(SESSIONS, onSelect);
    picker.handleKey("\x1b");
    expect(onSelect).toHaveBeenCalledWith(null);
  });

  it("should navigate down", () => {
    const picker = new SessionPicker(SESSIONS);
    picker.handleKey("\x1b[B");
    picker.handleKey("\r");
    expect(picker["selectedIndex"]).toBe(1);
  });

  it("should navigate up", () => {
    const picker = new SessionPicker(SESSIONS);
    picker.handleKey("\x1b[B");
    picker.handleKey("\x1b[A");
    picker.handleKey("\r");
    expect(picker["selectedIndex"]).toBe(0);
  });

  it("should not exceed bounds", () => {
    const picker = new SessionPicker(SESSIONS);
    picker.handleKey("\x1b[A");
    expect(picker["selectedIndex"]).toBe(0);
    picker.handleKey("\x1b[B");
    picker.handleKey("\x1b[B");
    picker.handleKey("\x1b[B");
    picker.handleKey("\x1b[B");
    expect(picker["selectedIndex"]).toBe(2);
  });

  it("should render list with sessions", () => {
    const picker = new SessionPicker(SESSIONS);
    const lines = picker.render(80);
    expect(lines[0]).toContain("Select Session");
    expect(lines.some((l) => l.includes("Session 1"))).toBe(true);
    expect(lines.some((l) => l.includes("\u2192"))).toBe(true);
    expect(lines[lines.length - 1]!.startsWith("└")).toBe(true);
  });

  it("should handle empty session list", () => {
    const picker = new SessionPicker([]);
    const result = picker.handleKey("\r");
    expect(result).toBe(ModalAction.Close);
  });

  it("should not throw without callback", () => {
    const picker = new SessionPicker(SESSIONS);
    expect(() => picker.handleKey("\x1b[B")).not.toThrow();
    expect(() => picker.handleKey("\r")).not.toThrow();
  });

  it("invalidate should not throw", () => {
    const picker = new SessionPicker(SESSIONS);
    expect(() => picker.invalidate()).not.toThrow();
  });
});
