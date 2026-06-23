/**
 * Interactive Script Type Definitions
 * Defines types for scripts that require runtime user interaction
 */

/**
 * Mode of interaction for interactive scripts
 * - blocking: Wait for user input at each interaction point
 * - llm-assisted: LLM provides automatic responses
 * - hybrid: LLM suggests, user confirms or modifies
 */
export type InteractionMode = "blocking" | "llm-assisted" | "hybrid";

/**
 * A point during script execution where interaction is needed
 */
export interface ScriptInteractionPoint {
  /** Prompt text to display to the user/LLM */
  prompt: string;
  /** Expected input type */
  expectedInputType?: "text" | "confirm" | "choice";
  /** Available options (for choice type) */
  options?: string[];
  /** Timeout in milliseconds for this interaction point */
  timeout?: number;
}

/**
 * Interactive Script Configuration
 * Configures a script that requires user interaction during execution
 */
export interface InteractiveScriptConfig {
  /** Interaction mode */
  mode: InteractionMode;
  /** Maximum interaction rounds */
  maxRounds?: number;
  /** Predefined interaction points (optional, can be auto-detected) */
  interactionPoints?: ScriptInteractionPoint[];
  /** Prompt patterns to detect (regex strings that indicate waiting for input) */
  promptPatterns?: string[];
  /** Timeout per interaction round (milliseconds) */
  roundTimeout?: number;
}