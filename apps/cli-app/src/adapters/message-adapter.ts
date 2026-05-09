/**
 * Message Adapter
 * Encapsulates SDK API calls related to messages
 */

import { BaseAdapter } from "./base-adapter.js";
import type { LLMMessage } from "@wf-agent/types";
import { getData, isFailure, getError } from "@wf-agent/sdk";
import { CLINotFoundError } from "../types/cli-types.js";

/**
 * Message Filter
 */
interface MessageFilter {
  /** Execution ID */
  executionId?: string;
  /** Role Filtering */
  role?: string;
  /** Content Keywords */
  content?: string;
  /** Timeframe start */
  startTimeFrom?: number;
  /** End of timeframe */
  startTimeTo?: number;
}

/**
 * Message Adapter
 */
export class MessageAdapter extends BaseAdapter {
  /**
   * List all messages
   */
  async listMessages(filter?: MessageFilter): Promise<LLMMessage[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.messages;
      const result = await api.getAll(filter);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      return getData(result) as LLMMessage[];
    }, "List Messages");
  }

  /**
   * Get message details
   */
  async getMessage(id: string): Promise<LLMMessage> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.messages;
      const result = await api.get(id);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      const message = getData(result);
      if (!message) {
        throw new CLINotFoundError(`Message not found: ${id}`, "Message", id);
      }
      
      return message as LLMMessage;
    }, "Get Message");
  }

  /**
   * List messages by execution ID
   */
  async listMessagesByExecution(executionId: string): Promise<LLMMessage[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.messages;
      const result = await api.getAll({ executionId });
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      return getData(result) as LLMMessage[];
    }, "List Execution Messages");
  }

  /**
   * Get message statistics
   * Note: This method currently only supports global statistics.
   * For agent loop specific stats, use the Agent Loop API directly.
   */
  async getMessageStats(agentLoopId?: string): Promise<{
    total: number;
    byRole: Record<string, number>;
    totalTokenUsage?: {
      promptTokens: number;
      completionTokens: number;
      totalTokens: number;
    };
  }> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.messages;
      
      // Currently only global stats are supported in Workflow MessageResourceAPI
      const result = await api.getGlobalMessageStats();
      return {
        total: result.total,
        byRole: result.byRole,
      };
    }, "Get message statistics");
  }

  /**
   * Normalize message history
   * Note: This feature is specific to Agent Loops and not available for Workflow executions.
   * To normalize agent loop messages, use the Agent Loop API directly.
   * @deprecated This method will be removed in a future version
   */
  async normalizeMessages(agentLoopId: string): Promise<void> {
    // This functionality is not available in Workflow MessageResourceAPI
    // It's only available in AgentLoopMessageResourceAPI
    throw new Error(
      "normalizeMessages is not supported for workflow executions. " +
      "This feature is only available for Agent Loops."
    );
  }
}
