/**
 * General Trigger Limiter
 *
 * Provides logic for limiting the number of triggers.
 */

import type { BaseTriggerDefinition, TriggerStatus } from "./types.js";

/**
 * Check if the trigger can be triggered
 *
 * @param trigger Trigger definition
 * @returns Whether it can be triggered
 */
export function canTrigger(trigger: BaseTriggerDefinition): boolean {
  // Check if it is disabled.
  if (trigger.enabled === false) {
    return false;
  }

  // Check if the maximum number of triggers has been reached.
  if (trigger.maxTriggers && trigger.maxTriggers > 0) {
    const currentCount = trigger.triggerCount || 0;
    if (currentCount >= trigger.maxTriggers) {
      return false;
    }
  }

  return true;
}

/**
 * Get Trigger Status
 *
 * @param trigger Trigger definition
 * @returns Trigger status
 */
export function getTriggerStatus(trigger: BaseTriggerDefinition): TriggerStatus {
  // Check if it is disabled.
  if (trigger.enabled === false) {
    return "disabled";
  }

  // Check if the maximum number of triggers has been reached.
  if (trigger.maxTriggers && trigger.maxTriggers > 0) {
    const currentCount = trigger.triggerCount || 0;
    if (currentCount >= trigger.maxTriggers) {
      return "expired";
    }
  }

  // Check if it has been triggered before.
  if (trigger.triggerCount && trigger.triggerCount > 0) {
    return "triggered";
  }

  return "idle";
}

/**
 * Increase the trigger count
 *
 * @param trigger Trigger definition
 * @returns Updated trigger count
 */
export function incrementTriggerCount(trigger: BaseTriggerDefinition): number {
  const newCount = (trigger.triggerCount || 0) + 1;
  trigger.triggerCount = newCount;
  return newCount;
}

/**
 * Reset the trigger count
 *
 * @param trigger Trigger definition
 */
export function resetTriggerCount(trigger: BaseTriggerDefinition): void {
  trigger.triggerCount = 0;
}

/**
 * Check if the trigger has expired
 *
 * @param trigger Trigger definition
 * @returns Whether it has expired
 */
export function isTriggerExpired(trigger: BaseTriggerDefinition): boolean {
  if (!trigger.maxTriggers || trigger.maxTriggers <= 0) {
    return false;
  }

  const currentCount = trigger.triggerCount || 0;
  return currentCount >= trigger.maxTriggers;
}

/**
 * Get the remaining number of triggers
 *
 * @param trigger Trigger definition
 * @returns Remaining number of triggers (-1 indicates unlimited)
 */
export function getRemainingTriggers(trigger: BaseTriggerDefinition): number {
  if (!trigger.maxTriggers || trigger.maxTriggers <= 0) {
    return -1; // Unlimited
  }

  const currentCount = trigger.triggerCount || 0;
  return Math.max(0, trigger.maxTriggers - currentCount);
}
