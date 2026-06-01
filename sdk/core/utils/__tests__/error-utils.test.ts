/**
 * Unit Testing for Error Utils
 * Testing the error handling utility functions
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { logError, emitErrorEvent, handleError } from "../error-utils.js";
import { SDKError } from "@wf-agent/types";
import type { EventRegistry } from "../../registry/event-registry.js";

// Create mock references using vi.hoisted to ensure they are initialized before vi.mock hoisting
const { mockLogger, mockEmit, mockBuildErrorEvent } = vi.hoisted(() => ({
  mockLogger: {
    error: vi.fn(),
    warn: vi.fn(),
    info: vi.fn(),
  },
  mockEmit: vi.fn(),
  mockBuildErrorEvent: vi.fn((params: Record<string, unknown>) => ({
    type: "ERROR",
    ...params,
  })),
}));

// Mock logger - sdkLogger is the actual export from logger.ts
vi.mock("../../../utils/logger", () => ({
  sdkLogger: mockLogger,
}));

// Mock event builders
vi.mock("../event/builders/index.js", () => ({
  buildErrorEvent: mockBuildErrorEvent,
}));

// Mock event emitter — must export 'emit' to match what error-utils.ts imports
vi.mock("../event/event-emitter.js", () => ({
  emit: mockEmit,
}));

describe("Error Utils", () => {
  let mockEventManager: EventRegistry;
  let mockSDKError: SDKError;

  beforeEach(() => {
    // Reset mocks
    vi.clearAllMocks();

    // Mock event manager
    mockEventManager = {
      emit: vi.fn().mockResolvedValue(undefined),
      on: vi.fn(),
      off: vi.fn(),
      waitFor: vi.fn(),
    } as unknown as EventRegistry;

    // Create a test SDKError
    mockSDKError = new SDKError(
      "Test error message",
      "error",
      { executionId: "test-workflow-execution", workflowId: "test-workflow", nodeId: "test-node" },
      new Error("Original error"),
    );
  });

  describe("logError", () => {
    it("Error-level errors should be logged using `logger.error`.", () => {
      const error = new SDKError("Test error", "error", {});
      const context = { executionId: "test-workflow-execution" };

      logError(error, context);

      expect(mockLogger.error).toHaveBeenCalledWith("Test error", {
        errorType: "SDKError",
        errorMessage: "Test error",
        severity: "error",
        executionId: "test-workflow-execution",
      });
    });

    it("The `logger.warn` should be used to record errors of the warning level.", () => {
      const error = new SDKError("Test warning", "warning", {});
      const context = { executionId: "test-workflow-execution" };

      logError(error, context);

      expect(mockLogger.warn).toHaveBeenCalledWith("Test warning", {
        errorType: "SDKError",
        errorMessage: "Test warning",
        severity: "warning",
        executionId: "test-workflow-execution",
      });
    });

    it("You should use `logger.info` to record errors at the info level.", () => {
      const error = new SDKError("Test info", "info", {});
      const context = { executionId: "test-workflow-execution" };

      logError(error, context);

      expect(mockLogger.info).toHaveBeenCalledWith("Test info", {
        errorType: "SDKError",
        errorMessage: "Test info",
        severity: "info",
        executionId: "test-workflow-execution",
      });
    });

    it("It should work properly even without providing context.", () => {
      const error = new SDKError("Test error", "error", {});

      logError(error);

      expect(mockLogger.error).toHaveBeenCalledWith("Test error", {
        errorType: "SDKError",
        errorMessage: "Test error",
        severity: "error",
      });
    });

    it("It should include all the provided context fields.", () => {
      const error = new SDKError("Test error", "error", {});
      const context = {
        executionId: "test-workflow-execution",
        workflowId: "test-workflow",
        nodeId: "test-node",
        additionalField: "additional-value",
      };

      logError(error, context);

      expect(mockLogger.error).toHaveBeenCalledWith(
        "Test error",
        expect.objectContaining({
          executionId: "test-workflow-execution",
          workflowId: "test-workflow",
          nodeId: "test-node",
          additionalField: "additional-value",
        }),
      );
    });
  });

  describe("emitErrorEvent", () => {
    it("The `emit` should be used to trigger error events.", async () => {
      await emitErrorEvent(mockEventManager, {
        executionId: "test-workflow-execution",
        workflowId: "test-workflow",
        nodeId: "test-node",
        error: mockSDKError,
      });

      expect(mockEmit).toHaveBeenCalledWith(
        mockEventManager,
        expect.objectContaining({
          type: "ERROR",
          executionId: "test-workflow-execution",
          workflowId: "test-workflow",
          nodeId: "test-node",
          error: mockSDKError,
        }),
      );
    });

    it("It should be able to handle the optional nodeId parameter.", async () => {
      await emitErrorEvent(mockEventManager, {
        executionId: "test-workflow-execution",
        workflowId: "test-workflow",
        error: mockSDKError,
      });

      expect(mockEmit).toHaveBeenCalledWith(
        mockEventManager,
        expect.objectContaining({
          type: "ERROR",
          executionId: "test-workflow-execution",
          workflowId: "test-workflow",
          error: mockSDKError,
        }),
      );
    });

    it("It should be able to handle the undefined eventManager.", async () => {
      await emitErrorEvent(undefined, {
        executionId: "test-workflow-execution",
        workflowId: "test-workflow",
        error: mockSDKError,
      });

      // When eventManager is undefined, emitErrorEvent logs a warning and returns early
      expect(mockEmit).not.toHaveBeenCalled();
      expect(mockLogger.warn).toHaveBeenCalled();
    });
  });

  describe("handleError", () => {
    it("Logs should be recorded simultaneously, and the events should be triggered accordingly.", async () => {
      await handleError(mockEventManager, mockSDKError, {
        executionId: "test-workflow-execution",
        workflowId: "test-workflow",
        nodeId: "test-node",
      });

      // Verify log entries.
      expect(mockLogger.error).toHaveBeenCalledWith(
        "Test error message",
        expect.objectContaining({
          executionId: "test-workflow-execution",
          workflowId: "test-workflow",
          nodeId: "test-node",
        }),
      );

      // Verify event triggering
      expect(mockEmit).toHaveBeenCalledWith(
        mockEventManager,
        expect.objectContaining({
          type: "ERROR",
          executionId: "test-workflow-execution",
          workflowId: "test-workflow",
          nodeId: "test-node",
        }),
      );
    });

    it("Warnings should be handled correctly.", async () => {
      const warningError = new SDKError("Warning message", "warning", {});

      await handleError(mockEventManager, warningError, {
        executionId: "test-workflow-execution",
        workflowId: "test-workflow",
      });

      expect(mockLogger.warn).toHaveBeenCalledWith("Warning message", expect.any(Object));
    });

    it("The info-level errors should be handled correctly.", async () => {
      const infoError = new SDKError("Info message", "info", {});

      await handleError(mockEventManager, infoError, {
        executionId: "test-workflow-execution",
        workflowId: "test-workflow",
      });

      expect(mockLogger.info).toHaveBeenCalledWith("Info message", expect.any(Object));
    });

    it("It should be able to handle the optional nodeId parameter.", async () => {
      await handleError(mockEventManager, mockSDKError, {
        executionId: "test-workflow-execution",
        workflowId: "test-workflow",
      });

      expect(mockLogger.error).toHaveBeenCalled();
      expect(mockEmit).toHaveBeenCalled();
    });

    it("It should be able to handle the undefined eventManager.", async () => {
      await handleError(undefined, mockSDKError, {
        executionId: "test-workflow-execution",
        workflowId: "test-workflow",
      });

      expect(mockLogger.error).toHaveBeenCalled();
      // handleError -> emitErrorEvent returns early when eventManager is undefined
      expect(mockEmit).not.toHaveBeenCalled();
    });
  });

  describe("Boundary cases", () => {
    it("The `context` field that may contain empty strings should be handled accordingly.", () => {
      const error = new SDKError("Test error", "error", {});
      const context = { executionId: "", workflowId: "", nodeId: "" };

      logError(error, context);

      expect(mockLogger.error).toHaveBeenCalledWith(
        "Test error",
        expect.objectContaining({
          executionId: "",
          workflowId: "",
          nodeId: "",
        }),
      );
    });

    it("The complex `context` object should be processed.", () => {
      const error = new SDKError("Test error", "error", {});
      const context = {
        executionId: "test-workflow-execution",
        nested: {
          field1: "value1",
          field2: "value2",
        },
        array: [1, 2, 3],
      };

      logError(error, context);

      expect(mockLogger.error).toHaveBeenCalledWith(
        "Test error",
        expect.objectContaining({
          executionId: "test-workflow-execution",
          nested: expect.any(Object),
          array: expect.any(Array),
        }),
      );
    });

    it("The Error object should be processed as the cause.", () => {
      const cause = new Error("Cause error");
      const error = new SDKError("Test error", "error", {}, cause);

      logError(error, { executionId: "test-workflow-execution" });

      expect(mockLogger.error).toHaveBeenCalledWith("Test error", expect.any(Object));
    });
  });
});
