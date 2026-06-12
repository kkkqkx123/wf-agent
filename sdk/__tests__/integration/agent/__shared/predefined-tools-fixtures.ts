/**
 * Predefined Tools Integration Test Fixtures
 *
 * Extends the base agent loop fixture with real predefined tools from
 * sdk/resources/predefined/tools. These fixtures register actual tool
 * implementations (filesystem read/glob/list/grep etc.) into the
 * ToolRegistry so the agent loop can execute them with real side effects.
 *
 * Usage: tests use MockLLMWrapper to instruct "which tool to call" while
 * the actual tool handler executes against the real filesystem.
 */

import { createPredefinedTools } from "@/resources/predefined/tools/registry.js";
import { toSdkTool } from "@sdk/services/executors/tools/utils.js";
import type { Tool } from "@wf-agent/types";
import type { PredefinedToolsOptions } from "@/resources/predefined/tools/types.js";
import {
  createFullAgentLoopFixture,
  createBasicAgentConfig,
  type FullAgentLoopTestFixture,
} from "./fixtures.js";
import type { AgentLoopRuntimeConfig } from "@wf-agent/types";

// =============================================================================
// Constants
// =============================================================================

/**
 * Default set of safe, read-only tools to register for testing.
 * These tools have no destructive side effects and are suitable
 * for automated integration tests.
 */
export const DEFAULT_SAFE_TOOLS = ["list_files", "glob", "read_file", "grep"] as const;

export type SafeToolId = (typeof DEFAULT_SAFE_TOOLS)[number];

// =============================================================================
// Fixture Factory
// =============================================================================

/**
 * Create a full agent loop fixture with real predefined tools registered.
 *
 * @param toolIds - Array of tool IDs to register (defaults to DEFAULT_SAFE_TOOLS)
 * @param workspaceDir - Workspace directory for file tools (defaults to process.cwd())
 * @returns FullAgentLoopTestFixture with real tools registered
 */
export async function createPredefinedToolsFixture(
  toolIds?: string[],
  workspaceDir?: string,
): Promise<FullAgentLoopTestFixture> {
  // 1. Create base fixture without the mock echo tool
  const fixture = await createFullAgentLoopFixture(false);

  // 2. Create predefined tool definitions (only the ones we want)
  const allowedTools = toolIds ?? [...DEFAULT_SAFE_TOOLS];
  const wsDir = workspaceDir ?? process.cwd();

  const options: PredefinedToolsOptions = {
    allowList: allowedTools,
    config: {
      readFile: { workspaceDir: wsDir },
    },
  };

  const toolDefs = createPredefinedTools(options);

  // 3. Convert to SDK Tool format and register
  for (const def of toolDefs) {
    const sdkTool: Tool = toSdkTool(def);
    fixture.toolRegistry.register(sdkTool, { skipIfExists: true });
  }

  return fixture;
}

/**
 * Create an agent loop runtime config pre-configured with tool availability.
 *
 * @param toolIds - Tool IDs to make available to the agent
 * @param overrides - Additional config overrides
 * @returns AgentLoopRuntimeConfig
 */
export function createAgentConfigWithTools(
  toolIds: string[],
  overrides?: Partial<AgentLoopRuntimeConfig>,
): AgentLoopRuntimeConfig {
  return createBasicAgentConfig({
    availableTools: {
      tools: toolIds,
    },
    maxIterations: 5,
    ...overrides,
  });
}
