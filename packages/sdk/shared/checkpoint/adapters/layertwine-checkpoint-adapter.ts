/**
 * Layertwine Checkpoint Storage Adapter
 *
 * Unified adapter supporting both Agent and Workflow checkpoints.
 * Single implementation using generics - no code duplication.
 *
 * Features:
 * - Branch isolation: Each Agent Loop/Workflow gets its own branch
 * - Structured parentId metadata in commits
 * - Batch operation support (for parallel invocation)
 * - Lazy evaluation for parentId extraction
 *
 * Usage:
 *   const adapter = new LayertwineCheckpointAdapter<AgentLoopCheckpoint>(executor);
 */

import type { LayertwineExecutor } from "../../../services/executors/remote/implementations/layertwine/index.js";
import type { BaseCheckpoint } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { BranchManager } from "../branch-manager.js";

const logger = createContextualLogger({ component: "LayertwineCheckpointAdapter" });

/**
 * Layertwine Checkpoint Storage Adapter
 *
 * @template TCheckpoint The checkpoint type (AgentLoopCheckpoint or Checkpoint)
 */
export class LayertwineCheckpointAdapter<
  TCheckpoint extends BaseCheckpoint<unknown, unknown> = BaseCheckpoint<unknown, unknown>,
> {
  private branchManager: BranchManager;

  constructor(private executor: LayertwineExecutor, branchManager?: BranchManager) {
    if (!executor) {
      throw new Error("LayertwineExecutor is required");
    }

    this.branchManager =
      branchManager ||
      new BranchManager({
        executor,
        enableCache: true,
        defaultBranch: "main",
      });
  }

  /**
   * Get the BranchManager instance
   */
  getBranchManager(): BranchManager {
    return this.branchManager;
  }

  /**
   * Extract parent ID from checkpoint - prioritizes structured fields
   */
  protected getCheckpointParentId(checkpoint: TCheckpoint): string {
    const parentId =
      (checkpoint as any).agentLoopId ||
      (checkpoint as any).executionId ||
      (checkpoint as any).parentId;
    return String(parentId || "unknown");
  }

  /**
   * Determine branch type based on checkpoint
   */
  protected getCheckpointBranchType(
    checkpoint: TCheckpoint
  ): "agent-loop" | "execution" {
    if ((checkpoint as any).executionId) {
      return "execution";
    }
    return "agent-loop";
  }

  /**
   * Build commit message with metadata tags
   */
  private buildCommitMessage(
    description: string,
    parentId: string,
    checkpointType?: string
  ): string {
    const type = checkpointType || "FULL";
    return `Checkpoint: ${description} [parent:${parentId}] [type:${type}]`;
  }

  /**
   * Extract parent ID from message
   */
  private parseParentIdFromMessage(message: string): string | null {
    const match = message.match(/\[parent:([^\]]+)\]/);
    return match && match[1] ? match[1] : null;
  }

  /**
   * Save checkpoint to Layertwine backend
   */
  async saveCheckpoint(checkpoint: TCheckpoint): Promise<string> {
    const startTime = performance.now();
    const parentId = this.getCheckpointParentId(checkpoint);
    const branchType = this.getCheckpointBranchType(checkpoint);

    try {
      this.validateCheckpoint(checkpoint);

      const description = checkpoint.metadata?.description ?? "Checkpoint";
      const creator = checkpoint.metadata?.customFields?.["creator"] ?? "system";
      const checkpointType = checkpoint.type ?? "FULL";

      // Ensure branch exists and switch
      const branchName = await this.branchManager.ensureBranch(parentId, branchType);
      await this.branchManager.switchBranch(branchName);

      logger.debug("Switched to checkpoint branch", {
        checkpointId: checkpoint.id,
        branchName,
        parentId,
      });

      // Serialize and store checkpoint
      const checkpointJson = JSON.stringify(checkpoint);
      const checkpointSize = checkpointJson.length;
      const checkpointFilePath = `.checkpoints/${checkpoint.id}.json`;

      await this.executor.edit({
        file: checkpointFilePath,
        content: checkpointJson,
      });

      // Commit with structured parentId
      const commitMessage = this.buildCommitMessage(description, parentId, checkpointType);
      const response = await this.executor.commit({
        message: commitMessage,
        author: String(creator),
        parentId: parentId,
      });

      const duration = performance.now() - startTime;

      logger.info("Checkpoint saved to Layertwine", {
        checkpointId: response.checkpointId,
        checkpointType,
        parentId,
        branchName,
        dataSize: checkpointSize,
        duration: Math.round(duration),
        creator,
        timestamp: new Date().toISOString(),
      });

      return response.checkpointId;
    } catch (error) {
      const duration = performance.now() - startTime;
      logger.error("Failed to save checkpoint to Layertwine", {
        error: error instanceof Error ? error.message : String(error),
        checkpointId: checkpoint.id,
        parentId,
        branchType,
        duration: Math.round(duration),
      });
      throw error;
    }
  }

  /**
   * Batch save multiple checkpoints in parallel (10 concurrent)
   */
  async batchSaveCheckpoints(checkpoints: TCheckpoint[]): Promise<{
    successful: string[];
    failed: Array<{ checkpointId: string; error: string }>;
    duration: number;
  }> {
    const startTime = performance.now();
    const batchSize = 10;
    const successful: string[] = [];
    const failed: Array<{ checkpointId: string; error: string }> = [];

    for (let i = 0; i < checkpoints.length; i += batchSize) {
      const batch = checkpoints.slice(i, i + batchSize);
      const results = await Promise.allSettled(
        batch.map((cp) => this.saveCheckpoint(cp))
      );

      results.forEach((result, index) => {
        const checkpoint = batch[index];
        if (!checkpoint) return;

        if (result.status === "fulfilled") {
          successful.push(result.value);
        } else {
          failed.push({
            checkpointId: checkpoint.id,
            error:
              result.reason instanceof Error
                ? result.reason.message
                : result.reason
                ? String(result.reason)
                : "Unknown error",
          });
        }
      });
    }

    const duration = performance.now() - startTime;
    return { successful, failed, duration };
  }

  /**
   * Get checkpoint from Layertwine backend
   */
  async getCheckpoint(id: string): Promise<TCheckpoint | null> {
    const startTime = performance.now();

    try {
      const restoreResponse = await this.executor.restoreCheckpoint({
        checkpointId: id,
      });

      if (!restoreResponse || !restoreResponse.snapshots) {
        logger.warn("Checkpoint restore returned no snapshots", { checkpointId: id });
        return null;
      }

      const checkpointFile = restoreResponse.snapshots.find(
        (snapshot) =>
          snapshot.source === `.checkpoints/${id}.json` ||
          snapshot.source.endsWith(`/${id}.json`)
      );

      if (!checkpointFile) {
        logger.warn("Checkpoint data file not found in restored snapshots", {
          checkpointId: id,
          snapshots: restoreResponse.snapshots.map((s) => s.source),
        });
        return null;
      }

      const snapshotResponse = await this.executor.getSnapshot({
        checkpointId: id,
        snapshotId: checkpointFile.id,
      });

      if (!snapshotResponse || !snapshotResponse.content) {
        logger.warn("Failed to retrieve checkpoint content", { checkpointId: id });
        return null;
      }

      const duration = performance.now() - startTime;

      try {
        const content =
          typeof snapshotResponse.content === "string"
            ? snapshotResponse.content
            : snapshotResponse.content.toString();

        const checkpoint = JSON.parse(content) as TCheckpoint;
        this.validateCheckpointStructure(checkpoint, id);

        logger.debug("Checkpoint retrieved from Layertwine", {
          checkpointId: id,
          dataSize: snapshotResponse.size,
          checkpointType: checkpoint.type,
          duration: Math.round(duration),
        });

        return checkpoint;
      } catch (parseError) {
        logger.error("Failed to parse checkpoint content", {
          checkpointId: id,
          parseError: parseError instanceof Error ? parseError.message : String(parseError),
        });
        return null;
      }
    } catch (error) {
      const duration = performance.now() - startTime;
      logger.error("Failed to get checkpoint from Layertwine", {
        error: error instanceof Error ? error.message : String(error),
        checkpointId: id,
        duration: Math.round(duration),
      });
      throw error;
    }
  }

  /**
   * Batch get multiple checkpoints in parallel (10 concurrent)
   */
  async batchGetCheckpoints(checkpointIds: string[]): Promise<(TCheckpoint | null)[]> {
    const startTime = performance.now();
    const batchSize = 10;
    const results: (TCheckpoint | null)[] = [];

    for (let i = 0; i < checkpointIds.length; i += batchSize) {
      const batch = checkpointIds.slice(i, i + batchSize);
      const batchResults = await Promise.allSettled(
        batch.map((id) => this.getCheckpoint(id))
      );

      batchResults.forEach((result) => {
        if (result.status === "fulfilled") {
          results.push(result.value);
        } else {
          results.push(null);
        }
      });
    }

    const duration = performance.now() - startTime;
    logger.debug("Batch get completed", {
      count: checkpointIds.length,
      duration: Math.round(duration),
    });

    return results;
  }

  /**
   * List checkpoints by parent entity ID
   */
  async listCheckpoints(parentId: string): Promise<string[]> {
    try {
      const branchType: "agent-loop" | "execution" = "agent-loop";
      const branchName = this.branchManager.getBranchName(parentId, branchType);

      await this.branchManager.switchBranch(branchName);

      logger.debug("Switched to branch for listing", {
        parentId,
        branchName,
      });

      const response = await this.executor.log({
        count: 1000,
      });

      if (!response || !response.checkpoints) {
        logger.debug("No checkpoints found in branch", { parentId, branchName });
        return [];
      }

      // Filter by structured parentId metadata first
      const checkpointIds = response.checkpoints
        .filter((cp) => {
          if (cp.parentId !== undefined && cp.parentId !== null) {
            return cp.parentId === parentId;
          }
          // Fallback to message parsing if metadata unavailable
          const parsed = this.parseParentIdFromMessage(cp.message);
          return parsed === parentId;
        })
        .map((cp) => cp.id);

      logger.debug("Checkpoints listed from Layertwine", {
        parentId,
        branchName,
        count: checkpointIds.length,
        totalCheckpoints: response.total,
      });

      return checkpointIds;
    } catch (error) {
      logger.error("Failed to list checkpoints from Layertwine", {
        error: error instanceof Error ? error.message : String(error),
        parentId,
      });
      throw error;
    }
  }

  /**
   * Batch list checkpoints for multiple parents (20 concurrent)
   */
  async batchListCheckpoints(parentIds: string[]): Promise<{
    checkpointIds: string[];
    duration: number;
  }> {
    const startTime = performance.now();
    const concurrency = 20;
    const allCheckpointIds: string[] = [];

    for (let i = 0; i < parentIds.length; i += concurrency) {
      const batch = parentIds.slice(i, i + concurrency);
      const results = await Promise.allSettled(
        batch.map((parentId) => this.listCheckpoints(parentId))
      );

      results.forEach((result) => {
        if (result.status === "fulfilled") {
          allCheckpointIds.push(...result.value);
        }
      });
    }

    const duration = performance.now() - startTime;
    logger.debug("Batch list completed", {
      parentCount: parentIds.length,
      checkpointCount: allCheckpointIds.length,
      duration: Math.round(duration),
    });

    return { checkpointIds: allCheckpointIds, duration };
  }

  /**
   * Validate checkpoint structure
   */
  private validateCheckpoint(checkpoint: TCheckpoint): void {
    if (!checkpoint || typeof checkpoint !== "object") {
      throw new Error("Invalid checkpoint: must be an object");
    }

    if (!checkpoint.id) {
      throw new Error("Invalid checkpoint: missing id field");
    }

    if (checkpoint.type && !["FULL", "DELTA"].includes(checkpoint.type)) {
      throw new Error(`Invalid checkpoint type: ${checkpoint.type}. Must be FULL or DELTA`);
    }
  }

  /**
   * Validate checkpoint structure after retrieval
   */
  private validateCheckpointStructure(checkpoint: TCheckpoint, checkpointId: string): void {
    try {
      if (!checkpoint.id) {
        logger.warn("Retrieved checkpoint missing id field", { checkpointId });
      }

      if (!checkpoint.type || !["FULL", "DELTA"].includes(checkpoint.type)) {
        logger.warn("Retrieved checkpoint has invalid type", {
          checkpointId,
          type: checkpoint.type,
        });
      }

      if (checkpoint.type === "DELTA") {
        if (!checkpoint.baseCheckpointId) {
          logger.warn("Delta checkpoint missing baseCheckpointId", { checkpointId });
        }
        if (!checkpoint.previousCheckpointId) {
          logger.warn("Delta checkpoint missing previousCheckpointId", { checkpointId });
        }
      }

      if (checkpoint.type === "FULL") {
        if (!checkpoint.snapshot) {
          logger.warn("Full checkpoint missing snapshot", { checkpointId });
        }
      }
    } catch (error) {
      logger.warn("Error validating checkpoint structure", {
        error: error instanceof Error ? error.message : String(error),
        checkpointId,
      });
    }
  }
}
