/**
 * Pause Timeout Manager Unit Tests
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { PauseTimeoutManager } from "../pause-timeout-manager.js";
import type { WorkflowExecutionRegistry } from "../../stores/workflow-execution-registry.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";

// Mock the contextual logger
vi.mock("../../../utils/contextual-logger.js", () => ({
  createContextualLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

// Mock the event builders
vi.mock("../../../core/utils/event/builders/workflow-execution-events.js", () => ({
  buildWorkflowExecutionCancelledEvent: vi.fn((entity, reason, details) => ({
    id: "event-1",
    type: "WORKFLOW_EXECUTION_CANCELLED",
    timestamp: Date.now(),
    executionId: entity.id,
    workflowId: entity.getWorkflowId(),
    reason,
    details,
  })),
}));

// Mock the emit function
vi.mock("../../../core/utils/event/emit-event.js", () => ({
  emit: vi.fn(),
}));

// Mock the TimeoutRegistry to avoid resource monitoring issues
vi.mock("../../../core/registry/timeout-registry.js", () => ({
  TimeoutRegistry: vi.fn().mockImplementation(() => ({
    getManager: vi.fn().mockReturnValue({
      register: vi.fn().mockReturnValue({
        cancel: vi.fn(),
      }),
    }),
    cleanup: vi.fn(),
    cleanupAll: vi.fn(),
  })),
}));

describe("PauseTimeoutManager", () => {
  let mockRegistry: WorkflowExecutionRegistry;
  let mockEventManager: EventRegistry;
  let manager: PauseTimeoutManager;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();

    mockRegistry = {
      get: vi.fn().mockReturnValue({
        getInterruptionState: vi.fn().mockReturnValue(null),
      }),
    } as unknown as WorkflowExecutionRegistry;

    mockEventManager = {
      emit: vi.fn(),
      waitFor: vi.fn(),
    } as unknown as EventRegistry;

    manager = new PauseTimeoutManager(mockRegistry, mockEventManager);
  });

  afterEach(() => {
    manager.cleanup();
    vi.useRealTimers();
  });

  describe("startMonitoring", () => {
    it("should start monitoring a paused workflow", () => {
      manager.startMonitoring("exec-1");

      expect(manager.isMonitoring("exec-1")).toBe(true);
    });

    it("should clear existing monitoring before starting new one", () => {
      manager.startMonitoring("exec-1");
      manager.startMonitoring("exec-1");

      // Should still be monitoring (only one entry)
      expect(manager.isMonitoring("exec-1")).toBe(true);
    });

    it("should record pausedAt timestamp", () => {
      const now = Date.now();
      vi.setSystemTime(now);

      manager.startMonitoring("exec-1");

      expect(manager.getPauseDuration("exec-1")).toBe(0);

      vi.advanceTimersByTime(1000);
      expect(manager.getPauseDuration("exec-1")).toBe(1000);
    });
  });

  describe("stopMonitoring", () => {
    it("should stop monitoring a workflow", () => {
      manager.startMonitoring("exec-1");
      manager.stopMonitoring("exec-1");

      expect(manager.isMonitoring("exec-1")).toBe(false);
    });

    it("should handle stopping non-monitored workflow", () => {
      manager.stopMonitoring("exec-unknown");

      expect(manager.isMonitoring("exec-unknown")).toBe(false);
    });

    it("should clear pause duration", () => {
      manager.startMonitoring("exec-1");
      vi.advanceTimersByTime(5000);
      manager.stopMonitoring("exec-1");

      expect(manager.getPauseDuration("exec-1")).toBeNull();
    });
  });

  describe("isMonitoring", () => {
    it("should return true for monitored workflow", () => {
      manager.startMonitoring("exec-1");

      expect(manager.isMonitoring("exec-1")).toBe(true);
    });

    it("should return false for non-monitored workflow", () => {
      expect(manager.isMonitoring("exec-unknown")).toBe(false);
    });

    it("should return false after stopping monitoring", () => {
      manager.startMonitoring("exec-1");
      manager.stopMonitoring("exec-1");

      expect(manager.isMonitoring("exec-1")).toBe(false);
    });
  });

  describe("getPauseDuration", () => {
    it("should return null for non-monitored workflow", () => {
      expect(manager.getPauseDuration("exec-unknown")).toBeNull();
    });

    it("should return correct duration for monitored workflow", () => {
      const now = Date.now();
      vi.setSystemTime(now);

      manager.startMonitoring("exec-1");

      vi.advanceTimersByTime(10000);
      expect(manager.getPauseDuration("exec-1")).toBe(10000);

      vi.advanceTimersByTime(5000);
      expect(manager.getPauseDuration("exec-1")).toBe(15000);
    });
  });

  describe("cleanup", () => {
    it("should clean up all monitoring", () => {
      manager.startMonitoring("exec-1");
      manager.startMonitoring("exec-2");
      manager.startMonitoring("exec-3");

      manager.cleanup();

      expect(manager.isMonitoring("exec-1")).toBe(false);
      expect(manager.isMonitoring("exec-2")).toBe(false);
      expect(manager.isMonitoring("exec-3")).toBe(false);
    });

    it("should handle cleanup when no monitoring", () => {
      manager.cleanup();

      expect(manager.isMonitoring("exec-1")).toBe(false);
    });
  });

  describe("custom configuration", () => {
    it("should use custom maxPauseDuration", () => {
      const customManager = new PauseTimeoutManager(
        mockRegistry,
        mockEventManager,
        { maxPauseDuration: 60000, warningThreshold: 30000 }
      );

      customManager.startMonitoring("exec-1");

      expect(customManager.isMonitoring("exec-1")).toBe(true);
    });

    it("should use default config when not provided", () => {
      const defaultManager = new PauseTimeoutManager(mockRegistry, mockEventManager);

      defaultManager.startMonitoring("exec-1");

      expect(defaultManager.isMonitoring("exec-1")).toBe(true);
    });
  });
});