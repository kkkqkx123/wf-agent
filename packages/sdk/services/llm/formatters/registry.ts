/**
 * Formatter Registry
 *
 * Manages the registration and retrieval of all format converters.
 * Using a registry pattern, it facilitates the easy expansion of new providers.
 */

import { BaseFormatter } from "./base.js";
import { OpenAIChatFormatter } from "./openai-chat.js";
import { OpenAIResponseFormatter } from "./openai-response.js";
import { AnthropicFormatter } from "./anthropic.js";
import { GeminiNativeFormatter } from "./gemini-native.js";
import { GeminiOpenAIFormatter } from "./gemini-openai.js";
import type { LLMProvider } from "@wf-agent/types";
import { sdkLogger as logger } from "../../../utils/index.js";

/**
 * Formatter Registry - Singleton instance for managing formatters
 */
export class FormatterRegistry {
  private formatters: Map<string, BaseFormatter> = new Map();
  private static instance: FormatterRegistry | null = null;

  private constructor() {
    this.registerDefaults();
  }

  /**
   * Get singleton instance
   */
  static getInstance(): FormatterRegistry {
    if (!FormatterRegistry.instance) {
      FormatterRegistry.instance = new FormatterRegistry();
    }
    return FormatterRegistry.instance;
  }

  /**
   * Register default formatters
   */
  private registerDefaults(): void {
    this.register(new OpenAIChatFormatter());
    this.register(new OpenAIResponseFormatter());
    this.register(new AnthropicFormatter());
    this.register(new GeminiNativeFormatter());
    this.register(new GeminiOpenAIFormatter());
  }

  /**
   * Register a formatter instance
   *
   * @param formatter Formatter instance to register
   */
  register(formatter: BaseFormatter): void {
    const provider = formatter.getSupportedProvider();
    if (this.formatters.has(provider)) {
      logger.warn(`Formatter for provider "${provider}" already registered, will be overwritten`, {
        provider,
      });
    }
    this.formatters.set(provider, formatter);
  }

  /**
   * Get formatter for a provider
   *
   * @param provider Provider type
   * @returns Formatter instance or undefined if not found
   */
  get(provider: LLMProvider | string): BaseFormatter | undefined {
    return this.formatters.get(provider);
  }

  /**
   * Check if a formatter is registered for a provider
   *
   * @param provider Provider type
   * @returns True if registered
   */
  has(provider: LLMProvider | string): boolean {
    return this.formatters.has(provider);
  }

  /**
   * Get all registered provider names
   *
   * @returns Array of provider names
   */
  getRegisteredProviders(): string[] {
    return Array.from(this.formatters.keys());
  }

  /**
   * Unregister a formatter
   *
   * @param provider Provider type
   * @returns True if successfully unregistered
   */
  unregister(provider: LLMProvider | string): boolean {
    return this.formatters.delete(provider);
  }

  /**
   * Clear all registrations
   */
  clear(): void {
    this.formatters.clear();
  }

  /**
   * Reset to default registrations
   */
  reset(): void {
    this.clear();
    this.registerDefaults();
  }
}

/**
 * Default registry instance
 */
export const formatterRegistry = FormatterRegistry.getInstance();

/**
 * Get formatter for a provider (throws if not found)
 *
 * @param provider Provider type
 * @returns Formatter instance
 * @throws Error if no formatter is registered for the provider
 */
export function getFormatter(provider: LLMProvider | string): BaseFormatter {
  const formatter = formatterRegistry.get(provider);
  if (!formatter) {
    throw new Error(`No formatter registered for provider: ${provider}`);
  }
  return formatter;
}

/**
 * Register a custom formatter
 *
 * @param formatter Formatter instance to register
 */
export function registerFormatter(formatter: BaseFormatter): void {
  formatterRegistry.register(formatter);
}
