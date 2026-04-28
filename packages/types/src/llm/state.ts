/**
 * LLM State Type Definitions
 */

/**
 * LLM Providers
 */
export type LLMProvider =
  /** OpenAI Chat API */
  | "OPENAI_CHAT"
  /** OpenAI Response API */
  | "OPENAI_RESPONSE"
  /** Anthropic */
  | "ANTHROPIC"
  /** Gemini Native API */
  | "GEMINI_NATIVE"
  /** Gemini OpenAI Compatible API */
  | "GEMINI_OPENAI"
  /** artificial relay */
  | "HUMAN_RELAY";
