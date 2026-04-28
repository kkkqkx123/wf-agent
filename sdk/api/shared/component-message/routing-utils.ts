/**
 * Routing Utilities
 *
 * Helper functions for routing rule matching and management.
 */

import type { BaseComponentMessage, RoutingRule } from "@wf-agent/types";

/**
 * Check if a message matches a routing rule
 * @param message Message to check
 * @param rule Routing rule
 * @returns true if message matches the rule
 */
export function matchesRoutingRule(message: BaseComponentMessage, rule: RoutingRule): boolean {
  const { match } = rule;

  // Check category
  if (match.categories && !match.categories.includes(message.category)) {
    return false;
  }

  // Check type
  if (match.types && !match.types.includes(message.type)) {
    return false;
  }

  // Check level
  if (match.levels && !match.levels.includes(message.level)) {
    return false;
  }

  // Check entity type
  if (match.entityTypes && !match.entityTypes.includes(message.entity.type)) {
    return false;
  }

  // Custom match function
  if (match.custom && !match.custom(message)) {
    return false;
  }

  return true;
}

/**
 * Find the first matching routing rule
 * @param message Message to match
 * @param rules Routing rules (will be sorted by priority)
 * @returns First matching rule, or undefined
 */
export function findMatchingRule(
  message: BaseComponentMessage,
  rules: RoutingRule[],
): RoutingRule | undefined {
  // Rules should be sorted by priority
  const sortedRules = [...rules].sort((a, b) => a.priority - b.priority);

  for (const rule of sortedRules) {
    if (matchesRoutingRule(message, rule)) {
      return rule;
    }
  }

  return undefined;
}

/**
 * Sort routing rules by priority
 * @param rules Rules to sort
 * @returns Sorted rules (lower priority number = higher priority)
 */
export function sortRulesByPriority(rules: RoutingRule[]): RoutingRule[] {
  return [...rules].sort((a, b) => a.priority - b.priority);
}
