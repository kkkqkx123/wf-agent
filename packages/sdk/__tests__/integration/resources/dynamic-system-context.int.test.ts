/**
 * Integration Test: Dynamic System Context
 *
 * Tests the buildSystemContextPrompt() function and its context fragments
 * (current time, environment, available tools, skills, workflows).
 *
 * Real business scenarios:
 * 1. Agent loop: Build system context for LLM system message at runtime
 * 2. KV cache optimization: Cache system context to preserve prompt cache hits
 * 3. Configurable sections: Enable/disable time, environment, custom sections
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { buildSystemContextPrompt } from "@/resources/dynamic/system-context/builder.js";
import {
  generateCurrentTimeSection,
  generateCurrentTimeContent,
} from "@/resources/dynamic/system-context/fragments/current-time.js";
import {
  generateEnvironmentSection,
  generateEnvironmentContent,
  getDefaultEnvironmentInfo,
} from "@/resources/dynamic/system-context/fragments/environment.js";
import {
  generateAvailableToolsContent,
  generateCompactToolsContent,
} from "@/resources/dynamic/system-context/fragments/available-tools.js";
import { generateSkillsContent } from "@/resources/dynamic/system-context/fragments/skills.js";
import { generateWorkflowsContent } from "@/resources/dynamic/system-context/fragments/workflows.js";
import { wrapSection, cleanupEmptyLines } from "@/resources/dynamic/system-context/fragments/utils.js";
import type { DynamicContextConfig } from "@wf-agent/types";

// =============================================================================
// Mock Tool
// =============================================================================

const mockTool = {
  id: "test_tool",
  description: "A test tool for verification",
  type: "STATELESS" as const,
  parameters: { type: "object" as const, properties: {} },
};

// =============================================================================
// Tests
// =============================================================================

describe("Dynamic System Context", () => {
  // ---------------------------------------------------------------------------
  // Scenario: buildSystemContextPrompt
  // ---------------------------------------------------------------------------
  describe("buildSystemContextPrompt", () => {
    it("should return empty string when no config is provided", async () => {
      const result = await buildSystemContextPrompt(undefined, 0);
      expect(result).toBe("");
    });

    it("should include current time when includeCurrentTime=true", async () => {
      const config: DynamicContextConfig = { includeCurrentTime: true };
      const result = await buildSystemContextPrompt(config, 0);

      expect(result).toContain("CURRENT TIME");
      expect(result).toContain("Current Time:");
    });

    it("should include environment info when includeEnvironmentInfo=true", async () => {
      const config: DynamicContextConfig = { includeEnvironmentInfo: true };
      const result = await buildSystemContextPrompt(config, 0);

      expect(result).toContain("ENVIRONMENT");
      expect(result).toContain("Operating System:");
    });

    it("should include custom sections when provided", async () => {
      const config: DynamicContextConfig = {
        customSections: {
          custom1: "## Custom Section\nCustom content here",
        },
      };
      const result = await buildSystemContextPrompt(config, 0);

      expect(result).toContain("Custom Section");
      expect(result).toContain("Custom content here");
    });

    it("should combine multiple sections", async () => {
      const config: DynamicContextConfig = {
        includeCurrentTime: true,
        includeEnvironmentInfo: true,
      };
      const result = await buildSystemContextPrompt(config, 0);

      expect(result).toContain("CURRENT TIME");
      expect(result).toContain("ENVIRONMENT");
    });

    it("should not include current time when disabled", async () => {
      const config: DynamicContextConfig = { includeCurrentTime: false };
      const result = await buildSystemContextPrompt(config, 0);
      expect(result).not.toContain("CURRENT TIME");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Caching behavior
  // ---------------------------------------------------------------------------
  describe("caching behavior", () => {
    it("should return cached content when called within TTL", async () => {
      const config: DynamicContextConfig = { includeCurrentTime: true };
      const result1 = await buildSystemContextPrompt(config, 60000);
      const result2 = await buildSystemContextPrompt(config, 60000);

      // Both calls should return identical content
      expect(result1).toBe(result2);
    });

    it("should return fresh content when cache is disabled (TTL=0)", async () => {
      const config: DynamicContextConfig = { includeCurrentTime: true };
      const result1 = await buildSystemContextPrompt(config, 0);

      // Small delay, then call again
      await new Promise((r) => setTimeout(r, 10));
      const result2 = await buildSystemContextPrompt(config, 0);

      // Both should be valid time strings (may differ slightly)
      expect(result1).toContain("CURRENT TIME");
      expect(result2).toContain("CURRENT TIME");
    });

    it("should use different cache keys for different configs", async () => {
      const config1: DynamicContextConfig = { includeCurrentTime: true };
      const config2: DynamicContextConfig = { includeEnvironmentInfo: true };

      const result1 = await buildSystemContextPrompt(config1, 60000);
      const result2 = await buildSystemContextPrompt(config2, 60000);

      expect(result1).toContain("CURRENT TIME");
      expect(result2).toContain("ENVIRONMENT");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Current time fragments
  // ---------------------------------------------------------------------------
  describe("current time fragments", () => {
    it("should generate current time content in ISO format", () => {
      const content = generateCurrentTimeContent();
      expect(content).toMatch(/Current Time: \d{4}-\d{2}-\d{2}T/);
    });

    it("should generate current time section with wrapper", () => {
      const section = generateCurrentTimeSection();
      expect(section).toContain("CURRENT TIME");
      expect(section).toContain("Current Time:");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Environment fragments
  // ---------------------------------------------------------------------------
  describe("environment fragments", () => {
    it("should generate environment content with OS info", () => {
      const envInfo = getDefaultEnvironmentInfo();
      const content = generateEnvironmentContent(envInfo);

      expect(content).toContain("Operating System:");
      expect(content).toContain("Timezone:");
      expect(content).toContain("User Language:");
    });

    it("should generate environment section with wrapper", () => {
      const envInfo = getDefaultEnvironmentInfo();
      const section = generateEnvironmentSection(envInfo);

      expect(section).toContain("ENVIRONMENT");
      expect(section).toContain("Operating System:");
    });

    it("should detect a valid operating system", () => {
      const envInfo = getDefaultEnvironmentInfo();
      expect(["Windows", "macOS", "Linux"]).toContain(envInfo.os);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Available tools fragments
  // ---------------------------------------------------------------------------
  describe("available tools fragments", () => {
    it("should generate tool description content for valid tools", () => {
      const content = generateAvailableToolsContent({ tools: [mockTool] });
      expect(content).toBeTruthy();
      expect(content).toContain("AVAILABLE TOOLS");
      expect(content).toContain("test_tool");
    });

    it("should return null for empty tools array", () => {
      const content = generateAvailableToolsContent({ tools: [] });
      expect(content).toBeNull();
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Skills fragments
  // ---------------------------------------------------------------------------
  describe("skills fragments", () => {
    it("should generate skills content for valid skills", () => {
      const skills = [{ name: "test_skill", description: "A test skill" }];
      const content = generateSkillsContent(skills);
      expect(content).toContain("ACTIVE SKILLS");
      expect(content).toContain("test_skill");
    });

    it("should return empty string for empty skills", () => {
      expect(generateSkillsContent([])).toBe("");
    });

    it("should return empty string for undefined skills", () => {
      expect(generateSkillsContent(undefined)).toBe("");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Workflows fragments
  // ---------------------------------------------------------------------------
  describe("workflows fragments", () => {
    it("should generate workflows content for valid workflows", () => {
      const workflows = [{ id: "test_wf", name: "Test Workflow", description: "A test" }];
      const content = generateWorkflowsContent(workflows);
      expect(content).toContain("AVAILABLE WORKFLOWS");
      expect(content).toContain("test_wf");
    });

    it("should return empty string for empty array", () => {
      expect(generateWorkflowsContent([])).toBe("");
    });

    it("should return empty string for non-array input", () => {
      expect(generateWorkflowsContent("not_an_array")).toBe("");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Utility functions
  // ---------------------------------------------------------------------------
  describe("utility functions", () => {
    it("should wrap content in a section", () => {
      const section = wrapSection("TEST", "content");
      expect(section).toContain("TEST");
      expect(section).toContain("content");
    });

    it("should clean up empty lines", () => {
      const result = cleanupEmptyLines("line1\n\n\nline2\n\n\n\nline3");
      expect(result).toBe("line1\n\nline2\n\nline3");
    });
  });
});