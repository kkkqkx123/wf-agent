/**
 * LLM Client Factory
 *
 * Responsible for creating client instances for different providers.
 * Clients are created using the factory pattern, and client instances are cached to improve performance.
 */

import type { LLMClient, LLMProfile } from "@wf-agent/types";
import { OpenAIChatClient } from "./clients/openai-chat.js";
import { OpenAIResponseClient } from "./clients/openai-response.js";
import { AnthropicClient } from "./clients/anthropic.js";
import { GeminiNativeClient } from "./clients/gemini-native.js";
import { GeminiOpenAIClient } from "./clients/gemini-openai.js";
import { ConfigurationError } from "@wf-agent/types";

/**
 * Client Factory Class
 */
export class ClientFactory {
  private clientCache: Map<string, LLMClient> = new Map();
  private mockClients: Map<string, LLMClient> = new Map();

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

    // Check mock clients first (for testing)
    const mockClient = this.mockClients.get(profile.id);
    if (mockClient) {
      this.clientCache.set(cacheKey, mockClient);
      return mockClient;
    }

    // Create a new client.
    const client = this.createClientByProvider(profile);

    // Cache Client
    this.clientCache.set(cacheKey, client);

    return client;
  }

  /**
   * Generate a cache key for a profile.
   */
  private getCacheKey(profile: LLMProfile): string {
    return `${profile.id}::${profile.model}`;
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
   * Register a mock client for testing.
   * When a profile with the given ID creates a client, the mock client will be used instead.
   *
   * @param profileId The profile ID to associate with the mock client
   * @param client The mock LLM client instance
   */
  registerMockClient(profileId: string, client: LLMClient): void {
    this.mockClients.set(profileId, client);
  }

  /**
   * Clear all registered mock clients
   */
  clearMockClients(): void {
    this.mockClients.clear();
  }

  /**
   * Clear the cache for a specific profile
   * @param profileId Profile ID to clear from cache
   */
  clearClientCache(profileId: string): void {
    for (const [key] of this.clientCache.entries()) {
      if (key.startsWith(profileId)) {
        this.clientCache.delete(key);
      }
    }
  }

  /**
   * Clear the client cache
   */
  clearCache(): void {
    this.clientCache.clear();
  }
}
