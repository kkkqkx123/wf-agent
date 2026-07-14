/**
 * Unit Test for ToolExecutionSignal
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { ToolExecutionSignal } from "../tool-execution-signal.js";

describe("ToolExecutionSignal", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(1000000);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should create a new ToolExecutionSignal with pending state", () => {
    const signal = new ToolExecutionSignal({
      toolId: "test-tool",
      toolName: "Test Tool",
      executionId: "exec-1",
    });

    expect(signal.toolId).toBe("test-tool");
    expect(signal.toolName).toBe("Test Tool");
    expect(signal.executionId).toBe("exec-1");
    expect(signal.state).toBe("pending");
    expect(signal.startedAt).toBe(1000000);
  });

  it("should mark the execution as completed", () => {
    const signal = new ToolExecutionSignal({
      toolId: "test-tool",
      toolName: "Test Tool",
      executionId: "exec-1",
    });

    signal.complete();
    expect(signal.state).toBe("completed");
    expect(signal.isCompleted).toBe(true);
  });

  it("should mark the execution as failed", () => {
    const signal = new ToolExecutionSignal({
      toolId: "test-tool",
      toolName: "Test Tool",
      executionId: "exec-1",
    });

    const error = new Error("Something went wrong");
    signal.fail(error);
    expect(signal.state).toBe("failed");
    expect(signal.error).toBe(error);
    expect(signal.isFailed).toBe(true);
  });

  it("should cancel the execution", () => {
    const signal = new ToolExecutionSignal({
      toolId: "test-tool",
      toolName: "Test Tool",
      executionId: "exec-1",
    });

    signal.cancel("user");
    expect(signal.state).toBe("cancelled");
    expect(signal.cancelledAt).toBe(1000000);
    expect(signal.cancelledBy).toBe("user");
    expect(signal.isCancelled).toBe(true);
  });

  it("should check if the execution has timed out", () => {
    const signal = new ToolExecutionSignal({
      toolId: "test-tool",
      toolName: "Test Tool",
      executionId: "exec-1",
      timeout: 5000,
    });

    expect(signal.isTimedOut).toBe(false);

    vi.setSystemTime(1000000 + 5001);
    expect(signal.isTimedOut).toBe(true);
  });

  it("should return false for timeout if no timeout is set", () => {
    const signal = new ToolExecutionSignal({
      toolId: "test-tool",
      toolName: "Test Tool",
      executionId: "exec-1",
    });

    expect(signal.isTimedOut).toBe(false);
  });

  it("should set the initial state correctly", () => {
    const signal = new ToolExecutionSignal({
      toolId: "test-tool",
      toolName: "Test Tool",
      executionId: "exec-1",
      state: "running",
    });

    expect(signal.state).toBe("running");
    expect(signal.isRunning).toBe(true);
  });
});