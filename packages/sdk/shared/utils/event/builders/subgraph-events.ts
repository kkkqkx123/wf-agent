/**
 * Subgraph Event Builders
 * Provides builders for subgraph-related events
 */

import { createBuilder, createStringErrorBuilder } from "./common.js";
import type {
  SubgraphStartedEvent,
  SubgraphCompletedEvent,
  TriggeredSubgraphStartedEvent,
  TriggeredSubgraphCompletedEvent,
  TriggeredSubgraphFailedEvent,
} from "@wf-agent/types";

// =============================================================================
// Subgraph Events
// =============================================================================

/**
 * Build subgraph started event
 */
export const buildSubgraphStartedEvent = createBuilder<SubgraphStartedEvent>("SUBGRAPH_STARTED");

/**
 * Build subgraph completed event
 */
export const buildSubgraphCompletedEvent =
  createBuilder<SubgraphCompletedEvent>("SUBGRAPH_COMPLETED");

// =============================================================================
// Triggered Subgraph Events
// =============================================================================

/**
 * Build triggered subgraph started event
 */
export const buildTriggeredSubgraphStartedEvent = createBuilder<TriggeredSubgraphStartedEvent>(
  "TRIGGERED_SUBGRAPH_STARTED",
);

/**
 * Build triggered subgraph completed event
 */
export const buildTriggeredSubgraphCompletedEvent = createBuilder<TriggeredSubgraphCompletedEvent>(
  "TRIGGERED_SUBGRAPH_COMPLETED",
);

/**
 * Build triggered subgraph failed event
 */
export const buildTriggeredSubgraphFailedEvent =
  createStringErrorBuilder<TriggeredSubgraphFailedEvent>("TRIGGERED_SUBGRAPH_FAILED");
