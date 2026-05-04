/**
 * Call Agent Tool Handler
 */

import type { BuiltinToolExecutionContext } from "@wf-agent/types";
import { getContainer } from "../../../../../../core/di/index.js";
import * as Identifiers from "../../../../../../core/di/service-identifiers.js";
import { RuntimeValidationError } from "@wf-agent/types";
import type { AgentLoopCoordinator } from "../../../../../../agent/execution/coordinators/agent-loop-coordinator.js";

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
    const container = getContainer();
    const coordinator = container.get(Identifiers.AgentLoopCoordinator) as {
      create: () => AgentLoopCoordinator;
    };

    if (!coordinator) {
      throw new RuntimeValidationError("AgentLoopCoordinator not available in DI container", {
        operation: "call_agent",
      });
    }

    const agentCoordinator = coordinator.create();

    try {
      // In a real implementation, we would fetch the profile configuration using agentProfileId.
      // For now, we use a basic config.
      const result = await agentCoordinator.execute(
        {
          profileId: agentProfileId,
          systemPrompt: `You are a specialized subagent with ID: ${agentProfileId}.`,
          initialMessages: [{ role: "user" as const, content: prompt }],
          tools: [], // Tools would be determined by the profile
          maxIterations: 10,
        },
        {
          parentExecutionId: context.executionId,
          initialVariables: input,
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
