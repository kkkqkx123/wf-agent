/**
 * Gemini Native Client Implementation
 *
 * Implementations of Gemini Native API calls, utilizing Gemini's native endpoints
 * Supports both streaming and non-streaming call methods
 */

import { BaseLLMClient } from "../base-client.js";
import { GeminiNativeFormatter } from "../formatters/index.js";
import type { LLMProfile } from "@wf-agent/types";

/**
 * Gemini Native Client
 */
export class GeminiNativeClient extends BaseLLMClient {
  constructor(profile: LLMProfile) {
    super(profile, new GeminiNativeFormatter());
  }
}
