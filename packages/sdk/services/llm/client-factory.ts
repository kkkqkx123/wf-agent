/**
 * LLM Client Factory
 *
 * Responsible for creating client instances for different providers.
 * Uses a unified LLMClientImpl with different formatters for each provider.
 * Client instances are cached to improve performance.
 */

import type { LLMClient, LLMProfile } from "@wf-agent/types";
import { LLMClientImpl } from "./client.js";
import { getFormatter } from "./formatters/index.js";
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
   * Gets the formatter from the FormatterRegistry and creates a unified LLMClientImpl.
   */
  private createClientByProvider(profile: LLMProfile): LLMClient {
    // Validate provider is supported
    const supportedProviders = [
      "OPENAI_CHAT",
      "OPENAI_RESPONSE",
      "ANTHROPIC",
      "GEMINI_NATIVE",
      "GEMINI_OPENAI",
    ];

    if (!supportedProviders.includes(profile.provider)) {
      throw new ConfigurationError(`Unsupported LLM provider: ${profile.provider}`, "provider", {
        provider: profile.provider,
        model: profile.model,
        supportedProviders,
      });
    }

    // Get formatter for this provider
    try {
      const formatter = getFormatter(profile.provider);
      return new LLMClientImpl(profile, formatter);
    } catch (error) {
      throw new ConfigurationError(`Failed to create client for provider: ${profile.provider}`, "provider", {
        provider: profile.provider,
        originalError: error instanceof Error ? error.message : String(error),
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

