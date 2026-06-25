/**
 * Dynamic Prompt Injection
 *
 * Handles injecting dynamic context into message arrays before LLM calls.
 * Two-layer injection strategy:
 * 1. staticSystem: Prepended to system message (stable content, cached)
 * 2. dynamicUserContext: Appended to last user message (variable content, not cached)
 */

import type { LLMMessage } from "@wf-agent/types";

export interface DynamicPromptInjectionResult {
  messages: LLMMessage[];
  systemInjected: boolean;
  userContextInjected: boolean;
}

/**
 * Inject dynamic prompts into message array
 *
 * @param messages Original message array
 * @param staticSystem System-level dynamic content to prepend
 * @param userContextSuffix User-level dynamic context to append
 * @returns Injection result with modified messages
 */
export function injectDynamicPrompts(
  messages: LLMMessage[],
  staticSystem: string | undefined,
  userContextSuffix: string | undefined,
): DynamicPromptInjectionResult {
  const result: LLMMessage[] = [...messages];
  let systemInjected = false;
  let userContextInjected = false;

  if (staticSystem) {
    const systemMsgIndex = result.findIndex((msg: LLMMessage) => {
      const role = msg && typeof msg === "object" && "role" in msg ? msg.role : undefined;
      return role === "system";
    });

    if (systemMsgIndex >= 0) {
      const msg = result[systemMsgIndex];
      if (msg && typeof msg === "object" && "content" in msg) {
        const currentContent = msg.content;
        result[systemMsgIndex] = {
          ...msg,
          content:
            typeof currentContent === "string"
              ? `${currentContent}\n\n${staticSystem}`
              : currentContent,
        } as LLMMessage;
      }
    } else {
      result.unshift({
        role: "system",
        content: staticSystem,
      } as LLMMessage);
    }
    systemInjected = true;
  }

  if (userContextSuffix) {
    let lastUserMsgIndex = -1;
    for (let i = result.length - 1; i >= 0; i--) {
      const msg = result[i];
      const role = msg && typeof msg === "object" && "role" in msg ? msg.role : undefined;
      if (role === "user") {
        lastUserMsgIndex = i;
        break;
      }
    }

    if (lastUserMsgIndex >= 0) {
      const lastUserMsg = result[lastUserMsgIndex];
      if (lastUserMsg && typeof lastUserMsg === "object" && "content" in lastUserMsg) {
        const currentContent = lastUserMsg.content;
        result[lastUserMsgIndex] = {
          ...lastUserMsg,
          content:
            typeof currentContent === "string"
              ? `${currentContent}\n\n${userContextSuffix}`
              : currentContent,
        } as LLMMessage;
      }
    } else {
      result.push({
        role: "user",
        content: userContextSuffix,
      } as LLMMessage);
    }
    userContextInjected = true;
  }

  return {
    messages: result,
    systemInjected,
    userContextInjected,
  };
}
