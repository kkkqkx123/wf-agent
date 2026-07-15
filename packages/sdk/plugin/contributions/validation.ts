/**
 * Contribution Validation - Validation utilities for plugin contributions.
 */

import type { ContributionType } from "./types.js";

/**
 * Validate that a contribution type is recognized.
 */
export function isValidContributionType(type: string): type is ContributionType {
  const validTypes: ContributionType[] = [
    'node-type',
    'tool-type',
    'llm-provider',
    'formatter',
    'hook-handler',
    'event-handler',
    'middleware',
  ];
  return validTypes.includes(type as ContributionType);
}

/**
 * Validate that a contribution registrar method is called with valid arguments.
 */
export function validateContribution(
  pluginId: string,
  type: ContributionType,
  key: string,
): string | null {
  if (!key || typeof key !== 'string' || key.trim().length === 0) {
    return `Plugin '${pluginId}' attempted to register a ${type} with an empty key`;
  }
  return null;
}