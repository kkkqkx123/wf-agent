/**
 * Universal Trigger Matcher
 *
 * Provides the logic for matching events with trigger conditions.
 */

import type { BaseTriggerCondition, BaseEventData, TriggerMatcher } from "./types.js";

/**
 * Default Trigger Matcher
 *
 * Matching Rules:
 * 1. The event type must match.
 * 2. If the condition specifies an eventName, the event must also match.
 *
 * @param condition: Trigger condition
 * @param event: Event data
 * @returns: Whether a match was found
 */
export const defaultTriggerMatcher: TriggerMatcher = (
  condition: BaseTriggerCondition,
  event: BaseEventData,
): boolean => {
  // Check the event type.
  if (condition.eventType !== event.type) {
    return false;
  }

  // If the condition specifies eventName, check whether there is a match.
  if (condition.eventName && condition.eventName !== event.eventName) {
    return false;
  }

  return true;
};

/**
 * Match Trigger Conditions
 *
 * Use the default matcher to determine whether an event meets the trigger conditions.
 *
 * @param condition Trigger condition
 * @param event Event data
 * @returns Whether a match was found
 */
export function matchTriggerCondition(
  condition: BaseTriggerCondition,
  event: BaseEventData,
): boolean {
  return defaultTriggerMatcher(condition, event);
}

/**
 * Batch Match Trigger Conditions
 *
 * Find all triggers that match from the list of triggers.
 *
 * @param triggers List of triggers
 * @param event Event data
 * @param matcher Matcher (optional, default is defaultTriggerMatcher)
 * @returns List of matched triggers
 */
export function matchTriggers<T extends { condition: BaseTriggerCondition; enabled?: boolean }>(
  triggers: T[],
  event: BaseEventData,
  matcher: TriggerMatcher = defaultTriggerMatcher,
): T[] {
  return triggers.filter(trigger => {
    // Skip the disabled triggers.
    if (trigger.enabled === false) {
      return false;
    }

    return matcher(trigger.condition, event);
  });
}

/**
 * Create a custom matcher
 *
 * A factory function for creating a matcher with custom matching logic.
 *
 * @param customMatcher: The custom matching function
 * @returns: The matcher
 */
export function createTriggerMatcher(
  customMatcher: (condition: BaseTriggerCondition, event: BaseEventData) => boolean,
): TriggerMatcher {
  return (condition, event) => {
    // Perform the default matching first.
    if (!defaultTriggerMatcher(condition, event)) {
      return false;
    }

    // Re-execute the custom matching.
    return customMatcher(condition, event);
  };
}
