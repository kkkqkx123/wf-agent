import { describe, it, expect, vi } from "vitest";
import { InteractionPhase, PhaseManager } from "../interaction-phase.js";

describe("InteractionPhase enum", () => {
  it("should have four phases", () => {
    expect(InteractionPhase.Idle).toBe("idle");
    expect(InteractionPhase.Streaming).toBe("streaming");
    expect(InteractionPhase.Approval).toBe("approval");
    expect(InteractionPhase.Normal).toBe("normal");
  });
});

describe("PhaseManager", () => {
  it("should start in Idle phase", () => {
    const pm = new PhaseManager();
    expect(pm.phase).toBe(InteractionPhase.Idle);
  });

  it("should return valid transitions for each phase", () => {
    const idleTransitions = PhaseManager.getValidTransitions(InteractionPhase.Idle);
    expect(idleTransitions).toContain(InteractionPhase.Streaming);
    expect(idleTransitions).toContain(InteractionPhase.Normal);

    const streamingTransitions = PhaseManager.getValidTransitions(InteractionPhase.Streaming);
    expect(streamingTransitions).toContain(InteractionPhase.Idle);
    expect(streamingTransitions).toContain(InteractionPhase.Approval);
    expect(streamingTransitions).toContain(InteractionPhase.Normal);

    const approvalTransitions = PhaseManager.getValidTransitions(InteractionPhase.Approval);
    expect(approvalTransitions).toContain(InteractionPhase.Idle);
    expect(approvalTransitions).toContain(InteractionPhase.Streaming);

    const normalTransitions = PhaseManager.getValidTransitions(InteractionPhase.Normal);
    expect(normalTransitions).toContain(InteractionPhase.Idle);
    expect(normalTransitions).toContain(InteractionPhase.Streaming);
  });

  it("should transition between valid phases", () => {
    const pm = new PhaseManager();
    expect(pm.transition(InteractionPhase.Streaming)).toBe(true);
    expect(pm.phase).toBe(InteractionPhase.Streaming);
  });

  it("should reject transition to same phase", () => {
    const pm = new PhaseManager();
    expect(pm.transition(InteractionPhase.Idle)).toBe(false);
    expect(pm.phase).toBe(InteractionPhase.Idle);
  });

  it("should reject invalid transition", () => {
    const pm = new PhaseManager();
    expect(pm.transition(InteractionPhase.Approval)).toBe(false);
    expect(pm.phase).toBe(InteractionPhase.Idle);
  });

  it("should fire onPhaseChange callback on transition", () => {
    const pm = new PhaseManager();
    const handler = vi.fn();
    pm.onPhaseChange = handler;

    pm.transition(InteractionPhase.Streaming);
    expect(handler).toHaveBeenCalledWith(InteractionPhase.Idle, InteractionPhase.Streaming, undefined);

    pm.transition(InteractionPhase.Normal);
    expect(handler).toHaveBeenCalledWith(InteractionPhase.Streaming, InteractionPhase.Normal, undefined);
  });

  it("should pass data through onPhaseChange", () => {
    const pm = new PhaseManager();
    const handler = vi.fn();
    pm.onPhaseChange = handler;

    pm.transition(InteractionPhase.Streaming, "submit");
    expect(handler).toHaveBeenCalledWith(InteractionPhase.Idle, InteractionPhase.Streaming, "submit");
  });

  it("should buffer printable input during Streaming phase", () => {
    const pm = new PhaseManager();
    pm.transition(InteractionPhase.Streaming);

    const result = pm.handleInput("h");
    expect(result.consumed).toBe(true);
    expect(result.buffered).toBe(true);
    expect(pm.hasBufferedInput).toBe(true);
  });

  it("should not buffer control keys during Streaming", () => {
    const pm = new PhaseManager();
    pm.transition(InteractionPhase.Streaming);

    const ctrlC = pm.handleInput("\x03");
    expect(ctrlC.consumed).toBe(false);
    expect(ctrlC.buffered).toBe(false);

    const esc = pm.handleInput("\x1b");
    expect(esc.consumed).toBe(false);

    const enter = pm.handleInput("\r");
    expect(enter.consumed).toBe(false);
  });

  it("should not buffer multi-byte control sequences during Streaming", () => {
    const pm = new PhaseManager();
    pm.transition(InteractionPhase.Streaming);

    const arrowUp = pm.handleInput("\x1b[A");
    expect(arrowUp.consumed).toBe(false);
  });

  it("should not buffer input during non-Streaming phases", () => {
    const pm = new PhaseManager();
    expect(pm.phase).toBe(InteractionPhase.Idle);

    const result = pm.handleInput("hello");
    expect(result.consumed).toBe(false);
    expect(result.buffered).toBe(false);
  });

  it("should release buffered input on transition away from Streaming", () => {
    const pm = new PhaseManager();
    const handler = vi.fn();
    pm.onInputRelease = handler;

    pm.transition(InteractionPhase.Streaming);
    pm.handleInput("a");
    pm.handleInput("b");
    pm.handleInput("c");

    pm.transition(InteractionPhase.Idle);
    expect(handler).toHaveBeenCalledWith(["a", "b", "c"]);
  });

  it("should clear buffered input with clearBufferedInput", () => {
    const pm = new PhaseManager();
    pm.transition(InteractionPhase.Streaming);
    pm.handleInput("a");
    pm.handleInput("b");

    pm.clearBufferedInput();
    expect(pm.hasBufferedInput).toBe(false);
  });

  it("should handle chained transitions correctly", () => {
    const pm = new PhaseManager();
    const handler = vi.fn();
    pm.onPhaseChange = handler;

    pm.transition(InteractionPhase.Streaming);
    pm.transition(InteractionPhase.Approval);
    expect(pm.phase).toBe(InteractionPhase.Approval);

    pm.transition(InteractionPhase.Idle);
    expect(pm.phase).toBe(InteractionPhase.Idle);

    expect(handler).toHaveBeenCalledTimes(3);
    expect(handler).toHaveBeenNthCalledWith(1, InteractionPhase.Idle, InteractionPhase.Streaming, undefined);
    expect(handler).toHaveBeenNthCalledWith(2, InteractionPhase.Streaming, InteractionPhase.Approval, undefined);
    expect(handler).toHaveBeenNthCalledWith(3, InteractionPhase.Approval, InteractionPhase.Idle, undefined);
  });

  it("should support pause/resume on internal input buffer", () => {
    const pm = new PhaseManager();
    pm.transition(InteractionPhase.Streaming);

    pm.inputBuffer.pause();
    pm.handleInput("a");
    pm.handleInput("b");
    expect(pm.inputBuffer.length).toBe(0);

    pm.inputBuffer.resume();
    expect(pm.inputBuffer.length).toBe(2);
  });
});
