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
 * Formatter Registry
 */
export class FormatterRegistry {
  private formatters: Map<string, BaseFormatter> = new Map();
  private static instance: FormatterRegistry | null = null;

  private constructor() {
    // Register the default format converter
    this.registerDefaults();
  }

  /**
   * Obtain a registry form instance
   */
  static getInstance(): FormatterRegistry {
    if (!FormatterRegistry.instance) {
      FormatterRegistry.instance = new FormatterRegistry();
    }
    return FormatterRegistry.instance;
  }

  /**
   * Register the default format converter
   */
  private registerDefaults(): void {
    this.register(new OpenAIChatFormatter());
    this.register(new OpenAIResponseFormatter());
    this.register(new AnthropicFormatter());
    this.register(new GeminiNativeFormatter());
    this.register(new GeminiOpenAIFormatter());
  }

  /**
   * Register Format Converter
   *
   * @param formatter Format converter instance
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
   * Get Format Converter
   *
   * @param provider: Type of the provider
   * @returns: An instance of the format converter; returns undefined if it does not exist
   */
  get(provider: LLMProvider | string): BaseFormatter | undefined {
    return this.formatters.get(provider);
  }

  /**
   * Check if it is registered
   *
   * @param provider Type of the provider
   * @returns Whether it is registered
   */
  has(provider: LLMProvider | string): boolean {
    return this.formatters.has(provider);
  }

  /**
   * Get all registered providers
   *
   * @returns List of providers
   */
  getRegisteredProviders(): string[] {
    return Array.from(this.formatters.keys());
  }

  /**
   * Cancel Format Converter
   *
   * @param provider Type of the provider
   * @returns Whether the cancellation was successful
   */
  unregister(provider: LLMProvider | string): boolean {
    return this.formatters.delete(provider);
  }

  /**
   * Clear all registrations.
   */
  clear(): void {
    this.formatters.clear();
  }

  /**
   * Reset to default registration
   */
  reset(): void {
    this.clear();
    this.registerDefaults();
  }
}

/**
 * Export the default registry instance
 */
export const formatterRegistry = FormatterRegistry.getInstance();

/**
 * Convenient function: Get Format Converter
 *
 * @param provider: Type of the provider
 * @returns: Instance of the format converter
 */
export function getFormatter(provider: LLMProvider | string): BaseFormatter {
  const formatter = formatterRegistry.get(provider);
  if (!formatter) {
    throw new Error(`No formatter registered for provider: ${provider}`);
  }
  return formatter;
}

/**
 * Convenient function: Registering a format converter
 *
 * @param formatter: An instance of the format converter
 */
export function registerFormatter(formatter: BaseFormatter): void {
  formatterRegistry.register(formatter);
}
