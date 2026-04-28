/**
 * Node Event Builders
 * Provides builders for node-related events
 */

import { createBuilder, createErrorBuilder } from "./common.js";
import type { NodeStartedEvent, NodeCompletedEvent, NodeFailedEvent } from "@wf-agent/types";

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
