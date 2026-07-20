/**
 * Skill Adapter Unit Tests
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { SkillAdapter } from "../skill-adapter.js";
import { success, failure } from "@wf-agent/sdk/api";

// Mock SDK dependencies
vi.mock("@wf-agent/sdk", async () => {
  const actual = await vi.importActual("@wf-agent/sdk");
  return {
    ...actual,
  };
});

vi.mock("../../src/services/sdk-globals.js", () => ({
  getSDKInstance: vi.fn(),
}));

vi.mock("../../src/utils/output.js", () => ({
  getOutput: vi.fn(() => ({
    infoLog: vi.fn(),
    errorLog: vi.fn(),
    success: vi.fn(),
    fail: vi.fn(),
    result: vi.fn(),
    errorResult: vi.fn(),
    debugLog: vi.fn(),
  })),
}));

describe("SkillAdapter", () => {
  let adapter: SkillAdapter;
  let mockSdk: any;
  let mockSkillsApi: any;

  beforeEach(() => {
    vi.clearAllMocks();

    // Mock skills API
    mockSkillsApi = {
      scanSkills: vi.fn(),
      getAll: vi.fn(),
      get: vi.fn(),
      generateMetadataPrompt: vi.fn(),
      loadContent: vi.fn(),
      loadResources: vi.fn(),
      search: vi.fn(),
      toPrompt: vi.fn(),
      listResources: vi.fn(),
    };

    // Mock SDK instance
    mockSdk = {
      skills: mockSkillsApi,
    };

    // Setup getSDKInstance mock
    const { getSDKInstance } = require("../../src/services/sdk-globals.js");
    getSDKInstance.mockReturnValue(mockSdk);

    // Create adapter instance
    adapter = new SkillAdapter();
  });

  describe("initialize", () => {
    it("should initialize skill registry successfully", async () => {
      const mockSkills = [
        { id: "skill-1", name: "Skill 1" },
        { id: "skill-2", name: "Skill 2" },
      ];

      mockSkillsApi.scanSkills.mockResolvedValue(success(undefined, 100));
      mockSkillsApi.getAll.mockResolvedValue(success(mockSkills, 30));

      await adapter.initialize("/path/to/skills");

      expect(mockSkillsApi.scanSkills).toHaveBeenCalledWith("/path/to/skills");
      expect(mockSkillsApi.getAll).toHaveBeenCalled();
    });

    it("should handle scan failure", async () => {
      const mockError = new Error("Scan failed");
      mockSkillsApi.scanSkills.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.initialize("/path/to/skills")).rejects.toThrow("Scan failed");
    });
  });

  describe("listSkills", () => {
    it("should list all skills", async () => {
      const mockSkills = [
        { id: "skill-1", name: "Skill 1", version: "1.0.0" },
        { id: "skill-2", name: "Skill 2", version: "2.0.0" },
      ];
      mockSkillsApi.getAll.mockResolvedValue(success(mockSkills, 25));

      const result = await adapter.listSkills();

      expect(mockSkillsApi.getAll).toHaveBeenCalled();
      expect(result).toHaveLength(2);
      expect(result[0]).toHaveProperty("id", "skill-1");
    });

    it("should list skills with filter", async () => {
      const mockSkills = [{ id: "skill-1", name: "Test Skill" }];
      const filter = { name: "Test" };
      mockSkillsApi.getAll.mockResolvedValue(success(mockSkills, 20));

      const result = await adapter.listSkills(filter);

      expect(mockSkillsApi.getAll).toHaveBeenCalledWith(filter);
      expect(result).toHaveLength(1);
    });

    it("should handle list failure", async () => {
      const mockError = new Error("List failed");
      mockSkillsApi.getAll.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.listSkills()).rejects.toThrow("List failed");
    });
  });

  describe("getSkill", () => {
    it("should get skill by ID", async () => {
      const mockSkill = { id: "skill-123", name: "Test Skill", version: "1.0.0" };
      mockSkillsApi.get.mockResolvedValue(success(mockSkill, 15));

      const result = await adapter.getSkill("skill-123");

      expect(mockSkillsApi.get).toHaveBeenCalledWith("skill-123");
      expect(result).toEqual(mockSkill);
    });

    it("should throw error when skill not found", async () => {
      mockSkillsApi.get.mockResolvedValue(success(null, 15));

      await expect(adapter.getSkill("non-existent")).rejects.toThrow(
        "Skill not found: non-existent",
      );
    });

    it("should handle get failure", async () => {
      const mockError = new Error("Get failed");
      mockSkillsApi.get.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.getSkill("skill-123")).rejects.toThrow("Get failed");
    });
  });

  describe("generateMetadataPrompt", () => {
    it("should generate metadata prompt", () => {
      const mockPrompt = "This is a skill metadata prompt";
      mockSkillsApi.generateMetadataPrompt.mockReturnValue(mockPrompt);

      const result = adapter.generateMetadataPrompt();

      expect(mockSkillsApi.generateMetadataPrompt).toHaveBeenCalled();
      expect(result).toBe(mockPrompt);
    });
  });

  describe("loadContent", () => {
    it("should load skill content", async () => {
      const mockContent = { type: "markdown", content: "# Skill Content" };
      mockSkillsApi.loadContent.mockResolvedValue(success(mockContent, 20));

      const result = await adapter.loadContent("skill-123");

      expect(mockSkillsApi.loadContent).toHaveBeenCalledWith("skill-123");
      expect(result).toEqual(mockContent);
    });

    it("should handle load failure", async () => {
      const mockError = new Error("Load failed");
      mockSkillsApi.loadContent.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.loadContent("skill-123")).rejects.toThrow("Load failed");
    });
  });

  describe("loadResources", () => {
    it("should load skill resources", async () => {
      const mockResources = new Map([
        ["file1.md", "# Content 1"],
        ["file2.md", "# Content 2"],
      ]);
      mockSkillsApi.loadResources.mockResolvedValue(success(mockResources, 30));

      const result = await adapter.loadResources("skill-123", "references");

      expect(mockSkillsApi.loadResources).toHaveBeenCalledWith("skill-123", "references");
      expect(result.size).toBe(2);
    });

    it("should handle resource load failure", async () => {
      const mockError = new Error("Resource load failed");
      mockSkillsApi.loadResources.mockResolvedValue(failure(mockError as any, 0));

      await expect(adapter.loadResources("skill-123", "references")).rejects.toThrow(
        "Skill resource not found: skill-123, references",
      );
    });
  });
});
