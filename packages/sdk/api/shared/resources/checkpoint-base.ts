/**
 * BaseCheckpointResourceAPI - Shared base class for checkpoint resource management
 *
 * Provides a unified base implementation for checkpoint CRUD and query operations.
 * Both Agent and Workflow versions extend this class, providing only entity-specific
 * type annotations and method naming.
 *
 * Shared features:
 * - Query methods (query, getByTimeRange, getByType, getByIds, getByTags)
 * - getLatestCheckpoint
 * - getCheckpointChainFrom (chain following)
 * - getCheckpointStatistics (via abstract entity ID extraction)
 *
 * Entity-specific subclasses must implement:
 * - CRUD: getResource / getAllResources / createResource / deleteResource / clearResources
 * - applyFilter / validateResource / validateUpdate
 * - Domain: createCheckpoint / restoreFromCheckpoint
 * - Helpers: getEntityId / getCheckpointType / getCheckpointTimestamp / getCheckpointPreviousId
 */

import { SimplifiedCrudResourceAPI } from "./generic-resource-api.js";
import type { EventRegistry } from "../../../shared/registry/event-registry.js";

/**
 * Base checkpoint filter - entity-specific filters extend this
 */
export interface BaseCheckpointFilter {
  /** ID list */
  ids?: string[];
  /** Entity ID (execution / agent loop) */
  entityId?: string;
  /** Checkpoint type (FULL or DELTA) */
  type?: "FULL" | "DELTA";
  /** Tags to filter by */
  tags?: string[];
  /** Time range */
  timestampRange?: { start?: number; end?: number };
}

/**
 * Base checkpoint statistics - entity-specific statistics extend this
 */
export interface BaseCheckpointStatistics {
  total: number;
  byType: Record<string, number>;
}

/**
 * BaseCheckpointResourceAPI - Shared base class for checkpoint resource management
 *
 * @typeParam TCheckpoint - Domain-specific checkpoint type
 * @typeParam TFilter - Domain-specific filter type (extends BaseCheckpointFilter)
 */
export abstract class BaseCheckpointResourceAPI<
  TCheckpoint extends { id: string },
  TFilter extends BaseCheckpointFilter = BaseCheckpointFilter,
> extends SimplifiedCrudResourceAPI<TCheckpoint, string, TFilter> {
  protected eventManager?: EventRegistry;

  // ============================================================================
  // Abstract helpers - Subclasses must implement these
  // ============================================================================

  /**
   * Extract the entity ID (execution ID / agent loop ID) from a checkpoint
   */
  protected abstract getEntityId(checkpoint: TCheckpoint): string;

  /**
   * Extract the checkpoint type from a checkpoint
   */
  protected abstract getCheckpointType(checkpoint: TCheckpoint): string;

  /**
   * Extract the timestamp from a checkpoint
   */
  protected abstract getCheckpointTimestamp(checkpoint: TCheckpoint): number;

  /**
   * Extract the previous checkpoint ID from a checkpoint
   */
  protected abstract getCheckpointPreviousId(checkpoint: TCheckpoint): string | undefined;

  /**
   * Get a checkpoint by ID (used by chain following)
   */
  protected abstract getCheckpointById(id: string): Promise<TCheckpoint | null>;

  // ============================================================================
  // Shared query methods
  // ============================================================================

  /**
   * Query checkpoints by filter
   * @param filter Filter criteria
   * @returns Filtered checkpoints
   */
  async query(filter: TFilter): Promise<TCheckpoint[]> {
    const all = await this.getAll();
    return this.applyFilter(all, filter);
  }

  /**
   * Get the latest checkpoint for an entity
   * @param entityId Entity ID
   * @returns The latest checkpoint; returns null if it does not exist
   */
  async getLatestCheckpoint(entityId: string): Promise<TCheckpoint | null> {
    const all = await this.getAll();
    const entityCheckpoints = all.filter((cp) => this.getEntityId(cp) === entityId);
    if (entityCheckpoints.length === 0) {
      return null;
    }
    // Sort in descending order by timestamp and return the latest
    const latest = entityCheckpoints.sort(
      (a, b) => this.getCheckpointTimestamp(b) - this.getCheckpointTimestamp(a),
    )[0];
    return latest || null;
  }

  /**
   * Get checkpoint statistics
   * @returns Statistical information
   */
  async getCheckpointStatistics(): Promise<BaseCheckpointStatistics> {
    const checkpoints = await this.getAll();
    const byType: Record<string, number> = {};

    for (const checkpoint of checkpoints) {
      const type = this.getCheckpointType(checkpoint) || "unknown";
      byType[type] = (byType[type] || 0) + 1;
    }

    return {
      total: checkpoints.length,
      byType,
    };
  }

  /**
   * Get checkpoints in time range
   * @param entityId Entity ID
   * @param startTime Start timestamp (ms)
   * @param endTime End timestamp (ms)
   * @returns Checkpoints in time range
   */
  async getByTimeRange(
    entityId: string,
    startTime: number,
    endTime: number,
  ): Promise<TCheckpoint[]> {
    return this.query({
      entityId,
      timestampRange: { start: startTime, end: endTime },
    } as unknown as TFilter);
  }

  /**
   * Get checkpoints by type
   * @param entityId Entity ID
   * @param type Checkpoint type (FULL | DELTA)
   * @returns Checkpoints of specified type
   */
  async getByType(entityId: string, type: "FULL" | "DELTA"): Promise<TCheckpoint[]> {
    return this.query({ entityId, type } as unknown as TFilter);
  }

  /**
   * Get checkpoints by multiple IDs
   * @param ids Checkpoint ID list
   * @returns Checkpoints with matching IDs
   */
  async getByIds(ids: string[]): Promise<TCheckpoint[]> {
    return this.query({ ids } as unknown as TFilter);
  }

  /**
   * Get checkpoints by tags
   * @param entityId Entity ID
   * @param tags Tags to filter by
   * @returns Checkpoints with matching tags
   */
  async getByTags(entityId: string, tags: string[]): Promise<TCheckpoint[]> {
    return this.query({ entityId, tags } as unknown as TFilter);
  }

  /**
   * Get checkpoint chain starting from a specific checkpoint.
   * Follows previousCheckpointId links backwards.
   * @param checkpointId Starting checkpoint ID
   * @returns Checkpoints in reverse chronological order (newest first)
   */
  async getCheckpointChainFrom(checkpointId: string): Promise<TCheckpoint[]> {
    const chain: TCheckpoint[] = [];
    let currentId: string | undefined = checkpointId;

    while (currentId) {
      const checkpoint = await this.getCheckpointById(currentId);
      if (!checkpoint) {
        break;
      }
      chain.push(checkpoint);
      currentId = this.getCheckpointPreviousId(checkpoint);
    }

    return chain;
  }
}