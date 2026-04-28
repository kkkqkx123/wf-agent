/**
 * LLM Client Factory
 *
 * Responsible for creating client instances for different providers.
 * Clients are created using the factory pattern, and client instances are cached to improve performance.
 */

import type { LLMClient, LLMProfile, HumanRelayHandler, HumanRelayContext } from "@wf-agent/types";
import { OpenAIChatClient } from "./clients/openai-chat.js";
import { OpenAIResponseClient } from "./clients/openai-response.js";
import { AnthropicClient } from "./clients/anthropic.js";
import { GeminiNativeClient } from "./clients/gemini-native.js";
import { GeminiOpenAIClient } from "./clients/gemini-openai.js";
import { HumanRelayClient } from "./clients/human-relay-client.js";
import { ConfigurationError } from "@wf-agent/types";

/**
 * Client Factory Class
 */
export class ClientFactory {
  private clientCache: Map<string, LLMClient> = new Map();
  private humanRelayHandler?: HumanRelayHandler;
  private humanRelayContextProvider?: () => HumanRelayContext;

  /**
   * Set HumanRelayHandler
   * @param handler Human Relay handler
   */
  setHumanRelayHandler(handler: HumanRelayHandler): void {
    this.humanRelayHandler = handler;
    // Clear cached HumanRelayClient to use new handler
    this.clearClientCacheByProvider("HUMAN_RELAY");
  }

  /**
   * Set HumanRelayContextProvider
   * @param provider Context provider function that returns HumanRelayContext
   */
  setHumanRelayContextProvider(provider: () => HumanRelayContext): void {
    this.humanRelayContextProvider = provider;
    // Clear cached HumanRelayClient to use new provider
    this.clearClientCacheByProvider("HUMAN_RELAY");
  }

  /**
   * Clear client cache by provider
   */
  private clearClientCacheByProvider(provider: string): void {
    for (const [key] of this.clientCache.entries()) {
      if (key.includes(provider)) {
        this.clientCache.delete(key);
      }
    }
  }

  /**
   * Create an LLM client
   *
   * @param profile LLM Profile configuration
   * @returns LLM client instance
   */
  createClient(profile: LLMProfile): LLMClient {
    const cacheKey = this.getCacheKey(profile);

    // Check the cache.
    const cachedClient = this.clientCache.get(cacheKey);
    if (cachedClient) {
      return cachedClient;
    }

    // Create a new client.
    const client = this.createClientByProvider(profile);

    // Cache Client
    this.clientCache.set(cacheKey, client);

    return client;
  }

  /**
   * Create the corresponding client based on the provider.
   */
  private createClientByProvider(profile: LLMProfile): LLMClient {
    switch (profile.provider) {
      case "OPENAI_CHAT":
        return new OpenAIChatClient(profile);

      case "OPENAI_RESPONSE":
        return new OpenAIResponseClient(profile);

      case "ANTHROPIC":
        return new AnthropicClient(profile);

      case "GEMINI_NATIVE":
        return new GeminiNativeClient(profile);

      case "GEMINI_OPENAI":
        return new GeminiOpenAIClient(profile);

      case "HUMAN_RELAY":
        if (!this.humanRelayHandler) {
          throw new ConfigurationError(
            "HumanRelayHandler not registered. Please call setHumanRelayHandler() first.",
            "provider",
            { provider: profile.provider },
          );
        }
        if (!this.humanRelayContextProvider) {
          throw new ConfigurationError(
            "HumanRelayContextProvider not registered. Please call setHumanRelayContextProvider() first.",
            "provider",
            { provider: profile.provider },
          );
        }
        return new HumanRelayClient(profile, {
          handler: this.humanRelayHandler,
          defaultTimeout: (profile.parameters?.["timeout"] as number) || 300000,
          contextProvider: this.humanRelayContextProvider,
        });

      default:
        throw new ConfigurationError(`Unsupported LLM provider: ${profile.provider}`, "provider", {
          provider: profile.provider,
          model: profile.model,
          supportedProviders: [
            "OPENAI_CHAT",
            "OPENAI_RESPONSE",
            "ANTHROPIC",
            "GEMINI_NATIVE",
            "GEMINI_OPENAI",
            "HUMAN_RELAY",
          ],
        });
    }
  }

  /**
   * Get the cached client
   *
   * @param profileId Profile ID
   * @returns LLM client instance or undefined
   */
  getClient(profileId: string): LLMClient | undefined {
    for (const [key, client] of this.clientCache.entries()) {
      if (key.startsWith(profileId)) {
        return client;
      }
    }
    return undefined;
  }

  /**
   * Clear the client cache
   */
  clearCache(): void {
    this.clientCache.clear();
  }

  /**
   * Clear the client cache for the specified Profile
   *
   * @param profileId Profile ID
   */
  clearClientCache(profileId: string): void {
    for (const key of this.clientCache.keys()) {
      if (key.startsWith(profileId)) {
        this.clientCache.delete(key);
      }
    }
  }

  /**
   * Get cache key
   */
  private getCacheKey(profile: LLMProfile): string {
    return `${profile.id}:${profile.provider}:${profile.model}`;
  }
}
