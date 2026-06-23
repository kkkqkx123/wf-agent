/**
 * Factory interface for services that need runtime parameters
 *
 * This interface provides a type-safe way to create service instances
 * that require runtime arguments (like executionId, workflowId, etc.)
 */

import type { ExecutionDomainContext } from "@wf-agent/types";
import type { WorkflowExecutionEntity } from "@sdk/workflow/entities/workflow-execution-entity.js";

/**
 * Generic factory interface for creating service instances
 * @template T The type of service to create
 * @template Args Arguments required for creation (defaults to void for no args)
 */
export interface ServiceFactory<T, Args extends unknown[] = []> {
  /**
   * Create a new instance of the service
   * @param args Arguments required for creation
   * @returns A new instance of type T
   */
  create(...args: Args): T;
}

/**
 * Factory for services that require a single string ID parameter
 * Common pattern for execution-scoped or session-scoped services
 * @template T The type of service to create
 */
export interface IdBasedServiceFactory<T> extends ServiceFactory<T, [string]> {
  create(id: string): T;
}

/**
 * Factory for services that require no parameters
 * @template T The type of service to create
 */
export interface NoArgServiceFactory<T> extends ServiceFactory<T, []> {
  create(): T;
}

/**
 * Factory for services with optional parameters
 * Used for services where some creation parameters are optional
 * @template T The type of service to create
 */
export interface OptionalParamsServiceFactory<T> {
  create(executionId?: string, workflowId?: string): T;
}

/**
 * Factory for services requiring execution entity
 * @template T The type of service to create
 */
export interface ExecutionEntityServiceFactory<T> {
  create(executionEntity: WorkflowExecutionEntity): T;
}

/**
 * Factory for node execution coordinator with multiple parameters
 * @template T The type of service to create
 */
export interface NodeExecutionCoordinatorFactory<T> {
  create(executionId: string, nodeId: string, executionEntity: WorkflowExecutionEntity): T;
}

/**
 * Factory for interruption state with execution ID and domain-specific context
 * @template T The type of service to create
 */
export interface InterruptionStateFactory<T> {
  create(executionId: string, context?: ExecutionDomainContext): T;
}

/**
 * Helper function to create a typed factory with proper type inference
 * @template T The service type
 * @template Args Creation arguments
 * @param factoryFn The factory creation function
 * @returns A properly typed factory
 */
export function createFactory<T, Args extends unknown[]>(
  factoryFn: (...args: Args) => T,
): ServiceFactory<T, Args> {
  return {
    create: factoryFn,
  };
}

/**
 * Helper function to create an ID-based factory
 * @template T The service type
 * @param factoryFn The factory creation function taking an ID
 * @returns A properly typed ID-based factory
 */
export function createIdBasedFactory<T>(factoryFn: (id: string) => T): IdBasedServiceFactory<T> {
  return {
    create: factoryFn,
  };
}
