/**
 * Gemini OpenAI-compatible client implementation
 *
 * Implement API calls compatible with Gemini OpenAI, using Gemini's OpenAI-compatible endpoints. Support special parameters such as `thinking_budget` and `cached_content`. Both streaming and non-streaming calls are supported.
 *
 *
 */

import { BaseLLMClient } from "../base-client.js";
import { GeminiOpenAIFormatter } from "../formatters/index.js";
import type { LLMProfile } from "@wf-agent/types";

/**
 * Gemini OpenAI-compatible client
 */
export class GeminiOpenAIClient extends BaseLLMClient {
  constructor(profile: LLMProfile) {
    super(profile, new GeminiOpenAIFormatter());
  }
}
