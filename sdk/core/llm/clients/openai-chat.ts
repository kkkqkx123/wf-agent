/**
 * OpenAI Chat Client Implementation
 *
 * Implementations for calling the OpenAI Chat API, using the /chat/completions endpoint
 * Supports both streaming and non-streaming calls
 */

import { BaseLLMClient } from "../base-client.js";
import { OpenAIChatFormatter } from "../formatters/index.js";
import type { LLMProfile } from "@wf-agent/types";

/**
 * OpenAI Chat Client
 */
export class OpenAIChatClient extends BaseLLMClient {
  constructor(profile: LLMProfile) {
    super(profile, new OpenAIChatFormatter());
  }
}
