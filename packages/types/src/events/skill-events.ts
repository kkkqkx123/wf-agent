/**
 * Skill Related Event Type Definitions
 *
 * Skill is a collection of static resources (prompts + reference scripts + resource files) that should be "loaded" rather than "executed".
 * should be "loaded" and not "executed".
 */

import type { BaseEvent } from "./base.js";

/**
 * Skill Load Type
 */
export type SkillLoadType = "metadata" | "content" | "resources";

/**
 * Skill load start event
 */
export interface SkillLoadStartedEvent extends BaseEvent {
  type: "SKILL_LOAD_STARTED";
  /** Skill Name */
  skillName: string;
  /** Load Type */
  loadType: SkillLoadType;
}

/**
 * Skill load completion event
 */
export interface SkillLoadCompletedEvent extends BaseEvent {
  type: "SKILL_LOAD_COMPLETED";
  /** Skill Name */
  skillName: string;
  /** Load Type */
  loadType: SkillLoadType;
  /** success or failure */
  success: boolean;
  /** Whether from cache */
  cached: boolean;
  /** Load time (milliseconds) */
  loadTime: number;
}

/**
 * Skill Load Failure Event
 */
export interface SkillLoadFailedEvent extends BaseEvent {
  type: "SKILL_LOAD_FAILED";
  /** Skill Name */
  skillName: string;
  /** Load Type */
  loadType: SkillLoadType;
  /** error message */
  error: string;
  /** Load time (milliseconds) */
  loadTime: number;
}

/**
 * Skill Event Union Type
 */
export type SkillEvent = SkillLoadStartedEvent | SkillLoadCompletedEvent | SkillLoadFailedEvent;
