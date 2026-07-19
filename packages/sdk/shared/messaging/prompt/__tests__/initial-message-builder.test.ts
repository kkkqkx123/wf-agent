import { describe, it, expect, vi, beforeEach } from "vitest";

// ── Hoisted Mocks ───────────────────────────────────────────────────────────
const { mockTemplateRegistry, mockRenderTemplate, mockLogger, mockResolveSystemPrompt } =
  vi.hoisted(() => ({
    mockTemplateRegistry: {
      render: vi.fn(),
    },
    mockRenderTemplate: vi.fn(),
    mockLogger: {
      warn: vi.fn(),
      info: vi.fn(),
      error: vi.fn(),
      debug: vi.fn(),
    },
    mockResolveSystemPrompt: vi.fn(),
  }));

// ── Module Mocks ────────────────────────────────────────────────────────────
vi.mock("../../../../resources/predefined/template-registry.js", () => ({
  templateRegistry: mockTemplateRegistry,
}));

vi.mock("../../../../utils/logger.js", () => ({
  sdkLogger: mockLogger,
}));

vi.mock("../../../utils/template-renderer/index.js", () => ({
  renderTemplate: mockRenderTemplate,
}));

vi.mock("../system-prompt-resolver.js", () => ({
  resolveSystemPrompt: mockResolveSystemPrompt,
}));

// ── Import after mocks ──────────────────────────────────────────────────────
import { buildInitialMessages } from "../initial-message-builder.js";
import type { LLMMessage } from "@wf-agent/types";

describe("buildInitialMessages", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("when initialMessages is provided", () => {
    it("should return a copy of initialMessages directly", () => {
      const initialMessages: LLMMessage[] = [
        { role: "user", content: "Hello" },
        { role: "assistant", content: "Hi" },
      ];

      const result = buildInitialMessages({ initialMessages });

      expect(result).toEqual(initialMessages);
      expect(result).not.toBe(initialMessages); // should be a copy
      expect(mockResolveSystemPrompt).not.toHaveBeenCalled();
    });

    it("should return empty array when initialMessages is empty", () => {
      const result = buildInitialMessages({ initialMessages: [] });
      expect(result).toEqual([]);
    });
  });

  describe("system prompt resolution", () => {
    it("should include system message when resolveSystemPrompt returns a string", () => {
      mockResolveSystemPrompt.mockReturnValue("You are a helpful assistant.");

      const result = buildInitialMessages({});

      expect(result).toHaveLength(1);
      expect(result[0]).toEqual({
        role: "system",
        content: "You are a helpful assistant.",
      });
    });

    it("should not include system message when resolveSystemPrompt returns empty string", () => {
      mockResolveSystemPrompt.mockReturnValue("");

      const result = buildInitialMessages({});

      expect(result).toHaveLength(0);
    });
  });

  describe("initial user message resolution", () => {
    it("should include user message from initialUserMessage", () => {
      mockResolveSystemPrompt.mockReturnValue("");

      const result = buildInitialMessages({ initialUserMessage: "Hello!" });

      expect(result).toHaveLength(1);
      expect(result[0]).toEqual({
        role: "user",
        content: "Hello!",
      });
    });

    it("should render user message from template when initialUserMessageTemplateId is provided", () => {
      mockResolveSystemPrompt.mockReturnValue("");
      mockTemplateRegistry.render.mockReturnValue("Rendered template message");

      const result = buildInitialMessages({
        initialUserMessageTemplateId: "greeting-template",
        initialUserMessageTemplateVariables: { name: "Alice" },
      }, mockTemplateRegistry as any);

      expect(result).toHaveLength(1);
      expect(result[0]).toEqual({
        role: "user",
        content: "Rendered template message",
      });
      expect(mockTemplateRegistry.render).toHaveBeenCalledWith("greeting-template", {
        name: "Alice",
      });
    });

    it("should fall back to initialUserMessage when template is not found", () => {
      mockResolveSystemPrompt.mockReturnValue("");
      mockTemplateRegistry.render.mockReturnValue(null);

      const result = buildInitialMessages({
        initialUserMessageTemplateId: "missing-template",
        initialUserMessage: "Fallback message",
      }, mockTemplateRegistry as any);

      expect(result).toHaveLength(1);
      expect(result[0]).toEqual({
        role: "user",
        content: "Fallback message",
      });
      expect(mockLogger.warn).toHaveBeenCalledWith(
        expect.stringContaining("not found"),
        expect.objectContaining({ templateId: "missing-template" }),
      );
    });

    it("should not include user message when neither template nor direct message is provided", () => {
      mockResolveSystemPrompt.mockReturnValue("");

      const result = buildInitialMessages({});

      expect(result).toHaveLength(0);
    });
  });

  describe("existing messages handling", () => {
    it("should append non-system existing messages", () => {
      mockResolveSystemPrompt.mockReturnValue("");

      const existingMessages: LLMMessage[] = [
        { role: "user", content: "Previous user message" },
        { role: "assistant", content: "Previous assistant message" },
      ];

      const result = buildInitialMessages({ existingMessages });

      expect(result).toHaveLength(2);
      expect(result).toEqual(existingMessages);
    });

    it("should filter out system messages from existingMessages", () => {
      mockResolveSystemPrompt.mockReturnValue("");

      const existingMessages: LLMMessage[] = [
        { role: "system", content: "Existing system prompt" },
        { role: "user", content: "User message" },
      ];

      const result = buildInitialMessages({ existingMessages });

      expect(result).toHaveLength(1);
      expect(result[0]).toEqual({ role: "user", content: "User message" });
    });

    it("should not include existingMessages when array is empty", () => {
      mockResolveSystemPrompt.mockReturnValue("");

      const result = buildInitialMessages({ existingMessages: [] });

      expect(result).toHaveLength(0);
    });
  });

  describe("combined scenarios", () => {
    it("should build messages with system prompt, user message, and existing messages", () => {
      mockResolveSystemPrompt.mockReturnValue("System prompt");

      const existingMessages: LLMMessage[] = [
        { role: "user", content: "Existing user msg" },
        { role: "assistant", content: "Existing assistant msg" },
      ];

      const result = buildInitialMessages({
        systemPrompt: "System prompt",
        initialUserMessage: "Initial user message",
        existingMessages,
      });

      expect(result).toHaveLength(4);
      expect(result[0]).toEqual({ role: "system", content: "System prompt" });
      expect(result[1]).toEqual({ role: "user", content: "Initial user message" });
      expect(result[2]).toEqual({ role: "user", content: "Existing user msg" });
      expect(result[3]).toEqual({ role: "assistant", content: "Existing assistant msg" });
    });

    it("should filter out system messages from existingMessages even when system prompt is also added", () => {
      mockResolveSystemPrompt.mockReturnValue("New system prompt");

      const existingMessages: LLMMessage[] = [
        { role: "system", content: "Old system prompt" },
        { role: "user", content: "User message" },
      ];

      const result = buildInitialMessages({
        systemPrompt: "New system prompt",
        existingMessages,
      });

      // Only one system message (the new one), and the user message
      expect(result).toHaveLength(2);
      expect(result[0]).toEqual({ role: "system", content: "New system prompt" });
      expect(result[1]).toEqual({ role: "user", content: "User message" });
    });
  });

  describe("edge cases", () => {
    it("should handle undefined existingMessages gracefully", () => {
      mockResolveSystemPrompt.mockReturnValue("");

      const result = buildInitialMessages({ initialUserMessage: "Hi" });

      expect(result).toHaveLength(1);
      expect(result[0]).toEqual({ role: "user", content: "Hi" });
    });

    it("should handle undefined initialUserMessageTemplateVariables gracefully", () => {
      mockResolveSystemPrompt.mockReturnValue("");
      mockTemplateRegistry.render.mockReturnValue("Rendered");

      const result = buildInitialMessages({
        initialUserMessageTemplateId: "template-1",
      }, mockTemplateRegistry as any);

      expect(result).toHaveLength(1);
      expect(mockTemplateRegistry.render).toHaveBeenCalledWith("template-1", {});
    });
  });
});
