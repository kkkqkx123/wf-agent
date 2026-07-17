/**
 * Resources Integration Test Fixtures
 *
 * Provides factory functions for creating mock registries, test data,
 * and temporary JSON files for resources integration testing.
 */

import { ToolRegistry } from "@/shared/registry/tool-registry.js";
import { FragmentRegistry } from "@/shared/registry/fragment-registry.js";
import { PromptTemplateRegistry } from "@/shared/registry/prompt-template-registry.js";
import { toolDescriptionRegistry } from "@/shared/tools/tool-description-registry.js";
import type { ToolDescriptionRegistry } from "@/shared/tools/tool-description-registry.js";
import type { ResourceRegistries } from "@/resources/registration/types.js";
import type { PresetsConfig, CustomResources } from "@wf-agent/sdk/resources";
import type { CustomToolDefinition, CustomTriggerDefinition, CustomPromptDefinition } from "@/resources/custom/types.js";
import { mkdtempSync, writeFileSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";

// =============================================================================
// Registry Factory
// =============================================================================

/**
 * Create mock registries for resource registration testing.
 * Uses real registry implementations for maximum fidelity.
 */
export function createMockRegistries(): ResourceRegistries {
  const { TriggerTemplateRegistry } = require("@sdk/shared/registry/trigger-template-registry.js");
  const { WorkflowRegistry } = require("@sdk/workflow/registry/workflow-registry.js");
  const { NodeTemplateRegistry } = require("@sdk/shared/registry/node-template-registry.js");
  const { HookTemplateRegistry } = require("@sdk/shared/registry/hook-template-registry.js");
  const { AgentLoopRegistry } = require("@sdk/agent/registry/agent-loop-registry.js");

  return {
    triggerRegistry: new TriggerTemplateRegistry(),
    workflowRegistry: new WorkflowRegistry(),
    toolRegistry: new ToolRegistry(),
    promptTemplateRegistry: new PromptTemplateRegistry(),
    fragmentRegistry: new FragmentRegistry(),
    toolDescriptionRegistry: toolDescriptionRegistry as unknown as ToolDescriptionRegistry,
    nodeTemplateRegistry: new NodeTemplateRegistry(),
    hookTemplateRegistry: new HookTemplateRegistry(),
    agentLoopRegistry: new AgentLoopRegistry(),
  };
}

// =============================================================================
// Presets Config Factories
// =============================================================================

/**
 * Create a default presets config with all resources enabled.
 */
export function createDefaultPresetsConfig(): PresetsConfig {
  return {
    contextCompression: { enabled: true },
    predefinedTools: { enabled: true },
    predefinedPrompts: { enabled: true },
    predefinedToolDescriptions: { enabled: true },
  };
}

/**
 * Create a presets config with all resources disabled.
 */
export function createDisabledPresetsConfig(): PresetsConfig {
  return {
    contextCompression: { enabled: false },
    predefinedTools: { enabled: false },
    predefinedPrompts: { enabled: false },
    predefinedToolDescriptions: { enabled: false },
  };
}

// =============================================================================
// Custom Resources Factories
// =============================================================================

/**
 * Create a valid custom tool definition for testing.
 */
export function createTestCustomTool(id: string = "test_custom_tool"): CustomToolDefinition {
  return {
    id,
    type: "STATELESS" as const,
    description: {
      summary: "A test custom tool",
      details: "Used for integration testing",
      examples: ["test example"],
    },
    schema: {
      type: "object",
      properties: {
        input: { type: "string", description: "Input parameter" },
      },
      required: ["input"],
    },
    handler: {
      type: "inline",
      code: "export default async (params) => ({ result: 'ok' })",
    },
  };
}

/**
 * Create a valid custom trigger definition for testing.
 */
export function createTestCustomTrigger(name: string = "test_custom_trigger"): CustomTriggerDefinition {
  return {
    name,
    description: "A test custom trigger",
    condition: {
      type: "event" as const,
      value: "TEST_EVENT",
    },
  };
}

/**
 * Create a valid custom prompt definition for testing.
 */
export function createTestCustomPrompt(id: string = "test_custom_prompt"): CustomPromptDefinition {
  return {
    id,
    name: "Test Custom Prompt",
    content: "You are a test assistant.",
    type: "system" as const,
    variables: ["language"],
  };
}

// =============================================================================
// Temporary File Helpers
// =============================================================================

let tempDirs: string[] = [];

/**
 * Create a temporary directory for test file operations.
 * Automatically cleaned up on afterEach via cleanupTempDirs().
 */
export function createTempDir(): string {
  const dir = mkdtempSync(join(tmpdir(), "resources-test-"));
  tempDirs.push(dir);
  return dir;
}

/**
 * Write a JSON file for custom resource loading tests.
 */
export function writeCustomResourcesJson(
  dir: string,
  filename: string,
  data: Record<string, unknown>,
): string {
  const filePath = join(dir, filename);
  writeFileSync(filePath, JSON.stringify(data, null, 2), "utf-8");
  return filePath;
}

/**
 * Clean up all temporary directories created during tests.
 */
export function cleanupTempDirs(): void {
  for (const dir of tempDirs) {
    try {
      rmSync(dir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup errors
    }
  }
  tempDirs = [];
}