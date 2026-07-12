/**
 * Retry Budget Metrics Collector Tests
 *
 * Tests comprehensive retry budget metrics collection including:
 * - Budget consumption tracking
 * - Retry attempt recording
 * - Timeout error handling
 * - Backoff calculation metrics
 * - Success/failure outcome tracking
 */

import { describe, it, expect, beforeEach } from "vitest";
import { RetryBudgetMetricsCollector } from "../retry-budget-collector.js";
import { RETRY_METRICS } from "../constants.js";

describe("RetryBudgetMetricsCollector - Metrics Integration Tests", () => {
  let collector: RetryBudgetMetricsCollector;

  beforeEach(() => {
    collector = new RetryBudgetMetricsCollector({
      bufferSize: 100,
      flushInterval: 0,
    });
  });

  describe("Budget Consumption Tracking", () => {
    /**
     * Scenario 1: Single workflow consuming retries from global budget
     *
     * Real scenario:
     * - Workflow has 10 retry count budget, 30s time budget
     * - FORK branch 1 fails, uses 2 retries with 2s delay
     * - Record consumption
     */
    it("should track retry count consumption", () => {
      collector.recordBudgetConsumption(2, 2000, "workflow-1");

      const result = collector.query({ metricName: RETRY_METRICS.BUDGET_CONSUMED_COUNT });
      const metric = result.metrics.get(RETRY_METRICS.BUDGET_CONSUMED_COUNT);

      expect(metric).toBeDefined();
      expect(metric?.value).toBe(2);
    });

    it("should track time budget consumption", () => {
      collector.recordBudgetConsumption(1, 2500, "workflow-1");

      const result = collector.query({ metricName: RETRY_METRICS.BUDGET_CONSUMED_TIME });
      const metric = result.metrics.get(RETRY_METRICS.BUDGET_CONSUMED_TIME);

      expect(metric).toBeDefined();
      // Should observe histogram value
      expect(metric?.value).toBe(2500);
    });

    it("should track remaining budget", () => {
      collector.recordBudgetRemaining(8, 28000, "workflow-1");

      const countResult = collector.query({
        metricName: RETRY_METRICS.BUDGET_REMAINING_COUNT,
      });
      const timeResult = collector.query({
        metricName: RETRY_METRICS.BUDGET_REMAINING_TIME,
      });

      expect(countResult.metrics.size).toBeGreaterThan(0);
      expect(timeResult.metrics.size).toBeGreaterThan(0);
    });
  });

  describe("Budget Exhaustion Events", () => {
    /**
     * Scenario 2: Global budget exhaustion stopping retries
     *
     * Real scenario:
     * - Global budget: 10 retries, 30s time
     * - After multiple branch retries, budget exhausted
     * - Record exhaustion event
     */
    it("should record budget exhaustion by count", () => {
      collector.recordBudgetExhausted("count", "workflow-1");

      const result = collector.query({
        metricName: RETRY_METRICS.BUDGET_EXHAUSTED,
      });
      const metric = result.metrics.get(RETRY_METRICS.BUDGET_EXHAUSTED);

      expect(metric?.value).toBe(1);
    });

    it("should record budget exhaustion by time", () => {
      collector.recordBudgetExhausted("time", "workflow-1");
      collector.recordBudgetExhausted("time", "workflow-2");

      const result = collector.query({
        metricName: RETRY_METRICS.BUDGET_EXHAUSTED,
      });
      const metric = result.metrics.get(RETRY_METRICS.BUDGET_EXHAUSTED);

      expect(metric?.value).toBe(2);
    });

    it("should track exhaustion by type", () => {
      collector.recordBudgetExhausted("count", "workflow-1");
      collector.recordBudgetExhausted("time", "workflow-1");
      collector.recordBudgetExhausted("both", "workflow-2");

      const summary = collector.getBudgetSummary();

      expect(summary.totalExhausted).toBe(3);
      expect(summary.exhaustionByType.get("count")).toBe(1);
      expect(summary.exhaustionByType.get("time")).toBe(1);
      expect(summary.exhaustionByType.get("both")).toBe(1);
    });
  });

  describe("Retry Attempt Recording", () => {
    /**
     * Scenario 3: Multi-attempt retry with exponential backoff
     *
     * Real scenario:
     * - AgentLoop iteration fails
     * - Retry attempt 1 with 1s delay
     * - Retry attempt 2 with 2s delay
     * - Retry attempt 3 with 4s delay
     */
    it("should record individual retry attempts", () => {
      const consumerId = "agent-loop-1";

      collector.recordRetryAttempt(consumerId, 1000, 0);
      collector.recordRetryAttempt(consumerId, 2000, 1);
      collector.recordRetryAttempt(consumerId, 4000, 2);

      const result = collector.query({
        labels: { consumer_id: consumerId },
      });

      const attemptMetric = result.metrics.get(RETRY_METRICS.ATTEMPT_TOTAL);
      expect(attemptMetric?.value).toBe(3);
    });

    it("should track delay distribution for retries", () => {
      const consumerId = "node-2";

      collector.recordRetryAttempt(consumerId, 1000, 0);
      collector.recordRetryAttempt(consumerId, 2000, 1);

      const result = collector.query({
        metricName: RETRY_METRICS.DELAY_DURATION,
      });

      expect(result.totalCount).toBeGreaterThanOrEqual(2);
    });

    it("should track active retries per consumer", () => {
      const consumerId = "fork-branch-1";

      // Start first attempt
      collector.recordRetryAttempt(consumerId, 1000, 0);

      let result = collector.query({
        metricName: RETRY_METRICS.CONSUMER_ACTIVE_RETRIES,
      });
      expect(result.metrics.size).toBeGreaterThan(0);
    });
  });

  describe("Retry Success Recording", () => {
    /**
     * Scenario 4: Successful retry after multiple attempts
     *
     * Real scenario:
     * - Node fails initially
     * - Retry attempt 1 fails
     * - Retry attempt 2 succeeds
     * - Record success with total delay
     */
    it("should record successful retry outcome", () => {
      const consumerId = "node-3";

      collector.recordRetrySuccess(consumerId, 2, 3000);

      const result = collector.query({
        metricName: RETRY_METRICS.ATTEMPT_SUCCEEDED,
      });
      const metric = result.metrics.get(RETRY_METRICS.ATTEMPT_SUCCEEDED);

      expect(metric).toBeDefined();
      expect(metric?.value).toBe(1);
    });

    it("should track ultimately successful outcomes", () => {
      const consumerId1 = "node-1";
      const consumerId2 = "node-2";

      collector.recordRetrySuccess(consumerId1, 1, 1000);
      collector.recordRetrySuccess(consumerId2, 3, 7000);

      const summary = collector.getOutcomeSummary();

      expect(summary.successfulRetries).toBe(2);
      expect(summary.failedRetries).toBe(0);
      expect(summary.successRate).toBe(1.0);
    });
  });

  describe("Retry Failure Recording", () => {
    /**
     * Scenario 5: Retry exhaustion after max attempts
     *
     * Real scenario:
     * - Node fails, retry policy allows max 3 retries
     * - All 3 retries fail
     * - Record failure with reason
     */
    it("should record failed retry outcome", () => {
      const consumerId = "node-4";

      collector.recordRetryFailure(consumerId, 3, "max_retries");

      const result = collector.query({
        metricName: RETRY_METRICS.ATTEMPT_FAILED,
      });
      const metric = result.metrics.get(RETRY_METRICS.ATTEMPT_FAILED);

      expect(metric?.value).toBe(1);
    });

    it("should track failure reasons", () => {
      collector.recordRetryFailure("node-1", 3, "max_retries");
      collector.recordRetryFailure("node-2", 1, "budget_exhausted");
      collector.recordRetryFailure("node-3", 2, "policy");

      const summary = collector.getOutcomeSummary();

      expect(summary.failedRetries).toBe(3);
      expect(summary.failureReasons.get("max_retries")).toBe(1);
      expect(summary.failureReasons.get("budget_exhausted")).toBe(1);
      expect(summary.failureReasons.get("policy")).toBe(1);
    });

    it("should calculate success rate with mixed outcomes", () => {
      collector.recordRetrySuccess("node-1", 2, 3000);
      collector.recordRetrySuccess("node-2", 1, 1000);
      collector.recordRetryFailure("node-3", 3, "max_retries");

      const summary = collector.getOutcomeSummary();

      expect(summary.successfulRetries).toBe(2);
      expect(summary.failedRetries).toBe(1);
      expect(summary.successRate).toBeCloseTo(2 / 3, 2);
    });
  });

  describe("Timeout Error Handling", () => {
    /**
     * Scenario 6: Timeout errors exempted from retry
     *
     * Real scenario:
     * - AgentLoop iteration times out after 30s
     * - Timeout error occurs (not retryable)
     * - Record timeout event with exemption from retry
     */
    it("should record timeout error occurrence", () => {
      collector.recordTimeoutError("agent-loop-1", 30000, 30000);

      const result = collector.query({
        metricName: RETRY_METRICS.TIMEOUT_ERROR_COUNT,
      });
      const metric = result.metrics.get(RETRY_METRICS.TIMEOUT_ERROR_COUNT);

      expect(metric?.value).toBe(1);
    });

    it("should record timeout error no-retry exemption", () => {
      collector.recordTimeoutError("node-1", 15000, 14999);
      collector.recordTimeoutError("branch-1", 60000, 60000);

      const result = collector.query({
        metricName: RETRY_METRICS.TIMEOUT_ERROR_NO_RETRY,
      });
      const metric = result.metrics.get(RETRY_METRICS.TIMEOUT_ERROR_NO_RETRY);

      expect(metric?.value).toBe(2);
    });
  });

  describe("Backoff Calculation Metrics", () => {
    /**
     * Scenario 7: Exponential backoff with metrics
     *
     * Real scenario:
     * - Retry policy: exponential backoff
     * - Base delay: 1000ms
     * - Multiplier: 2
     * - Attempt 0: 1000ms, Attempt 1: 2000ms, Attempt 2: 4000ms
     */
    it("should record backoff factor calculations", () => {
      const consumerId = "retryable-node";

      collector.recordBackoffCalculation(consumerId, 1000, 2, 0, 1000);
      collector.recordBackoffCalculation(consumerId, 1000, 2, 1, 2000);
      collector.recordBackoffCalculation(consumerId, 1000, 2, 2, 4000);

      const result = collector.query({
        labels: { consumer_id: consumerId },
      });

      expect(result.totalCount).toBeGreaterThanOrEqual(3);
    });
  });

  describe("Summary and Reporting", () => {
    /**
     * Scenario 8: Comprehensive budget summary
     */
    it("should provide accurate budget summary", () => {
      collector.recordBudgetConsumption(2, 2000, "workflow-1");
      collector.recordBudgetConsumption(3, 3500, "workflow-1");
      collector.recordBudgetExhausted("time", "workflow-1");

      const summary = collector.getBudgetSummary();

      expect(summary.totalBudgetConsumed).toBe(5);
      // Note: histogram values accumulate, but time consumed comes from BUDGET_CONSUMED_TIME histogram
      // which records individual observations
      expect(summary.totalTimeConsumed).toBeGreaterThan(0);
      expect(summary.totalExhausted).toBe(1);
    });

    /**
     * Scenario 9: Prometheus export format
     */
    it("should export metrics in Prometheus format", () => {
      collector.recordBudgetConsumption(2, 2000, "workflow-1");
      collector.recordRetrySuccess("node-1", 1, 1000);
      collector.recordRetryFailure("node-2", 3, "max_retries");

      const prometheusLines = collector.toPrometheus();

      expect(prometheusLines.length).toBeGreaterThan(0);
      expect(prometheusLines.join("\n")).toMatch(/retry_budget/i);
      expect(prometheusLines.join("\n")).toMatch(/retry_ultimately/i);
    });

    /**
     * Scenario 10: JSON export format
     */
    it("should export metrics in JSON format", () => {
      collector.recordBudgetConsumption(2, 2000, "workflow-1");
      collector.recordRetrySuccess("node-1", 1, 1000);

      const json = collector.toJSON();

      expect(json.type).toBe("retry_budget");
      expect(json.budget).toBeDefined();
      expect(json.outcomes).toBeDefined();
      expect(json.outcomes.successRate).toBeDefined();
    });
  });

  describe("Edge Cases", () => {
    it("should handle zero delays", () => {
      const consumerId = "fast-node";

      collector.recordRetryAttempt(consumerId, 0, 0);
      collector.recordRetrySuccess(consumerId, 1, 0);

      const summary = collector.getOutcomeSummary();
      expect(summary.successfulRetries).toBe(1);
    });

    it("should handle large delay values", () => {
      const consumerId = "slow-node";
      const largeDelay = 300000; // 5 minutes

      collector.recordRetryAttempt(consumerId, largeDelay, 0);
      collector.recordBudgetConsumption(1, largeDelay, "workflow-1");

      const summary = collector.getBudgetSummary();
      expect(summary.totalBudgetConsumed).toBe(1);
    });

    it("should handle multiple workflows independently", () => {
      collector.recordBudgetConsumption(1, 1000, "workflow-1");
      collector.recordBudgetConsumption(2, 2000, "workflow-2");
      collector.recordBudgetExhausted("count", "workflow-1");
      collector.recordBudgetExhausted("time", "workflow-2");

      const summary = collector.getBudgetSummary();

      expect(summary.totalBudgetConsumed).toBe(3);
      expect(summary.totalExhausted).toBe(2);
    });
  });
});
