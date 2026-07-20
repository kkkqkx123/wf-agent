/**
 * Message Adapter
 * Manage messages via the SDK MessageResourceAPI.
 */

import { BaseAdapter } from "./base-adapter.js";
import { findByIdOrThrow } from "@wf-agent/runtime/adapters";

export interface MessageFilter {
  executionId?: string;
  role?: string;
  content?: string;
}

export class MessageAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "Message";
  }

  async listMessages(filter?: MessageFilter): Promise<Record<string, any>[]> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("listMessages", filter);
      const api = this.sdk.messages;
      const messages = await api.getAll(filter as any);
      return messages.map((m: any) => ({
        id: m.id,
        role: m.role,
        executionId: m.executionId,
        content: m.content?.substring(0, 200),
        timestamp: m.timestamp,
      }));
    }, "List messages");
  }

  async getMessage(id: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getMessage", { id });
      if (!id || id.trim().length === 0) throw new Error("Message ID is required");
      const message = await findByIdOrThrow(this.sdk.messages, id, "Message");
      return message as any;
    }, "Get message");
  }

  async listMessagesByExecution(executionId: string): Promise<Record<string, any>[]> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("listMessagesByExecution", { executionId });
      const api = this.sdk.messages;
      const messages = await api.getAll({ executionId } as any);
      return messages.map((m: any) => ({
        id: m.id,
        role: m.role,
        executionId: m.executionId,
        content: m.content?.substring(0, 200),
        timestamp: m.timestamp,
      }));
    }, "List messages by execution");
  }

  async getMessageStats(executionId: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getMessageStats", { executionId });
      const api = this.sdk.messages;
      const messages = await api.getAll({ executionId } as any);
      const byRole: Record<string, number> = {};
      const byType: Record<string, number> = {};
      for (const m of messages as any[]) {
        byRole[m.role] = (byRole[m.role] || 0) + 1;
        byType[m.type || "unknown"] = (byType[m.type || "unknown"] || 0) + 1;
      }
      return { total: messages.length, byRole, byType };
    }, "Get message stats");
  }

  async getGlobalMessageStats(): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getGlobalMessageStats");
      const api = this.sdk.messages;
      const messages = await api.getAll();
      const byExecution: Record<string, number> = {};
      const byRole: Record<string, number> = {};
      for (const m of messages as any[]) {
        const execId = m.executionId || "unknown";
        byExecution[execId] = (byExecution[execId] || 0) + 1;
        byRole[m.role] = (byRole[m.role] || 0) + 1;
      }
      return { total: messages.length, byExecution, byRole };
    }, "Get global message stats");
  }

  async compress(executionId: string, strategy?: string, keepRecent?: number): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("compress", { executionId, strategy, keepRecent });
      if (!executionId || executionId.trim().length === 0) throw new Error("Execution ID is required");
      const s = strategy || "TRUNCATE";
      const k = keepRecent ?? 10;
      // Dispatch a context compression event through the SDK
      const eventsApi = this.sdk.events as any;
      await eventsApi.emit({
        type: "CONTEXT_COMPRESSION_REQUESTED",
        executionId,
        timestamp: Date.now(),
        data: { strategy: s, keepRecent: k },
      } as any);
      return { executionId, strategy: s, keepRecent: k, status: "compression_requested" };
    }, "Compress messages");
  }
}