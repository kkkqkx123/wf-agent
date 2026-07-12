/**
 * Retry Budget Metrics Collector
 *
 * Collects and reports metrics related to retry budget management including:
 * - Global budget consumption (count and time)
 * - Retry attempts and outcomes
 * - Timeout error handling
 * - Per-consumer/branch retry tracking
 * - Backoff delay distribution
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig, MetricFilter, MetricQueryResult } from "./types.js";
import { RETRY_METRICS } from "./constants.js";
import { PrometheusFormatter, type PrometheusMetric } from "./utils/prometheus-formatter.js";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";

const logger = createContextualLogger({ component: "RetryBudgetMetricsCollector" });

/**
 * Retry budget-specific metric collector
 * Extends BaseMetricCollector with retry and timeout-specific convenience methods
 */
export class RetryBudgetMetricsCollector extends BaseMetricCollector {
  private activeRetries: Map<string, number> = new Map();

  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record retry budget consumption
   * @param consumedCount Number of retries consumed
   * @param consumedTimeMs Time budget consumed in milliseconds
   * @param workflowId Workflow ID for context
   */
  recordBudgetConsumption(
    consumedCount: number,
    consumedTimeMs: number,
    workflowId?: string,
  ): void {
    const labels: Record<string, string> = {};
    if (workflowId) {
      labels["workflow_id"] = workflowId;
    }

    if (consumedCount > 0) {
      this.incrementCounter(RETRY_METRICS.BUDGET_CONSUMED_COUNT, labels, consumedCount);
    }

    if (consumedTimeMs > 0) {
      this.observeHistogram(RETRY_METRICS.BUDGET_CONSUMED_TIME, consumedTimeMs, labels);
    }
  }

  /**
   * Record remaining budget state
   * @param remainingCount Remaining retry count
   * @param remainingTimeMs Remaining time budget in milliseconds
   * @param workflowId Workflow ID for context
   */
  recordBudgetRemaining(
    remainingCount: number,
    remainingTimeMs: number,
    workflowId?: string,
  ): void {
    const labels: Record<string, string> = {};
    if (workflowId) {
      labels["workflow_id"] = workflowId;
    }

    this.setGauge(RETRY_METRICS.BUDGET_REMAINING_COUNT, Math.max(0, remainingCount), labels);
    this.setGauge(RETRY_METRICS.BUDGET_REMAINING_TIME, Math.max(0, remainingTimeMs), labels);
  }

  /**
   * Record budget exhaustion event
   * @param exhaustionType "count" | "time" | "both"
   * @param workflowId Workflow ID
   */
  recordBudgetExhausted(exhaustionType: "count" | "time" | "both", workflowId?: string): void {
    const labels: Record<string, string> = {
      exhaustion_type: exhaustionType,
    };
    if (workflowId) {
      labels["workflow_id"] = workflowId;
    }

    this.incrementCounter(RETRY_METRICS.BUDGET_EXHAUSTED, labels);
  }

  /**
   * Record a retry attempt
   * @param consumerId Unique identifier for retry consumer (e.g., node ID, branch ID)
   * @param delayMs Delay before this retry attempt
   * @param attemptNumber Which attempt number this is (0-based)
   */
  recordRetryAttempt(consumerId: string, delayMs: number, attemptNumber: number): void {
    const labels: Record<string, string> = {
      consumer_id: consumerId,
      attempt_number: attemptNumber.toString(),
    };

    this.incrementCounter(RETRY_METRICS.ATTEMPT_TOTAL, labels);
    this.observeHistogram(RETRY_METRICS.DELAY_DURATION, delayMs, labels);

    // Track active retries
    const currentActive = this.activeRetries.get(consumerId) ?? 0;
    this.activeRetries.set(consumerId, currentActive + 1);
    this.setGauge(RETRY_METRICS.CONSUMER_ACTIVE_RETRIES, currentActive + 1, {
      consumer_id: consumerId,
    });
  }

  /**
   * Record successful retry attempt
   * @param consumerId Unique identifier for retry consumer
   * @param totalAttempts Total number of attempts made
   * @param totalDelayMs Total delay across all attempts
   */
  recordRetrySuccess(consumerId: string, totalAttempts: number, totalDelayMs: number): void {
    const labels: Record<string, string> = {
      consumer_id: consumerId,
      total_attempts: totalAttempts.toString(),
    };

    this.incrementCounter(RETRY_METRICS.ATTEMPT_SUCCEEDED, labels);
    this.incrementCounter(RETRY_METRICS.ULTIMATELY_SUCCEEDED, labels);

    // Record aggregate delay
    if (totalDelayMs > 0) {
      this.observeHistogram(RETRY_METRICS.DELAY_DURATION, totalDelayMs, {
        consumer_id: consumerId,
        aggregated: "true",
      });
    }

    // Clear active retries
    this.activeRetries.delete(consumerId);
    this.setGauge(RETRY_METRICS.CONSUMER_ACTIVE_RETRIES, 0, {
      consumer_id: consumerId,
    });
  }

  /**
   * Record failed retry attempt (exhausted all retries)
   * @param consumerId Unique identifier for retry consumer
   * @param totalAttempts Total number of attempts made
   * @param reason Reason for failure ("budget_exhausted" | "max_retries" | "policy")
   */
  recordRetryFailure(
    consumerId: string,
    totalAttempts: number,
    reason: "budget_exhausted" | "max_retries" | "policy",
  ): void {
    const labels: Record<string, string> = {
      consumer_id: consumerId,
      total_attempts: totalAttempts.toString(),
      failure_reason: reason,
    };

    this.incrementCounter(RETRY_METRICS.ATTEMPT_FAILED, labels);
    this.incrementCounter(RETRY_METRICS.ULTIMATELY_FAILED, labels);

    // Clear active retries
    this.activeRetries.delete(consumerId);
    this.setGauge(RETRY_METRICS.CONSUMER_ACTIVE_RETRIES, 0, {
      consumer_id: consumerId,
    });
  }

  /**
   * Record timeout error occurrence
   * @param consumerId Where timeout occurred
   * @param timeoutMs Configured timeout duration
   * @param actualMs Actual execution time before timeout
   */
  recordTimeoutError(consumerId: string, timeoutMs: number, actualMs: number): void {
    const labels: Record<string, string> = {
      consumer_id: consumerId,
      timeout_configured_ms: timeoutMs.toString(),
    };

    this.incrementCounter(RETRY_METRICS.TIMEOUT_ERROR_COUNT, labels);

    // Timeout errors should NOT be retried (exempted)
    this.incrementCounter(RETRY_METRICS.TIMEOUT_ERROR_NO_RETRY, labels);

    // Record the actual timeout value
    this.observeHistogram("retry.timeout.actual_ms", actualMs, { consumer_id: consumerId });
  }

  /**
   * Record backoff factor for exponential backoff strategy
   * @param consumerId Consumer identifier
   * @param baseDelay Base delay in milliseconds
   * @param multiplier Backoff multiplier (e.g., 2 for exponential)
   * @param attemptNumber Current attempt number
   * @param calculatedDelay The delay calculated for this attempt
   */
  recordBackoffCalculation(
    consumerId: string,
    baseDelay: number,
    multiplier: number,
    attemptNumber: number,
    calculatedDelay: number,
  ): void {
    const labels: Record<string, string> = {
      consumer_id: consumerId,
      base_delay_ms: baseDelay.toString(),
      multiplier: multiplier.toString(),
      attempt_number: attemptNumber.toString(),
    };

    this.setGauge(RETRY_METRICS.BACKOFF_FACTOR, multiplier, labels);
    this.observeHistogram("retry.backoff.calculated_delay_ms", calculatedDelay, labels);
  }

  /**
   * Get retry metrics for specific consumer
   * @param consumerId Optional consumer ID filter
   * @returns Aggregated retry statistics
   */
  getRetryStats(consumerId?: string): MetricQueryResult {
    const filter: MetricFilter = consumerId
      ? {
          labels: { consumer_id: consumerId },
        }
      : {};

    return this.query(filter);
  }

  /**
   * Get budget summary
   * @returns Summary of budget consumption patterns
   */
  getBudgetSummary(): {
    totalBudgetConsumed: number;
    totalTimeConsumed: number;
    totalExhausted: number;
    exhaustionByType: Map<string, number>;
  } {
    const result = this.query({});

    let totalBudgetConsumed = 0;
    let totalTimeConsumed = 0;
    let totalExhausted = 0;
    const exhaustionByType = new Map<string, number>();

    for (const [metricName, aggregated] of result.metrics.entries()) {
      if (metricName === RETRY_METRICS.BUDGET_CONSUMED_COUNT) {
        totalBudgetConsumed += aggregated.value;
      } else if (metricName === RETRY_METRICS.BUDGET_CONSUMED_TIME) {
        totalTimeConsumed += aggregated.value;
      } else if (metricName === RETRY_METRICS.BUDGET_EXHAUSTED) {
        totalExhausted += aggregated.value;

        // Count by exhaustion type
        for (const [labelKey, labelAgg] of aggregated.byLabel.entries()) {
          try {
            const labels = JSON.parse(labelKey);
            const exhaustionType = labels.exhaustion_type || "unknown";
            exhaustionByType.set(
              exhaustionType,
              (exhaustionByType.get(exhaustionType) || 0) + labelAgg.value,
            );
          } catch (error) {
            logger.warn("Failed to parse exhaustion type label", { labelKey, error });
          }
        }
      }
    }

    return {
      totalBudgetConsumed,
      totalTimeConsumed,
      totalExhausted,
      exhaustionByType,
    };
  }

  /**
   * Get retry outcomes summary
   * @returns Success and failure counts
   */
  getOutcomeSummary(): {
    successfulRetries: number;
    failedRetries: number;
    successRate: number;
    failureReasons: Map<string, number>;
  } {
    const result = this.query({});

    let successfulRetries = 0;
    let failedRetries = 0;
    const failureReasons = new Map<string, number>();

    for (const [metricName, aggregated] of result.metrics.entries()) {
      if (metricName === RETRY_METRICS.ULTIMATELY_SUCCEEDED) {
        successfulRetries += aggregated.value;
      } else if (metricName === RETRY_METRICS.ULTIMATELY_FAILED) {
        failedRetries += aggregated.value;

        // Count by failure reason
        for (const [labelKey, labelAgg] of aggregated.byLabel.entries()) {
          try {
            const labels = JSON.parse(labelKey);
            const reason = labels.failure_reason || "unknown";
            failureReasons.set(reason, (failureReasons.get(reason) || 0) + labelAgg.value);
          } catch (error) {
            logger.warn("Failed to parse failure reason label", { labelKey, error });
          }
        }
      }
    }

    const total = successfulRetries + failedRetries;
    const successRate = total > 0 ? successfulRetries / total : 0;

    return {
      successfulRetries,
      failedRetries,
      successRate,
      failureReasons,
    };
  }

  /**
   * Export as Prometheus format
   */
  toPrometheus(): string[] {
    const budgetSummary = this.getBudgetSummary();
    const outcomeSummary = this.getOutcomeSummary();
    const metrics: PrometheusMetric[] = [];

    // Budget consumption metrics
    metrics.push({
      name: "retry_budget_consumed_count",
      type: "counter",
      help: "Total retries consumed from global budget",
      samples: [{ value: budgetSummary.totalBudgetConsumed }],
    });

    metrics.push({
      name: "retry_budget_consumed_time_ms",
      type: "counter",
      help: "Total time budget consumed in milliseconds",
      samples: [{ value: budgetSummary.totalTimeConsumed }],
    });

    // Exhaustion metrics
    if (budgetSummary.totalExhausted > 0) {
      metrics.push({
        name: "retry_budget_exhausted_total",
        type: "counter",
        help: "Total budget exhaustion events",
        samples: [{ value: budgetSummary.totalExhausted }],
      });

      for (const [exhaustionType, count] of budgetSummary.exhaustionByType) {
        metrics.push({
          name: "retry_budget_exhausted_by_type",
          type: "counter",
          help: "Budget exhaustion by type",
          samples: [{ labels: { exhaustion_type: exhaustionType }, value: count }],
        });
      }
    }

    // Outcome metrics
    metrics.push({
      name: "retry_ultimately_succeeded_total",
      type: "counter",
      help: "Total retries that ultimately succeeded",
      samples: [{ value: outcomeSummary.successfulRetries }],
    });

    metrics.push({
      name: "retry_ultimately_failed_total",
      type: "counter",
      help: "Total retries that ultimately failed",
      samples: [{ value: outcomeSummary.failedRetries }],
    });

    metrics.push({
      name: "retry_success_rate",
      type: "gauge",
      help: "Retry success rate (0-1)",
      samples: [{ value: outcomeSummary.successRate }],
    });

    // Timeout error metrics
    const timeoutResult = this.query({
      metricName: RETRY_METRICS.TIMEOUT_ERROR_COUNT,
    });

    const timeoutCount = Array.from(timeoutResult.metrics.values()).reduce(
      (sum, m) => sum + m.value,
      0,
    );
    if (timeoutCount > 0) {
      metrics.push({
        name: "retry_timeout_error_total",
        type: "counter",
        help: "Total timeout errors",
        samples: [{ value: timeoutCount }],
      });

      const noRetryResult = this.query({
        metricName: RETRY_METRICS.TIMEOUT_ERROR_NO_RETRY,
      });
      const noRetryCount = Array.from(noRetryResult.metrics.values()).reduce(
        (sum, m) => sum + m.value,
        0,
      );
      metrics.push({
        name: "retry_timeout_error_no_retry_total",
        type: "counter",
        help: "Timeout errors exempted from retry",
        samples: [{ value: noRetryCount }],
      });
    }

    // Active retries gauge
    const activeRetrySum = Array.from(this.activeRetries.values()).reduce((a, b) => a + b, 0);
    if (activeRetrySum > 0) {
      metrics.push({
        name: "retry_active_count",
        type: "gauge",
        help: "Currently active retries by consumer",
        samples: [{ value: activeRetrySum }],
      });
    }

    // Add internal metrics
    const internalMetrics = this.exportInternalMetrics();

    // Format all metrics
    return [...metrics.flatMap(m => PrometheusFormatter.formatMetric(m)), ...internalMetrics];
  }

  /**
   * Export as JSON
   */
  toJSON(): Record<string, unknown> {
    const budgetSummary = this.getBudgetSummary();
    const outcomeSummary = this.getOutcomeSummary();

    const exhaustionByType: Record<string, number> = {};
    for (const [type, count] of budgetSummary.exhaustionByType) {
      exhaustionByType[type] = count;
    }

    const failureReasons: Record<string, number> = {};
    for (const [reason, count] of outcomeSummary.failureReasons) {
      failureReasons[reason] = count;
    }

    return {
      type: "retry_budget",
      budget: {
        consumedCount: budgetSummary.totalBudgetConsumed,
        consumedTimeMs: budgetSummary.totalTimeConsumed,
        totalExhausted: budgetSummary.totalExhausted,
        exhaustionByType,
      },
      outcomes: {
        successfulRetries: outcomeSummary.successfulRetries,
        failedRetries: outcomeSummary.failedRetries,
        successRate: outcomeSummary.successRate.toFixed(2),
        failureReasons,
      },
      activeRetries: Array.from(this.activeRetries.entries()).map(([consumerId, count]) => ({
        consumerId,
        activeCount: count,
      })),
    };
  }
}
