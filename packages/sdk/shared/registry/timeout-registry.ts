/**
 * Timeout Registry (Deprecated and Removed)
 *
 * This file is kept for reference only.
 * TimeoutManager is now held directly on ExecutionEntity.
 *
 * @deprecated Use executionEntity.timeoutManager directly
 */

// TimeoutRegistry class has been removed.
// Use executionEntity.timeoutManager instead:
//
// Example:
// ```typescript
// const entity = executionHierarchyRegistry.get(executionId);
// const manager = entity.timeoutManager;
// const handle = manager.register({ ... });
// ```
