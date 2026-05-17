/**
 * Node Event Builders
 * Provides builders for node-related events
 */

import { createBuilder, createErrorBuilder } from "./common.js";
import type {
  NodeStartedEvent,
  NodeCompletedEvent,
  NodeFailedEvent,
  ForkStartedEvent,
  ForkBranchStartedEvent,
  ForkBranchCompletedEvent,
  ForkCompletedEvent,
} from "@wf-agent/types";

// =============================================================================
// Node Events
// =============================================================================

/**
 * Build node started event
 */
export const buildNodeStartedEvent = createBuilder<NodeStartedEvent>("NODE_STARTED");

/**
 * Build node completed event
 */
export const buildNodeCompletedEvent = createBuilder<NodeCompletedEvent>("NODE_COMPLETED");

/**
 * Build node failed event
 */
export const buildNodeFailedEvent = createErrorBuilder<NodeFailedEvent>("NODE_FAILED");

// =============================================================================
// Fork Events
// =============================================================================

/**
 * Build fork started event
 */
export const buildForkStartedEvent = createBuilder<ForkStartedEvent>("FORK_STARTED");

/**
 * Build fork branch started event
 */
export const buildForkBranchStartedEvent = createBuilder<ForkBranchStartedEvent>("FORK_BRANCH_STARTED");

/**
 * Build fork branch completed event
 */
export const buildForkBranchCompletedEvent = createBuilder<ForkBranchCompletedEvent>("FORK_BRANCH_COMPLETED");

/**
 * Build fork completed event
 */
export const buildForkCompletedEvent = createBuilder<ForkCompletedEvent>("FORK_COMPLETED");
