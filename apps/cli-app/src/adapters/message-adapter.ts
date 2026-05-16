/**
 * Message Adapter
 * Encapsulates SDK API calls related to messages
 */

import { BaseAdapter } from "./base-adapter.js";
import type { LLMMessage } from "@wf-agent/types";
import { getData, isFailure, getError } from "@wf-agent/sdk/api";
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
   * Get message statistics for a specific workflow execution
   * @param executionId Workflow execution ID
   */
  async getMessageStats(executionId: string): Promise<{
    total: number;
    byRole: Record<string, number>;
    byType: Record<string, number>;
  }> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.messages;
      const result = await api.getMessageStats(executionId);
      return result;
    }, "Get message statistics");
  }

  /**
   * Get global message statistics across all executions (for debugging/audit)
   * Note: This is primarily for system-level monitoring, not typical user operations.
   */
  async getGlobalMessageStats(): Promise<{
    total: number;
    byExecution: Record<string, number>;
    byRole: Record<string, number>;
  }> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.messages;
      const result = await api.getGlobalMessageStats();
      return result;
    }, "Get global message statistics");
  }
}
