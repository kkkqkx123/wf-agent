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
    }, `delete agent loop ${id}`);
  }

  async run(config: Record<string, any>): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("run", { config });
      const coordinator = this.getCoordinator();
      const result = await coordinator.start(config);
      return { id: (result as any)?.id || "unknown", status: "running", config } as any;
    }, "run agent loop");
  }

  async stop(id: ID): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("stop", { id });
      const coordinator = this.getCoordinator();
      await coordinator.stop(id);
    }, `stop agent loop ${id}`);
  }

  async createCheckpoint(agentLoopId: ID, name?: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("createCheckpoint", { agentLoopId, name });
      const coordinator = this.getCoordinator() as any;
      const checkpoint = await coordinator.createCheckpoint(agentLoopId, { name });
      return checkpoint as any;
    }, `create checkpoint for agent loop ${agentLoopId}`);
  }

  async listCheckpoints(agentLoopId: ID): Promise<Record<string, any>[]> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("listCheckpoints", { agentLoopId });
      const coordinator = this.getCoordinator() as any;
      const checkpoints = await coordinator.getCheckpoints(agentLoopId);
      return (checkpoints || []) as any[];
    }, `list checkpoints for agent loop ${agentLoopId}`);
  }

  async loadCheckpoint(checkpointId: string): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("loadCheckpoint", { checkpointId });
      const coordinator = this.getCoordinator() as any;
      await coordinator.restoreCheckpoint(checkpointId);
    }, `load checkpoint ${checkpointId}`);
  }
}