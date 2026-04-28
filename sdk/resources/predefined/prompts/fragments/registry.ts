/**
 * Fragment Registry
 *
 * Manages and retrieves all system prompt fragments
 * Reuses the FragmentRegistry infrastructure from @wf-agent/prompt-templates
 */

// Reuse the FragmentRegistry class and types from the packages layer.
import { FragmentRegistry, type SystemPromptFragment } from "@wf-agent/prompt-templates";

// Import all fragments
import {
  ASSISTANT_ROLE_FRAGMENT,
  CODER_ROLE_FRAGMENT,
  ANALYST_ROLE_FRAGMENT,
} from "./role/assistant.js";

import {
  GENERAL_CAPABILITY_FRAGMENT,
  GENERAL_WORK_PRINCIPLES_FRAGMENT,
  CODING_CAPABILITY_FRAGMENT,
  CODING_WORK_PRINCIPLES_FRAGMENT,
  CODING_INTERACTION_FRAGMENT,
} from "./capability/index.js";

import {
  GENERAL_CONSTRAINTS_FRAGMENT,
  GENERAL_INTERACTION_FRAGMENT,
  CODING_CONSTRAINTS_FRAGMENT,
  CODE_SAFETY_FRAGMENT,
} from "./constraint/index.js";

import {
  TOOL_USAGE_XML_SUMMARY_FRAGMENT,
  TOOL_USAGE_JSON_SUMMARY_FRAGMENT,
} from "./tool-usage/summary.js";

/**
 * All predefined segments
 */
export const ALL_PREDEFINED_FRAGMENTS: SystemPromptFragment[] = [
  // Role fragments
  ASSISTANT_ROLE_FRAGMENT,
  CODER_ROLE_FRAGMENT,
  ANALYST_ROLE_FRAGMENT,

  // Capability fragments
  GENERAL_CAPABILITY_FRAGMENT,
  GENERAL_WORK_PRINCIPLES_FRAGMENT,
  CODING_CAPABILITY_FRAGMENT,
  CODING_WORK_PRINCIPLES_FRAGMENT,
  CODING_INTERACTION_FRAGMENT,

  // Constraint fragments
  GENERAL_CONSTRAINTS_FRAGMENT,
  GENERAL_INTERACTION_FRAGMENT,
  CODING_CONSTRAINTS_FRAGMENT,
  CODE_SAFETY_FRAGMENT,

  // Tool usage fragments
  TOOL_USAGE_XML_SUMMARY_FRAGMENT,
  TOOL_USAGE_JSON_SUMMARY_FRAGMENT,
];

/**
 * Global Fragment Registry Instance
 */
export const fragmentRegistry = new FragmentRegistry();

/**
 * Initialize the registry (register all predefined segments)
 */
export function initializeFragmentRegistry(): void {
  fragmentRegistry.registerAll(ALL_PREDEFINED_FRAGMENTS);
}

/**
 * Check if the registry has been initialized.
 */
export function isFragmentRegistryInitialized(): boolean {
  return ALL_PREDEFINED_FRAGMENTS.every(f => fragmentRegistry.has(f.id));
}
