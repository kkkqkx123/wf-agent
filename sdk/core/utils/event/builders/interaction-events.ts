/**
 * Interaction Event Builders
 * Provides builders for user interaction and human relay events
 */

import { now } from "@wf-agent/common-utils";
import { createBuilder, type BuildParams } from "./common.js";
import type {
  UserInteractionRequestedEvent,
  UserInteractionRespondedEvent,
  UserInteractionProcessedEvent,
  UserInteractionFailedEvent,
  HumanRelayRequestedEvent,
  HumanRelayRespondedEvent,
  HumanRelayProcessedEvent,
  HumanRelayFailedEvent,
} from "@wf-agent/types";

// =============================================================================
// User Interaction Events
// =============================================================================

/**
 * Build user interaction requested event
 */
export const buildUserInteractionRequestedEvent = (
  params: BuildParams<UserInteractionRequestedEvent> & { workflowId?: string; nodeId?: string },
): UserInteractionRequestedEvent =>
  ({
    type: "USER_INTERACTION_REQUESTED",
    timestamp: now(),
    ...params,
  }) as UserInteractionRequestedEvent;

/**
 * Build user interaction responded event
 */
export const buildUserInteractionRespondedEvent = createBuilder<UserInteractionRespondedEvent>(
  "USER_INTERACTION_RESPONDED",
);

/**
 * Build user interaction processed event
 */
export const buildUserInteractionProcessedEvent = (
  params: BuildParams<UserInteractionProcessedEvent> & { workflowId?: string },
): UserInteractionProcessedEvent =>
  ({
    type: "USER_INTERACTION_PROCESSED",
    timestamp: now(),
    ...params,
  }) as UserInteractionProcessedEvent;

/**
 * Build user interaction failed event
 */
export const buildUserInteractionFailedEvent = (
  params: BuildParams<UserInteractionFailedEvent>,
): UserInteractionFailedEvent =>
  ({ type: "USER_INTERACTION_FAILED", timestamp: now(), ...params }) as UserInteractionFailedEvent;

// =============================================================================
// Human Relay Events
// =============================================================================

/**
 * Build human relay requested event
 */
export const buildHumanRelayRequestedEvent =
  createBuilder<HumanRelayRequestedEvent>("HUMAN_RELAY_REQUESTED");

/**
 * Build human relay responded event
 */
export const buildHumanRelayRespondedEvent =
  createBuilder<HumanRelayRespondedEvent>("HUMAN_RELAY_RESPONDED");

/**
 * Build human relay processed event
 */
export const buildHumanRelayProcessedEvent =
  createBuilder<HumanRelayProcessedEvent>("HUMAN_RELAY_PROCESSED");

/**
 * Build human relay failed event
 */
export const buildHumanRelayFailedEvent = (
  params: BuildParams<HumanRelayFailedEvent>,
): HumanRelayFailedEvent =>
  ({ type: "HUMAN_RELAY_FAILED", timestamp: now(), ...params }) as HumanRelayFailedEvent;
