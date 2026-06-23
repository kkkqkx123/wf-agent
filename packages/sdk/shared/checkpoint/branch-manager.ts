/**
 * Branch Manager for Layertwine
 *
 * Manages branch lifecycle:
 * - Creating branches for Agent Loops and Workflow Executions
 * - Switching between branches
 * - Tracking active branches
 * - Cleaning up completed branches (optional)
 *
 * Design:
 * - Each Agent Loop gets its own branch: agent-loop/{agentLoopId}
 * - Each Workflow Execution gets its own branch: execution/{executionId}
 * - Branch operations are automatically handled internally
 * - Cache prevents redundant branch creation calls
 */

import type { LayertwineExecutor } from "../../services/executors/remote/implementations/layertwine/index.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "BranchManager" });

export interface BranchManagerConfig {
  executor: LayertwineExecutor;
  enableCache?: boolean;
  defaultBranch?: string;
}

export interface BranchInfo {
  name: string;
  parentId: string;
  type: "agent-loop" | "execution";
  created: number;
}

/**
 * Manages Layertwine branches for checkpoint isolation
 */
export class BranchManager {
  private executor: LayertwineExecutor;
  private currentBranch: string = "main";
  private branchCache: Map<string, BranchInfo> = new Map();
  private enableCache: boolean = true;
  private defaultBranch: string = "main";

  constructor(config: BranchManagerConfig) {
    this.executor = config.executor;
    this.enableCache = config.enableCache ?? true;
    this.defaultBranch = config.defaultBranch ?? "main";
    this.currentBranch = this.defaultBranch;
  }

  /**
   * Get branch name for a parent ID (Agent Loop or Execution)
   *
   * @param parentId Agent Loop ID or Execution ID
   * @param type Whether it's an agent-loop or execution
   * @returns Branch name
   */
  getBranchName(parentId: string, type: "agent-loop" | "execution" = "agent-loop"): string {
    return `${type}/${parentId}`;
  }

  /**
   * Ensure a branch exists, creating it if necessary
   *
   * @param parentId Agent Loop ID or Execution ID
   * @param type Branch type (agent-loop or execution)
   * @returns Branch name
   */
  async ensureBranch(
    parentId: string,
    type: "agent-loop" | "execution" = "agent-loop"
  ): Promise<string> {
    const branchName = this.getBranchName(parentId, type);
    const startTime = performance.now();

    try {
      // Check cache first
      if (this.enableCache && this.branchCache.has(branchName)) {
        logger.debug("Branch found in cache", { branchName });
        return branchName;
      }

      // List existing branches
      const branchList = await this.executor.branchList();
      const branchExists = branchList.branches.some((b) => b.name === branchName);

      if (branchExists) {
        logger.debug("Branch already exists", { branchName });
        this.branchCache.set(branchName, {
          name: branchName,
          parentId,
          type,
          created: Date.now(),
        });
        return branchName;
      }

      // Create branch if it doesn't exist
      logger.info("Creating new branch", { branchName, parentId, type });
      await this.executor.branchCreate({ name: branchName });

      // Cache the new branch
      this.branchCache.set(branchName, {
        name: branchName,
        parentId,
        type,
        created: Date.now(),
      });

      const duration = performance.now() - startTime;
      logger.debug("Branch created successfully", {
        branchName,
        duration: Math.round(duration),
      });

      return branchName;
    } catch (error) {
      const duration = performance.now() - startTime;
      logger.error("Failed to ensure branch", {
        branchName,
        error: error instanceof Error ? error.message : String(error),
        duration: Math.round(duration),
      });
      throw error;
    }
  }

  /**
   * Switch to a specific branch
   *
   * @param branchName The branch to switch to
   */
  async switchBranch(branchName: string): Promise<void> {
    if (this.currentBranch === branchName) {
      logger.debug("Already on branch", { branchName });
      return;
    }

    const startTime = performance.now();

    try {
      logger.debug("Switching branch", { from: this.currentBranch, to: branchName });
      await this.executor.branchSwitch({ name: branchName });

      this.currentBranch = branchName;
      const duration = performance.now() - startTime;

      logger.debug("Branch switched successfully", {
        branchName,
        duration: Math.round(duration),
      });
    } catch (error) {
      const duration = performance.now() - startTime;
      logger.error("Failed to switch branch", {
        branchName,
        error: error instanceof Error ? error.message : String(error),
        duration: Math.round(duration),
      });
      throw error;
    }
  }

  /**
   * Switch to default branch
   */
  async switchToDefaultBranch(): Promise<void> {
    await this.switchBranch(this.defaultBranch);
  }

  /**
   * Get current branch
   */
  getCurrentBranch(): string {
    return this.currentBranch;
  }

  /**
   * List all active branches
   *
   * @returns Array of branch names
   */
  async listActiveBranches(): Promise<string[]> {
    try {
      const response = await this.executor.branchList();
      return response.branches.map((b) => b.name);
    } catch (error) {
      logger.error("Failed to list branches", {
        error: error instanceof Error ? error.message : String(error),
      });
      throw error;
    }
  }

  /**
   * Get all branches with their parent IDs
   *
   * @returns Map of branch name to BranchInfo
   */
  async listBranchesWithMetadata(): Promise<Map<string, BranchInfo>> {
    const result = new Map<string, BranchInfo>();

    try {
      const response = await this.executor.branchList();

      for (const branch of response.branches) {
        // Try to parse parent ID from branch name
        const match = branch.name.match(/^(agent-loop|execution)\/(.+)$/);
        if (match && match[1] && match[2]) {
          const [, type, parentId] = match;
          result.set(branch.name, {
            name: branch.name,
            parentId: parentId || "unknown",
            type: type as "agent-loop" | "execution",
            created: Date.now(),
          });
        }
      }

      logger.debug("Listed branches with metadata", { count: result.size });
      return result;
    } catch (error) {
      logger.error("Failed to list branches with metadata", {
        error: error instanceof Error ? error.message : String(error),
      });
      throw error;
    }
  }

  /**
   * Clean up branches for completed parent IDs
   *
   * @param completedParentIds Parent IDs to clean up
   * @returns Number of branches cleaned
   */
  async cleanupBranches(completedParentIds: string[]): Promise<number> {
    let cleanedCount = 0;
    const branchesToClean: string[] = [];

    try {
      // Identify branches to clean
      for (const parentId of completedParentIds) {
        const agentLoopBranch = this.getBranchName(parentId, "agent-loop");
        const executionBranch = this.getBranchName(parentId, "execution");

        branchesToClean.push(agentLoopBranch, executionBranch);
      }

      // Note: Branch deletion is not implemented in Layertwine yet
      // For now, we only clear the cache
      for (const branchName of branchesToClean) {
        if (this.branchCache.has(branchName)) {
          this.branchCache.delete(branchName);
          cleanedCount++;
        }
      }

      logger.info("Branches cleanup completed", {
        cleanedCount,
        parentIds: completedParentIds.length,
      });

      return cleanedCount;
    } catch (error) {
      logger.warn("Error during branch cleanup", {
        error: error instanceof Error ? error.message : String(error),
      });
      return cleanedCount;
    }
  }

  /**
   * Clear the branch cache
   */
  clearCache(): void {
    const cacheSize = this.branchCache.size;
    this.branchCache.clear();
    logger.debug("Branch cache cleared", { cacheSize });
  }

  /**
   * Get cache size
   */
  getCacheSize(): number {
    return this.branchCache.size;
  }
}
