/**
 * Token Usage Metrics Collector
 *
 * Collects and aggregates metrics related to LLM token usage including:
 * - Total, prompt, and completion token counts
 * - Cost tracking
 * - Request frequency
 * - Usage patterns by model and profile
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig, MetricFilter, MetricQueryResult } from "./types.js";
import { TOKEN_METRICS } from "./constants.js";
import { PrometheusFormatter, type PrometheusMetric } from "./utils/prometheus-formatter.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "TokenMetricsCollector" });

/**
 * Token usage data structure
 */
export interface TokenUsageData {
  /** Profile ID or model identifier */
  profileId: string;
  /** Execution ID (optional) */
  executionId?: string;
  /** Node ID (optional) */
  nodeId?: string;
  /** Total tokens used */
  totalTokens: number;
  /** Prompt tokens used */
  promptTokens: number;
  /** Completion tokens used */
  completionTokens: number;
  /** Cost in currency units (optional) */
  cost?: number;
}

/**
 * Token-specific metric collector
 * Extends BaseMetricCollector with token-specific convenience methods
 */
export class TokenMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record token usage from LLM response
   * @param data Token usage data
   */
  recordTokenUsage(data: TokenUsageData): void {
    const labels: Record<string, string> = {
      profile_id: data.profileId,
    };

    if (data.executionId) {
      labels["execution_id"] = data.executionId;
    }

    if (data.nodeId) {
      labels["node_id"] = data.nodeId;
    }

    // Record total tokens
    this.incrementCounter(TOKEN_METRICS.TOTAL_TOKENS, labels, data.totalTokens);

    // Record prompt tokens
    this.incrementCounter(TOKEN_METRICS.PROMPT_TOKENS, labels, data.promptTokens);

    // Record completion tokens
    this.incrementCounter(TOKEN_METRICS.COMPLETION_TOKENS, labels, data.completionTokens);

    // Record cost if available
    if (data.cost !== undefined && data.cost > 0) {
      this.incrementCounter(TOKEN_METRICS.COST, labels, data.cost);
    }

    // Record request count
    this.incrementCounter(TOKEN_METRICS.REQUEST_COUNT, labels, 1);
  }

  /**
   * Record token usage warning
   * @param profileId Profile ID
   * @param currentUsage Current token usage
   * @param limit Token limit
   * @param usagePercentage Usage percentage
   */
  recordTokenWarning(
    profileId: string,
    currentUsage: number,
    limit: number,
    usagePercentage: number,
  ): void {
    this.setGauge("token.usage.warning.percentage", usagePercentage, {
      profile_id: profileId,
      current_usage: currentUsage.toString(),
      limit: limit.toString(),
    });
  }

  /**
   * Get token usage statistics by profile
   * @param profileId Optional profile ID filter
   * @returns Aggregated statistics
   */
  getTokenStatsByProfile(profileId?: string): MetricQueryResult {
    const filter: MetricFilter = profileId
      ? {
          labels: { profile_id: profileId },
        }
      : {};

    return this.query(filter);
  }

  /**
   * Get comprehensive token usage summary
   * @returns Summary of token usage across all profiles
   */
  getTokenUsageSummary(): {
    totalTokens: number;
    totalPromptTokens: number;
    totalCompletionTokens: number;
    totalCost: number;
    totalRequests: number;
    byProfile: Map<
      string,
      {
        totalTokens: number;
        promptTokens: number;
        completionTokens: number;
        cost: number;
        requests: number;
      }
    >;
  } {
    const result = this.query({});

    let totalTokens = 0;
    let totalPromptTokens = 0;
    let totalCompletionTokens = 0;
    let totalCost = 0;
    let totalRequests = 0;
    const byProfile = new Map<
      string,
      {
        totalTokens: number;
        promptTokens: number;
        completionTokens: number;
        cost: number;
        requests: number;
      }
    >();

    for (const [metricName, aggregated] of result.metrics.entries()) {
      const value = aggregated.value;

      switch (metricName) {
        case TOKEN_METRICS.TOTAL_TOKENS:
          totalTokens += value;
          break;
        case TOKEN_METRICS.PROMPT_TOKENS:
          totalPromptTokens += value;
          break;
        case TOKEN_METRICS.COMPLETION_TOKENS:
          totalCompletionTokens += value;
          break;
        case TOKEN_METRICS.COST:
          totalCost += value;
          break;
        case TOKEN_METRICS.REQUEST_COUNT:
          totalRequests += value;
          break;
      }

      // Aggregate by profile
      for (const [labelKey, labelAgg] of aggregated.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          const profileId = labels.profile_id;
          if (!profileId) continue;

          if (!byProfile.has(profileId)) {
            byProfile.set(profileId, {
              totalTokens: 0,
              promptTokens: 0,
              completionTokens: 0,
              cost: 0,
              requests: 0,
            });
          }

          const profileStats = byProfile.get(profileId)!;

          switch (metricName) {
            case TOKEN_METRICS.TOTAL_TOKENS:
              profileStats.totalTokens += labelAgg.value;
              break;
            case TOKEN_METRICS.PROMPT_TOKENS:
              profileStats.promptTokens += labelAgg.value;
              break;
            case TOKEN_METRICS.COMPLETION_TOKENS:
              profileStats.completionTokens += labelAgg.value;
              break;
            case TOKEN_METRICS.COST:
              profileStats.cost += labelAgg.value;
              break;
            case TOKEN_METRICS.REQUEST_COUNT:
              profileStats.requests += labelAgg.value;
              break;
          }
        } catch (error) {
          logger.warn("Failed to parse label key", { labelKey, error });
        }
      }
    }

    return {
      totalTokens,
      totalPromptTokens,
      totalCompletionTokens,
      totalCost,
      totalRequests,
      byProfile,
    };
  }

  /**
   * Get average tokens per request by profile
   * @returns Map of profile ID to average token usage
   */
  getAverageTokensPerRequest(): Map<
    string,
    {
      avgTotalTokens: number;
      avgPromptTokens: number;
      avgCompletionTokens: number;
    }
  > {
    const summary = this.getTokenUsageSummary();
    const averages = new Map<
      string,
      {
        avgTotalTokens: number;
        avgPromptTokens: number;
        avgCompletionTokens: number;
      }
    >();

    for (const [profileId, stats] of summary.byProfile.entries()) {
      if (stats.requests > 0) {
        averages.set(profileId, {
          avgTotalTokens: stats.totalTokens / stats.requests,
          avgPromptTokens: stats.promptTokens / stats.requests,
          avgCompletionTokens: stats.completionTokens / stats.requests,
        });
      }
    }

    return averages;
  }

  /**
   * Export as Prometheus format
   */
  toPrometheus(): string[] {
    const summary = this.getTokenUsageSummary();
    const metrics: PrometheusMetric[] = [];

    // Total tokens counter
    metrics.push({
      name: "token_usage_total",
      type: "counter",
      help: "Total tokens used",
      samples: [{ value: summary.totalTokens }],
    });

    // Prompt tokens
    metrics.push({
      name: "token_prompt_total",
      type: "counter",
      help: "Total prompt tokens used",
      samples: [{ value: summary.totalPromptTokens }],
    });

    // Completion tokens
    metrics.push({
      name: "token_completion_total",
      type: "counter",
      help: "Total completion tokens used",
      samples: [{ value: summary.totalCompletionTokens }],
    });

    // Total cost
    if (summary.totalCost > 0) {
      metrics.push({
        name: "token_cost_total",
        type: "counter",
        help: "Total cost in currency units",
        samples: [{ value: summary.totalCost }],
      });
    }

    // Total requests
    metrics.push({
      name: "token_request_total",
      type: "counter",
      help: "Total LLM requests",
      samples: [{ value: summary.totalRequests }],
    });

    // By profile breakdown
    for (const [profileId, stats] of summary.byProfile) {
      metrics.push({
        name: "token_usage_by_profile_total",
        type: "counter",
        help: "Token usage by profile",
        samples: [
          { labels: { profile_id: profileId, type: "total" }, value: stats.totalTokens },
          { labels: { profile_id: profileId, type: "prompt" }, value: stats.promptTokens },
          { labels: { profile_id: profileId, type: "completion" }, value: stats.completionTokens },
        ],
      });

      if (stats.cost > 0) {
        metrics.push({
          name: "token_cost_by_profile_total",
          type: "counter",
          help: "Cost by profile",
          samples: [{ labels: { profile_id: profileId }, value: stats.cost }],
        });
      }
    }

    // Format all metrics
    return metrics.flatMap(m => PrometheusFormatter.formatMetric(m));
  }

  /**
   * Export as JSON
   */
  toJSON(): Record<string, unknown> {
    const summary = this.getTokenUsageSummary();
    const profilesData: Record<string, unknown> = {};

    for (const [profileId, stats] of summary.byProfile) {
      profilesData[profileId] = {
        totalTokens: stats.totalTokens,
        promptTokens: stats.promptTokens,
        completionTokens: stats.completionTokens,
        cost: stats.cost,
        requests: stats.requests,
      };
    }

    return {
      type: "token",
      summary: {
        totalTokens: summary.totalTokens,
        totalPromptTokens: summary.totalPromptTokens,
        totalCompletionTokens: summary.totalCompletionTokens,
        totalCost: summary.totalCost,
        totalRequests: summary.totalRequests,
      },
      byProfile: profilesData,
    };
  }
}
