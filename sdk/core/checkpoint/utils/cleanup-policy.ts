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
  CheckpointCleanupStrategy,
} from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";

/**
 * Implementation of a time-based cleaning strategy
 */
export class TimeBasedCleanupStrategy implements CheckpointCleanupStrategy {
  constructor(private policy: TimeBasedCleanupPolicy) {}

  execute(checkpoints: CheckpointInfo[]): string[] {
    const currentTime = now();
    const retentionMs = this.policy.retentionDays * 24 * 60 * 60 * 1000;
    const minRetention = this.policy.minRetention || 0;

    const sorted = [...checkpoints].sort((a, b) => a.metadata.timestamp - b.metadata.timestamp);

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

    return toDelete;
  }
}

/**
 * Implementation of a cleaning strategy based on quantity
 */
export class CountBasedCleanupStrategy implements CheckpointCleanupStrategy {
  constructor(private policy: CountBasedCleanupPolicy) {}

  execute(checkpoints: CheckpointInfo[]): string[] {
    const maxCount = this.policy.maxCount;
    const minRetention = this.policy.minRetention || 0;

    const sorted = [...checkpoints].sort((a, b) => b.metadata.timestamp - a.metadata.timestamp);

    if (sorted.length <= maxCount) {
      return [];
    }

    const deleteCount = Math.max(0, sorted.length - maxCount);

    const actualDeleteCount = Math.min(deleteCount, sorted.length - minRetention);

    return sorted.slice(sorted.length - actualDeleteCount).map(cp => cp.checkpointId);
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

  execute(checkpoints: CheckpointInfo[]): string[] {
    const maxSize = this.policy.maxSizeBytes;
    const minRetention = this.policy.minRetention || 0;

    const sorted = [...checkpoints].sort((a, b) => a.metadata.timestamp - b.metadata.timestamp);

    let totalSize = 0;
    for (const checkpoint of sorted) {
      const size = this.checkpointSizes.get(checkpoint.checkpointId) || 0;
      totalSize += size;
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
      const size = this.checkpointSizes.get(checkpointId) || 0;

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
    default:
      throw new Error(`Unknown cleanup policy type: ${(policy as { type?: string }).type}`);
  }
}