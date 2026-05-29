/**
 * MockLLMService - Simulates LLM responses for integration testing
 *
 * Provides a configurable mock LLM service that:
 * - Returns canned responses
 * - Tracks call counts
 * - Supports response sequences for multi-iteration testing
 * - Creates a minimal AgentLoopExecutor
 */

import type { AgentLoopResult } from "@wf-agent/types";
import type { AgentLoopEntity } from "@/agent/entities/agent-loop-entity.js";
import { AgentLoopExecutor } from "@/agent/execution/executors/agent-loop-executor.js";
import { LLMExecutor } from "@/core/executors/llm-executor.js";

// =============================================================================
// Constants
// =============================================================================

const MOCK_RESPONSE = "This is a mock LLM response for integration testing.";

// =============================================================================
// Types
// =============================================================================

export interface MockLLMConfig {
  /** Default response text */
  defaultResponse?: string;
  /** Response sequence (overrides defaultResponse, cycles through list) */
  responseSequence?: string[];
  /** Simulate processing delay in ms */
  simulateDelay?: number;
}

// =============================================================================
// MockLLMService
// =============================================================================

export class MockLLMService {
  private requests: string[] = [];
  private config: Required<MockLLMConfig>;
  private callIndex = 0;

  constructor(config: MockLLMConfig = {}) {
    this.config = {
      defaultResponse: config.defaultResponse ?? MOCK_RESPONSE,
      responseSequence: config.responseSequence ?? [],
      simulateDelay: config.simulateDelay ?? 5,
    };
  }

  /**
   * Get the next response in sequence, or default if no sequence configured
   */
  private getNextResponse(): string {
    if (this.config.responseSequence.length > 0) {
      const response = this.config.responseSequence[this.callIndex % this.config.responseSequence.length];
      this.callIndex++;
      return response;
    }
    return this.config.defaultResponse;
  }

  /**
   * Simulate an LLM generate call
   */
  async generate(messages: string[]): Promise<{ content: string; toolCalls?: any[] }> {
    this.requests.push(...messages);
    if (this.config.simulateDelay > 0) {
      await new Promise(resolve => setTimeout(resolve, this.config.simulateDelay));
    }
    return {
      content: this.getNextResponse(),
      toolCalls: [],
    };
  }

  /**
   * Get total request count
   */
  getRequestCount(): number {
    return this.requests.length;
  }

  /**
   * Clear request history
   */
  clearRequests(): void {
    this.requests = [];
    this.callIndex = 0;
  }

  /**
   * Create a minimal AgentLoopExecutor for testing
   * This creates an executor with mock dependencies wired to the mock LLM
   */
  createExecutor(): AgentLoopExecutor {
    // Create individual mock dependencies
    const mockLLMExecutor = {
      execute: async (messages: any[], options?: any, signal?: AbortSignal) => {
        const content = this.getNextResponse();
        return {
          content,
          toolCalls: [],
          usage: { promptTokens: 10, completionTokens: 20, totalTokens: 30 },
        };
      },
      executeStream: async function* () {},
    };

    return {
      execute: async (entity: AgentLoopEntity): Promise<AgentLoopResult> => {
        // Simulate a single iteration of agent loop execution
        const response = this.getNextResponse();
        entity.state.complete();
        return {
          success: true,
          content: response,
          iterations: 1,
          toolCallCount: 0,
        };
      },
      executeStream: async function* (entity: AgentLoopEntity) {},
    } as unknown as AgentLoopExecutor;
  }
}
