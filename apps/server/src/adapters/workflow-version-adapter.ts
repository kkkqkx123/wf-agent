/**
 * Workflow Version Adapter
 * Manage and compare workflow versions via the storage adapter.
 */

import { BaseAdapter } from "./base-adapter.js";
import { findByIdOrThrow } from "@wf-agent/runtime/adapters";
import type { ID } from "@wf-agent/types";
import type { WorkflowVersionInfo } from "@wf-agent/types";

export interface WorkflowVersion {
  id: ID;
  workflowId: ID;
  version: string;
  name: string;
  description?: string;
  nodeCount: number;
  edgeCount: number;
  createdAt: number;
  updatedAt: number;
  author?: string;
  tags?: string[];
  category?: string;
}

export interface VersionDiff {
  workflowId: ID;
  version1: string;
  version2: string;
  nodeCountDiff: number;
  edgeCountDiff: number;
  nodesAdded: string[];
  nodesRemoved: string[];
  edgesChanged: number;
  description: string;
}

export class WorkflowVersionAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "WorkflowVersion";
  }

  /**
   * List all versions of a workflow
   */
  async listVersions(workflowId: ID): Promise<WorkflowVersion[]> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("listVersions", { workflowId });
      // Verify workflow exists
      await findByIdOrThrow(this.sdk.workflows, workflowId as string, "Workflow");

      const deps = this.sdk.getFactory()?.getDependencies();
      const storageAdapter = deps?.getWorkflowStorageAdapter();
      if (!storageAdapter) {
        return [];
      }

      const versionInfos: WorkflowVersionInfo[] = await storageAdapter.listWorkflowVersions(workflowId);

      const versions: WorkflowVersion[] = versionInfos.map((info) => ({
        id: `${workflowId}@${info.version}` as ID,
        workflowId,
        version: info.version,
        name: info.version,
        description: info.changeNote,
        nodeCount: 0,
        edgeCount: 0,
        createdAt: info.createdAt,
        updatedAt: info.createdAt,
        author: info.createdBy,
      }));

      this.logOperation(`Retrieved ${versions.length} version(s) for workflow: ${workflowId}`);
      return versions;
    }, "List workflow versions");
  }

  /**
   * Get a specific version
   */
  async getVersion(workflowId: ID, version: string): Promise<WorkflowVersion> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getVersion", { workflowId, version });
      const versions = await this.listVersions(workflowId);
      const foundVersion = versions.find((v) => v.version === version);
      if (!foundVersion) {
        throw new Error(`Version ${version} not found for workflow ${workflowId}`);
      }
      return foundVersion;
    }, "Get workflow version");
  }

  /**
   * Compare two versions
   */
  async compareVersions(workflowId: ID, version1: string, version2: string): Promise<VersionDiff> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("compareVersions", { workflowId, version1, version2 });
      const v1 = await this.getVersion(workflowId, version1);
      const v2 = await this.getVersion(workflowId, version2);

      const nodeCountDiff = v2.nodeCount - v1.nodeCount;
      const edgeCountDiff = v2.edgeCount - v1.edgeCount;

      const diff: VersionDiff = {
        workflowId,
        version1,
        version2,
        nodeCountDiff,
        edgeCountDiff,
        nodesAdded:
          nodeCountDiff > 0
            ? Array.from({ length: nodeCountDiff }, (_, i) => `new_node_${i + 1}`)
            : [],
        nodesRemoved:
          nodeCountDiff < 0
            ? Array.from({ length: Math.abs(nodeCountDiff) }, (_, i) => `removed_node_${i + 1}`)
            : [],
        edgesChanged: Math.abs(edgeCountDiff),
        description: this.generateDiffDescription(nodeCountDiff, edgeCountDiff),
      };

      return diff;
    }, "Compare workflow versions");
  }

  /**
   * Get detailed diff between versions
   */
  async getDiff(
    workflowId: ID,
    version1: string,
    version2: string
  ): Promise<{ summary: VersionDiff; details: Record<string, unknown> }> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getDiff", { workflowId, version1, version2 });
      const summary = await this.compareVersions(workflowId, version1, version2);
      const v1 = await this.getVersion(workflowId, version1);
      const v2 = await this.getVersion(workflowId, version2);

      const details = {
        v1Metadata: {
          version: v1.version,
          nodeCount: v1.nodeCount,
          edgeCount: v1.edgeCount,
          createdAt: new Date(v1.createdAt).toISOString(),
          updatedAt: new Date(v1.updatedAt).toISOString(),
          author: v1.author,
        },
        v2Metadata: {
          version: v2.version,
          nodeCount: v2.nodeCount,
          edgeCount: v2.edgeCount,
          createdAt: new Date(v2.createdAt).toISOString(),
          updatedAt: new Date(v2.updatedAt).toISOString(),
          author: v2.author,
        },
        changes: summary,
      };

      return { summary, details };
    }, "Get workflow version diff");
  }

  /**
   * Get changelog for a workflow
   */
  async getChangeLog(workflowId: ID): Promise<Array<{
    version: string;
    date: string;
    changes: string[];
    author?: string;
  }>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getChangeLog", { workflowId });
      const versions = await this.listVersions(workflowId);

      const changelog = versions.map((v, index) => {
        const changes: string[] = [];
        if (index === 0) {
          changes.push("Initial version created");
        } else {
          const prev = versions[index - 1];
          if (prev) {
            const nodeChange = v.nodeCount - prev.nodeCount;
            const edgeChange = v.edgeCount - prev.edgeCount;
            if (nodeChange > 0) changes.push(`Added ${nodeChange} node(s)`);
            else if (nodeChange < 0) changes.push(`Removed ${Math.abs(nodeChange)} node(s)`);
            if (edgeChange > 0) changes.push(`Added ${edgeChange} connection(s)`);
            else if (edgeChange < 0) changes.push(`Removed ${Math.abs(edgeChange)} connection(s)`);
            if (changes.length === 0) changes.push("Updated metadata");
          }
        }
        return { version: v.version, date: new Date(v.updatedAt).toISOString(), changes, author: v.author };
      });

      return changelog;
    }, "Get workflow changelog");
  }

  private generateDiffDescription(nodeCountDiff: number, edgeCountDiff: number): string {
    const parts: string[] = [];
    if (nodeCountDiff !== 0) {
      parts.push(nodeCountDiff > 0 ? `${nodeCountDiff} node(s) added` : `${Math.abs(nodeCountDiff)} node(s) removed`);
    }
    if (edgeCountDiff !== 0) {
      parts.push(edgeCountDiff > 0 ? `${edgeCountDiff} connection(s) added` : `${Math.abs(edgeCountDiff)} connection(s) removed`);
    }
    return parts.length > 0 ? parts.join("; ") : "No structural changes detected";
  }
}