import { describe, it, expect, vi, beforeEach } from "vitest";

// ── Hoisted Mocks ───────────────────────────────────────────────────────────
const { mockTemplateRegistry, mockRenderTemplate, mockLogger } = vi.hoisted(() => ({
  mockTemplateRegistry: {
    get: vi.fn(),
  },
  mockRenderTemplate: vi.fn((template: string, _variables: Record<string, unknown>) => template),
  mockLogger: {
    warn: vi.fn(),
    info: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  },
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

// ── Import after mocks ──────────────────────────────────────────────────────
import { resolveSystemPrompt } from "../system-prompt-resolver.js";

describe("resolveSystemPrompt", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("when no config is provided", () => {
    it("should return empty string when config is empty", () => {
      const result = resolveSystemPrompt({});
      expect(result).toBe("");
    });

    it("should return empty string when systemPrompt is empty string", () => {
      const result = resolveSystemPrompt({ systemPrompt: "" });
      expect(result).toBe("");
    });
  });

  describe("when systemPrompt is provided directly", () => {
    it("should return the systemPrompt string", () => {
      const result = resolveSystemPrompt({ systemPrompt: "You are a helpful assistant." });
      expect(result).toBe("You are a helpful assistant.");
    });

    it("should not call templateRegistry.get", () => {
      resolveSystemPrompt({ systemPrompt: "Be concise." });
      expect(mockTemplateRegistry.get).not.toHaveBeenCalled();
    });
  });

  describe("when systemPromptTemplateId is provided", () => {
    it("should return template content when template is found", () => {
      mockTemplateRegistry.get.mockReturnValue({ content: "Template content here" });

      const result = resolveSystemPrompt({ systemPromptTemplateId: "template-1" }, mockTemplateRegistry as any);
      expect(result).toBe("Template content here");
      expect(mockTemplateRegistry.get).toHaveBeenCalledWith("template-1");
    });

    it("should fall back to systemPrompt when template is not found", () => {
      mockTemplateRegistry.get.mockReturnValue(undefined);

      const result = resolveSystemPrompt({
        systemPromptTemplateId: "missing-template",
        systemPrompt: "Fallback prompt",
      }, mockTemplateRegistry as any);
      expect(result).toBe("Fallback prompt");
      expect(mockLogger.warn).toHaveBeenCalledWith(
        expect.stringContaining("not found"),
        expect.objectContaining({ templateId: "missing-template" }),
      );
    });

    it("should fall back to empty string when template is not found and no systemPrompt", () => {
      mockTemplateRegistry.get.mockReturnValue(undefined);

      const result = resolveSystemPrompt({ systemPromptTemplateId: "missing-template" }, mockTemplateRegistry as any);
      expect(result).toBe("");
      expect(mockLogger.warn).toHaveBeenCalled();
    });

    it("should not call renderTemplate when no variables provided", () => {
      mockTemplateRegistry.get.mockReturnValue({ content: "Static content" });

      resolveSystemPrompt({ systemPromptTemplateId: "template-1" }, mockTemplateRegistry as any);
      expect(mockRenderTemplate).not.toHaveBeenCalled();
    });
  });

  describe("variable rendering", () => {
    it("should render variables when systemPromptTemplateVariables is provided with direct prompt", () => {
      mockRenderTemplate.mockReturnValue("Hello, World!");

      const result = resolveSystemPrompt({
        systemPrompt: "Hello, {{name}}!",
        systemPromptTemplateVariables: { name: "World" },
      });
      expect(result).toBe("Hello, World!");
      expect(mockRenderTemplate).toHaveBeenCalledWith("Hello, {{name}}!", { name: "World" });
    });

    it("should render variables when systemPromptTemplateVariables is provided with template", () => {
      mockTemplateRegistry.get.mockReturnValue({ content: "Template {{var}}" });
      mockRenderTemplate.mockReturnValue("Template rendered");

      const result = resolveSystemPrompt({
        systemPromptTemplateId: "template-1",
        systemPromptTemplateVariables: { var: "rendered" },
      }, mockTemplateRegistry as any);
      expect(result).toBe("Template rendered");
      expect(mockRenderTemplate).toHaveBeenCalledWith("Template {{var}}", { var: "rendered" });
    });
  });

  describe("edge cases", () => {
    it("should handle systemPromptTemplateId with empty systemPrompt fallback", () => {
      mockTemplateRegistry.get.mockReturnValue(undefined);

      const result = resolveSystemPrompt({
        systemPromptTemplateId: "missing",
        systemPrompt: "",
      });
      expect(result).toBe("");
    });

    it("should prefer template over direct systemPrompt when both are provided", () => {
      mockTemplateRegistry.get.mockReturnValue({ content: "From template" });

      const result = resolveSystemPrompt({
        systemPromptTemplateId: "template-1",
        systemPrompt: "Direct prompt",
      }, mockTemplateRegistry as any);
      expect(result).toBe("From template");
    });
  });
});
