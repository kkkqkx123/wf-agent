/**
 * Unit tests for Tool Format Selector
 */
import { describe, it, expect, vi } from "vitest";
import {
  getToolFormatTemplates,
  getToolFormatDisplayName,
  getToolFormatDescription,
  getAvailableToolFormats,
  handleProtocolViolation,
  ProtocolViolationError,
  type ProtocolViolationContext,
} from "../tool-format-selector.js";

describe("ToolFormatSelector", () => {
  describe("getToolFormatTemplates", () => {
    it("should return raw templates for function_call format", () => {
      const templates = getToolFormatTemplates("native");
      expect(templates.listTemplate).toBeDefined();
      expect(templates.singleTemplate).toBeDefined();
      expect(templates.parameterTemplate).toBeDefined();
    });

    it("should return xml templates for xml format", () => {
      const templates = getToolFormatTemplates("xml");
      expect(templates.listTemplate).toBeDefined();
      expect(templates.singleTemplate).toBeDefined();
      expect(templates.parameterTemplate).toBeDefined();
    });

    it("should return json templates for json_wrapped format", () => {
      const templates = getToolFormatTemplates("json_wrapped");
      expect(templates.listTemplate).toBeDefined();
      expect(templates.singleTemplate).toBeDefined();
      expect(templates.parameterTemplate).toBeDefined();
    });

    it("should return raw templates for json_raw format when compact is false", () => {
      const templates = getToolFormatTemplates("json_raw", false);
      expect(templates.listTemplate).toBeDefined();
      expect(templates.singleTemplate).toBeDefined();
      expect(templates.parameterTemplate).toBeDefined();
    });

    it("should return compact templates for json_raw format when compact is true", () => {
      const templates = getToolFormatTemplates("json_raw", true);
      expect(templates.listTemplate).toBeDefined();
      expect(templates.singleTemplate).toBeDefined();
      expect(templates.parameterTemplate).toBeDefined();
    });
  });

  describe("getToolFormatDisplayName", () => {
    it("should return display name for function_call", () => {
      expect(getToolFormatDisplayName("native")).toBe("Native Function Call");
    });

    it("should return display name for xml", () => {
      expect(getToolFormatDisplayName("xml")).toBe("XML Format");
    });

    it("should return display name for json_wrapped", () => {
      expect(getToolFormatDisplayName("json_wrapped")).toBe("Wrapped JSON Format");
    });

    it("should return display name for json_raw", () => {
      expect(getToolFormatDisplayName("json_raw")).toBe("Raw JSON Format");
    });

    it("should return the format name itself for unknown formats", () => {
      expect(getToolFormatDisplayName("unknown" as any)).toBe("unknown");
    });
  });

  describe("getToolFormatDescription", () => {
    it("should return description for function_call", () => {
      expect(getToolFormatDescription("native")).toBe(
        "Uses the LLM provider's native function calling API (OpenAI, Anthropic, etc.)",
      );
    });

    it("should return description for xml", () => {
      expect(getToolFormatDescription("xml")).toBe(
        "Tools described in XML format, LLM outputs XML tool calls",
      );
    });

    it("should return description for json_wrapped", () => {
      expect(getToolFormatDescription("json_wrapped")).toBe(
        "Tools described in JSON, LLM outputs JSON wrapped with custom markers",
      );
    });

    it("should return description for json_raw", () => {
      expect(getToolFormatDescription("json_raw")).toBe(
        "Tools described in JSON, LLM outputs raw JSON without markers",
      );
    });

    it("should return fallback message for unknown format", () => {
      expect(getToolFormatDescription("unknown" as any)).toBe("Unknown format: unknown");
    });
  });

  describe("getAvailableToolFormats", () => {
    it("should return all four formats", () => {
      const formats = getAvailableToolFormats();
      expect(formats).toHaveLength(4);
    });

    it("should include function_call format", () => {
      const formats = getAvailableToolFormats();
      expect(formats.find(f => f.value === "native")).toBeDefined();
    });

    it("should include xml format", () => {
      const formats = getAvailableToolFormats();
      expect(formats.find(f => f.value === "xml")).toBeDefined();
    });

    it("should include json_wrapped format", () => {
      const formats = getAvailableToolFormats();
      expect(formats.find(f => f.value === "json_wrapped")).toBeDefined();
    });

    it("should include json_raw format", () => {
      const formats = getAvailableToolFormats();
      expect(formats.find(f => f.value === "json_raw")).toBeDefined();
    });
  });

  describe("handleProtocolViolation", () => {
    const baseContext: ProtocolViolationContext = {
      lockedFormat: { format: "native", markers: { start: "<tool>", end: "</tool>" } },
      attemptedFormat: { format: "xml", markers: { start: "<tool>", end: "</tool>" } },
      executionId: "exec-123",
      entityId: "entity-456",
      profileId: "profile-789",
      iteration: 3,
    };

    describe("ignore policy", () => {
      it("should silently return without errors when policy is 'ignore'", () => {
        expect(() => handleProtocolViolation(baseContext, "ignore")).not.toThrow();
      });

      it("should call metrics recorder when provided", () => {
        const metricsRecorder = vi.fn();
        const ctx = { ...baseContext, recordMetrics: metricsRecorder };
        handleProtocolViolation(ctx, "ignore");
        expect(metricsRecorder).toHaveBeenCalledWith("ignore");
      });
    });

    describe("warn policy", () => {
      it("should not throw error when policy is 'warn'", () => {
        expect(() => handleProtocolViolation(baseContext, "warn")).not.toThrow();
      });

      it("should call metrics recorder when provided", () => {
        const metricsRecorder = vi.fn();
        const ctx = { ...baseContext, recordMetrics: metricsRecorder };
        handleProtocolViolation(ctx, "warn");
        expect(metricsRecorder).toHaveBeenCalledWith("warn");
      });
    });

    describe("fail policy", () => {
      it("should throw ProtocolViolationError when policy is 'fail'", () => {
        expect(() => handleProtocolViolation(baseContext, "fail")).toThrow(ProtocolViolationError);
      });

      it("should include format details in error message", () => {
        expect(() => handleProtocolViolation(baseContext, "fail")).toThrow(
          /locked "native".*profile "profile-789".*"xml"/,
        );
      });

      it("should call metrics recorder before throwing", () => {
        const metricsRecorder = vi.fn();
        const ctx = { ...baseContext, recordMetrics: metricsRecorder };
        expect(() => handleProtocolViolation(ctx, "fail")).toThrow(ProtocolViolationError);
        expect(metricsRecorder).toHaveBeenCalledWith("fail");
      });
    });

    describe("auto_convert policy", () => {
      it("should not throw error when policy is 'auto_convert'", () => {
        expect(() => handleProtocolViolation(baseContext, "auto_convert")).not.toThrow();
      });

      it("should call metrics recorder when provided", () => {
        const metricsRecorder = vi.fn();
        const ctx = { ...baseContext, recordMetrics: metricsRecorder };
        handleProtocolViolation(ctx, "auto_convert");
        expect(metricsRecorder).toHaveBeenCalledWith("auto_convert");
      });
    });

    describe("edge cases", () => {
      it("should handle undefined attemptedFormat", () => {
        const ctx = { ...baseContext, attemptedFormat: undefined };
        expect(() => handleProtocolViolation(ctx, "warn")).not.toThrow();
      });

      it("should handle undefined iteration", () => {
        const ctx = { ...baseContext, iteration: undefined };
        expect(() => handleProtocolViolation(ctx, "warn")).not.toThrow();
      });

      it("should handle undefined entityId", () => {
        const ctx = { ...baseContext, entityId: undefined };
        expect(() => handleProtocolViolation(ctx, "warn")).not.toThrow();
      });
    });
  });
});
