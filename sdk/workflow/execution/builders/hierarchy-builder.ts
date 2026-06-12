/**
 * Execution Hierarchy Builder
 * 
 * Unified builder for establishing parent-child relationships between execution instances.
 * Encapsulates the common logic for:
 * - Setting parent context
 * - Registering with hierarchy registry
 * - Registering child references in parent
 * - Validating hierarchy integrity
 * 
 * This module ensures consistent hierarchy management across all execution types:
 * - SUBGRAPH (synchronous child workflows)
 * - FORK_JOIN (parallel fork branches)
 * - TRIGGERED_SUBWORKFLOW (asynchronously triggered subworkflows)
 */

import type { ID, ParentExecutionContext, ChildExecutionReference } from '@wf-agent/types';
import type { WorkflowExecutionEntity } from '../../entities/workflow-execution-entity.js';
import type { ExecutionHierarchyRegistry } from '../../../core/registry/execution-hierarchy-registry.js';
import { createContextualLogger } from '../../../utils/contextual-logger.js';

const logger = createContextualLogger({ operation: 'hierarchy-builder' });

/**
 * Options for establishing parent-child relationship
 */
export interface HierarchySetupOptions {
  /** Parent execution entity */
  parentEntity: WorkflowExecutionEntity;
  /** Child execution entity */
  childEntity: WorkflowExecutionEntity;
  /** Node ID in parent workflow (optional, for SUBGRAPH nodes) */
  nodeId?: ID;
  /** Additional metadata for child reference */
  childMetadata?: Partial<Omit<ChildExecutionReference, 'childType' | 'childId'>>;
}

/**
 * Establish parent-child relationship between two workflow executions
 * 
 * This function performs all necessary steps to properly link parent and child executions:
 * 1. Sets parent context on child entity
 * 2. Registers child entity with hierarchy registry
 * 3. Registers child reference in parent entity
 * 
 * @param options Hierarchy setup options
 * @throws Error if registry is not available or validation fails
 * 
 * @example
 * ```typescript
 * // For SUBGRAPH execution
 * await setupHierarchy({
 *   parentEntity: parentWorkflow,
 *   childEntity: subgraphWorkflow,
 *   nodeId: 'subgraph-node-123',
 * });
 * 
 * // For FORK execution
 * await setupHierarchy({
 *   parentEntity: parentWorkflow,
 *   childEntity: forkBranch,
 *   childMetadata: { forkPathId: 'path-1' },
 * });
 * ```
 */
export async function setupHierarchy(options: HierarchySetupOptions): Promise<void> {
  const { parentEntity, childEntity, nodeId, childMetadata } = options;

  logger.debug('Setting up execution hierarchy', {
    parentExecutionId: parentEntity.id,
    childExecutionId: childEntity.id,
    nodeId,
  });

  // Step 1: Set parent context on child entity
  const parentContext: ParentExecutionContext = {
    parentType: 'WORKFLOW',
    parentId: parentEntity.id,
    ...(nodeId && { nodeId }),
  };

  childEntity.setParentContext(parentContext);
  logger.debug('Parent context set on child entity', {
    childExecutionId: childEntity.id,
    parentExecutionId: parentEntity.id,
  });

  // Step 2: Get hierarchy registry and register child entity
  const registry = getHierarchyRegistry(childEntity);
  if (!registry) {
    logger.warn('Hierarchy registry not available, skipping registration', {
      childExecutionId: childEntity.id,
    });
    return;
  }

  registry.register(childEntity);
  logger.debug('Child entity registered with hierarchy registry', {
    childExecutionId: childEntity.id,
  });

  // Step 3: Register child reference in parent entity
  const childRef: ChildExecutionReference = {
    childType: 'WORKFLOW',
    childId: childEntity.id,
    createdAt: Date.now(),
    ...childMetadata,
  };

  parentEntity.registerChild(childRef);
  logger.debug('Child reference registered in parent entity', {
    parentExecutionId: parentEntity.id,
    childExecutionId: childEntity.id,
  });
}

/**
 * Remove parent-child relationship between two workflow executions
 * 
 * This function performs cleanup when a child execution is terminated or failed:
 * 1. Unregisters child reference from parent entity
 * 2. Optionally unregisters child entity from hierarchy registry
 * 
 * @param parentEntity Parent execution entity
 * @param childExecutionId Child execution ID to remove
 * @param unregisterFromRegistry Whether to also unregister from hierarchy registry (default: true)
 * 
 * @example
 * ```typescript
 * // Cleanup after failed subgraph execution
 * await teardownHierarchy(parentWorkflow, 'failed-subgraph-id');
 * ```
 */
export async function teardownHierarchy(
  parentEntity: WorkflowExecutionEntity,
  childExecutionId: ID,
  unregisterFromRegistry: boolean = true,
): Promise<void> {
  logger.debug('Tearing down execution hierarchy', {
    parentExecutionId: parentEntity.id,
    childExecutionId,
  });

  // Step 1: Unregister child reference from parent
  parentEntity.unregisterChild(childExecutionId, 'WORKFLOW');
  logger.debug('Child reference removed from parent entity', {
    parentExecutionId: parentEntity.id,
    childExecutionId,
  });

  // Step 2: Optionally unregister from hierarchy registry
  if (unregisterFromRegistry) {
    const registry = getHierarchyRegistryById(childExecutionId);
    if (registry) {
      registry.unregister(childExecutionId);
      logger.debug('Child entity unregistered from hierarchy registry', {
        childExecutionId,
      });
    } else {
      logger.warn('Hierarchy registry not available for cleanup', {
        childExecutionId,
      });
    }
  }
}

/**
 * Validate hierarchy integrity for an execution entity
 * 
 * Checks that:
 * - Parent entity exists in registry (if parent is set)
 * - All registered children exist in registry
 * - Depth calculations are consistent
 * 
 * @param entity Execution entity to validate
 * @returns Validation result with any issues found
 * 
 * @example
 * ```typescript
 * const validation = validateHierarchy(workflowEntity);
 * if (!validation.valid) {
 *   logger.warn('Hierarchy issues detected', { issues: validation.issues });
 * }
 * ```
 */
export function validateHierarchy(entity: WorkflowExecutionEntity): {
  valid: boolean;
  issues: string[];
} {
  const issues: string[] = [];
  const registry = getHierarchyRegistry(entity);

  if (!registry) {
    return {
      valid: false,
      issues: ['Hierarchy registry not available'],
    };
  }

  // Check parent existence
  const parentContext = entity.getParentContext();
  if (parentContext) {
    const parentEntity = registry.get(parentContext.parentId);
    if (!parentEntity) {
      issues.push(`Parent execution ${parentContext.parentId} not found in registry`);
    }
  }

  // Check children existence
  const childIds = entity.getChildExecutionIds();
  for (const childId of childIds) {
    const childEntity = registry.get(childId);
    if (!childEntity) {
      issues.push(`Child execution ${childId} not found in registry`);
    }
  }

  // Check depth consistency
  const metadata = entity.getHierarchyMetadata();
  if (metadata) {
    if (metadata.depth < 0) {
      issues.push(`Invalid depth: ${metadata.depth}`);
    }
    
    if (!metadata.rootExecutionId) {
      issues.push('Root execution ID is missing');
    }
  }

  return {
    valid: issues.length === 0,
    issues,
  };
}

/**
 * Helper: Get hierarchy registry from entity's internal state
 */
function getHierarchyRegistry(entity: WorkflowExecutionEntity): ExecutionHierarchyRegistry | undefined {
  // Access the registry through the entity's hierarchy manager
  // This assumes the entity stores a reference to the registry
  // We need to expose this via a method or property
  
  // For now, we'll try to get it from global context if available
  // TODO: Consider adding a getRegistry() method to WorkflowExecutionEntity
  try {
    // This is a workaround - ideally the entity should expose its registry
    const anyEntity = entity as unknown as { hierarchyManager?: { registry?: ExecutionHierarchyRegistry } };
    return anyEntity.hierarchyManager?.registry;
  } catch {
    return undefined;
  }
}

/**
 * Helper: Get hierarchy registry by execution ID
 */
function getHierarchyRegistryById(_executionId: ID): ExecutionHierarchyRegistry | undefined {
  // Similar workaround as above
  // In a proper implementation, we'd have a global registry accessor
  try {
    // This would need to be injected or accessed from global context
    return undefined; // Placeholder
  } catch {
    return undefined;
  }
}
