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

    // Sort in ascending order by timestamp (with the oldest item at the front)
    const sorted = [...checkpoints].sort((a, b) => a.metadata.timestamp - b.metadata.timestamp);

    // Identify the checkpoints that need to be deleted.
    const toDelete: string[] = [];

    // Check from the oldest checkpoint.
    for (let i = 0; i < sorted.length; i++) {
      const checkpoint = sorted[i];
      if (!checkpoint) continue;

      const age = currentTime - checkpoint.metadata.timestamp;

      // Ensure that at least `minRetention` checkpoints are retained.
      // Retain the latest `minRetention` checkpoints (the `minRetention` number of checkpoints at the end of the array).
      if (i < sorted.length - minRetention) {
        // Delete checkpoints that exceed the retention period.
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

    // Sort in descending order by timestamp.
    const sorted = [...checkpoints].sort((a, b) => b.metadata.timestamp - a.metadata.timestamp);

    // If the number of checkpoints does not exceed the maximum value, there is no need to delete any.
    if (sorted.length <= maxCount) {
      return [];
    }

    // Calculate the number of items that need to be deleted.
    const deleteCount = Math.max(0, sorted.length - maxCount);

    // Ensure that at least `minRetention` checkpoints are retained.
    const actualDeleteCount = Math.min(deleteCount, sorted.length - minRetention);

    // Return the checkpoint IDs that need to be deleted (starting from the oldest one).
    return sorted.slice(sorted.length - actualDeleteCount).map(cp => cp.checkpointId);
  }
}

/**
 * Implementation of a storage space cleanup strategy
 */
export class SizeBasedCleanupStrategy implements CheckpointCleanupStrategy {
  constructor(
    private policy: SizeBasedCleanupPolicy,
    private checkpointSizes: Map<string, number>, // checkpointId -> size in bytes
  ) {}

  execute(checkpoints: CheckpointInfo[]): string[] {
    const maxSize = this.policy.maxSizeBytes;
    const minRetention = this.policy.minRetention || 0;

    // Sort in ascending order by timestamp (with the oldest first)
    const sorted = [...checkpoints].sort((a, b) => a.metadata.timestamp - b.metadata.timestamp);

    // Calculate the total storage space
    let totalSize = 0;
    for (const checkpoint of sorted) {
      const size = this.checkpointSizes.get(checkpoint.checkpointId) || 0;
      totalSize += size;
    }

    // If the total storage space does not exceed the maximum value, there is no need to delete anything.
    if (totalSize <= maxSize) {
      return [];
    }

    // Delete from the oldest checkpoint until the space requirements are met.
    const toDelete: string[] = [];
    let currentSize = totalSize;

    // Delete from the oldest checkpoint onwards.
    for (let i = 0; i < sorted.length; i++) {
      const checkpoint = sorted[i];
      if (!checkpoint) continue;

      const checkpointId = checkpoint.checkpointId;
      const size = this.checkpointSizes.get(checkpointId) || 0;

      // Ensure that at least `minRetention` checkpoints are retained.
      // Retain the latest `minRetention` checkpoints (the `minRetention` number of checkpoints at the end of the array).
      if (i < sorted.length - minRetention) {
        // Delete checkpoints
        toDelete.push(checkpointId);
        currentSize -= size;

        // Stop deleting if the space requirements have already been met.
        if (currentSize <= maxSize) {
          break;
        }
      } else {
        // If the current checkpoint is one of the `minRetention` checkpoints that need to be retained, stop the deletion process.
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
