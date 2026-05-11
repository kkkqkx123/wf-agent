/**
 * Tests for AgentLoopCheckpointConfigResolver
 */

import { describe, it, expect, beforeEach } from "vitest";
import { AgentLoopCheckpointConfigResolver } from "../utils/config-resolver.js";
import type {
  AgentLoopCheckpointConfigLayer,
  AgentLoopCheckpointConfigContext,
  AgentLoopCheckpointTriggerType,
} from "@wf-agent/types";

describe("AgentLoopCheckpointConfigResolver", () => {
  let resolver: AgentLoopCheckpointConfigResolver;

  beforeEach(() => {
    resolver = new AgentLoopCheckpointConfigResolver();
  });

  const createContext = (
    overrides?: Partial<AgentLoopCheckpointConfigContext>,
  ): AgentLoopCheckpointConfigContext => ({
    triggerType: "ITERATION_END" as AgentLoopCheckpointTriggerType,
    currentIteration: 1,
    hasError: false,
    ...overrides,
  });

  const createLayer = (
    source: AgentLoopCheckpointConfigLayer["source"],
    config: AgentLoopCheckpointConfigLayer["config"],
  ): AgentLoopCheckpointConfigLayer => ({
    source,
    config,
  });

  describe("resolveAgentConfig", () => {
    it("should return default configuration when no layers provided", () => {
      const layers: AgentLoopCheckpointConfigLayer[] = [];
      const context = createContext();

      const result = resolver.resolveAgentConfig(layers, context);

      expect(result.shouldCreate).toBe(true);
      expect(result.effectiveSource).toBe("default");
      expect(result.triggerType).toBe("ITERATION_END");
    });

    it("should disable checkpoint when enabled is false", () => {
      const layers = [createLayer("runtime", { enabled: false })];
      const context = createContext();

      const result = resolver.resolveAgentConfig(layers, context);

      expect(result.shouldCreate).toBe(false);
      expect(result.effectiveSource).toBe("runtime");
    });

    it("should create checkpoint only on error when onErrorOnly is true", () => {
      const layers = [createLayer("agent", { onErrorOnly: true })];
      const context = createContext({ hasError: false });

      const result = resolver.resolveAgentConfig(layers, context);

      expect(result.shouldCreate).toBe(false);
    });

    it("should create checkpoint on error when onErrorOnly is true and hasError is true", () => {
      const layers = [createLayer("agent", { onErrorOnly: true })];
      const context = createContext({ hasError: true });

      const result = resolver.resolveAgentConfig(layers, context);

      expect(result.shouldCreate).toBe(true);
    });

    it("should respect interval configuration", () => {
      const layers = [createLayer("global", { interval: 5 })];
      const context = createContext({ currentIteration: 10 });

      const result = resolver.resolveAgentConfig(layers, context);

      expect(result.shouldCreate).toBe(true);
    });

    it("should not create checkpoint when iteration is not multiple of interval", () => {
      const layers = [createLayer("global", { interval: 5 })];
      const context = createContext({ currentIteration: 7 });

      const result = resolver.resolveAgentConfig(layers, context);

      expect(result.shouldCreate).toBe(false);
    });

    it("should prioritize runtime config over agent config", () => {
      const layers = [
        createLayer("runtime", { enabled: false }),
        createLayer("agent", { enabled: true }),
      ];
      const context = createContext();

      const result = resolver.resolveAgentConfig(layers, context);

      // Runtime (first layer) should win
      expect(result.shouldCreate).toBe(false);
      expect(result.effectiveSource).toBe("runtime");
    });

    it("should merge configurations from multiple layers", () => {
      const layers = [
        createLayer("runtime", { enabled: true }),
        createLayer("agent", { interval: 3, onErrorOnly: true }),
        createLayer("global", { deltaStorage: { enabled: true } }),
      ];
      const context = createContext({ hasError: true, currentIteration: 6 });

      const result = resolver.resolveAgentConfig(layers, context);

      expect(result.shouldCreate).toBe(true);
    });

    it("should return error description when trigger type is ERROR", () => {
      const layers = [createLayer("runtime", { enabled: true })];
      const context = createContext({ triggerType: "ERROR" as AgentLoopCheckpointTriggerType });

      const result = resolver.resolveAgentConfig(layers, context);

      expect(result.description).toBe("Error checkpoint");
    });

    it("should return iteration description for ITERATION_END trigger", () => {
      const layers = [createLayer("runtime", { enabled: true })];
      const context = createContext({ currentIteration: 5 });

      const result = resolver.resolveAgentConfig(layers, context);

      expect(result.description).toContain("Iteration 5");
    });

    it("should handle interval of 1 (create every iteration)", () => {
      const layers = [createLayer("global", { interval: 1 })];
      const context = createContext({ currentIteration: 1 });

      const result = resolver.resolveAgentConfig(layers, context);

      expect(result.shouldCreate).toBe(true);
    });

    it("should find effective source as first layer with enabled field", () => {
      const layers = [
        createLayer("runtime", { interval: 5 }),
        createLayer("agent", { enabled: true }),
        createLayer("global", { enabled: false }),
      ];
      const context = createContext();

      const result = resolver.resolveAgentConfig(layers, context);

      // agent is the first layer with enabled field
      expect(result.effectiveSource).toBe("agent");
    });
  });

  describe("mergeConfigs", () => {
    it("should merge enabled field from layers", () => {
      const layers = [
        createLayer("runtime", { enabled: false }),
        createLayer("agent", { enabled: true }),
      ];

      const merged = (resolver as any).mergeConfigs(layers);

      expect(merged.enabled).toBe(false); // First layer wins
    });

    it("should merge interval field from layers", () => {
      const layers = [
        createLayer("runtime", { interval: 10 }),
        createLayer("agent", { interval: 5 }),
      ];

      const merged = (resolver as any).mergeConfigs(layers);

      expect(merged.interval).toBe(10); // First layer wins
    });

    it("should merge onErrorOnly field from layers", () => {
      const layers = [
        createLayer("runtime", { onErrorOnly: true }),
        createLayer("agent", { onErrorOnly: false }),
      ];

      const merged = (resolver as any).mergeConfigs(layers);

      expect(merged.onErrorOnly).toBe(true); // First layer wins
    });

    it("should merge deltaStorage field from layers", () => {
      const layers = [
        createLayer("runtime", { deltaStorage: { enabled: true } }),
        createLayer("agent", { deltaStorage: { enabled: false } }),
      ];

      const merged = (resolver as any).mergeConfigs(layers);

      expect(merged.deltaStorage?.enabled).toBe(true); // First layer wins
    });
  });

  describe("evaluateTrigger", () => {
    it("should return false when enabled is false", () => {
      const config = { enabled: false };
      const context = createContext();

      const result = (resolver as any).evaluateTrigger(config, context);

      expect(result).toBe(false);
    });

    it("should return false when onErrorOnly is true and hasError is false", () => {
      const config = { onErrorOnly: true };
      const context = createContext({ hasError: false });

      const result = (resolver as any).evaluateTrigger(config, context);

      expect(result).toBe(false);
    });

    it("should return true when onErrorOnly is true and hasError is true", () => {
      const config = { onErrorOnly: true };
      const context = createContext({ hasError: true });

      const result = (resolver as any).evaluateTrigger(config, context);

      expect(result).toBe(true);
    });

    it("should check interval correctly", () => {
      const config = { interval: 3 };
      const context = createContext({ currentIteration: 6 });

      const result = (resolver as any).evaluateTrigger(config, context);

      expect(result).toBe(true);
    });

    it("should return false when iteration is not divisible by interval", () => {
      const config = { interval: 3 };
      const context = createContext({ currentIteration: 7 });

      const result = (resolver as any).evaluateTrigger(config, context);

      expect(result).toBe(false);
    });
  });

  describe("findEffectiveSource", () => {
    it("should return first source with enabled field", () => {
      const layers = [
        createLayer("runtime", { interval: 5 }),
        createLayer("agent", { enabled: true }),
        createLayer("global", { enabled: false }),
      ];

      const source = (resolver as any).findEffectiveSource(layers);

      expect(source).toBe("agent");
    });

    it("should return default when no layer has enabled field", () => {
      const layers = [
        createLayer("runtime", { interval: 5 }),
        createLayer("agent", { onErrorOnly: true }),
      ];

      const source = (resolver as any).findEffectiveSource(layers);

      expect(source).toBe("default");
    });
  });

  describe("buildDescription", () => {
    it("should return error description for ERROR trigger", () => {
      const context = createContext({ triggerType: "ERROR" as AgentLoopCheckpointTriggerType });

      const description = (resolver as any).buildDescription(context);

      expect(description).toBe("Error checkpoint");
    });

    it("should return iteration description for ITERATION_END trigger", () => {
      const context = createContext({ currentIteration: 10 });

      const description = (resolver as any).buildDescription(context);

      expect(description).toBe("Iteration 10 checkpoint");
    });
  });
});
