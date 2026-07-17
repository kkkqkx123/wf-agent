/**
 * Integration Test: Predefined Builtin Tools
 *
 * Tests the createBuiltinTools() and createAllPredefinedTools() functions.
 *
 * Real business scenarios:
 * 1. SDK bootstrap: Create all builtin tools for workflow management, agent calls, user interaction
 * 2. Feature gating: Use allowList to enable only specific builtin tools (e.g., only workflow tools)
 * 3. Security: Use blockList to disable specific builtin tools (e.g., block call_agent)
 * 4. Dynamic descriptions: Pass workflow/agent loaders to generate dynamic tool descriptions
 */

import { describe, it, expect, afterEach } from "vitest";
import { createBuiltinTools } from "@/resources/predefined/tools/builtin/registry.js";
import { createAllPredefinedTools } from "@/resources/predefined/tools/registry.js";
import type { BuiltinToolsOptions } from "@/resources/predefined/tools/builtin/types.js";
import { createTempDir, cleanupTempDirs } from "../resources/__shared/fixtures.js";
import { join } from "path";

// =============================================================================
// Constants
// =============================================================================

const BUILTIN_TOOL_COUNT = 6;

const BUILTIN_TOOL_IDS = [
  "builtin_execute_workflow",
  "builtin_query_workflow_status",
  "builtin_cancel_workflow",
  "builtin_call_agent",
  "ask_followup_question",
  "attempt_completion",
];

// =============================================================================
// Tests
// =============================================================================

describe("Predefined Builtin Tools", () => {
  // ---------------------------------------------------------------------------
  // Scenario: Create all builtin tools
  // ---------------------------------------------------------------------------
  describe("createBuiltinTools — all tools", () => {
    it("should create all 6 builtin tools by default", () => {
      const tools = createBuiltinTools();

      expect(tools).toHaveLength(BUILTIN_TOOL_COUNT);
      const toolIds = tools.map((t) => t.id);
      expect(toolIds).toEqual(expect.arrayContaining(BUILTIN_TOOL_IDS));
    });

    it("should create tools with BUILTIN type", () => {
      const tools = createBuiltinTools();

      for (const tool of tools) {
        expect(tool.type).toBe("BUILTIN");
      }
    });

    it("should provide non-empty descriptions for all builtin tools", () => {
      const tools = createBuiltinTools();

      for (const tool of tools) {
        expect(tool.description).toBeTruthy();
        expect(typeof tool.description).toBe("string");
        expect(tool.description.length).toBeGreaterThan(10);
      }
    });

    it("should provide parameter schemas for all builtin tools", () => {
      const tools = createBuiltinTools();

      for (const tool of tools) {
        expect(tool.parameters).toBeDefined();
      }
    });

    it("should provide execute handlers for all builtin tools", () => {
      const tools = createBuiltinTools();

      for (const tool of tools) {
        expect(tool.config?.execute).toBeDefined();
        expect(typeof tool.config!.execute).toBe("function");
      }
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: allowList — only enable specific tools
  // ---------------------------------------------------------------------------
  describe("allowList filtering", () => {
    it("should create only the tools in the allowList", () => {
      const options: BuiltinToolsOptions = {
        allowList: ["execute_workflow", "query_workflow_status"],
      };
      const tools = createBuiltinTools(options);

      expect(tools).toHaveLength(2);
      const toolIds = tools.map((t) => t.id);
      expect(toolIds).toContain("builtin_execute_workflow");
      expect(toolIds).toContain("builtin_query_workflow_status");
      expect(toolIds).not.toContain("builtin_cancel_workflow");
    });

    it("should create all tools when allowList is empty (no filtering)", () => {
      const tools = createBuiltinTools({ allowList: [] });

      expect(tools).toHaveLength(BUILTIN_TOOL_COUNT);
    });

    it("should create only the ask_followup_question tool when specified in allowList", () => {
      const tools = createBuiltinTools({ allowList: ["ask_followup_question"] });

      expect(tools).toHaveLength(1);
      expect(tools[0].id).toBe("ask_followup_question");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: blockList — disable specific tools
  // ---------------------------------------------------------------------------
  describe("blockList filtering", () => {
    it("should skip tools in the blockList", () => {
      const options: BuiltinToolsOptions = {
        blockList: ["call_agent", "ask_followup_question"],
      };
      const tools = createBuiltinTools(options);

      expect(tools).toHaveLength(BUILTIN_TOOL_COUNT - 2);
      const toolIds = tools.map((t) => t.id);
      expect(toolIds).not.toContain("builtin_call_agent");
      expect(toolIds).not.toContain("ask_followup_question");
      expect(toolIds).toContain("builtin_execute_workflow");
      expect(toolIds).toContain("attempt_completion");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: allowList + blockList interaction (allowList wins)
  // ---------------------------------------------------------------------------
  describe("allowList + blockList interaction", () => {
    it("should give priority to allowList when both are set", () => {
      const options: BuiltinToolsOptions = {
        allowList: ["execute_workflow", "call_agent"],
        blockList: ["execute_workflow"], // ignored when allowList is set
      };
      const tools = createBuiltinTools(options);

      // allowList wins, so only execute_workflow and call_agent are created
      expect(tools).toHaveLength(2);
      const toolIds = tools.map((t) => t.id);
      expect(toolIds).toContain("builtin_execute_workflow");
      expect(toolIds).toContain("builtin_call_agent");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Workflow tool description generation
  // ---------------------------------------------------------------------------
  describe("workflow tool description generation", () => {
    it("should include workflow list in description when workflow config is provided", () => {
      const options: BuiltinToolsOptions = {
        workflow: {
          loader: {
            getAvailableWorkflows: () => [
              { id: "wf1", name: "Workflow 1" },
              { id: "wf2", name: "Workflow 2" },
            ],
          },
        },
      };
      const tools = createBuiltinTools(options);

      const executeTool = tools.find((t) => t.id === "builtin_execute_workflow")!;
      expect(executeTool.description).toContain("wf1");
      expect(executeTool.description).toContain("wf2");
    });

    it("should use default description when no workflow config is provided", () => {
      const tools = createBuiltinTools();

      const executeTool = tools.find((t) => t.id === "builtin_execute_workflow")!;
      expect(executeTool.description).toBeTruthy();
      // Default description should not contain specific workflow IDs
      expect(executeTool.description).not.toContain("wf1");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Agent tool description generation
  // ---------------------------------------------------------------------------
  describe("agent tool description generation", () => {
    it("should include agent profile list in description when agent config is provided", () => {
      const options: BuiltinToolsOptions = {
        agent: {
          loader: {
            getAvailableAgentProfiles: () => [
              { id: "agent1", name: "Agent 1" },
            ],
          },
        },
      };
      const tools = createBuiltinTools(options);

      const callAgentTool = tools.find((t) => t.id === "builtin_call_agent")!;
      expect(callAgentTool.description).toContain("agent1");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: ask_followup_question has interaction metadata
  // ---------------------------------------------------------------------------
  describe("ask_followup_question metadata", () => {
    it("should have interaction metadata set", () => {
      const tools = createBuiltinTools();

      const askTool = tools.find((t) => t.id === "ask_followup_question")!;
      expect(askTool.metadata).toBeDefined();
      expect(askTool.metadata!.category).toBe("interaction");
      expect(askTool.metadata!.requiresUserInteraction).toBe(true);
      expect(askTool.metadata!.interactionType).toBe("ASK_FOLLOWUP_QUESTION");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: createAllPredefinedTools — returns all tools
  // ---------------------------------------------------------------------------
  describe("createAllPredefinedTools", () => {
    let tmpDir: string;

    afterEach(() => {
      cleanupTempDirs();
    });

    it("should return tools including both stateless/stateful and builtin tools", () => {
      tmpDir = createTempDir();
      const tools = createAllPredefinedTools({
        config: {
          sessionNote: { workspaceDir: tmpDir, dbPath: join(tmpDir, "test.db"), sessionId: "test", maxNotes: 100 },
        },
      });

      // Should include all predefined stateless/stateful tools AND builtin tools
      const toolIds = tools.map((t) => t.id);
      expect(toolIds).toContain("read_file");
      expect(toolIds).toContain("write_file");
      expect(toolIds).toContain("builtin_execute_workflow");
      expect(toolIds).toContain("ask_followup_question");
      expect(toolIds).toContain("attempt_completion");
    });

    it("should have Tool[] type for all returned tools", () => {
      tmpDir = createTempDir();
      const tools = createAllPredefinedTools({
        config: {
          sessionNote: { workspaceDir: tmpDir, dbPath: join(tmpDir, "test.db"), sessionId: "test", maxNotes: 100 },
        },
      });

      for (const tool of tools) {
        expect(tool.type).toBeDefined();
        expect(tool.id).toBeDefined();
        expect(tool.description).toBeDefined();
      }
    });
  });
});