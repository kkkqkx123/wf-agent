/**
 * Skill Event Builders
 * Provides builders for skill-related events
 */

import { createBuilder, createStringErrorBuilder } from "./common.js";
import type {
  SkillLoadStartedEvent,
  SkillLoadCompletedEvent,
  SkillLoadFailedEvent,
} from "@wf-agent/types";

// =============================================================================
// Skill Load Events
// =============================================================================

/**
 * Build skill load started event
 */
export const buildSkillLoadStartedEvent =
  createBuilder<SkillLoadStartedEvent>("SKILL_LOAD_STARTED");

/**
 * Build skill load completed event
 */
export const buildSkillLoadCompletedEvent =
  createBuilder<SkillLoadCompletedEvent>("SKILL_LOAD_COMPLETED");

/**
 * Build skill load failed event
 */
export const buildSkillLoadFailedEvent =
  createStringErrorBuilder<SkillLoadFailedEvent>("SKILL_LOAD_FAILED");
