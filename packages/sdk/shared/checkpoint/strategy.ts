/**
 * Unified Checkpoint Strategy
 *
 * Provides a unified interface for managing checkpoint creation across
 * different execution contexts (Workflow, Agent Loop). Encapsulates
 * the logic for determining when to create checkpoints and what content
 * to include.
 *
 * Design:
 * - Single responsibility: checkpoint decision logic
 * - Stateless: enables easy composition and testing
 * - Framework-agnostic: works with any executor/coordinator
 */

import type {
  CheckpointTriggerType,
  UnifiedCheckpointPolicy,
  CheckpointContext,
  CheckpointMetadata,
  CheckpointContentConfig,
  CheckpointRetentionConfig,
} from '@wf-agent/types';
import { CheckpointTrigger, CHECKPOINT_POLICIES } from '@wf-agent/types';

/**
 * Unified Checkpoint Strategy
 *
 * Manages checkpoint creation decisions based on configured policy
 * and current execution context.
 */
export class CheckpointStrategy {
  private policy: UnifiedCheckpointPolicy;
  private triggers: Set<CheckpointTriggerType>;

  /**
   * Create a new checkpoint strategy instance
   *
   * @param policy - Checkpoint policy configuration
   */
  constructor(policy: UnifiedCheckpointPolicy) {
    this.policy = policy;

    // Normalize triggers to a Set for efficient lookup
    const triggers = Array.isArray(policy.triggers)
      ? policy.triggers
      : [policy.triggers];
    this.triggers = new Set(triggers);
  }

  /**
   * Determine if a checkpoint should be created for a given trigger
   *
   * @param trigger - The checkpoint trigger event
   * @param context - Optional execution context for advanced decisions
   * @returns true if a checkpoint should be created
   *
   * @example
   * ```typescript
   * const strategy = new CheckpointStrategy(CHECKPOINT_POLICY_STANDARD);
   *
   * if (strategy.shouldCheckpoint(CheckpointTrigger.ON_ERROR, {
   *   entityType: 'agent-loop',
   *   entityId: 'agent-123',
   *   error: new Error('Failed')
   * })) {
   *   await createCheckpoint(...);
   * }
   * ```
   */
  shouldCheckpoint(
    trigger: CheckpointTriggerType,
    _context?: CheckpointContext
  ): boolean {
    // If checkpointing is disabled, never create checkpoints
    if (!this.policy.enabled) {
      return false;
    }

    // NEVER is a special trigger that always means "don't checkpoint"
    if (this.triggers.has(CheckpointTrigger.NEVER)) {
      return false;
    }

    // Check if this trigger is configured
    const shouldCreate = this.triggers.has(trigger);

    // Future enhancement: context-aware filtering
    // Example: skip ON_INTERVAL if we just created a checkpoint
    // The _context parameter is reserved for future sophisticated decision-making
    // while maintaining the current simple behavior for now
    return shouldCreate;
  }

  /**
   * Get the content configuration for checkpoints created by this strategy
   *
   * @returns Content configuration object
   *
   * @example
   * ```typescript
   * const contentConfig = strategy.getContentConfig();
   * const checkpoint = {
   *   ...snapshot,
   *   includeState: contentConfig.includeState,
   *   includeHistory: contentConfig.includeHistory,
   * };
   * ```
   */
  getContentConfig(): Required<CheckpointContentConfig> {
    return {
      includeState: this.policy.content?.includeState ?? true,
      includeHistory: this.policy.content?.includeHistory ?? true,
      includeStatistics: this.policy.content?.includeStatistics ?? false,
      metadata: this.policy.content?.metadata ?? {},
    };
  }

  /**
   * Check if retention limit is exceeded
   *
   * Used by cleanup processes to determine if old checkpoints should be deleted.
   *
   * @param currentCount - Current number of checkpoints for this entity
   * @param oldestTimestamp - Timestamp of the oldest checkpoint (milliseconds)
   * @returns true if retention limits are exceeded
   *
   * @example
   * ```typescript
   * const count = await storage.countCheckpoints(entityId);
   * const oldest = await storage.getOldestCheckpointTime(entityId);
   *
   * if (strategy.isRetentionExceeded(count, oldest)) {
   *   await cleanupOldCheckpoints(entityId);
   * }
   * ```
   */
  isRetentionExceeded(currentCount: number, oldestTimestamp: number): boolean {
    const retention = this.policy.retention;
    if (!retention) {
      return false;
    }

    // Check max checkpoint count limit
    if (
      retention.maxCheckpoints !== undefined &&
      retention.maxCheckpoints >= 0 &&
      currentCount > retention.maxCheckpoints
    ) {
      return true;
    }

    // Check max age limit
    if (
      retention.maxAge !== undefined &&
      retention.maxAge >= 0 &&
      Date.now() - oldestTimestamp > retention.maxAge
    ) {
      return true;
    }

    return false;
  }

  /**
   * Get metadata to attach to created checkpoints
   *
   * Automatically generates tags based on content configuration
   * and merges with custom metadata from policy.
   *
   * @returns Checkpoint metadata object
   *
   * @example
   * ```typescript
   * const metadata = strategy.getMetadata();
   * const checkpoint = {
   *   ...core,
   *   metadata: metadata,
   * };
   * ```
   */
  getMetadata(): CheckpointMetadata {
    const tags = this.generateTags();
    const customFields = this.policy.content?.metadata ?? {};

    return {
      tags: tags.length > 0 ? tags : undefined,
      customFields: Object.keys(customFields).length > 0 ? customFields : undefined,
    };
  }

  /**
   * Get the retention configuration
   *
   * @returns Retention configuration or undefined if not configured
   */
  getRetentionConfig(): CheckpointRetentionConfig | undefined {
    return this.policy.retention;
  }

  /**
   * Check if compression is enabled for checkpoints
   *
   * @returns Compression strategy
   */
  getCompressionStrategy(): 'none' | 'gzip' | 'auto' {
    return this.policy.retention?.compression ?? 'auto';
  }

  /**
   * Get error handling configuration
   */
  getErrorHandlingConfig() {
    return {
      failOnCheckpointError: this.policy.errorHandling?.failOnCheckpointError ?? false,
      retryOnFailure: this.policy.errorHandling?.retryOnFailure ?? true,
      maxRetries: this.policy.errorHandling?.maxRetries ?? 3,
    };
  }

  /**
   * Check if checkpointing is completely disabled
   */
  isDisabled(): boolean {
    return !this.policy.enabled || this.triggers.has(CheckpointTrigger.NEVER);
  }

  /**
   * Get a human-readable description of this strategy
   *
   * Useful for logging and debugging.
   *
   * @returns Description string
   *
   * @example
   * ```
   * "CheckpointStrategy(enabled=true, triggers=[ON_ERROR, ON_COMPLETE])"
   * ```
   */
  toString(): string {
    if (this.isDisabled()) {
      return 'CheckpointStrategy(enabled=false)';
    }

    const triggerList = Array.from(this.triggers)
      .sort()
      .join(', ');

    return `CheckpointStrategy(enabled=true, triggers=[${triggerList}])`;
  }

  // ============================================================================
  // Private Helpers
  // ============================================================================

  /**
   * Generate tags based on content configuration
   *
   * Tags help with filtering and organizing checkpoints.
   */
  private generateTags(): string[] {
    const tags: string[] = [];
    const content = this.policy.content;

    if (content?.includeState) {
      tags.push('has-state');
    }
    if (content?.includeHistory) {
      tags.push('has-history');
    }
    if (content?.includeStatistics) {
      tags.push('has-statistics');
    }

    return tags;
  }
}

// =============================================================================
// Factory Functions
// =============================================================================

/**
 * Create a checkpoint strategy from a policy name
 *
 * @param policyName - One of 'MINIMAL', 'STANDARD', 'COMPREHENSIVE', 'NONE'
 * @returns CheckpointStrategy instance
 *
 * @example
 * ```typescript
 * const strategy = createCheckpointStrategy('STANDARD');
 * ```
 */
export function createCheckpointStrategy(
  policyName: keyof typeof CHECKPOINT_POLICIES
): CheckpointStrategy {
  const policy = CHECKPOINT_POLICIES[policyName];
  return new CheckpointStrategy(policy);
}

/**
 * Create a checkpoint strategy from a custom policy
 *
 * @param policy - Custom checkpoint policy
 * @returns CheckpointStrategy instance
 *
 * @example
 * ```typescript
 * const policy: UnifiedCheckpointPolicy = {
 *   enabled: true,
 *   triggers: [CheckpointTrigger.ON_ERROR],
 * };
 * const strategy = createCheckpointStrategyFromPolicy(policy);
 * ```
 */
export function createCheckpointStrategyFromPolicy(
  policy: UnifiedCheckpointPolicy
): CheckpointStrategy {
  return new CheckpointStrategy(policy);
}

// =============================================================================
// Utility Functions
// =============================================================================

/**
 * Check if a trigger should create a checkpoint with state included
 *
 * Helper for determining if full state capture is needed.
 */
export function shouldIncludeStateForTrigger(trigger: CheckpointTriggerType): boolean {
  // State is typically important for error and lifecycle events
  const stateImportantTriggers = [
    CheckpointTrigger.BEFORE_EXECUTE,
    CheckpointTrigger.ON_ERROR,
    CheckpointTrigger.ITERATION_FAILED,
    CheckpointTrigger.ON_COMPLETE,
  ];

  return stateImportantTriggers.includes(trigger);
}

/**
 * Check if a trigger typically involves tool invocation
 */
export function isTriggerRelatedToToolInvocation(trigger: CheckpointTriggerType): boolean {
  const toolRelatedTriggers = [
    CheckpointTrigger.TOOL_BEFORE,
    CheckpointTrigger.TOOL_AFTER,
  ];

  return toolRelatedTriggers.includes(trigger);
}

/**
 * Check if a trigger is related to error recovery (retry, fallback)
 */
export function isTriggerRelatedToRecovery(trigger: CheckpointTriggerType): boolean {
  const recoveryTriggers = [
    CheckpointTrigger.ON_ERROR,
    CheckpointTrigger.BEFORE_RETRY,
    CheckpointTrigger.AFTER_RETRY_SUCCESS,
    CheckpointTrigger.ON_FALLBACK,
  ];

  return recoveryTriggers.includes(trigger);
}
