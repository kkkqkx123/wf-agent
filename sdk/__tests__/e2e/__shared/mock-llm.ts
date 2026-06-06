/**
 * Mock LLM utilities for E2E testing.
 *
 * Provides a MockLLMClient that returns predefined responses
 * without making real API calls. This is critical for E2E tests that involve
 * LLM nodes or Agent Loop execution.
 *
 * Supports:
 * - Single default response (simple mode)
 * - Sequential response sequences (for multi-iteration tests)
 * - Tool calls in responses (for tool execution tests)
 * - Error throwing (for error handling tests)
 */

import type { LLMProfile, LLMClient, LLMRequest, LLMResult, LLMToolCall } from "@wf-agent/types";
import type { SDKInstance } from "@/api/index.js";

/**
 * Configuration for the Mock LLM Client
 */
export interface MockLLMConfig {
  /** The response content to return for any LLM request (used if no responseSequence) */
  defaultResponse?: string;
  /** Simulated delay in ms before responding (default: 10) */
  simulateDelay?: number;
  /** Whether to record all requests for later inspection */
  recordRequests?: boolean;
  /**
   * Sequential responses for multi-step tests.
   * Each entry in the array defines the response for one LLM call.
   * Once the sequence is exhausted, the last response is repeated.
   */
  responseSequence?: MockResponseEntry[];
  /**
   * If set, throws this error on the Nth request (1-indexed).
   * Useful for testing error handling.
   */
  throwOnRequest?: number;
  /** The error message to throw (used with throwOnRequest) */
  throwErrorMessage?: string;
}

/**
 * A single response entry in a response sequence.
 */
export interface MockResponseEntry {
  /** Response text content */
  content: string;
  /**
   * Optional tool calls to include in the LLM result.
   * When present, the agent loop will execute these tools and continue
   * to the next iteration, enabling multi-iteration testing.
   */
  toolCalls?: LLMToolCall[];
  /**
   * Optional simulated delay in ms for this specific response.
   * Falls back to the handler's global simulateDelay if not set.
   */
  delay?: number;
}

/**
 * MockLLMClient.
 *
 * Implements LLMClient to return canned responses instead of
 * making real API calls. This serves as a mock LLM for E2E tests.
 *
 * Usage:
 *   const mock = new MockLLMClient({ defaultResponse: "Hello world" });
 *   const result = await mock.generate({ messages: [...], profileId: "mock-profile" });
 */
export class MockLLMClient implements LLMClient {
  private config: Required<Omit<MockLLMConfig, 'responseSequence' | 'throwOnRequest' | 'throwErrorMessage'>> & {
    responseSequence: MockResponseEntry[];
    throwOnRequest: number;
    throwErrorMessage: string;
  };
  private requests: LLMRequest[] = [];
  private callCount = 0;
  public readonly profileId: string;

  constructor(config: MockLLMConfig = {}, profileId: string = "mock-llm-profile") {
    this.config = {
      defaultResponse: config.defaultResponse ?? "Mock LLM response for E2E testing",
      simulateDelay: config.simulateDelay ?? 10,
      recordRequests: config.recordRequests ?? true,
      responseSequence: config.responseSequence ?? [],
      throwOnRequest: config.throwOnRequest ?? -1,
      throwErrorMessage: config.throwErrorMessage ?? "Mock LLM error for E2E testing",
    };
    this.profileId = profileId;
  }

  /**
   * Non-streaming generation: returns a canned response.
   */
  async generate(request: LLMRequest): Promise<LLMResult> {
    this.callCount++;

    if (this.config.recordRequests) {
      this.requests.push(request);
    }

    // Check if we should throw on this request
    if (this.config.throwOnRequest > 0 && this.callCount >= this.config.throwOnRequest) {
      throw new Error(this.config.throwErrorMessage);
    }

    // Determine which response to use from the sequence
    const idx = this.config.responseSequence.length > 0
      ? Math.min(this.callCount - 1, this.config.responseSequence.length - 1)
      : -1;

    let content: string;
    let toolCalls: LLMToolCall[] | undefined;
    let delay: number;

    if (idx >= 0) {
      const entry = this.config.responseSequence[idx]!;
      content = entry.content;
      toolCalls = entry.toolCalls;
      delay = entry.delay ?? this.config.simulateDelay;
    } else {
      content = this.config.defaultResponse;
      toolCalls = undefined;
      delay = this.config.simulateDelay;
    }

    if (delay > 0) {
      await new Promise(resolve => setTimeout(resolve, delay));
    }

    return {
      id: `mock-result-${this.callCount}`,
      model: "mock-model-v1",
      content,
      message: {
        role: "assistant",
        content,
        toolCalls: toolCalls as any,
      },
      toolCalls: toolCalls as any,
      finishReason: "stop",
      duration: delay,
    };
  }

  /**
   * Streaming generation: yields a single chunk with the canned response.
   */
  async *generateStream(request: LLMRequest): AsyncIterable<LLMResult> {
    yield await this.generate(request);
  }

  /** Get all recorded requests */
  getRequests(): LLMRequest[] {
    return [...this.requests];
  }

  /** Clear recorded requests */
  clearRequests(): void {
    this.requests = [];
    this.callCount = 0;
  }

  /** Get the count of requests received */
  getRequestCount(): number {
    return this.requests.length;
  }

  /** Get the total call count */
  getCallCount(): number {
    return this.callCount;
  }

  /**
   * Set the response sequence for subsequent requests.
   * Clears any previous sequence.
   */
  setResponseSequence(sequence: MockResponseEntry[]): void {
    this.config.responseSequence = [...sequence];
    this.callCount = 0;
  }

  /**
   * Configure error throwing on a specific request.
   * @param requestNumber The 1-indexed request number to throw on
   * @param message The error message
   */
  setThrowOnRequest(requestNumber: number, message?: string): void {
    this.config.throwOnRequest = requestNumber;
    if (message) {
      this.config.throwErrorMessage = message;
    }
    this.callCount = 0;
  }

  /** Clear the throw-on-request configuration */
  clearThrowOnRequest(): void {
    this.config.throwOnRequest = -1;
  }
}

/**
 * Create SDK options for mock LLM setup.
 * Returns an object that can be spread into createSDK() options.
 *
 * Sets up:
 * - An LLM profile with a standard provider (e.g. OPENAI_CHAT)
 * - The MockLLMClient is registered directly on the ClientFactory at test setup time
 *
 * Note: Unlike the old HumanRelay approach, the mock client registration
 * happens at the factory level via registerMockClient(), not via SDK options.
 * Use setupMockContextProvider() after SDK creation to register the mock client.
 *
 * @param profileId - Profile ID (default: "mock-llm-profile")
 * @returns Partial SDKOptions with mock LLM configuration
 */
export function createMockLLMOptions(
  profileId: string = "mock-llm-profile",
): Record<string, unknown> {
  return {
    profiles: {
      defaultProfileId: profileId,
      profiles: [createMockLLMProfile(profileId)],
    },
  };
}

/**
 * Create a mock LLM profile for E2E tests.
 * Uses OPENAI_CHAT provider (the mock client will intercept via ClientFactory).
 *
 * @param profileId - Unique profile ID (default: "mock-llm-profile")
 * @returns LLMProfile config ready to be passed to SDKOptions.profiles
 */
export function createMockLLMProfile(profileId: string = "mock-llm-profile"): LLMProfile {
  return {
    id: profileId,
    name: "Mock LLM Profile",
    provider: "OPENAI_CHAT",
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
 * Registers the MockLLMClient on the ClientFactory so that
 * any LLM request using the mock profile returns canned responses.
 *
 * @param sdk - The SDK instance
 * @param mockClient - The MockLLMClient instance
 * @param profileId - Profile ID (must match createMockLLMOptions)
 */
export function setupMockContextProvider(
  sdk: SDKInstance,
  mockClient: MockLLMClient,
  profileId: string = "mock-llm-profile",
): void {
  const deps = sdk.getFactory().getDependencies();
  const llmWrapper = deps.getLLMWrapper();

  // Register the mock client on the ClientFactory
  llmWrapper.registerMockClient(profileId, mockClient);

  // Register profile directly on LLMWrapper's ProfileManager
  const profile = createMockLLMProfile(profileId);
  llmWrapper.registerProfile(profile);
  llmWrapper.setDefaultProfile(profileId);
}