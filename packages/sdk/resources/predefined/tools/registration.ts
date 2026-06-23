/**
 * Predefined Tool Registration Entrance
 *
 * Responsible for the registration and deregistration of predefined tools.
 */

import type { ToolRegistry } from "@sdk/shared/registry/tool-registry.js";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";
import { createPredefinedTools } from "./registry.js";
import { toSdkTool } from "@sdk/services/tools/utils.js";

const logger = createContextualLogger({ component: "PredefinedTools" });

/**
 * Register a predefined tool
 *
 * @param toolService: The tool service
 * @param options: Configuration options
 * @param skipIfExists: Whether to skip the registration if the tool already exists (instead of reporting an error)
 * @returns: The registration result
 */
export function registerPredefinedTools(
  toolService: ToolRegistry,
  options?: {
    /** Enable only the specified tools (allowlist). */
    allowList?: string[];
    /** Disable the specified tool (blocklist it). */
    blockList?: string[];
    /** Tool-specific configuration */
    config?: {
      readFile?: { workspaceDir?: string; maxFileSize?: number };
      writeFile?: { workspaceDir?: string };
      editFile?: { workspaceDir?: string };
      runShell?: {
        defaultTimeout?: number;
        maxTimeout?: number;
        shellPolicy?: import("@wf-agent/types").ShellPolicy;
      };
      sessionNote?: { workspaceDir?: string; memoryFile?: string };
      backendShell?: {
        workspaceDir?: string;
        maxBackgroundTimeout?: number;
        shellPolicy?: import("@wf-agent/types").ShellPolicy;
      };
    };
  },
  skipIfExists: boolean = true,
): {
  success: string[];
  failures: Array<{ toolId: string; error: string }>;
} {
  const success: string[] = [];
  const failures: Array<{ toolId: string; error: string }> = [];

  try {
    // Create a predefined tool
    const tools = createPredefinedTools(options);

    // Register with the tool service.
    for (const tool of tools) {
      try {
        // Check if it already exists.
        if (skipIfExists && toolService.has(tool.id)) {
          logger.info(`Tool already registered, skipping: ${tool.id}`);
          continue;
        }

        // Convert to SDK format and register (memory-only, no persistence needed for predefined).
        const sdkTool = toSdkTool(tool);
        toolService.register(sdkTool);
        success.push(tool.id);
        logger.info(`Registered predefined tool: ${tool.id}`);
      } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error);
        failures.push({ toolId: tool.id, error: errorMsg });
        logger.error(`Failed to register predefined tool: ${tool.id}`, { error: errorMsg });
      }
    }

    logger.info(
      `Predefined tools registration completed: ${success.length} succeeded, ${failures.length} failed`,
    );
  } catch (error) {
    logger.error(`Failed to create predefined tools`, { error });
  }

  return { success, failures };
}

/**
 * Cancel predefined tools
 *
 * @param toolService: Tool service
 * @param toolIds: List of tool IDs to be canceled; if empty, all predefined tools will be canceled
 * @returns: Cancellation result
 */
export async function unregisterPredefinedTools(
  toolService: ToolRegistry,
  toolIds?: string[],
): Promise<{
  success: string[];
  failures: Array<{ toolId: string; error: string }>;
}> {
  const success: string[] = [];
  const failures: Array<{ toolId: string; error: string }> = [];

  // If no tool ID is specified, retrieve all predefined tool IDs.
  const predefinedToolIds = toolIds || [
    "read_file",
    "write_file",
    "edit_file",
    "run_shell",
    "record_note",
    "recall_notes",
    "backend_shell",
    "shell_output",
    "shell_kill",
  ];

  for (const toolId of predefinedToolIds) {
    try {
      await toolService.unregisterTool(toolId);
      success.push(toolId);
      logger.info(`Unregistered predefined tool: ${toolId}`);
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      failures.push({ toolId, error: errorMsg });
      logger.error(`Failed to unregister predefined tool: ${toolId}`, { error: errorMsg });
    }
  }

  logger.info(
    `Predefined tools unregistration completed: ${success.length} succeeded, ${failures.length} failed`,
  );
  return { success, failures };
}

/**
 * Check if the predefined tools have been registered.
 */
export function isPredefinedToolRegistered(toolService: ToolRegistry, toolId: string): boolean {
  return toolService.has(toolId);
}
