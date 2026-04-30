/**
 * Component Message System
 *
 * This module provides a unified message system for output routing and multi-target delivery.
 * It is distinct from:
 * - LLM Messages (sdk/core/messaging) - for LLM interaction
 * - Events (packages/types/src/events) - for internal component communication
 *
 * Component messages support:
 * - Multiple output targets (TUI, files, logs)
 * - Flexible routing rules
 * - Aggregation and parent notification
 */

// Base types
export {
  type ComponentMessageId,
  MessageCategory,
  type MessageLevel,
  type BaseComponentMessage,
  type CreateMessageInput,
} from "./base.js";

// Entity types
export {
  type EntityType,
  type ParallelGroupInfo,
  type EntityIdentity,
  type MessageTrace,
  type EntityContext,
  createRootEntityIdentity,
  createChildEntityIdentity,
  createForkBranchEntityIdentity,
} from "./entity.js";

// Output types
export {
  OutputTarget,
  type AggregationLevel,
  type OutputDecision,
  type OutputHandler,
} from "./output.js";

// Routing types
export {
  type RoutingMatchCondition,
  type RoutingRule,
} from "./routing.js";

// Category types (all message types and data interfaces)
export * from "./categories/index.js";
