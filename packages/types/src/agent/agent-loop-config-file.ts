/**
 * Agent Loop Profile Type Definition
 *
 * Used for configuration file format, supports TOML and JSON.
 */

import type { ID } from "../common.js";
import type { Message } from "../message/index.js";
import type { AgentHookType } from "./hooks.js";

/**
 * Agent Loop Hook Configuration File Format
 */
export interface AgentHookConfigFile {
  /** Hook Type */
  hookType: AgentHookType;
  /** Trigger condition expression (optional) */
  condition?: string;
  /** Name of the custom event to trigger */
  eventName: string;
  /** Event load (optional) */
  eventPayload?: Record<string, unknown>;
  /** Enable or not (default true) */
  enabled?: boolean;
  /** Weighting (the higher the number the higher the priority) */
  weight?: number;
  /** Hook Whether to create checkpoints when triggered */
  createCheckpoint?: boolean;
  /** Checkpoint Description */
  checkpointDescription?: string;
}

/**
 * Agent Loop Trigger Configuration
 *
 * Note: Trigger is not currently supported directly in the Agent-Loop layer.
 * This configuration is used for future extensions or indirectly through Graph nodes.
 */
export interface AgentTriggerConfigFile {
  /** Trigger ID */
  id: string;
  /** Trigger Type */
  type: "event" | "condition" | "schedule";
  /** trigger condition */
  condition?: string;
  /** Event name (event type) */
  eventName?: string;
  /** Enable or disable */
  enabled?: boolean;
  /** Actions after triggering */
  action: {
    type: "pause" | "stop" | "checkpoint" | "custom";
    config?: Record<string, unknown>;
  };
}

/**
 * Agent Loop Configuration File Format
 *
 * Supports defining all parameters of Agent Loop via configuration file.
 */
export interface AgentLoopConfigFile {
  /** layout ID */
  id: ID;
  /** Placement Name */
  name?: string;
  /** Configuration Description */
  description?: string;
  /** Configuration version */
  version?: string;

  /** LLM Profile ID */
  profileId?: ID;
  /** system cue */
  systemPrompt?: string;
  /** System prompt word template ID (referencing prompts configuration) */
  systemPromptTemplate?: string;
  /** Maximum number of iterations (-1 means unlimited) */
  maxIterations?: number;
  /** Initial message list */
  initialMessages?: Message[];
  /** List of allowed tools (array of tool IDs) */
  tools?: string[];
  /** Streaming output or not */
  stream?: boolean;

  /** Checkpoint Configuration */
  checkpoint?: {
    /** Whether to create checkpoints at the end */
    createOnEnd?: boolean;
    /** Whether to create checkpoints on error */
    createOnError?: boolean;
    /** Whether or not to create checkpoints after each iteration */
    createOnIteration?: boolean;
  };

  /** Hook Configuration List */
  hooks?: AgentHookConfigFile[];

  /** Trigger Configuration List (future expansion) */
  triggers?: AgentTriggerConfigFile[];

  /** metadata */
  metadata?: {
    /** author */
    author?: string;
    /** Creation time */
    createdAt?: string;
    /** update time */
    updatedAt?: string;
    /** tab (of a window) (computing) */
    tags?: string[];
    /** Custom Properties */
    [key: string]: unknown;
  };
}
