/**
 * Call Agent Tool Handler
 */

import type { BuiltinToolExecutionContext } from "@wf-agent/types";
import type { AgentLoopRuntimeConfig } from "@wf-agent/types";
import * as Identifiers from "@sdk/core/di/service-identifiers.js";
import { RuntimeValidationError, ConfigurationError } from "@wf-agent/types";
import type { ToolRegistry } from "@sdk/core/registry/tool-registry.js";
import { resolveSystemPrompt } from "@sdk/core/prompt/system-prompt-resolver.js";
import { transformToAgentLoopConfig } from "@sdk/api/shared/config/processors/agent-loop.js";
import { existsSync } from "fs";
import { loadAgentLoopConfig } from "@sdk/api/shared/config/utils/config-utils.js";
import type { ServiceFactory } from "@sdk/core/di/factory-types.js";
import type { AgentLoopCoordinator } from "@sdk/agent/execution/coordinators/agent-loop-coordinator.js";

export interface CallAgentParams {
  agentProfileId: string;
  prompt: string;
  input?: Record<string, unknown>;
  waitForCompletion?: boolean;
}

export interface CallAgentResult {
  success: boolean;
  content?: string;
  agentLoopId?: string;
  error?: Error;
  executionTime?: number;
}

/**
 * Create call agent handler
 */
export function createCallAgentHandler() {
  return async (
    params: Record<string, unknown>,
    context: BuiltinToolExecutionContext,
  ): Promise<CallAgentResult> => {
    const typedParams = params as unknown as CallAgentParams;
    const { agentProfileId, prompt, input = {} } = typedParams;

    if (!agentProfileId || !prompt) {
      throw new RuntimeValidationError("agentProfileId and prompt are required for call_agent", {
        operation: "call_agent",
        field: "params",
        value: params,
      });
    }

    const startTime = Date.now();
    
    // Get GlobalContext from execution context
    // The context object may have a globalContext property or be the GlobalContext itself
    const globalContext = (context as unknown as { globalContext?: import("@sdk/core/global-context.js").GlobalContext }).globalContext;
    if (!globalContext) {
      throw new RuntimeValidationError("GlobalContext not available in execution context", {
        operation: "call_agent",
      });
    }
    
    const coordinatorFactory = globalContext.container.get(Identifiers.AgentLoopCoordinator);

    if (!coordinatorFactory) {
      throw new RuntimeValidationError("AgentLoopCoordinator not available in DI container", {
        operation: "call_agent",
      });
    }

    const agentLoopCoordinator = (coordinatorFactory as unknown as ServiceFactory<AgentLoopCoordinator, []>).create();

    try {
      // Load agent profile configuration
      let runtimeConfig: AgentLoopRuntimeConfig;
      
      // Check if agentProfileId is a file path
      if (existsSync(agentProfileId)) {
        // Load from configuration file
        try {
          const parsedConfig = await loadAgentLoopConfig(agentProfileId);
          runtimeConfig = transformToAgentLoopConfig(parsedConfig.config);
        } catch (error) {
          throw new ConfigurationError(
            `Failed to load agent profile configuration from ${agentProfileId}`,
            undefined,
            { originalError: error instanceof Error ? error.message : String(error) },
          );
        }
      } else {
        // Treat as inline configuration or profile ID
        // Construct a minimal config with the provided parameters
        runtimeConfig = {
          profileId: agentProfileId,
          systemPrompt: prompt,
          initialMessages: [],
          availableTools: { tools: [] },
          maxIterations: 10,
        };
      }

      // Resolve system prompt (supports template rendering)
      const resolvedSystemPrompt = resolveSystemPrompt({
        systemPrompt: runtimeConfig.systemPrompt,
        systemPromptTemplateId: runtimeConfig.systemPromptTemplateId,
        systemPromptTemplateVariables: {
          ...runtimeConfig.systemPromptTemplateVariables,
          input,
        },
      });

      // Validate and filter tools from ToolRegistry
      const toolService = globalContext.container.get(Identifiers.ToolRegistry) as ToolRegistry | undefined;
      let validTools: string[] = [];
      
      if (runtimeConfig.availableTools?.tools && runtimeConfig.availableTools.tools.length > 0) {
        if (!toolService) {
          throw new ConfigurationError("ToolRegistry not available for tool validation");
        }
        
        // Filter out invalid tool IDs/names
        // Note: getTool() now supports both ID and name lookup for LLM compatibility
        validTools = runtimeConfig.availableTools.tools.filter((toolId: string) => {
          try {
            toolService.getTool(toolId);
            return true;
          } catch {
            // Tool not found, skip it
            return false;
          }
        });
        
        if (validTools.length === 0 && runtimeConfig.availableTools.tools.length > 0) {
          // All tools were invalid, use empty array
          validTools = [];
        }
      }

      const result = await agentLoopCoordinator.execute(
        {
          profileId: runtimeConfig.profileId || agentProfileId,
          systemPrompt: resolvedSystemPrompt,
          systemPromptTemplateId: runtimeConfig.systemPromptTemplateId,
          systemPromptTemplateVariables: runtimeConfig.systemPromptTemplateVariables,
          initialMessages: [{ role: "user" as const, content: prompt }],
          availableTools: { tools: validTools },
          maxIterations: runtimeConfig.maxIterations || 10,
        },
        {
          parentExecutionId: context.executionId,
        },
      );

      return {
        success: result.success,
        content: typeof result.content === "string" ? result.content : undefined,
        agentLoopId: result.agentLoopId,
        error: result.error instanceof Error ? result.error : new Error(String(result.error)),
        executionTime: Date.now() - startTime,
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error : new Error(String(error)),
        executionTime: Date.now() - startTime,
      };
    }
  };
}
