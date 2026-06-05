/**
 * File Delta Store
 *
 * Stores incremental file changes (content) between checkpoints.
 * Works with HashBaselineStore to provide efficient checkpoint storage.
 *
 * Storage model:
 * - Baseline: only hashes (32 bytes per file)
 * - Delta: only changed file content
 */

import * as fs from "fs/promises";

/**
 * File change types
 */
export type FileDeltaChangeType = "added" | "modified" | "deleted";

/**
 * Single file change in a delta
 */
export interface FileDeltaChange {
  /** Relative path from workspace root */
  path: string;
  /** Change type */
  type: FileDeltaChangeType;
  /** Old hash (for modified/deleted) */
  oldHash?: string;
  /** New hash (for added/modified) */
  newHash?: string;
  /** File content (for added/modified, optional - can be stored externally) */
  content?: Buffer;
}

/**
 * Delta checkpoint data
 */
export interface FileDelta {
  /** Delta ID */
  id: string;
  /** Type marker */
  type: "delta";
  /** Base checkpoint ID (baseline or previous delta) */
  baseCheckpointId: string;
  /** File changes */
  changes: FileDeltaChange[];
  /** Total file count at this checkpoint */
  totalFileCount: number;
  /** Creation timestamp */
  createdAt: number;
  /** Workspace root directory */
  workspaceRoot: string;
}

/**
 * File delta store configuration
 */
export interface FileDeltaStoreConfig {
  /** Workspace root directory */
  workspaceRoot: string;
  /** Storage adapter for persistence */
  storageAdapter?: FileDeltaStorageAdapter;
  /** Whether to store file content in delta (default: true) */
  storeContent?: boolean;
}

/**
 * Storage adapter interface for file delta persistence
 */
export interface FileDeltaStorageAdapter {
  /** Save delta */
  saveDelta(delta: FileDelta): Promise<void>;
  /** Load delta by ID */
  loadDelta(id: string): Promise<FileDelta | null>;
  /** Delete delta */
  deleteDelta(id: string): Promise<void>;
  /** List all delta IDs for a base checkpoint chain */
  listDeltaIds(baseCheckpointId: string): Promise<string[]>;
}

/**
 * File Delta Store
 *
 * Manages delta checkpoints containing file changes.
 */
export class FileDeltaStore {
  private readonly workspaceRoot: string;
  private readonly storageAdapter?: FileDeltaStorageAdapter;
  private readonly storeContent: boolean;

  constructor(config: FileDeltaStoreConfig) {
    this.workspaceRoot = config.workspaceRoot;
    this.storageAdapter = config.storageAdapter;
    this.storeContent = config.storeContent ?? true;
  }

  /**
   * Create a delta from file changes
   */
  async createDelta(
    id: string,
    baseCheckpointId: string,
    changes: Array<{
      path: string;
      type: FileDeltaChangeType;
      oldHash?: string;
      newHash?: string;
      absolutePath?: string;
    }>,
    totalFileCount: number,
  ): Promise<FileDelta> {
    const deltaChanges: FileDeltaChange[] = [];

    for (const change of changes) {
      const deltaChange: FileDeltaChange = {
        path: change.path,
        type: change.type,
        oldHash: change.oldHash,
        newHash: change.newHash,
      };

      // Load content for added/modified files if enabled
      if (
        this.storeContent &&
        (change.type === "added" || change.type === "modified") &&
        change.absolutePath
      ) {
        try {
          deltaChange.content = await fs.readFile(change.absolutePath);
        } catch {
          // Skip if file can't be read
        }
      }

      deltaChanges.push(deltaChange);
    }

    const delta: FileDelta = {
      id,
      type: "delta",
      baseCheckpointId,
      changes: deltaChanges,
      totalFileCount,
      createdAt: Date.now(),
      workspaceRoot: this.workspaceRoot,
    };

    if (this.storageAdapter) {
      await this.storageAdapter.saveDelta(delta);
    }

    return delta;
  }

  /**
   * Create delta from pre-loaded content (no file I/O)
   */
  async createDeltaFromContent(
    id: string,
    baseCheckpointId: string,
    changes: FileDeltaChange[],
    totalFileCount: number,
  ): Promise<FileDelta> {
    const delta: FileDelta = {
      id,
      type: "delta",
      baseCheckpointId,
      changes,
      totalFileCount,
      createdAt: Date.now(),
      workspaceRoot: this.workspaceRoot,
    };

    if (this.storageAdapter) {
      await this.storageAdapter.saveDelta(delta);
    }

    return delta;
  }

  /**
   * Load delta by ID
   */
  async loadDelta(id: string): Promise<FileDelta | null> {
    if (this.storageAdapter) {
      return this.storageAdapter.loadDelta(id);
    }
    return null;
  }

  /**
   * Resolve delta chain to get final file state
   *
   * Returns a map of file paths to their content at the target checkpoint.
   * Files not in the map should be read from host FS (they haven't changed).
   */
  async resolveDeltaChain(
    targetCheckpointId: string,
    _loadBaseline: (id: string) => Promise<{ files: Map<string, string> } | null>,
    loadDelta: (id: string) => Promise<FileDelta | null>,
  ): Promise<Map<string, Buffer | null>> {
    const result = new Map<string, Buffer | null>();

    // Walk back to find the baseline
    const chain: string[] = [];
    let currentId: string | null = targetCheckpointId;

    while (currentId) {
      chain.unshift(currentId);
      const delta = await loadDelta(currentId);
      if (delta) {
        currentId = delta.baseCheckpointId;
      } else {
        // This must be a baseline
        break;
      }
    }

    // Apply deltas in order
    for (const checkpointId of chain) {
      const delta = await loadDelta(checkpointId);
      if (delta) {
        for (const change of delta.changes) {
          if (change.type === "deleted") {
            result.set(change.path, null);
          } else if (change.content) {
            result.set(change.path, change.content);
          }
        }
      }
    }

    return result;
  }

  /**
   * Get changed files from a delta
   */
  getChangedFiles(delta: FileDelta): {
    added: string[];
    modified: string[];
    deleted: string[];
  } {
    const added: string[] = [];
    const modified: string[] = [];
    const deleted: string[] = [];

    for (const change of delta.changes) {
      switch (change.type) {
        case "added":
          added.push(change.path);
          break;
        case "modified":
          modified.push(change.path);
          break;
        case "deleted":
          deleted.push(change.path);
          break;
      }
    }

    return { added, modified, deleted };
  }

  /**
   * Merge multiple deltas into one
   *
   * Useful for compacting delta chain.
   */
  mergeDeltas(deltas: FileDelta[]): FileDelta {
    if (deltas.length === 0) {
      throw new Error("Cannot merge empty delta array");
    }

    const firstDelta = deltas[0]!;
    const lastDelta = deltas[deltas.length - 1]!;

    if (deltas.length === 1) {
      return firstDelta;
    }

    // Track final state of each file
    const fileStates = new Map<
      string,
      { type: FileDeltaChangeType; oldHash?: string; newHash?: string; content?: Buffer }
    >();

    // Apply deltas in order
    for (const delta of deltas) {
      for (const change of delta.changes) {
        const existing = fileStates.get(change.path);

        if (change.type === "deleted") {
          if (existing && existing.type === "added") {
            // Added then deleted - remove from tracking
            fileStates.delete(change.path);
          } else {
            fileStates.set(change.path, {
              type: "deleted",
              oldHash: existing?.newHash ?? change.oldHash,
            });
          }
        } else {
          fileStates.set(change.path, {
            type: existing?.type === "added" ? "added" : change.type,
            oldHash: existing?.oldHash ?? change.oldHash,
            newHash: change.newHash,
            content: change.content,
          });
        }
      }
    }

    // Build merged delta
    const mergedChanges: FileDeltaChange[] = [];
    for (const [path, state] of fileStates) {
      mergedChanges.push({
        path,
        type: state.type,
        oldHash: state.oldHash,
        newHash: state.newHash,
        content: state.content,
      });
    }

    return {
      id: `merged-${firstDelta.id}-${lastDelta.id}`,
      type: "delta",
      baseCheckpointId: firstDelta.baseCheckpointId,
      changes: mergedChanges,
      totalFileCount: lastDelta.totalFileCount,
      createdAt: Date.now(),
      workspaceRoot: firstDelta.workspaceRoot,
    };
  }
}
