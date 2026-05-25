/**
 * Unit tests for Tool Format Selector
 */
import { describe, it, expect } from "vitest";
import {
  getToolFormatTemplates,
  getToolFormatDisplayName,
  getToolFormatDescription,
  getAvailableToolFormats,
} from "../tool-format-selector.js";

describe("ToolFormatSelector", () => {
  describe("getToolFormatTemplates", () => {
    it("should return raw templates for function_call format", () => {
      const templates = getToolFormatTemplates("function_call");
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
      expect(getToolFormatDisplayName("function_call")).toBe("Native Function Call");
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
      expect(getToolFormatDescription("function_call")).toBe(
        "Uses the LLM provider's native function calling API (OpenAI, Anthropic, etc.)"
      );
    });

    it("should return description for xml", () => {
      expect(getToolFormatDescription("xml")).toBe(
        "Tools described in XML format, LLM outputs XML tool calls"
      );
    });

    it("should return description for json_wrapped", () => {
      expect(getToolFormatDescription("json_wrapped")).toBe(
        "Tools described in JSON, LLM outputs JSON wrapped with custom markers"
      );
    });

    it("should return description for json_raw", () => {
      expect(getToolFormatDescription("json_raw")).toBe(
        "Tools described in JSON, LLM outputs raw JSON without markers"
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
      expect(formats.find(f => f.value === "function_call")).toBeDefined();
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
});
