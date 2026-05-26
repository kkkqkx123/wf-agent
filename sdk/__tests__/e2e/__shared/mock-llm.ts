/**
 * Mock LLM utilities for E2E testing.
 *
 * Provides a HumanRelay-based mock LLM client that returns predefined responses
 * without making real API calls. This is critical for E2E tests that involve
 * LLM nodes or Agent Loop execution.
 */

import type { LLMProfile, HumanRelayHandler, HumanRelayRequest, HumanRelayResponse } from "@wf-agent/types";
import type { SDKInstance } from "@/api/index.js";

/**
 * Configuration for the Mock HumanRelay handler
 */
export interface MockLLMConfig {
  /** The response content to return for any LLM request */
  defaultResponse?: string;
  /** Simulated delay in ms before responding (default: 10) */
  simulateDelay?: number;
  /** Whether to record all requests for later inspection */
  recordRequests?: boolean;
}

/**
 * Mock HumanRelayHandler.
 *
 * Implements HumanRelayHandler to return a canned response instead of
 * requiring actual human input. This serves as a mock LLM for E2E tests.
 */
export class MockHumanRelayHandler implements HumanRelayHandler {
  private config: Required<MockLLMConfig>;
  private requests: HumanRelayRequest[] = [];

  constructor(config: MockLLMConfig = {}) {
    this.config = {
      defaultResponse: config.defaultResponse ?? "Mock LLM response for E2E testing",
      simulateDelay: config.simulateDelay ?? 10,
      recordRequests: config.recordRequests ?? true,
    };
  }

  async handle(request: HumanRelayRequest): Promise<HumanRelayResponse> {
    if (this.config.recordRequests) {
      this.requests.push(request);
    }

    if (this.config.simulateDelay > 0) {
      await new Promise(resolve => setTimeout(resolve, this.config.simulateDelay));
    }

    return {
      requestId: request.requestId,
      content: this.config.defaultResponse,
      timestamp: Date.now(),
    };
  }

  /** Get all recorded requests */
  getRequests(): HumanRelayRequest[] {
    return [...this.requests];
  }

  /** Clear recorded requests */
  clearRequests(): void {
    this.requests = [];
  }

  /** Get the count of requests received */
  getRequestCount(): number {
    return this.requests.length;
  }
}

/**
 * Create a mock LLM profile for E2E tests.
 * Uses HUMAN_RELAY provider so that it goes through the mock handler.
 *
 * @param profileId - Unique profile ID (default: "mock-llm-profile")
 * @returns LLMProfile config ready to be passed to SDKOptions.profiles
 */
export function createMockLLMProfile(profileId: string = "mock-llm-profile"): LLMProfile {
  return {
    id: profileId,
    name: "Mock LLM Profile",
    provider: "HUMAN_RELAY",
    model: "mock-model-v1",
    apiKey: "mock-api-key",
    parameters: {
      temperature: 0,
      maxTokens: 100,
    },
  };
}

/**
 * Configure the mock LLM environment on an SDK instance.
 * Must be called after SDK bootstrap.
 *
 * Registers the mock profile directly on the LLMWrapper's ProfileManager
 * (bypassing LLMProfileRegistryAPI which has its own disconnected ProfileManager)
 *
 * @param sdk - The SDK instance
 * @param profileId - Profile ID (must match createMockLLMOptions)
 */
export function setupMockContextProvider(
  sdk: SDKInstance,
  profileId: string = "mock-llm-profile",
): void {
  const deps = sdk.getFactory().getDependencies();
  const llmWrapper = deps.getLLMWrapper();

  // Register profile directly on LLMWrapper's ProfileManager
  // (LLMProfileRegistryAPI creates its own disconnected ProfileManager)
  const profile = createMockLLMProfile(profileId);
  llmWrapper.registerProfile(profile);
  llmWrapper.setDefaultProfile(profileId);
}
