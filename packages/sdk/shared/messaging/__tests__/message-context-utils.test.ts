/**
 * Message Context Utils Tests
 */
import { describe, it, expect, beforeEach } from "vitest";
import { InMemoryMessageContextRegistry } from "../message-context-registry.js";
import { initializeExecutionContext, getOrCreateContext } from "../message-context-utils.js";
import type { WorkflowConfig } from "@wf-agent/types";

describe("message-context-utils", () => {
  let registry: InMemoryMessageContextRegistry;

  beforeEach(() => {
    registry = new InMemoryMessageContextRegistry();
    registry.clear();
  });

  describe("initializeExecutionContext", () => {
    it('should create the "current" context', () => {
      initializeExecutionContext(registry);

      expect(registry.has("current")).toBe(true);
      const ctx = registry.get("current");
      expect(ctx?.id).toBe("current");
      expect(ctx?.messages).toEqual([]);
    });

    it('should pre-populate initial messages into the "current" context', () => {
      const workflowConfig: WorkflowConfig = {
        initialMessages: [
          { role: "system", content: "System prompt" },
          { role: "user", content: "Hello" },
        ],
      };

      initializeExecutionContext(registry, workflowConfig);

      const ctx = registry.get("current");
      expect(ctx?.messages).toHaveLength(2);
      expect(ctx?.messages[0]?.role).toBe("system");
      expect(ctx?.messages[1]?.content).toBe("Hello");
    });

    it("should handle empty workflowConfig", () => {
      initializeExecutionContext(registry, {} as WorkflowConfig);

      expect(registry.has("current")).toBe(true);
    });

    it("should register static contexts if defined", () => {
      const workflowConfig: WorkflowConfig = {
        staticContexts: [
          {
            id: "memory",
            messages: [{ role: "system", content: "Memory context" }],
          },
        ],
      };

      initializeExecutionContext(registry, workflowConfig);

      expect(registry.has("memory")).toBe(true);
      const ctx = registry.get("memory");
      expect(ctx?.messages[0]?.content).toBe("Memory context");
    });

    it("should handle static contexts without messages", () => {
      const workflowConfig: WorkflowConfig = {
        staticContexts: [{ id: "empty-ctx" } as any],
      };

      initializeExecutionContext(registry, workflowConfig);

      expect(registry.has("empty-ctx")).toBe(true);
    });

    it("should set metadata description for current context", () => {
      initializeExecutionContext(registry);

      const ctx = registry.get("current");
      expect(ctx?.metadata?.description).toBe("Main conversation context");
    });
  });

  describe("getOrCreateContext", () => {
    it("should return existing context", () => {
      const ctx = {
        id: "existing",
        messages: [{ role: "user", content: "Hello" }],
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };
      registry.register(ctx as any);

      const result = getOrCreateContext(registry, "existing");
      expect(result.id).toBe("existing");
      expect(result.messages).toHaveLength(1);
    });

    it("should create and register new context if not found", () => {
      const result = getOrCreateContext(registry, "new-context");

      expect(result.id).toBe("new-context");
      expect(result.messages).toEqual([]);
      expect(registry.has("new-context")).toBe(true);
    });

    it("should set createdAt and updatedAt on new context", () => {
      const result = getOrCreateContext(registry, "timed-context");

      expect(result.createdAt).toBeDefined();
      expect(typeof result.createdAt).toBe("number");
      expect(result.updatedAt).toBeDefined();
      expect(typeof result.updatedAt).toBe("number");
    });

    it("should return the same context on subsequent calls", () => {
      const first = getOrCreateContext(registry, "shared");
      const second = getOrCreateContext(registry, "shared");

      expect(first.id).toBe(second.id);
      expect(first.messages).toBe(second.messages);
      expect(registry.size()).toBe(1);
    });

    it("should preserve existing messages when context already exists", () => {
      const ctx = {
        id: "has-messages",
        messages: [{ role: "user", content: "Existing message" }],
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };
      registry.register(ctx as any);

      const result = getOrCreateContext(registry, "has-messages");
      expect(result.messages).toHaveLength(1);
      expect(result.messages[0]!.content).toBe("Existing message");
    });
  });

  describe("Integration: initializeExecutionContext + getOrCreateContext", () => {
    it("should work together seamlessly", () => {
      initializeExecutionContext(registry);

      // Should be able to getOrCreateContext on already initialized context
      const current = getOrCreateContext(registry, "current");
      expect(current.id).toBe("current");

      // Should create a new one
      const custom = getOrCreateContext(registry, "custom-context");
      expect(custom.id).toBe("custom-context");
      expect(registry.size()).toBe(2);
    });
  });
});
