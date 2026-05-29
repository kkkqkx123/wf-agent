/**
 * Unit tests for Formatter Registry
 */
import { describe, it, expect, beforeEach } from "vitest";
import {
  FormatterRegistry,
  formatterRegistry,
  getFormatter,
  registerFormatter,
} from "../registry.js";
import { BaseFormatter } from "../base.js";
import type { LLMRequest } from "@wf-agent/types";
import type { FormatterConfig, BuildRequestResult, ParseStreamChunkResult } from "../types.js";

class MockFormatter extends BaseFormatter {
  getSupportedProvider(): string {
    return "MOCK";
  }

  buildNativeRequest(_request: LLMRequest, _config: FormatterConfig): BuildRequestResult {
    return {
      httpRequest: {
        url: "/mock",
        method: "POST",
        headers: {},
        body: {},
      },
    };
  }

  parseNativeResponse(data: unknown, _config: FormatterConfig) {
    return {
      id: "mock-id",
      model: "mock-model",
      content: (data as any)?.content || "",
      message: { role: "assistant" as const, content: (data as any)?.content || "" },
      finishReason: "stop",
      duration: 0,
    };
  }

  parseStreamChunk(_data: unknown): ParseStreamChunkResult {
    return {
      chunk: { delta: "", done: true },
      valid: true,
    };
  }

  convertTools(_tools: any[]): unknown[] {
    return [];
  }

  convertMessages(_messages: any[]): unknown[] {
    return [];
  }

  parseToolCalls(_toolCalls: unknown): any[] {
    return [];
  }
}

describe("FormatterRegistry", () => {
  let registry: FormatterRegistry;

  beforeEach(() => {
    // Reset to defaults before each test
    formatterRegistry.reset();
  });

  describe("Singleton", () => {
    it("should return the same instance via getInstance()", () => {
      const instance1 = FormatterRegistry.getInstance();
      const instance2 = FormatterRegistry.getInstance();
      expect(instance1).toBe(instance2);
    });
  });

  describe("register", () => {
    it("should register a new formatter", () => {
      registry = FormatterRegistry.getInstance();
      const formatter = new MockFormatter();
      registry.register(formatter);
      expect(registry.has("MOCK")).toBe(true);
    });

    it("should not throw when registering a duplicate formatter (only warns)", () => {
      registry = FormatterRegistry.getInstance();
      const formatter = new MockFormatter();
      registry.register(formatter);
      expect(() => registry.register(formatter)).not.toThrow();
    });

    it("should overwrite existing formatter when registering duplicate", () => {
      registry = FormatterRegistry.getInstance();
      const formatter1 = new MockFormatter();
      const formatter2 = new MockFormatter();
      registry.register(formatter1);
      registry.register(formatter2); // silently overwrites
      expect(registry.has("MOCK")).toBe(true);
    });
  });

  describe("get", () => {
    it("should return a registered formatter", () => {
      registry = FormatterRegistry.getInstance();
      const formatter = registry.get("OPENAI_CHAT");
      expect(formatter).toBeDefined();
      expect(formatter!.getSupportedProvider()).toBe("OPENAI_CHAT");
    });

    it("should return undefined for unregistered provider", () => {
      registry = FormatterRegistry.getInstance();
      const formatter = registry.get("NONEXISTENT");
      expect(formatter).toBeUndefined();
    });
  });

  describe("has", () => {
    it("should return true for registered provider", () => {
      registry = FormatterRegistry.getInstance();
      expect(registry.has("OPENAI_CHAT")).toBe(true);
    });

    it("should return false for unregistered provider", () => {
      registry = FormatterRegistry.getInstance();
      expect(registry.has("UNKNOWN")).toBe(false);
    });
  });

  describe("getRegisteredProviders", () => {
    it("should return all registered provider names", () => {
      registry = FormatterRegistry.getInstance();
      const providers = registry.getRegisteredProviders();
      expect(providers).toContain("OPENAI_CHAT");
      expect(providers).toContain("OPENAI_RESPONSE");
      expect(providers).toContain("ANTHROPIC");
      expect(providers).toContain("GEMINI_NATIVE");
      expect(providers).toContain("GEMINI_OPENAI");
    });
  });

  describe("unregister", () => {
    it("should unregister a formatter", () => {
      registry = FormatterRegistry.getInstance();
      const result = registry.unregister("OPENAI_CHAT");
      expect(result).toBe(true);
      expect(registry.has("OPENAI_CHAT")).toBe(false);
    });

    it("should return false for unregistered provider", () => {
      registry = FormatterRegistry.getInstance();
      const result = registry.unregister("NONEXISTENT");
      expect(result).toBe(false);
    });
  });

  describe("clear", () => {
    it("should clear all registrations", () => {
      registry = FormatterRegistry.getInstance();
      registry.clear();
      expect(registry.getRegisteredProviders()).toHaveLength(0);
    });
  });

  describe("reset", () => {
    it("should restore default registrations", () => {
      registry = FormatterRegistry.getInstance();
      registry.clear();
      registry.reset();
      expect(registry.getRegisteredProviders().length).toBeGreaterThanOrEqual(5);
    });
  });

  describe("getFormatter (function)", () => {
    it("should get formatter for a known provider", () => {
      const formatter = getFormatter("OPENAI_CHAT");
      expect(formatter).toBeDefined();
      expect(formatter.getSupportedProvider()).toBe("OPENAI_CHAT");
    });

    it("should throw for unknown provider", () => {
      expect(() => getFormatter("UNKNOWN")).toThrow("No formatter registered for provider: UNKNOWN");
    });
  });

  describe("registerFormatter (function)", () => {
    it("should register a custom formatter", () => {
      const formatter = new MockFormatter();
      registerFormatter(formatter);
      expect(formatterRegistry.has("MOCK")).toBe(true);
    });
  });
});
