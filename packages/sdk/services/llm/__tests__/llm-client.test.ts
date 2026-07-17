/**
 * LLMClientImpl Unit Tests — Tool Call Protocol Enforcement
 *
 * Business scenarios:
 * 1. Request has lockedToolCallFormat → enforced regardless of profile format
 * 2. Locked format matches profile format → no violation, protocolAutoConverted=false
 * 3. Locked format differs from profile format → violation detected, policy resolved
 * 4. Violation policy=auto_convert → protocolAutoConverted=true
 * 5. Violation policy=fail → throws ProtocolViolationError
 * 6. Violation policy=warn → protocolAutoConverted=false, continues
 * 7. Violation policy=ignore → protocolAutoConverted=false, continues silently
 * 8. No locked format → effective format from request or profile used
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { LLMClientImpl } from "../client.js";
import type { LLMRequest, LLMProfile } from "@wf-agent/types";
import { DEFAULT_TOOL_CALL_PROTOCOL_CONFIG } from "@wf-agent/types";

// Mock the formatter
const mockFormatter = {
  buildRequest: vi.fn().mockReturnValue({
    httpRequest: { url: "/test", method: "POST", headers: {}, body: {} },
  }),
  parseResponse: vi.fn().mockReturnValue({ content: "test", toolCalls: [] }),
  parseStreamLine: vi.fn().mockReturnValue({ valid: false, chunk: {} }),
  getSupportedProvider: vi.fn().mockReturnValue("OPENAI_CHAT"),
  buildCountTokensRequest: undefined,
};

// Mock the HttpClient and SseTransport
vi.mock("../../services/index.js", () => ({
  HttpClient: vi.fn().mockImplementation(() => ({
    post: vi.fn().mockResolvedValue({ data: { content: "test" } }),
  })),
  HttpSseTransport: vi.fn().mockImplementation(() => ({
    executeStream: vi.fn().mockReturnValue({
      [Symbol.asyncIterator]: async function* () {},
    }),
  })),
}));

// Mock deep imports to avoid @sdk/ path alias resolution issues
vi.mock("@sdk/services/evaluation/shared/path-resolver.js", () => ({
  resolveContextPath: vi.fn(),
}));
vi.mock("@sdk/services/evaluation/cache-manager.js", () => ({
  cacheManager: {},
}));
vi.mock("../../services/evaluation/index.js", () => ({}));
vi.mock("../formatters/index.js", () => ({
  BaseFormatter: class {},
}));

// Mock anthropic formatter
vi.mock("../formatters/anthropic.js", () => ({
  AnthropicFormatter: class {},
}));

// Mock handleProtocolViolation
const mockHandleProtocolViolation = vi.fn().mockReturnValue(false);
vi.mock("../formatters/tool-format-selector.js", () => ({
  handleProtocolViolation: (...args: unknown[]) => mockHandleProtocolViolation(...args),
}));

describe("LLMClientImpl — Tool Call Protocol Enforcement", () => {
  let client: LLMClientImpl;
  const baseProfile: LLMProfile = {
    id: "test-profile",
    provider: "openai",
    model: "gpt-4",
    apiKey: "test-key",
    parameters: {},
    toolCallFormat: undefined,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockHandleProtocolViolation.mockReturnValue(false);
    client = new LLMClientImpl(baseProfile, mockFormatter as any);
  });

  // ===========================================================================
  // Scenario 1: No locked format — uses effective format
  // ===========================================================================
  describe("no locked format", () => {
    it("should use profile toolCallFormat when no locked format and no request format", () => {
      const profileWithFormat: LLMProfile = {
        ...baseProfile,
        toolCallFormat: { format: "xml" },
      };
      const formatClient = new LLMClientImpl(profileWithFormat, mockFormatter as any);
      const request: LLMRequest = { messages: [{ role: "user", content: "hi" }] };

      // Access protected method via cast
      const config = (formatClient as any).getFormatterConfig(request, false);
      expect(config.toolCallFormat.format).toBe("xml");
      expect(config.protocolAutoConverted).toBeUndefined();
    });

    it("should use request toolCallFormat over profile format when no locked format", () => {
      const profileWithFormat: LLMProfile = {
        ...baseProfile,
        toolCallFormat: { format: "xml" },
      };
      const formatClient = new LLMClientImpl(profileWithFormat, mockFormatter as any);
      const request: LLMRequest = {
        messages: [{ role: "user", content: "hi" }],
        toolCallFormat: { format: "native" },
      };

      const config = (formatClient as any).getFormatterConfig(request, false);
      expect(config.toolCallFormat.format).toBe("native");
    });
  });

  // ===========================================================================
  // Scenario 2: Locked format matches — no violation
  // ===========================================================================
  describe("locked format matches profile format", () => {
    it("should use locked format and not call handleProtocolViolation when formats match", () => {
      const request: LLMRequest = {
        messages: [{ role: "user", content: "hi" }],
        lockedToolCallFormat: { format: "xml" },
        toolCallFormat: { format: "xml" },
      };

      const config = (client as any).getFormatterConfig(request, false);
      expect(config.toolCallFormat.format).toBe("xml");
      expect(mockHandleProtocolViolation).not.toHaveBeenCalled();
      expect(config.protocolAutoConverted).toBeUndefined();
    });

    it("should use locked format when profile has no format and locked is set", () => {
      const request: LLMRequest = {
        messages: [{ role: "user", content: "hi" }],
        lockedToolCallFormat: { format: "native" },
      };

      const config = (client as any).getFormatterConfig(request, false);
      expect(config.toolCallFormat.format).toBe("native");
      expect(mockHandleProtocolViolation).not.toHaveBeenCalled();
    });
  });

  // ===========================================================================
  // Scenario 3: Locked format differs — violation detected, policy resolved
  // ===========================================================================
  describe("locked format differs from profile format", () => {
    it("should detect mismatch and call handleProtocolViolation with default policy", () => {
      mockHandleProtocolViolation.mockReturnValue(false);
      const request: LLMRequest = {
        messages: [{ role: "user", content: "hi" }],
        lockedToolCallFormat: { format: "xml" },
        toolCallFormat: { format: "native" },
      };

      const config = (client as any).getFormatterConfig(request, false);
      expect(config.toolCallFormat.format).toBe("xml");
      expect(mockHandleProtocolViolation).toHaveBeenCalledTimes(1);

      const context = mockHandleProtocolViolation.mock.calls[0][0];
      expect(context.lockedFormat.format).toBe("xml");
      expect(context.attemptedFormat.format).toBe("native");
    });

    it("should use request-level violationPolicy over global default", () => {
      const request: LLMRequest = {
        messages: [{ role: "user", content: "hi" }],
        lockedToolCallFormat: { format: "xml" },
        toolCallFormat: { format: "native" },
        violationPolicy: "auto_convert",
      };

      const config = (client as any).getFormatterConfig(request, false);
      expect(mockHandleProtocolViolation).toHaveBeenCalledWith(
        expect.anything(),
        "auto_convert",
      );
    });

    it("should default to global default violationPolicy when not on request", () => {
      const request: LLMRequest = {
        messages: [{ role: "user", content: "hi" }],
        lockedToolCallFormat: { format: "xml" },
        toolCallFormat: { format: "native" },
      };

      (client as any).getFormatterConfig(request, false);
      expect(mockHandleProtocolViolation).toHaveBeenCalledWith(
        expect.anything(),
        DEFAULT_TOOL_CALL_PROTOCOL_CONFIG.violationPolicy,
      );
    });
  });

  // ===========================================================================
  // Scenario 4: auto_convert policy → protocolAutoConverted=true
  // ===========================================================================
  describe("auto_convert policy", () => {
    it("should set protocolAutoConverted=true when handleProtocolViolation returns true", () => {
      mockHandleProtocolViolation.mockReturnValue(true);
      const request: LLMRequest = {
        messages: [{ role: "user", content: "hi" }],
        lockedToolCallFormat: { format: "xml" },
        toolCallFormat: { format: "native" },
        violationPolicy: "auto_convert",
      };

      const config = (client as any).getFormatterConfig(request, false);
      expect(config.toolCallFormat.format).toBe("xml");
      expect(config.protocolAutoConverted).toBe(true);
    });
  });

  // ===========================================================================
  // Scenario 5: fail policy → throws
  // ===========================================================================
  describe("fail policy", () => {
    it("should propagate the ProtocolViolationError thrown by handleProtocolViolation", () => {
      mockHandleProtocolViolation.mockImplementation(() => {
        throw new Error("Tool call protocol conflict: locked \"xml\" but profile attempted \"native\"");
      });
      const request: LLMRequest = {
        messages: [{ role: "user", content: "hi" }],
        lockedToolCallFormat: { format: "xml" },
        toolCallFormat: { format: "native" },
        violationPolicy: "fail",
      };

      expect(() => (client as any).getFormatterConfig(request, false)).toThrow(
        "Tool call protocol conflict",
      );
    });
  });

  // ===========================================================================
  // Scenario 6: warn policy → protocolAutoConverted=false, continues
  // ===========================================================================
  describe("warn policy", () => {
    it("should keep protocolAutoConverted=false when policy is warn", () => {
      mockHandleProtocolViolation.mockReturnValue(false);
      const request: LLMRequest = {
        messages: [{ role: "user", content: "hi" }],
        lockedToolCallFormat: { format: "xml" },
        toolCallFormat: { format: "native" },
        violationPolicy: "warn",
      };

      const config = (client as any).getFormatterConfig(request, false);
      expect(config.toolCallFormat.format).toBe("xml");
      expect(config.protocolAutoConverted).toBe(false);
    });
  });

  // ===========================================================================
  // Scenario 7: ignore policy → protocolAutoConverted=false, silent
  // ===========================================================================
  describe("ignore policy", () => {
    it("should keep protocolAutoConverted=false when policy is ignore", () => {
      mockHandleProtocolViolation.mockReturnValue(false);
      const request: LLMRequest = {
        messages: [{ role: "user", content: "hi" }],
        lockedToolCallFormat: { format: "xml" },
        toolCallFormat: { format: "native" },
        violationPolicy: "ignore",
      };

      const config = (client as any).getFormatterConfig(request, false);
      expect(config.toolCallFormat.format).toBe("xml");
      expect(config.protocolAutoConverted).toBe(false);
    });
  });

  // ===========================================================================
  // Scenario 8: Locked format with no attempted format
  // ===========================================================================
  describe("locked format without profile/request format", () => {
    it("should not trigger violation when attemptedFormat is undefined", () => {
      const request: LLMRequest = {
        messages: [{ role: "user", content: "hi" }],
        lockedToolCallFormat: { format: "xml" },
      };

      const config = (client as any).getFormatterConfig(request, false);
      expect(config.toolCallFormat.format).toBe("xml");
      // Verify violation NOT called because attemptedFormat is undefined
      expect(mockHandleProtocolViolation).not.toHaveBeenCalled();
    });
  });
});