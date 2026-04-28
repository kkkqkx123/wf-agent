/**
 * Common Event Builder Utilities
 * Provides common utilities for event builders
 *
 * Design Principles:
 * - Pure functions: All methods are pure functions with no side effects
 * - Type safety: Automatic parameter type inference through generics
 * - Unified format: Ensures all event objects have consistent format
 */

import { now } from "@wf-agent/common-utils";
import type { BaseEvent } from "@wf-agent/types";
import { SDKError } from "@wf-agent/types";

// =============================================================================
// Type Utilities
// =============================================================================

/** Build parameters type (exclude type and timestamp) */
export type BuildParams<T extends BaseEvent> = Omit<T, "type" | "timestamp">;

/** Build parameters type with optional workflowId and nodeId */
export type BuildParamsWithOptionalContext<T extends BaseEvent> = BuildParams<T> & {
  workflowId?: string;
  nodeId?: string;
};

/** Error parameters type */
export type ErrorParams<T extends BaseEvent & { error: unknown }> = Omit<
  BuildParams<T>,
  "error"
> & {
  error: Error;
};

// =============================================================================
// Builder Factories
// =============================================================================

/**
 * Create standard event builder
 * @param type Event type
 * @returns Event builder function
 */
export const createBuilder =
  <T extends BaseEvent>(type: T["type"]) =>
  (params: BuildParams<T>): T =>
    ({ type, timestamp: now(), ...params }) as T;

/**
 * Create event builder with optional context (workflowId, nodeId)
 * @param type Event type
 * @returns Event builder function
 */
export const createBuilderWithOptionalContext =
  <T extends BaseEvent>(type: T["type"]) =>
  (params: BuildParamsWithOptionalContext<T>): T =>
    ({ type, timestamp: now(), ...params }) as T;

/**
 * Create error event builder with Error transformation
 * @param type Event type
 * @returns Event builder function
 */
export const createErrorBuilder =
  <T extends BaseEvent & { error: unknown }>(type: T["type"]) =>
  (params: ErrorParams<T>): T =>
    ({
      type,
      timestamp: now(),
      ...params,
      error:
        params.error instanceof SDKError
          ? params.error.toJSON()
          : { message: params.error.message, name: params.error.name },
    }) as T;

/**
 * Create error event builder that converts Error to string
 * @param type Event type
 * @returns Event builder function
 */
export const createStringErrorBuilder =
  <T extends BaseEvent & { error: string }>(type: T["type"]) =>
  (params: Omit<BuildParams<T>, "error"> & { error: Error }): T =>
    ({
      type,
      timestamp: now(),
      ...params,
      error:
        params.error instanceof SDKError
          ? JSON.stringify(params.error.toJSON())
          : params.error.message,
    }) as T;
