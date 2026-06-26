/**
 * CheckpointCleanupPolicy - Implementation of Checkpoint Cleanup Strategy
 *
 * Defines the implementation of an automatic checkpoint cleanup strategy
 *
 * Core Responsibilities:
 * 1. Implement a time-based cleanup strategy
 * 2. Implement a quantity-based cleanup strategy
 * 3. Implement a space-based cleanup strategy
 * 4. Provide a strategy factory function
 *
 * Design Principles:
 * - Flexible Configuration: Supports the combination of multiple cleanup strategies
 * - Scalability: Easy to add new cleanup strategies
 * - Security: Ensures that not all checkpoints are deleted
 */

import type {
  CheckpointInfo,
  CleanupPolicy,
  TimeBasedCleanupPolicy,
  CountBasedCleanupPolicy,
  SizeBasedCleanupPolicy,
  TieredCleanupPolicy,
  CheckpointCleanupStrategy,
  CheckpointDependencyGraph,
} from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "CleanupPolicy" });

/**
 * Implementation of tiered retention strategy.
 * Keeps checkpoints based on age-based tiers with different retention granularity.
 * 
 * Tiers are evaluated from youngest to oldest. Within each tier, the retentionIntervalDays
 * determines how many checkpoints to keep: keep at most one checkpoint per interval window.
 * A retentionIntervalDays of 0 means keep all checkpoints in that tier.
 * 
 * Example tiers:
 *   [{ minAgeDays: 0, maxAgeDays: 7, retentionIntervalDays: 0 }]     → keep all <7 days
 *   [{ minAgeDays: 7, maxAgeDays: 30, retentionIntervalDays: 1 }]    → keep daily for 7-30 days
 *   [{ minAgeDays: 30, retentionIntervalDays: 7 }]                   → keep weekly for 30+ days
 */
export class TieredCleanupStrategy implements CheckpointCleanupStrategy {
  constructor(private policy: TieredCleanupPolicy) {}

  execute(checkpoints: CheckpointInfo[], dependencyGraph?: CheckpointDependencyGraph): string[] {
    const currentTime = Date.now();
    const minRetention = this.policy.minRetention || 0;

    const sorted = [...checkpoints]
      .filter((cp): cp is CheckpointInfo => cp != null)
      .sort((a, b) => a.metadata.timestamp - b.metadata.timestamp);

    if (sorted.length <= minRetention) {
      return [];
    }

    // Build tier boundaries (milliseconds)
    const tiers = this.policy.tiers
      .map(t => ({
        minAge: t.minAgeDays * 24 * 60 * 60 * 1000,
        maxAge: t.maxAgeDays !== undefined
          ? t.maxAgeDays * 24 * 60 * 60 * 1000
          : Infinity,
        interval: t.retentionIntervalDays * 24 * 60 * 60 * 1000,
      }))
      .sort((a, b) => a.minAge - b.minAge);

    const toDelete: string[] = [];

    // Protect minRetention newest checkpoints
    const protectedSet = new Set(
      sorted.slice(-minRetention).map(cp => cp.checkpointId),
    );

    for (const checkpoint of sorted) {
      if (protectedSet.has(checkpoint.checkpointId)) continue;

      const age = currentTime - checkpoint.metadata.timestamp;

      // Find the highest (oldest) applicable tier
      const tier = [...tiers]
        .reverse()
        .find(t => age >= t.minAge && age < t.maxAge);

      if (!tier) continue; // Too young for any tier, keep it

      if (tier.interval === 0) continue; // Keep all in this tier

      // Group checkpoints in this tier by their interval window
      const windowKey = Math.floor(age / tier.interval);

      // Check if there's already a newer checkpoint kept in the same window
      const newerKept = sorted.some(cp => {
        if (protectedSet.has(cp.checkpointId)) return false;
        const cpAge = currentTime - cp.metadata.timestamp;
        return (
          cpAge >= tier.minAge &&
          cpAge < tier.maxAge &&
          Math.floor(cpAge / tier.interval) === windowKey &&
          cp.metadata.timestamp > checkpoint.metadata.timestamp &&
          !toDelete.includes(cp.checkpointId)
        );
      });

      if (newerKept) {
        toDelete.push(checkpoint.checkpointId);
      }
    }

    return this.applyDependencyProtection(toDelete, sorted, dependencyGraph);
  }

  /**
   * Apply dependency protection to prevent breaking delta chains
   */
  private applyDependencyProtection(
    toDelete: string[],
    sorted: CheckpointInfo[],
    dependencyGraph?: CheckpointDependencyGraph,
  ): string[] {
    if (!dependencyGraph || toDelete.length === 0) {
      return toDelete;
    }

    const candidateSet = new Set(toDelete);
    const allIds = new Set(sorted.map(cp => cp.checkpointId));

    for (const candidateId of toDelete) {
      const chainRoot = dependencyGraph.chainRootMap.get(candidateId);
      if (!chainRoot) continue;

      const chainMembers = dependencyGraph.chainGroups.get(chainRoot) || [];
      const survivingMembers = chainMembers.filter(id => !candidateSet.has(id) || id === candidateId);

      if (survivingMembers.length === 0) continue;

      const latestSurviving = survivingMembers[survivingMembers.length - 1];
      if (latestSurviving && latestSurviving !== candidateId) {
        const referencedBy = dependencyGraph.referencedBy.get(candidateId) || [];
        const hasDependent = referencedBy.some(ref => allIds.has(ref) && !candidateSet.has(ref));
        if (hasDependent) {
          const idx = toDelete.indexOf(candidateId);
          if (idx > -1) {
            toDelete.splice(idx, 1);
          }
        }
      }
    }

    return toDelete;
  }
}

/**
 * Implementation of a time-based cleaning strategy
 */
export class TimeBasedCleanupStrategy implements CheckpointCleanupStrategy {
  constructor(private policy: TimeBasedCleanupPolicy) {}

  execute(checkpoints: CheckpointInfo[], dependencyGraph?: CheckpointDependencyGraph): string[] {
    const currentTime = now();
    const retentionMs = this.policy.retentionDays * 24 * 60 * 60 * 1000;
    const minRetention = this.policy.minRetention || 0;

    const sorted = checkpoints
      .filter((cp): cp is CheckpointInfo => cp != null)
      .sort((a, b) => a.metadata.timestamp - b.metadata.timestamp);

    const toDelete: string[] = [];

    for (let i = 0; i < sorted.length; i++) {
      const checkpoint = sorted[i];
      if (!checkpoint) continue;

      const age = currentTime - checkpoint.metadata.timestamp;

      if (i < sorted.length - minRetention) {
        if (age > retentionMs) {
          toDelete.push(checkpoint.checkpointId);
        }
      }
    }

    return this.applyDependencyProtection(toDelete, sorted, dependencyGraph);
  }

  /**
   * Apply dependency protection to prevent breaking delta chains
   */
  private applyDependencyProtection(
    toDelete: string[],
    _sorted: CheckpointInfo[],
    dependencyGraph?: CheckpointDependencyGraph,
  ): string[] {
    if (!dependencyGraph || toDelete.length === 0) {
      return toDelete;
    }

    const candidateSet = new Set(toDelete);

    for (const candidateId of toDelete) {
      const chainRoot = dependencyGraph.chainRootMap.get(candidateId);
      if (!chainRoot) continue;

      const referencedBy = dependencyGraph.referencedBy.get(candidateId) || [];
      const hasDependent = referencedBy.some(ref => !candidateSet.has(ref));
      if (hasDependent) {
        const idx = toDelete.indexOf(candidateId);
        if (idx > -1) {
          toDelete.splice(idx, 1);
        }
      }
    }

    return toDelete;
  }
}

/**
 * Implementation of a cleaning strategy based on quantity
 */
export class CountBasedCleanupStrategy implements CheckpointCleanupStrategy {
  constructor(private policy: CountBasedCleanupPolicy) {}

  execute(checkpoints: CheckpointInfo[], dependencyGraph?: CheckpointDependencyGraph): string[] {
    const maxCount = this.policy.maxCount;
    const minRetention = this.policy.minRetention || 0;

    const sorted = checkpoints
      .filter((cp): cp is CheckpointInfo => cp != null)
      .sort((a, b) => a.metadata.timestamp - b.metadata.timestamp);

    if (sorted.length <= maxCount) {
      return [];
    }

    const toDelete: string[] = [];
    for (let i = 0; i < sorted.length; i++) {
      const checkpoint = sorted[i];
      if (!checkpoint) continue;

      if (i < sorted.length - minRetention) {
        toDelete.push(checkpoint.checkpointId);
      }

      if (toDelete.length >= sorted.length - maxCount) {
        break;
      }
    }

    return this.applyDependencyProtection(toDelete, sorted, dependencyGraph);
  }

  /**
   * Apply dependency protection to prevent breaking delta chains
   */
  private applyDependencyProtection(
    toDelete: string[],
    _sorted: CheckpointInfo[],
    dependencyGraph?: CheckpointDependencyGraph,
  ): string[] {
    if (!dependencyGraph || toDelete.length === 0) {
      return toDelete;
    }

    const candidateSet = new Set(toDelete);

    for (const candidateId of toDelete) {
      const referencedBy = dependencyGraph.referencedBy.get(candidateId) || [];
      const hasDependent = referencedBy.some(ref => !candidateSet.has(ref));
      if (hasDependent) {
        const idx = toDelete.indexOf(candidateId);
        if (idx > -1) {
          toDelete.splice(idx, 1);
        }
      }
    }

    return toDelete;
  }
}

/**
 * Implementation of a storage space cleanup strategy
 */
export class SizeBasedCleanupStrategy implements CheckpointCleanupStrategy {
  constructor(
    private policy: SizeBasedCleanupPolicy,
    private checkpointSizes: Map<string, number>,
  ) {}

  execute(checkpoints: CheckpointInfo[], dependencyGraph?: CheckpointDependencyGraph): string[] {
    const maxSize = this.policy.maxSizeBytes;
    const minRetention = this.policy.minRetention || 0;

    const sorted = checkpoints
      .filter((cp): cp is CheckpointInfo => cp != null)
      .sort((a, b) => a.metadata.timestamp - b.metadata.timestamp);

    let totalSize = 0;
    const missingSizeIds: string[] = [];
    for (const checkpoint of sorted) {
      const size = this.checkpointSizes.get(checkpoint.checkpointId);
      if (size === undefined) {
        missingSizeIds.push(checkpoint.checkpointId);
      }
      totalSize += size ?? 0;
    }

    if (missingSizeIds.length > 0) {
      logger.warn(
        `SizeBasedCleanupStrategy: ${missingSizeIds.length} checkpoint(s) have no recorded size and will be treated as 0 bytes`,
        { missingSizeIds },
      );
    }

    if (totalSize <= maxSize) {
      return [];
    }

    const toDelete: string[] = [];
    let currentSize = totalSize;

    for (let i = 0; i < sorted.length; i++) {
      const checkpoint = sorted[i];
      if (!checkpoint) continue;

      const checkpointId = checkpoint.checkpointId;
      const size = this.checkpointSizes.get(checkpointId) ?? 0;

      if (i < sorted.length - minRetention) {
        toDelete.push(checkpointId);
        currentSize -= size;

        if (currentSize <= maxSize) {
          break;
        }
      } else {
        break;
      }
    }

    return this.applyDependencyProtection(toDelete, sorted, dependencyGraph);
  }

  /**
   * Apply dependency protection to prevent breaking delta chains
   */
  private applyDependencyProtection(
    toDelete: string[],
    sorted: CheckpointInfo[],
    dependencyGraph?: CheckpointDependencyGraph,
  ): string[] {
    if (!dependencyGraph || toDelete.length === 0) {
      return toDelete;
    }

    const candidateSet = new Set(toDelete);
    const allIds = new Set(sorted.map(cp => cp.checkpointId));

    for (const candidateId of [...toDelete]) {
      const referencedBy = dependencyGraph.referencedBy.get(candidateId) || [];
      const hasDependent = referencedBy.some(ref => allIds.has(ref) && !candidateSet.has(ref));

      if (hasDependent) {
        const chainRoot = dependencyGraph.chainRootMap.get(candidateId);
        const chainMembers = chainRoot ? dependencyGraph.chainGroups.get(chainRoot) || [] : [];
        const fullInChain = chainMembers.some(
          id => !candidateSet.has(id) && sorted.find(s => s.checkpointId === id)?.metadata.checkpointType === "FULL",
        );

        if (!fullInChain) {
          const idx = toDelete.indexOf(candidateId);
          if (idx > -1) {
            toDelete.splice(idx, 1);
          }
        }
      }
    }

    return toDelete;
  }
}

/**
 * Create a cleanup policy instance
 *
 * @param policy Cleanup policy configuration
 * @param checkpointSizes Checkpoint size mapping (only for space-based policies)
 * @returns Cleanup policy instance
 */
export function createCleanupStrategy(
  policy: CleanupPolicy,
  checkpointSizes?: Map<string, number>,
): CheckpointCleanupStrategy {
  switch (policy.type) {
    case "time":
      return new TimeBasedCleanupStrategy(policy);
    case "count":
      return new CountBasedCleanupStrategy(policy);
    case "size":
      if (!checkpointSizes) {
        throw new Error("Size-based cleanup policy requires checkpointSizes parameter");
      }
      return new SizeBasedCleanupStrategy(policy, checkpointSizes);
    case "tiered":
      return new TieredCleanupStrategy(policy);
    default:
      throw new Error(`Unknown cleanup policy type: ${(policy as { type?: string }).type}`);
  }
}
