/**
 * Agent Loop Error Handling Integration Tests
 */

import { describe, it, expect, beforeEach } from "vitest";
import { AgentLoopEntity } from "../entities/agent-loop-entity.js";
import {
  handleAgentError,
  handleAgentInterruption,
  isRecoverableAgentError,
  createAgentExecutionError,
} from "../execution/handlers/agent-error-handler.js";
import { EventRegistry } from "../../core/registry/event-registry.js";
import { SDKError, AbortError } from "@wf-agent/types";
import type { AgentLoopRuntimeConfig } from "@wf-agent/types";

describe("Agent Loop Error Handling", () => {
  let entity: AgentLoopEntity;
  let eventManager: EventRegistry;
  const config: AgentLoopRuntimeConfig = {
    maxIterations: 10,
    profileId: "test-profile",
  };

  beforeEach(() => {
    entity = new AgentLoopEntity("test-id", config);
    eventManager = new EventRegistry();
  });

  describe("Error Standardization", () => {
    it("should convert error to SDKError", async () => {
      entity.state.start();
      const error = new Error("Test error");

      const sdkError = await handleAgentError(
        entity,
        error,
        "test_operation",
        undefined,
        eventManager,
      );

      expect(sdkError).toBeInstanceOf(SDKError);
      expect(sdkError.message).toBe("Test error");
    });

    it("should keep SDKError unchanged", async () => {
      entity.state.start();
      const sdkError = new SDKError("Test", "warning", {});

      const result = await handleAgentError(
        entity,
        sdkError,
        "test_operation",
        undefined,
        eventManager,
      );

      expect(result).toBe(sdkError);
    });

    it("should build error context", async () => {
      entity.state.start();
      entity.state.startIteration();

      const error = new Error("Test error");
      const sdkError = await handleAgentError(
        entity,
        error,
        "test_operation",
        { additionalInfo: "test" },
        eventManager,
      );

      expect(sdkError.context).toBeDefined();
      expect(sdkError.context!["operation"]).toBe("test_operation");
    });
  });

  describe("Error Severity Handling", () => {
    it("should stop execution for ERROR severity", async () => {
      entity.state.start();
      const error = new Error("Critical error");

      await handleAgentError(entity, error, "test", undefined, eventManager);

      expect(entity.state.status).toBe("FAILED");
    });

    it("should continue execution for WARNING severity", async () => {
      entity.state.start();
      const error = new SDKError("Warning", "warning", {});

      await handleAgentError(entity, error, "test", undefined, eventManager);

      expect(entity.state.status).toBe("RUNNING");
    });

    it("should continue execution for INFO severity", async () => {
      entity.state.start();
      const error = new SDKError("Info", "info", {});

      await handleAgentError(entity, error, "test", undefined, eventManager);

      expect(entity.state.status).toBe("RUNNING");
    });
  });

  describe("Recoverable Error", () => {
    it("should identify recoverable error", () => {
      const warning = new SDKError("Warning", "warning", {});
      const info = new SDKError("Info", "info", {});
      const error = new SDKError("Error", "error", {});

      expect(isRecoverableAgentError(warning)).toBe(true);
      expect(isRecoverableAgentError(info)).toBe(true);
      expect(isRecoverableAgentError(error)).toBe(false);
    });
  });

  describe("Interruption Error Handling", () => {
    it("should handle abort error", async () => {
      entity.state.start();
      const controller = new AbortController();
      controller.abort();

      const error = new AbortError("Aborted");

      const isInterruption = await handleAgentInterruption(entity, error, "test", eventManager);

      expect(isInterruption).toBe(true);
    });

    it("should return false for non-abort error", async () => {
      entity.state.start();
      const error = new Error("Regular error");

      const isInterruption = await handleAgentInterruption(entity, error, "test", eventManager);

      expect(isInterruption).toBe(false);
    });
  });

  describe("Create Agent Execution Error", () => {
    it("should create SDKError with context", () => {
      entity.state.start();

      const error = createAgentExecutionError(
        entity,
        "Test error",
        "test_operation",
        new Error("Cause"),
        "error",
      );

      expect(error).toBeInstanceOf(SDKError);
      expect(error.message).toBe("Test error");
      expect(error.severity).toBe("error");
    });
  });
});
