/**
 * ConditionWaiter - Condition Waiter Function (General Part)
 *
 * Responsibilities:
 * - Encapsulate the logic for conditional waiting
 * - Provide timeout control
 * - Simplify the way to call conditional waiting functions
 *
 * Design Principles:
 * - Pure functions: All methods are pure functions
 * - Provide a concise interface for waiting
 */

import { TimeoutError } from "@wf-agent/types";
import { now, diffTimestamp } from "@wf-agent/common-utils";

/**
 * Special values indicating indefinite waiting
 * The value -1 is used to represent indefinite waiting, which is in line with system-level programming conventions (such as C#, Java, POSIX).
 */
export const WAIT_FOREVER = -1;

/**
 * Wait for the condition to be met
 *
 * @param condition: The condition function
 * @param checkInterval: The interval between checks (in milliseconds), with a default value of 100ms
 * @param timeout: The timeout period (in milliseconds), with a default value of 30000ms
 * @returns: A Promise that resolves when the condition is met or when the timeout occurs
 * @throws: An Error is thrown if the timeout is reached
 */
export async function waitForCondition(
  condition: () => boolean,
  checkInterval: number = 100,
  timeout: number = 30000,
): Promise<void> {
  const startTime = now();

  while (diffTimestamp(startTime, now()) < timeout) {
    if (condition()) {
      return;
    }

    await new Promise(resolve => setTimeout(resolve, checkInterval));
  }

  throw new TimeoutError(`Condition not met within ${timeout}ms`, timeout, {
    operation: "wait_for_condition",
  });
}

/**
 * Wait for multiple conditions to be met
 *
 * @param conditions: An array of condition functions
 * @param checkInterval: The interval between checks (in milliseconds), default is 100ms
 * @param timeout: The timeout period (in milliseconds), default is 30000ms
 * @returns: A Promise that resolves when all conditions are met or the timeout occurs
 * @throws: Throws an Error if the timeout is reached
 */
export async function waitForAllConditions(
  conditions: Array<() => boolean>,
  checkInterval: number = 100,
  timeout: number = 30000,
): Promise<void> {
  const startTime = now();

  while (diffTimestamp(startTime, now()) < timeout) {
    if (conditions.every(condition => condition())) {
      return;
    }

    await new Promise(resolve => setTimeout(resolve, checkInterval));
  }

  throw new TimeoutError(`Not all conditions met within ${timeout}ms`, timeout, {
    operation: "wait_for_all_conditions",
    conditionCount: conditions.length,
  });
}

/**
 * Wait for any condition to be met
 *
 * @param conditions Array of condition functions
 * @param checkInterval Check interval (in milliseconds), default is 100ms
 * @param timeout Timeout period (in milliseconds), default is 30000ms
 * @returns Promise: Resolves when any condition is met or the timeout occurs, returning the index of the condition that was met
 * @throws Error: Throws an exception if the timeout occurs
 */
export async function waitForAnyCondition(
  conditions: Array<() => boolean>,
  checkInterval: number = 100,
  timeout: number = 30000,
): Promise<number> {
  const startTime = now();

  while (diffTimestamp(startTime, now()) < timeout) {
    for (let i = 0; i < conditions.length; i++) {
      const condition = conditions[i];
      if (condition && condition()) {
        return i;
      }
    }

    await new Promise(resolve => setTimeout(resolve, checkInterval));
  }

  throw new TimeoutError(`No condition met within ${timeout}ms`, timeout, {
    operation: "wait_for_any_condition",
    conditionCount: conditions.length,
  });
}
