/**
 * Token Usage Tracker
 *
 * Core Responsibilities:
 * 1. Accumulate the number of tokens used over multiple rounds of conversations.
 * 2. Support the accumulation of tokens in a streaming response format.
 * 3. Provide a local estimation method as a fallback option.
 * 4. Support extended features such as cost calculation.
 * 5. Record the history of each API call, enabling precise rollback and historical analysis.
 *
 * Design Principles:
 * - An independent token statistics module, decoupled from message management.
 * - Support for cumulative statistics over multiple rounds of conversations.
 * - Refer to the streaming token accumulation mechanism of the Anthropic SDK.
 * - Support for historical recording and precise rollback capabilities.
 */

import type {
  LLMMessage,
  LLMUsage,
  TokenUsageHistory,
  TokenUsageStatistics,
  TokenUsageStats,
} from "@wf-agent/types";
import { generateId } from "../../../utils/id-utils.js";
import { now } from "@wf-agent/common-utils";
import {
  estimateTokens as estimateTokensUtil,
  getTokenUsage as getTokenUsageUtil,
  isTokenLimitExceeded as isTokenLimitExceededUtil,
} from "./token-utils.js";

/**
 * Complete Token usage statistics (including current cumulative and lifecycle statistics)
 */
export interface FullTokenUsageStats {
  /** Current cumulative statistics (can be rolled back) */
  current: TokenUsageStats;
  /** Lifecycle statistics (non-reversible, reflecting the actual total token consumption) */
  lifetime: TokenUsageStats;
}

/**
 * Token uses tracker configuration options.
 */
export interface TokenUsageTrackerOptions {
  /** Token limit threshold; exceeding this value triggers a warning. */
  tokenLimit?: number;
  /** Whether to enable the history feature */
  enableHistory?: boolean;
  /** Maximum number of historical records */
  maxHistorySize?: number;
}

/**
 * Token uses a tracker class
 */
export class TokenUsageTracker {
  private cumulativeUsage: TokenUsageStats | null = null;
  private currentRequestUsage: TokenUsageStats | null = null;
  private totalLifetimeUsage: TokenUsageStats | null = null; // Ignore the actual total number of tokens for the rollback.
  private usageHistory: TokenUsageHistory[] = []; // History record array
  private tokenLimit: number;
  private enableHistory: boolean;
  private maxHistorySize: number;

  constructor(options: TokenUsageTrackerOptions = {}) {
    this.tokenLimit = options.tokenLimit || 4000;
    this.enableHistory = options.enableHistory ?? true;
    this.maxHistorySize = options.maxHistorySize || 1000;
  }

  /**
   * 更新 API 返回的 Token 使用统计
   *
   * 保存当前请求的 usage，但不立即累加到总使用量
   * 需要调用 finalizeCurrentRequest() 来完成累加
   *
   * @param usage Token 使用数据
   */
  updateApiUsage(usage: LLMUsage): void {
    // Save the usage of the current request.
    this.currentRequestUsage = {
      promptTokens: usage.promptTokens,
      completionTokens: usage.completionTokens,
      totalTokens: usage.totalTokens,
      rawUsage: usage,
    };
  }

  /**
   * 累积流式响应的 Token 使用统计
   *
   * 参考 Anthropic SDK 的 #accumulateMessage() 方法：
   * - 在流式传输期间持续更新 token 统计
   * - message_delta 事件提供增量更新
   *
   * 修复说明：
   * - 不再在流式传输期间更新 cumulativeUsage
   * - 只更新 currentRequestUsage，避免统计错误
   * - 在 finalizeCurrentRequest() 中统一累加到 cumulativeUsage
   *
   * @param usage Token 使用数据（增量）
   */
  accumulateStreamUsage(usage: LLMUsage): void {
    if (!this.currentRequestUsage) {
      // The first time the usage is received, it is usually in conjunction with the message_start event.
      this.currentRequestUsage = {
        promptTokens: usage.promptTokens,
        completionTokens: usage.completionTokens,
        totalTokens: usage.totalTokens,
        rawUsage: usage,
      };
    } else {
      // Subsequent usage typically involves the `message_delta` event for incremental updates.
      this.currentRequestUsage.promptTokens = usage.promptTokens;
      this.currentRequestUsage.completionTokens = usage.completionTokens;
      this.currentRequestUsage.totalTokens = usage.totalTokens;
      this.currentRequestUsage.rawUsage = usage;
    }

    // Note: CumulativeUsage is no longer updated during streaming
    // Avoid overwriting previous cumulative statistics
    // 累积操作将在 finalizeCurrentRequest() 中统一处理
  }

  /**
   * Token statistics for the current request
   *
   * Add the current request's usage to the total usage
   * Call this function after each API call is completed
   */
  finalizeCurrentRequest(): void {
    if (this.currentRequestUsage) {
      if (!this.cumulativeUsage) {
        this.cumulativeUsage = { ...this.currentRequestUsage };
      } else {
        // Accumulated to the total usage amount
        this.cumulativeUsage.promptTokens += this.currentRequestUsage.promptTokens;
        this.cumulativeUsage.completionTokens += this.currentRequestUsage.completionTokens;
        this.cumulativeUsage.totalTokens += this.currentRequestUsage.totalTokens;
      }

      // Accumulate this amount to the total lifetime usage (ignoring any rollbacks).
      if (!this.totalLifetimeUsage) {
        this.totalLifetimeUsage = { ...this.currentRequestUsage };
      } else {
        this.totalLifetimeUsage.promptTokens += this.currentRequestUsage.promptTokens;
        this.totalLifetimeUsage.completionTokens += this.currentRequestUsage.completionTokens;
        this.totalLifetimeUsage.totalTokens += this.currentRequestUsage.totalTokens;
      }

      // Add to the history.
      if (this.enableHistory) {
        this.addToHistory(this.currentRequestUsage);
      }

      this.currentRequestUsage = null;
    }
  }

  /**
   * Add to the history
   * @param usage Token usage statistics
   */
  private addToHistory(usage: TokenUsageStats): void {
    const rawUsage = usage.rawUsage as Record<string, unknown> | undefined;
    const historyItem: TokenUsageHistory = {
      requestId: generateId(),
      timestamp: now(),
      promptTokens: usage.promptTokens,
      completionTokens: usage.completionTokens,
      totalTokens: usage.totalTokens,
      cost: rawUsage?.["totalCost"] as number | undefined,
      model: rawUsage?.["model"] as string | undefined,
      rawUsage: usage.rawUsage as LLMUsage | undefined,
    };

    this.usageHistory.push(historyItem);

    // Limit the number of historical records.
    if (this.usageHistory.length > this.maxHistorySize) {
      this.usageHistory.shift();
    }
  }

  /**
   * Get the cumulative statistics of Token usage (which will be restored upon rollback)
   * @returns Token usage statistics
   */
  getCumulativeUsage(): TokenUsageStats | null {
    return this.cumulativeUsage ? { ...this.cumulativeUsage } : null;
  }

  /**
   * Get the total token usage statistics over the lifecycle (ignoring reverts to reflect the actual total token consumption)
   * @returns Token usage statistics
   */
  getTotalLifetimeUsage(): TokenUsageStats | null {
    return this.totalLifetimeUsage ? { ...this.totalLifetimeUsage } : null;
  }

  /**
   * Obtain complete Token usage statistics (including current cumulative and lifecycle statistics)
   * @returns Complete Token usage statistics
   */
  getFullUsageStats(): FullTokenUsageStats | null {
    if (!this.cumulativeUsage && !this.totalLifetimeUsage) {
      return null;
    }

    return {
      current: {
        promptTokens: this.cumulativeUsage?.promptTokens || 0,
        completionTokens: this.cumulativeUsage?.completionTokens || 0,
        totalTokens: this.cumulativeUsage?.totalTokens || 0,
        rawUsage: this.cumulativeUsage?.rawUsage,
      },
      lifetime: {
        promptTokens: this.totalLifetimeUsage?.promptTokens || 0,
        completionTokens: this.totalLifetimeUsage?.completionTokens || 0,
        totalTokens: this.totalLifetimeUsage?.totalTokens || 0,
        rawUsage: this.totalLifetimeUsage?.rawUsage,
      },
    };
  }

  /**
   * Get statistics on the Token used for the current request
   * @returns Statistics on Token usage
   */
  getCurrentRequestUsage(): TokenUsageStats | null {
    return this.currentRequestUsage ? { ...this.currentRequestUsage } : null;
  }

  /**
   * Estimate the number of tokens used by the messages (local estimation method)
   *
   * This method is used as a fallback when the API does not return the usage data.
   *
   * @param messages: An array of messages
   * @returns: The total number of tokens
   */
  estimateTokens(messages: LLMMessage[]): number {
    return estimateTokensUtil(messages);
  }

  /**
   * Get Token usage (prefer using API statistics; otherwise, use local estimates)
   *
   * @param messages Array of messages (for local estimation)
   * @returns Number of Tokens
   */
  getTokenUsage(messages: LLMMessage[]): number {
    // Prioritize the use of accumulated API statistics.
    if (this.cumulativeUsage) {
      return this.cumulativeUsage.totalTokens;
    }

    // Use local estimation methods
    return getTokenUsageUtil(null, messages);
  }

  /**
   * Check whether the use of tokens exceeds the limit
   *
   * @param messages Array of messages (used for local estimation)
   * @returns Whether the limit has been exceeded
   */
  isTokenLimitExceeded(messages: LLMMessage[]): boolean {
    const tokensUsed = this.getTokenUsage(messages);
    return isTokenLimitExceededUtil(tokensUsed, this.tokenLimit);
  }

  /**
   * Resetting tokens using statistics. Note: This method will not reset totalLifetimeUsage, as it is part of the lifetime statistics.
   *
   */
  reset(): void {
    this.cumulativeUsage = null;
    this.currentRequestUsage = null;
    this.usageHistory = [];
    // Do not reset totalLifetimeUsage; maintain the lifetime statistics.
  }

  /**
   * Fully reset tokens using statistics (including lifecycle statistics). Use only when a complete reset is necessary, such as when a thread is destroyed.
   *
   */
  fullReset(): void {
    this.cumulativeUsage = null;
    this.currentRequestUsage = null;
    this.totalLifetimeUsage = null;
    this.usageHistory = [];
  }

  /**
   * Clone a TokenUsageTracker instance (including its history)
   * @returns The cloned TokenUsageTracker instance
   */
  clone(): TokenUsageTracker {
    const clonedTracker = new TokenUsageTracker({
      tokenLimit: this.tokenLimit,
      enableHistory: this.enableHistory,
      maxHistorySize: this.maxHistorySize,
    });

    if (this.cumulativeUsage) {
      clonedTracker.cumulativeUsage = { ...this.cumulativeUsage };
    }

    if (this.currentRequestUsage) {
      clonedTracker.currentRequestUsage = { ...this.currentRequestUsage };
    }

    if (this.totalLifetimeUsage) {
      clonedTracker.totalLifetimeUsage = { ...this.totalLifetimeUsage };
    }

    // Clone the history record
    clonedTracker.usageHistory = [...this.usageHistory];

    return clonedTracker;
  }

  /**
   * Set the Token to use statistical status
   * This is for restoring the state from a checkpoint.
   * Note: This method will not restore the totalLifetimeUsage, as it is part of the lifetime statistics.
   * @param cumulativeUsage: Cumulative Token usage statistics
   * @param currentRequestUsage: Current request Token usage statistics (optional)
   */
  setState(
    cumulativeUsage: TokenUsageStats | null,
    currentRequestUsage?: TokenUsageStats | null,
  ): void {
    if (cumulativeUsage) {
      this.cumulativeUsage = { ...cumulativeUsage };
    } else {
      this.cumulativeUsage = null;
    }

    if (currentRequestUsage !== undefined) {
      if (currentRequestUsage) {
        this.currentRequestUsage = { ...currentRequestUsage };
      } else {
        this.currentRequestUsage = null;
      }
    }
    // Do not restore totalLifetimeUsage; maintain the lifetime statistics.
  }

  /**
   * Get the token usage statistics
   * Used for saving to checkpoints
   * @returns Token usage statistics
   */
  getState(): {
    cumulativeUsage: TokenUsageStats | null;
    currentRequestUsage: TokenUsageStats | null;
  } {
    return {
      cumulativeUsage: this.cumulativeUsage ? { ...this.cumulativeUsage } : null,
      currentRequestUsage: this.currentRequestUsage ? { ...this.currentRequestUsage } : null,
    };
  }

  /**
   * Set the total lifetime token usage statistics
   * Used to restore lifetime statistics from persistent storage
   * @param totalLifetimeUsage: Total lifetime token usage statistics
   */
  setTotalLifetimeUsage(totalLifetimeUsage: TokenUsageStats | null): void {
    if (totalLifetimeUsage) {
      this.totalLifetimeUsage = { ...totalLifetimeUsage };
    } else {
      this.totalLifetimeUsage = null;
    }
  }

  /**
   * Get the total number of Tokens used throughout the lifecycle
   * Used for saving to persistent storage
   * @returns Total number of Tokens used during the lifecycle
   */
  getTotalLifetimeUsageState(): TokenUsageStats | null {
    return this.totalLifetimeUsage ? { ...this.totalLifetimeUsage } : null;
  }

  /**
   * Get historical records
   * @returns A copy of the historical records array
   */
  getUsageHistory(): TokenUsageHistory[] {
    return [...this.usageHistory];
  }

  /**
   * Get the last N historical records
   * @param n: Number of records
   * @returns: The last N historical records
   */
  getRecentHistory(n: number): TokenUsageHistory[] {
    return this.usageHistory.slice(-n);
  }

  /**
   * Get statistical information
   * @returns Statistical information
   */
  getStatistics(): TokenUsageStatistics {
    if (this.usageHistory.length === 0) {
      return {
        totalRequests: 0,
        averageTokens: 0,
        maxTokens: 0,
        minTokens: 0,
        totalCost: 0,
        totalPromptTokens: 0,
        totalCompletionTokens: 0,
      };
    }

    const totalTokens = this.usageHistory.reduce((sum, h) => sum + h.totalTokens, 0);
    const totalPromptTokens = this.usageHistory.reduce((sum, h) => sum + h.promptTokens, 0);
    const totalCompletionTokens = this.usageHistory.reduce((sum, h) => sum + h.completionTokens, 0);
    const maxTokens = Math.max(...this.usageHistory.map(h => h.totalTokens));
    const minTokens = Math.min(...this.usageHistory.map(h => h.totalTokens));
    const totalCost = this.usageHistory.reduce((sum, h) => sum + (h.cost || 0), 0);

    return {
      totalRequests: this.usageHistory.length,
      averageTokens: totalTokens / this.usageHistory.length,
      maxTokens,
      minTokens,
      totalCost,
      totalPromptTokens,
      totalCompletionTokens,
    };
  }

  /**
   * Back to the request before the specified one
   * @param requestIndex Request index (starting from 0)
   */
  rollbackToRequest(requestIndex: number): void {
    if (requestIndex < 0 || requestIndex > this.usageHistory.length) {
      throw new Error(
        `Invalid request index: ${requestIndex}. Valid range: 0-${this.usageHistory.length}`,
      );
    }

    // Recalculate cumulativeUsage
    this.cumulativeUsage = {
      promptTokens: 0,
      completionTokens: 0,
      totalTokens: 0,
    };

    for (let i = 0; i < requestIndex; i++) {
      const item = this.usageHistory[i];
      if (item) {
        this.cumulativeUsage.promptTokens += item.promptTokens;
        this.cumulativeUsage.completionTokens += item.completionTokens;
        this.cumulativeUsage.totalTokens += item.totalTokens;
      }
    }

    // Delete the historical records after the rollback.
    this.usageHistory = this.usageHistory.slice(0, requestIndex);
  }

  /**
   * Revert to a state before the specified request ID
   * @param requestId Request ID
   */
  rollbackToRequestId(requestId: string): void {
    const index = this.usageHistory.findIndex(h => h.requestId === requestId);
    if (index === -1) {
      throw new Error(`Request ID not found: ${requestId}`);
    }
    this.rollbackToRequest(index);
  }

  /**
   * Roll back to a time before the specified timestamp
   * @param timestamp Timestamp
   */
  rollbackToTimestamp(timestamp: number): void {
    const index = this.usageHistory.findIndex(h => h.timestamp >= timestamp);
    if (index === -1) {
      // All records are before the timestamp; no rollback will be performed.
      return;
    }
    this.rollbackToRequest(index);
  }

  /**
   * Clear the history records.
   */
  clearHistory(): void {
    this.usageHistory = [];
  }
}
