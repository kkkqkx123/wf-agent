/**
 * Registry Options - Registry Operation Options Type Definition
 *
 * Provides unified options for registering, updating, and deleting operations
 */

/**
 * Register Action Options
 */
export interface RegisterOptions {
  /**
   * Skip existing items (idempotent operation)
   * - true: skip if already exists, no error is thrown
   * - false/undefined: throw error if already exists
   */
  skipIfExists?: boolean;
}

/**
 * Batch registration operation options
 */
export interface BatchRegisterOptions extends RegisterOptions {
  /**
   * Skip errors and continue execution
   * - true: skips the item when an error is encountered and proceeds to the next item
   * - false/undefined: throws the item immediately upon encountering an error
   */
  skipErrors?: boolean;
}

/**
 * Delete Operation Options
 */
export interface UnregisterOptions {
  /**
   * Force deletion, ignore reference checking
   * - true: force delete even if there is a reference
   * - false/undefined: throws an error if there is a reference.
   */
  force?: boolean;

  /**
   * Whether to check for references
   * - true/undefined: check for references, if there are references the behavior is determined by the force
   * - false: skip reference checking
   */
  checkReferences?: boolean;
}

/**
 * Batch delete operation options
 */
export interface BatchUnregisterOptions extends UnregisterOptions {
  /**
   * Skip error to continue execution
   */
  skipErrors?: boolean;
}

/**
 * Update Operation Options
 */
export interface UpdateOptions {
  /**
   * Allow updating of non-existing items (automatic creation)
   * - true: create if it doesn't exist
   * - false/undefined: throws an error if it doesn't exist
   */
  createIfNotExists?: boolean;
}
