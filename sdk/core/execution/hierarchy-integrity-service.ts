/**
 * Hierarchy Integrity Service
 * 
 * Handles the integrity validation and repair of execution hierarchy metadata
 * during checkpoint restoration. Ensures that parent and child references point
 * to valid entities in the registry and repairs any inconsistencies.
 */

import type { ExecutionHierarchyMetadata, ID } from '@wf-agent/types';
import type { AnyExecutionEntity } from '../registry/execution-hierarchy-registry.js';
import { sdkLogger as logger } from '../../utils/logger.js';

export interface HierarchyValidationResult {
  valid: boolean;
  issues: string[];
}

/**
 * Abstract interface for the registry to avoid circular dependencies
 */
export interface IHierarchyRegistry {
  has(executionId: ID): boolean;
  get(executionId: ID): AnyExecutionEntity | undefined;
}

export class HierarchyIntegrityService {
  /**
   * Validates hierarchy integrity against the registry
   * 
   * Checks:
   * 1. Parent reference exists in registry (if present)
   * 2. All child references exist in registry
   * 3. No orphaned references
   */
  static validateIntegrity(
    hierarchy: ExecutionHierarchyMetadata,
    registry: IHierarchyRegistry
  ): HierarchyValidationResult {
    const issues: string[] = [];

    // Validate parent reference
    if (hierarchy.parent) {
      const parentId = hierarchy.parent.parentId;
      if (!registry.has(parentId)) {
        issues.push(`Parent ${parentId} (${hierarchy.parent.parentType}) not found in registry`);
      }
    }

    // Validate child references
    for (const child of hierarchy.children) {
      if (!registry.has(child.childId)) {
        issues.push(
          `Child ${child.childId} (${child.childType}) not found in registry`
        );
      }
    }

    return {
      valid: issues.length === 0,
      issues,
    };
  }

  /**
   * Cleans up orphaned references in hierarchy metadata
   * 
   * Removes references to entities that no longer exist in the registry.
   */
  static cleanupOrphanedReferences(
    hierarchy: ExecutionHierarchyMetadata,
    registry: IHierarchyRegistry
  ): ExecutionHierarchyMetadata {
    const cleaned: ExecutionHierarchyMetadata = {
      ...hierarchy,
      children: [],
    };

    // Keep parent only if it exists in registry
    if (hierarchy.parent && registry.has(hierarchy.parent.parentId)) {
      cleaned.parent = hierarchy.parent;
    } else if (hierarchy.parent) {
      logger.warn('Removing orphaned parent reference', {
        parentId: hierarchy.parent.parentId,
        parentType: hierarchy.parent.parentType,
      });
      cleaned.parent = undefined;
    }

    // Keep only children that exist in registry
    for (const child of hierarchy.children) {
      if (registry.has(child.childId)) {
        cleaned.children.push(child);
      } else {
        logger.warn('Removing orphaned child reference', {
          childId: child.childId,
          childType: child.childType,
        });
      }
    }

    return cleaned;
  }

  /**
   * Repairs hierarchy by recalculating root information
   * 
   * If parent reference is valid but root info is incorrect,
   * this function recalculates the correct root execution info.
   */
  static repairRootInfo(
    hierarchy: ExecutionHierarchyMetadata,
    registry: IHierarchyRegistry
  ): ExecutionHierarchyMetadata {
    if (!hierarchy.parent) {
      // No parent, this is root - root info should match own identity
      return hierarchy;
    }

    const parentEntity = registry.get(hierarchy.parent.parentId);
    if (!parentEntity || !('getRootExecutionId' in parentEntity)) {
      // Cannot repair without parent entity
      logger.warn('Cannot repair root info: parent entity not available', {
        parentId: hierarchy.parent.parentId,
      });
      return hierarchy;
    }

    // Inherit correct root info from parent
    const repaired: ExecutionHierarchyMetadata = {
      ...hierarchy,
      rootExecutionId: parentEntity.getRootExecutionId(),
      rootExecutionType: parentEntity.getRootExecutionType(),
    };

    logger.info('Repaired root execution info', {
      oldRootId: hierarchy.rootExecutionId,
      newRootId: repaired.rootExecutionId,
      oldRootType: hierarchy.rootExecutionType,
      newRootType: repaired.rootExecutionType,
    });

    return repaired;
  }
}
