/**
 * Contribution Types - Type definitions for the contribution system.
 *
 * Defines ContributionType, ContributionRegistrar interface, and related types.
 */

// ============================================================
// Contribution Types
// ============================================================

/**
 * Recognized contribution types.
 * Only 7 types are currently implemented.
 * The following are removed until implemented:
 * 'evaluator', 'script-executor', 'resource',
 * 'prompt-template', 'fragment', 'skill-loader'
 */
export type ContributionType =
  | 'node-type'
  | 'tool-type'
  | 'llm-provider'
  | 'formatter'
  | 'hook-handler'
  | 'event-handler'
  | 'middleware';

// ============================================================
// Contribution Handler Types
// ============================================================

import type { IToolExecutor } from "../../services/tools/core/interfaces.js";

/**
 * @internal - Used internally by ContributionManager for tool executor storage.
 */
export type IToolExecutorConstructor = new (...args: unknown[]) => IToolExecutor;

export type HookHandler = (context: Record<string, unknown>) => Promise<void>;

export type EventHandler = (event: Record<string, unknown>) => void | Promise<void>;