import { describe, it, expect } from "vitest";
import { injectDynamicPrompts } from "../dynamic-injection.js";
import type { LLMMessage } from "@wf-agent/types";

describe("injectDynamicPrompts", () => {
  const baseMessages: LLMMessage[] = [
    { role: "system", content: "You are a helpful assistant." },
    { role: "user", content: "Hello!" },
  ];

  describe("system injection", () => {
    it("should append staticSystem to existing system message", () => {
      const result = injectDynamicPrompts(baseMessages, "Dynamic system info", undefined);

      expect(result.systemInjected).toBe(true);
      expect(result.userContextInjected).toBe(false);
      expect(result.messages[0].content).toBe(
        "You are a helpful assistant.\n\nDynamic system info",
      );
      expect(result.messages).toHaveLength(2);
    });

    it("should insert new system message if none exists", () => {
      const messagesWithoutSystem: LLMMessage[] = [
        { role: "user", content: "Hello!" },
      ];
      const result = injectDynamicPrompts(messagesWithoutSystem, "System context", undefined);

      expect(result.systemInjected).toBe(true);
      expect(result.messages).toHaveLength(2);
      expect(result.messages[0]).toEqual({
        role: "system",
        content: "System context",
      });
    });

    it("should not inject when staticSystem is undefined", () => {
      const result = injectDynamicPrompts(baseMessages, undefined, undefined);

      expect(result.systemInjected).toBe(false);
      expect(result.userContextInjected).toBe(false);
      expect(result.messages).toEqual(baseMessages);
    });

    it("should not inject when staticSystem is empty string", () => {
      const result = injectDynamicPrompts(baseMessages, "", undefined);

      expect(result.systemInjected).toBe(false);
    });
  });

  describe("user context injection", () => {
    it("should append userContextSuffix to last user message", () => {
      const result = injectDynamicPrompts(baseMessages, undefined, "User context info");

      expect(result.systemInjected).toBe(false);
      expect(result.userContextInjected).toBe(true);
      expect(result.messages[1].content).toBe("Hello!\n\nUser context info");
      expect(result.messages).toHaveLength(2);
    });

    it("should handle multiple user messages by appending to last one", () => {
      const multiMessages: LLMMessage[] = [
        { role: "system", content: "System" },
        { role: "user", content: "First user" },
        { role: "assistant", content: "Assistant reply" },
        { role: "user", content: "Second user" },
      ];
      const result = injectDynamicPrompts(multiMessages, undefined, "Context");

      expect(result.userContextInjected).toBe(true);
      expect(result.messages[3].content).toBe("Second user\n\nContext");
      expect(result.messages[1].content).toBe("First user");
    });

    it("should append new user message if no user message exists", () => {
      const noUserMessages: LLMMessage[] = [
        { role: "system", content: "System" },
        { role: "assistant", content: "Assistant" },
      ];
      const result = injectDynamicPrompts(noUserMessages, undefined, "New context");

      expect(result.userContextInjected).toBe(true);
      expect(result.messages).toHaveLength(3);
      expect(result.messages[2]).toEqual({
        role: "user",
        content: "New context",
      });
    });
  });

  describe("combined injection", () => {
    it("should inject both system and user context", () => {
      const result = injectDynamicPrompts(baseMessages, "System context", "User context");

      expect(result.systemInjected).toBe(true);
      expect(result.userContextInjected).toBe(true);
      expect(result.messages[0].content).toBe(
        "You are a helpful assistant.\n\nSystem context",
      );
      expect(result.messages[1].content).toBe("Hello!\n\nUser context");
    });

    it("should handle empty messages array", () => {
      const result = injectDynamicPrompts([], "System", "User");

      expect(result.messages).toHaveLength(2);
      expect(result.messages[0]).toEqual({ role: "system", content: "System" });
      expect(result.messages[1]).toEqual({ role: "user", content: "User" });
    });
  });

  describe("immutability", () => {
    it("should not mutate original messages array", () => {
      const original = [...baseMessages];
      injectDynamicPrompts(baseMessages, "System", "User");

      expect(baseMessages).toEqual(original);
    });
  });

  describe("non-string content handling", () => {
    it("should preserve non-string content in system message", () => {
      const messagesWithArrayContent: LLMMessage[] = [
        { role: "system", content: [{ type: "text", text: "Existing" }] },
        { role: "user", content: "Hello" },
      ];
      const result = injectDynamicPrompts(messagesWithArrayContent, "Dynamic", undefined);

      expect(result.messages[0].content).toEqual([{ type: "text", text: "Existing" }]);
    });

    it("should preserve non-string content in user message", () => {
      const messagesWithArrayContent: LLMMessage[] = [
        { role: "user", content: [{ type: "text", text: "Hello" }] },
      ];
      const result = injectDynamicPrompts(messagesWithArrayContent, undefined, "Context");

      expect(result.messages[0].content).toEqual([{ type: "text", text: "Hello" }]);
    });
  });
});
