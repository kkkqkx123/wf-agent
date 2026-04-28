/**
 * Component Message API
 *
 * Public API for the component message system.
 */

// Core implementation
export {
  MessageBus,
  createMessageBus,
  type MessageFilter,
  type MessageHandler,
  type MessageSubscription,
  type MessageBusOptions,
  type EntityStatus,
  type EntityContext,
} from "./message-bus.js";

// Routing utilities
export { matchesRoutingRule, findMatchingRule, sortRulesByPriority } from "./routing-utils.js";

// Publisher API
export { MessagePublisher, createMessagePublisher } from "./publisher-api.js";

// Re-export types from @wf-agent/types
export type {
  BaseComponentMessage,
  CreateMessageInput,
  MessageCategory,
  MessageLevel,
  RoutingRule,
  RoutingMatchCondition,
  OutputDecision,
  OutputTarget,
  OutputHandler,
  EntityIdentity,
} from "@wf-agent/types";
