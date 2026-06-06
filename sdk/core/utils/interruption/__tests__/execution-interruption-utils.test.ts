/**
 * Unit Tests for Execution Interruption Utilities
 */

import { describe, it, expect } from "vitest";
import {
  checkExecutionInterruption,
  shouldContinueExecution,
  getExecutionInterruptionType,
  getExecutionInterruptionDescription,
} from "../execution-interruption-utils.js";

describe("checkExecutionInterruption", () => {
  it("should return continue for undefined signal", () => {
    const result = checkExecutionInterruption(undefined);
    expect(result).toEqual({ type: "continue" });
  });

  it("should return continue for non-aborted signal", () => {
    const controller = new AbortController();
    const result = checkExecutionInterruption(controller.signal);
    expect(result).toEqual({ type: "continue" });
  });

  it("should return paused when signal aborted with PAUSE interruptionType", () => {
    const controller = new AbortController();
    const reason = new Error("Execution paused") as Error & {
      interruptionType: string;
      executionId: string;
    };
    reason.interruptionType = "PAUSE";
    reason.executionId = "exec-123";
    controller.abort(reason);

    const result = checkExecutionInterruption(controller.signal);
    expect(result).toEqual({ type: "paused", executionId: "exec-123" });
  });

  it("should return stopped when signal aborted with STOP interruptionType", () => {
    const controller = new AbortController();
    const reason = new Error("Execution stopped") as Error & {
      interruptionType: string;
      executionId: string;
    };
    reason.interruptionType = "STOP";
    reason.executionId = "exec-456";
    controller.abort(reason);

    const result = checkExecutionInterruption(controller.signal);
    expect(result).toEqual({ type: "stopped", executionId: "exec-456" });
  });

  it("should return aborted when signal aborted without interruptionType", () => {
    const controller = new AbortController();
    controller.abort("some reason");

    const result = checkExecutionInterruption(controller.signal);
    expect(result).toEqual({ type: "aborted", reason: "some reason" });
  });

  it("should return aborted when reason is not an object", () => {
    const controller = new AbortController();
    controller.abort("plain string reason");

    const result = checkExecutionInterruption(controller.signal);
    expect(result.type).toBe("aborted");
    expect(result.reason).toBe("plain string reason");
  });
});

describe("shouldContinueExecution", () => {
  it("should return true for continue", () => {
    expect(shouldContinueExecution({ type: "continue" })).toBe(true);
  });

  it("should return false for paused", () => {
    expect(shouldContinueExecution({ type: "paused" })).toBe(false);
  });

  it("should return false for stopped", () => {
    expect(shouldContinueExecution({ type: "stopped" })).toBe(false);
  });

  it("should return false for aborted", () => {
    expect(shouldContinueExecution({ type: "aborted" })).toBe(false);
  });
});

describe("getExecutionInterruptionType", () => {
  it("should return PAUSE for paused result", () => {
    expect(getExecutionInterruptionType({ type: "paused" })).toBe("PAUSE");
  });

  it("should return STOP for stopped result", () => {
    expect(getExecutionInterruptionType({ type: "stopped" })).toBe("STOP");
  });

  it("should return null for continue result", () => {
    expect(getExecutionInterruptionType({ type: "continue" })).toBeNull();
  });

  it("should return null for aborted result", () => {
    expect(getExecutionInterruptionType({ type: "aborted" })).toBeNull();
  });
});

describe("getExecutionInterruptionDescription", () => {
  it("should describe continue state", () => {
    expect(getExecutionInterruptionDescription({ type: "continue" })).toBe(
      "Execution continuing",
    );
  });

  it("should describe paused state with executionId", () => {
    const desc = getExecutionInterruptionDescription({
      type: "paused",
      executionId: "exec-1",
    });
    expect(desc).toContain("Execution paused");
    expect(desc).toContain("exec-1");
  });

  it("should describe paused state without executionId", () => {
    const desc = getExecutionInterruptionDescription({ type: "paused" });
    expect(desc).toBe("Execution paused");
  });

  it("should describe stopped state with executionId", () => {
    const desc = getExecutionInterruptionDescription({
      type: "stopped",
      executionId: "exec-2",
    });
    expect(desc).toContain("Execution stopped");
    expect(desc).toContain("exec-2");
  });

  it("should describe aborted state with reason", () => {
    const desc = getExecutionInterruptionDescription({
      type: "aborted",
      reason: "timeout",
    });
    expect(desc).toBe("timeout");
  });

  it("should describe aborted state without reason", () => {
    const desc = getExecutionInterruptionDescription({ type: "aborted" });
    expect(desc).toBe("Execution operation aborted");
  });
});