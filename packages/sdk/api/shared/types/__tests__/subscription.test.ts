/**
 * Tests for subscription factories and classes
 */

import { describe, it, expect, vi } from "vitest";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import {
  createOnEventSubscription,
  createOnceEventSubscription,
  createExecutionScopedSubscription,
  createExecutionScopedOnceSubscription,
  OnEventSubscription,
  OnceEventSubscription,
} from "../subscription.js";
import type { EventRegistry } from "../../../../shared/registry/event-registry.js";
import type { BaseEvent } from "@wf-agent/types";

describe("Subscription Factories", () => {
  // Mock EventRegistry
  const createMockEventRegistry = (): EventRegistry => {
    const mockEmitter = {
      on: vi.fn(() => () => {}),
      once: vi.fn(() => () => {}),
    };

    return {
      getEmitter: vi.fn(() => mockEmitter),
    } as any;
  };

  // Mock APIDependencyManager
  const createMockDependencies = (eventRegistry: EventRegistry): APIDependencyManager => {
    return {
      getEventManager: () => eventRegistry,
    } as any;
  };

  describe("createOnEventSubscription", () => {
    it("should create an OnEventSubscription instance", () => {
      const eventRegistry = createMockEventRegistry();
      const dependencies = createMockDependencies(eventRegistry);

      const listener = vi.fn();
      const subscription = createOnEventSubscription(
        "TEST_EVENT",
        listener,
        dependencies,
        { executionId: "exec-123" },
      );

      expect(subscription).toBeInstanceOf(OnEventSubscription);
    });

    it("should accept additional options", () => {
      const eventRegistry = createMockEventRegistry();
      const dependencies = createMockDependencies(eventRegistry);

      const listener = vi.fn();
      const filter = (event: BaseEvent) => true;
      const subscription = createOnEventSubscription(
        "TEST_EVENT",
        listener,
        dependencies,
        {
          executionId: "exec-123",
          priority: 10,
          timeout: 5000,
          filter,
        },
      );

      expect(subscription).toBeInstanceOf(OnEventSubscription);
    });
  });

  describe("createOnceEventSubscription", () => {
    it("should create an OnceEventSubscription instance", () => {
      const eventRegistry = createMockEventRegistry();
      const dependencies = createMockDependencies(eventRegistry);

      const listener = vi.fn();
      const subscription = createOnceEventSubscription(
        "TEST_EVENT",
        listener,
        dependencies,
        { executionId: "exec-123" },
      );

      expect(subscription).toBeInstanceOf(OnceEventSubscription);
    });
  });

  describe("createExecutionScopedSubscription", () => {
    it("should automatically inject executionId", () => {
      const eventRegistry = createMockEventRegistry();
      const dependencies = createMockDependencies(eventRegistry);

      const listener = vi.fn();
      const subscription = createExecutionScopedSubscription(
        "exec-123",
        "TEST_EVENT",
        listener,
        dependencies,
      );

      expect(subscription).toBeInstanceOf(OnEventSubscription);
    });

    it("should accept additional options", () => {
      const eventRegistry = createMockEventRegistry();
      const dependencies = createMockDependencies(eventRegistry);

      const listener = vi.fn();
      const filter = (event: BaseEvent) => true;
      const subscription = createExecutionScopedSubscription(
        "exec-123",
        "TEST_EVENT",
        listener,
        dependencies,
        {
          priority: 10,
          timeout: 5000,
          filter,
        },
      );

      expect(subscription).toBeInstanceOf(OnEventSubscription);
    });
  });

  describe("createExecutionScopedOnceSubscription", () => {
    it("should automatically inject executionId for once subscription", () => {
      const eventRegistry = createMockEventRegistry();
      const dependencies = createMockDependencies(eventRegistry);

      const listener = vi.fn();
      const subscription = createExecutionScopedOnceSubscription(
        "exec-123",
        "TEST_EVENT",
        listener,
        dependencies,
      );

      expect(subscription).toBeInstanceOf(OnceEventSubscription);
    });
  });

  describe("Subscription usage pattern", () => {
    it("should support chained subscribe() call", () => {
      const eventRegistry = createMockEventRegistry();
      const dependencies = createMockDependencies(eventRegistry);

      const listener = vi.fn();
      const unsubscribe = createExecutionScopedSubscription(
        "exec-123",
        "TEST_EVENT",
        listener,
        dependencies,
      ).subscribe();

      expect(typeof unsubscribe).toBe("function");
    });
  });
});
