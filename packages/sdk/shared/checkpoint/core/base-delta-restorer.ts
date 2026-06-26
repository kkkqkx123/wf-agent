import type { BaseCheckpoint } from "@wf-agent/types";
import { BaseDiffCalculator } from "./base-diff-calculator.js";
import type { DeltaRestoreResult } from "../types.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "BaseDeltaRestorer" });

export type BatchLoader<TCheckpoint extends BaseCheckpoint<unknown, unknown>> = (
  ids: string[],
) => Promise<Map<string, TCheckpoint | null>>;

export type MetadataLoader = (
  entityId: string,
  entityType: string,
) => Promise<Array<{
  id: string;
  previousCheckpointId?: string;
  checkpointType: "FULL" | "DELTA";
  timestamp: number;
  chainRootId?: string;
  chainPosition?: number;
}>>;

export class BaseDeltaRestorer<TCheckpoint extends BaseCheckpoint<unknown, unknown>, TState> {
  private diffCalculator: BaseDiffCalculator;
  private loadCheckpoint: (id: string) => Promise<TCheckpoint | null>;
  private batchLoader?: BatchLoader<TCheckpoint>;
  private metadataLoader?: MetadataLoader;

  constructor(
    loadCheckpoint: (id: string) => Promise<TCheckpoint | null>,
    batchLoader?: BatchLoader<TCheckpoint>,
    metadataLoader?: MetadataLoader,
  ) {
    this.diffCalculator = new BaseDiffCalculator();
    this.loadCheckpoint = loadCheckpoint;
    this.batchLoader = batchLoader;
    this.metadataLoader = metadataLoader;
  }

  async restore(
    checkpointId: string,
    entityId?: string,
    entityType?: string,
  ): Promise<DeltaRestoreResult<TState>> {
    logger.debug("Starting checkpoint restoration", { checkpointId });

    const targetCheckpoint = await this.loadCheckpoint(checkpointId);
    if (!targetCheckpoint) {
      throw new Error(`Checkpoint not found: ${checkpointId}`);
    }

    if (targetCheckpoint.type === "FULL") {
      logger.debug("Restoring from full checkpoint", { checkpointId });
      return {
        snapshot: targetCheckpoint.snapshot as TState,
        metadata: {
          checkpointChain: [checkpointId],
          baseCheckpointId: checkpointId,
        },
      };
    }

    logger.debug("Restoring from delta checkpoint, traversing chain", {
      checkpointId,
      baseCheckpointId: targetCheckpoint.baseCheckpointId,
    });

    const chain = entityId && entityType && this.metadataLoader
      ? await this.buildCheckpointChainFromMetadata(checkpointId, entityId, entityType)
      : await this.buildCheckpointChain(checkpointId);
      
    const baseCheckpointId = chain[0];

    if (!baseCheckpointId) {
      throw new Error(`Empty checkpoint chain for: ${checkpointId}`);
    }

    const chainCheckpoints = await this.loadChainCheckpoints(chain);

    const baseCheckpoint = chainCheckpoints.get(baseCheckpointId);
    if (!baseCheckpoint || baseCheckpoint.type !== "FULL") {
      throw new Error(`Base checkpoint not found or invalid: ${baseCheckpointId}`);
    }

    let currentSnapshot = baseCheckpoint.snapshot as TState;

    for (let i = 1; i < chain.length; i++) {
      const deltaCheckpointId = chain[i];
      if (!deltaCheckpointId) continue;

      const deltaCheckpoint = chainCheckpoints.get(deltaCheckpointId);

      if (!deltaCheckpoint || deltaCheckpoint.type !== "DELTA") {
        throw new Error(`Invalid delta checkpoint in chain: ${deltaCheckpointId}`);
      }

      currentSnapshot = this.diffCalculator.applyDelta(
        currentSnapshot as Record<string, unknown>,
        deltaCheckpoint.delta as Record<string, { from: unknown; to: unknown }>,
      ) as TState;

      logger.debug("Applied delta", {
        from: deltaCheckpoint.previousCheckpointId,
        to: deltaCheckpointId,
      });
    }

    logger.info("Checkpoint restoration completed", {
      checkpointId,
      chainLength: chain.length,
      baseCheckpointId,
    });

    return {
      snapshot: currentSnapshot,
      metadata: {
        checkpointChain: chain,
        baseCheckpointId,
      },
    };
  }

  private async buildCheckpointChainFromMetadata(
    targetCheckpointId: string,
    entityId: string,
    entityType: string,
  ): Promise<string[]> {
    logger.debug("Building chain from metadata (optimized)", {
      targetCheckpointId,
      entityId,
      entityType,
    });

    const metadataList = await this.metadataLoader!(entityId, entityType);

    const metadataMap = new Map<string, {
      previousCheckpointId?: string;
      checkpointType: "FULL" | "DELTA";
      chainRootId?: string;
      chainPosition?: number;
    }>();
    for (const meta of metadataList) {
      metadataMap.set(meta.id, {
        previousCheckpointId: meta.previousCheckpointId,
        checkpointType: meta.checkpointType,
        chainRootId: meta.chainRootId,
        chainPosition: meta.chainPosition,
      });
    }

    const targetMeta = metadataMap.get(targetCheckpointId);
    if (!targetMeta) {
      throw new Error(`Checkpoint not found in metadata: ${targetCheckpointId}`);
    }

    if (targetMeta.chainRootId && targetMeta.checkpointType === "DELTA") {
      const rootMeta = metadataMap.get(targetMeta.chainRootId);
      if (rootMeta && rootMeta.checkpointType === "FULL") {
        logger.debug("Using chainRootId for direct lookup optimization", {
          chainRootId: targetMeta.chainRootId,
          chainPosition: targetMeta.chainPosition,
        });

        const chainMembers = metadataList
          .filter(m => m.chainRootId === targetMeta.chainRootId || m.id === targetMeta.chainRootId)
          .sort((a, b) => {
            const posA = a.chainPosition ?? (a.checkpointType === "FULL" ? 0 : 999);
            const posB = b.chainPosition ?? (b.checkpointType === "FULL" ? 0 : 999);
            return posA - posB;
          });

        const chain: string[] = [];
        for (const member of chainMembers) {
          chain.push(member.id);
          if (member.id === targetCheckpointId) {
            break;
          }
        }

        logger.debug("Checkpoint chain built using chainRootId optimization", {
          chainLength: chain.length,
          baseCheckpointId: chain[0],
          targetCheckpointId: chain[chain.length - 1],
        });

        return chain;
      }
    }

    const chain: string[] = [];
    let currentId: string | undefined = targetCheckpointId;
    const visited = new Set<string>();

    while (currentId) {
      if (visited.has(currentId)) {
        throw new Error(`Circular reference detected in checkpoint chain at: ${currentId}`);
      }
      visited.add(currentId);

      const meta = metadataMap.get(currentId);
      if (!meta) {
        throw new Error(`Checkpoint not found in metadata: ${currentId}`);
      }

      chain.unshift(currentId);

      if (meta.checkpointType === "FULL") {
        break;
      }

      currentId = meta.previousCheckpointId;
    }

    logger.debug("Checkpoint chain built from metadata (fallback traversal)", {
      chainLength: chain.length,
      baseCheckpointId: chain[0],
      targetCheckpointId: chain[chain.length - 1],
    });

    return chain;
  }

  private async buildCheckpointChain(checkpointId: string): Promise<string[]> {
    const chain: string[] = [];
    let currentId: string | undefined = checkpointId;
    const visited = new Set<string>();

    while (currentId) {
      if (visited.has(currentId)) {
        throw new Error(`Circular reference detected in checkpoint chain at: ${currentId}`);
      }
      visited.add(currentId);

      const checkpoint = await this.loadCheckpoint(currentId);
      if (!checkpoint) {
        throw new Error(`Checkpoint not found in chain: ${currentId}`);
      }

      chain.unshift(currentId);

      if (checkpoint.type === "FULL") {
        break;
      }

      currentId = checkpoint.previousCheckpointId;
    }

    logger.debug("Checkpoint chain built", {
      chainLength: chain.length,
      baseCheckpointId: chain[0],
      targetCheckpointId: chain[chain.length - 1],
    });

    return chain;
  }

  private async loadChainCheckpoints(
    chain: string[],
  ): Promise<Map<string, TCheckpoint | null>> {
    if (this.batchLoader) {
      logger.debug("Using batch loader for chain", { chainLength: chain.length });
      return this.batchLoader(chain);
    }

    const result = new Map<string, TCheckpoint | null>();
    for (const id of chain) {
      const checkpoint = await this.loadCheckpoint(id);
      result.set(id, checkpoint);
    }
    return result;
  }
}
