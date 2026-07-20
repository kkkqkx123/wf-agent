/**
 * Agent Loop Adapter
 * Manage agent loops via the SDK.
 */

import { BaseAdapter, type QueryOptions, type PaginatedResponse } from "./base-adapter.js";
import type { ID } from "@wf-agent/types";

export class AgentLoopAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "AgentLoop";
  }

  private getDeps() {
    return this.sdk.getFactory().getDependencies();
  }

  private getRegistry() {
    return this.getDeps().getAgentLoopRegistry();
  }

  private getCoordinator() {
    return this.getDeps().getAgentLoopCoordinator();
  }

  async list(query?: QueryOptions): Promise<PaginatedResponse<Record<string, any>>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("list", query);
      const registry = this.getRegistry();
      const allLoops = await registry.getAll() as any[];
      const items = allLoops.map((loop: any) => ({
        id: loop.id,
        name: loop.name || loop.id,
        status: loop.status || "unknown",
        createdAt: loop.createdAt,
      }));
      return this.applyPagination(items, query);
    }, "list agent loops");
  }

  async get(id: ID): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("get", { id });
      const registry = this.getRegistry();
      const loop = await registry.get(id) as any;
      if (!loop) {
        throw new Error(`Agent loop not found: ${id}`);
      }
      return {
        id: loop.id,
        name: loop.name || loop.id,
        status: loop.status || "unknown",
        createdAt: loop.createdAt,
        config: loop.config,
      };
    }, `get agent loop ${id}`);
  }

  async delete(id: ID): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("delete", { id });
      const coordinator = this.getCoordinator();
      await coordinator.stop(id);
      // Registry cleanup is handled by the coordinator
    }, `delete agent loop ${id}`);
  }
}