/**
 * Tool Definition Conversion Utilities
 * Provides conversion between tool definition formats from various LLM providers
 */

import type { ToolSchema } from "@wf-agent/types";

/**
 * Tool definitions in OpenAI format
 */
export interface OpenAITool {
  type: "function";
  function: {
    name: string;
    description: string;
    parameters: Record<string, unknown>;
  };
}

/**
 * Anthropic format tool definition
 */
export interface AnthropicTool {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
}

/**
 * Gemini format tool definition
 */
export interface GeminiTool {
  functionDeclarations: Array<{
    name: string;
    description: string;
    parameters: Record<string, unknown>;
  }>;
}

/**
 * Convert to OpenAI format tool definitions
 */
export function convertToolsToOpenAIFormat(tools: ToolSchema[]): OpenAITool[] {
  if (!tools || tools.length === 0) {
    return [];
  }

  return tools.map(tool => ({
    type: "function" as const,
    function: {
      name: tool.id,
      description: tool.description,
      parameters: tool.parameters as unknown as Record<string, unknown>,
    },
  }));
}

/**
 * Convert to Anthropic format tool definitions
 */
export function convertToolsToAnthropicFormat(tools: ToolSchema[]): AnthropicTool[] {
  if (!tools || tools.length === 0) {
    return [];
  }

  return tools.map(tool => ({
    name: tool.id,
    description: tool.description,
    input_schema: tool.parameters as unknown as Record<string, unknown>,
  }));
}

/**
 * Convert to Gemini format tool definitions
 */
export function convertToolsToGeminiFormat(tools: ToolSchema[]): GeminiTool[] {
  if (!tools || tools.length === 0) {
    return [];
  }

  return tools.map(tool => ({
    functionDeclarations: [
      {
        name: tool.id,
        description: tool.description,
        parameters: tool.parameters as unknown as Record<string, unknown>,
      },
    ],
  }));
}
