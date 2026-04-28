/**
 * OpenAI Response Client Implementation
 *
 * Implementations for calling the OpenAI Response API, using the /responses endpoint
 * Supports special parameters such as reasoning_effort and previous_response_id
 * Supports both streaming and non-streaming calls
 */

import { BaseLLMClient } from "../base-client.js";
import { OpenAIResponseFormatter } from "../formatters/index.js";
import type { LLMProfile } from "@wf-agent/types";

/**
 * OpenAI Response Client
 */
export class OpenAIResponseClient extends BaseLLMClient {
  constructor(profile: LLMProfile) {
    super(profile, new OpenAIResponseFormatter());
  }
}
