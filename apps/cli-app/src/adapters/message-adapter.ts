/**
 * Message Adapter
 * Encapsulates SDK API calls related to messages
 */

import { BaseAdapter } from "./base-adapter.js";
import type { LLMMessage } from "@wf-agent/types";

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
      const messages = (result as any).data || result;
      return messages as LLMMessage[];
    }, "List Messages");
  }

  /**
   * Get message details
   */
  async getMessage(id: string): Promise<LLMMessage> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.messages;
      const result = await api.get(id);
      const message = (result as any).data || result;
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
      const messages = (result as any).data || result;
      return messages as LLMMessage[];
    }, "List Execution Messages");
  }

  /**
   * Get message statistics
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
      if (agentLoopId) {
        const result = await (api as any).getMessageStats(agentLoopId);
        return {
          total: result.total,
          byRole: result.byRole,
          totalTokenUsage: result.totalTokenUsage,
        };
      } else {
        const result = await api.getGlobalMessageStats();
        return {
          total: result.total,
          byRole: result.byRole,
        };
      }
    }, "Get message statistics");
  }

  /**
   * Normalize message history
   */
  async normalizeMessages(agentLoopId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.messages;
      await (api as any).normalizeHistory(agentLoopId);
      this.output.infoLog(`Message history normalized for Agent Loop: ${agentLoopId}`);
    }, "Normalize Messages");
  }
}
