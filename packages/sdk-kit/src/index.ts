/**
 * SDK-Kit - High-level API wrapper for Workflow SDK
 *
 * Simplifies common scenarios for new applications.
 *
 * Phase 1 Core Features:
 * - WorkflowAPI: Define workflows programmatically
 * - ExecutionAPI: Execute workflows with simplified interface
 * - QueryAPI: Query execution records
 * - ResourceAPI: Manage workflow resources (CRUD + versioning)
 * - Event management: Execution event tracking and monitoring
 * - Unified error handling
 *
 * Usage:
 *
 * ```typescript
 * import { SDKKit } from '@wf-agent/sdk-kit';
 * import { createSDK } from '@wf-agent/sdk';
 *
 * const sdk = createSDK();
 * const kit = new SDKKit(sdk);
 *
 * // Define workflow
 * const template = kit.workflow()
 *   .create('my-workflow')
 *   .node('start', { type: 'START' })
 *   .node('task', { type: 'LLM' })
 *   .edge('start', 'task')
 *   .build();
 *
 * // Manage workflow resources
 * const id = await kit.resource()
 *   .workflows()
 *   .create(template);
 *
 * // Execute workflow
 * const result = await kit.execution()
 *   .workflow('my-workflow')
 *   .execute();
 *
 * // Query executions
 * const executions = await kit.query()
 *   .filter({ status: 'completed' })
 *   .get();
 * ```
 */

// Main class
export { SDKKit } from './kit.js';

// Public API types
export type {
  WorkflowAPI,
  ExecutionAPI,
  QueryAPI,
  ResourceAPI,
  ExecutionBuilder,
  QueryBuilder,
  WorkflowResource,
} from './api/index.js';

// Error handling
export {
  KitError,
  KitErrorCode,
  ErrorConverter,
} from './converters/index.js';

// Event management
export {
  EventManager,
  ExecutionEventType,
  getEventManager,
} from './managers/event.manager.js';

export type {
  ExecutionEventPayload,
  EventHandler,
  Unsubscribe,
  EventFilter,
} from './managers/event.manager.js';

// Common types
export type {
  ExecutionResult,
  ExecutionEvent,
  ExecutionOptions,
  ExecutionRecord,
  FilterCriteria,
  SortOptions,
  PaginationOptions,
  WorkflowTemplate,
  Node,
  Edge,
} from './types/index.js';

// Resource types
export type {
  ResourceFilter,
  WorkflowVersion,
  WorkflowMetadata,
} from './types/index.js';

// SDK types (for advanced users)
export type {
  SDK,
  SDKFactory,
  SDKResult,
} from './types/index.js';

// Predefined resources have been migrated to @wf-agent/sdk/resources/predefined/
// Import from @wf-agent/sdk/resources instead.

// Plugin system
export {
  PluginManager,
} from './plugin/index.js';

export type {
  PluginInfo,
} from './plugin/index.js';
