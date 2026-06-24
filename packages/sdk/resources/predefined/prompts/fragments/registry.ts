/**
 * Fragment definitions for predefined system prompt fragments.
 *
 * This module exports all predefined fragment definitions.
 * Registration is handled by the unified registration module:
 * - registration/prompts-registration.ts
 */

import type { SystemPromptFragment } from "@wf-agent/types";

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

import { CODE_REVIEW_FRAGMENT, DATA_ANALYSIS_FRAGMENT } from "../../prompts/user-commands/index.js";

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

  // Task instruction fragments
  CODE_REVIEW_FRAGMENT,
  DATA_ANALYSIS_FRAGMENT,
];
