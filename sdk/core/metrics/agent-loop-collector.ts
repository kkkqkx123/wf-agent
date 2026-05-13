/**
 * Agent Loop Execution Metrics Collector
 * 
 * Collects and aggregates metrics specific to Agent Loop executions including:
 * - Execution duration and lifecycle
 * - Iteration counts and distributions
 * - Tool call statistics per iteration
 * - Pause/resume patterns
 * - Success/failure rates
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig, MetricFilter, MetricQueryResult } from "./types.js";
import { AGENT_LOOP_METRICS } from "./constants.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "AgentLoopMetricsCollector" });

/**
 * Agent Loop-specific metric collector
 * Extends BaseMetricCollector with agent loop-specific convenience methods
 */
export class AgentLoopMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record agent loop execution start
   * @param agentLoopId Agent Loop ID
   * @param executionId Execution ID (usually same as agentLoopId)
   */
  recordExecutionStart(agentLoopId: string, executionId: string): void {
    this.incrementCounter(AGENT_LOOP_METRICS.EXECUTION_COUNT, {
      agent_loop_id: agentLoopId,
      execution_id: executionId,
      status: "started",
    });

    // Track active agent loops
    this.incrementCounter(AGENT_LOOP_METRICS.ACTIVE_COUNT, {
      agent_loop_id: agentLoopId,
    }, 1);
  }

  /**
   * Record agent loop execution completion
   * @param agentLoopId Agent Loop ID
   * @param executionId Execution ID
   * @param duration Total execution duration in milliseconds
   * @param iterations Number of iterations completed
   * @param toolCallCount Total tool calls made
   * @param success Whether execution was successful
   */
  recordExecutionComplete(
    agentLoopId: string,
    executionId: string,
    duration: number,
    iterations: number,
    toolCallCount: number,
    success: boolean,
  ): void {
    // Record execution duration
    this.observeHistogram(AGENT_LOOP_METRICS.EXECUTION_DURATION, duration, {
      agent_loop_id: agentLoopId,
      execution_id: executionId,
      success: success.toString(),
    });

    // Record iteration count
    this.setGauge(AGENT_LOOP_METRICS.ITERATION_COUNT, iterations, {
      agent_loop_id: agentLoopId,
      execution_id: executionId,
    });

    // Record total tool calls
    this.setGauge(AGENT_LOOP_METRICS.TOOL_CALLS_TOTAL, toolCallCount, {
      agent_loop_id: agentLoopId,
      execution_id: executionId,
    });

    // Record success/failure
    this.incrementCounter(AGENT_LOOP_METRICS.ERROR_COUNT, {
      agent_loop_id: agentLoopId,
      execution_id: executionId,
      error_type: success ? "none" : "execution_failed",
    }, success ? 0 : 1);

    // Record success rate (gauge: 1 for success, 0 for failure)
    this.setGauge(AGENT_LOOP_METRICS.SUCCESS_RATE, success ? 1 : 0, {
      agent_loop_id: agentLoopId,
    });

    // Decrement active count
    this.incrementCounter(AGENT_LOOP_METRICS.ACTIVE_COUNT, {
      agent_loop_id: agentLoopId,
    }, -1);
  }

  /**
   * Record iteration start
   * @param agentLoopId Agent Loop ID
   * @param iteration Iteration number (1-based)
   */
  recordIterationStart(agentLoopId: string, iteration: number): void {
    this.incrementCounter(`${AGENT_LOOP_METRICS.ITERATION_COUNT}.start`, {
      agent_loop_id: agentLoopId,
      iteration_number: iteration.toString(),
    });
  }

  /**
   * Record iteration completion
   * @param agentLoopId Agent Loop ID
   * @param iteration Iteration number
   * @param duration Iteration duration in milliseconds
   * @param toolCallCount Tool calls in this iteration
   */
  recordIterationComplete(
    agentLoopId: string,
    iteration: number,
    duration: number,
    toolCallCount: number,
  ): void {
    // Record iteration duration distribution
    this.observeHistogram(AGENT_LOOP_METRICS.ITERATION_DURATION, duration, {
      agent_loop_id: agentLoopId,
      iteration_number: iteration.toString(),
    });

    // Record tool calls per iteration
    this.setGauge(AGENT_LOOP_METRICS.TOOL_CALLS_PER_ITERATION, toolCallCount, {
      agent_loop_id: agentLoopId,
      iteration_number: iteration.toString(),
    });
  }

  /**
   * Record that max iterations limit was reached
   * @param agentLoopId Agent Loop ID
   * @param maxIterations Configured max iterations
   */
  recordMaxIterationsReached(agentLoopId: string, maxIterations: number): void {
    this.incrementCounter(AGENT_LOOP_METRICS.MAX_ITERATIONS_REACHED, {
      agent_loop_id: agentLoopId,
      max_iterations: maxIterations.toString(),
    });
  }

  /**
   * Record agent loop pause
   * @param agentLoopId Agent Loop ID
   */
  recordPause(agentLoopId: string): void {
    this.incrementCounter(AGENT_LOOP_METRICS.PAUSE_COUNT, {
      agent_loop_id: agentLoopId,
    });
  }

  /**
   * Record agent loop resume
   * @param agentLoopId Agent Loop ID
   * @param pauseDuration Duration of pause in milliseconds
   */
  recordResume(agentLoopId: string, pauseDuration: number): void {
    this.incrementCounter(AGENT_LOOP_METRICS.RESUME_COUNT, {
      agent_loop_id: agentLoopId,
    });

    // Record pause duration distribution
    this.observeHistogram(AGENT_LOOP_METRICS.PAUSE_DURATION, pauseDuration, {
      agent_loop_id: agentLoopId,
    });
  }

  /**
   * Record agent loop error
   * @param agentLoopId Agent Loop ID
   * @param errorType Error type/category
   * @param iteration Optional iteration where error occurred
   */
  recordError(
    agentLoopId: string,
    errorType: string,
    iteration?: number,
  ): void {
    this.incrementCounter(AGENT_LOOP_METRICS.ERROR_COUNT, {
      agent_loop_id: agentLoopId,
      error_type: errorType,
      iteration: iteration?.toString() || "unknown",
    });
  }

  /**
   * Get agent loop-specific statistics
   * @param agentLoopId Optional agent loop ID filter
   * @returns Aggregated statistics
   */
  getAgentLoopStats(agentLoopId?: string): MetricQueryResult {
    const filter: MetricFilter = agentLoopId
      ? {
          labels: { agent_loop_id: agentLoopId },
        }
      : {};

    return this.query(filter);
  }

  /**
   * Get count of currently active agent loops
   * @returns Number of active agent loops
   */
  getActiveAgentLoops(): number {
    const result = this.query({
      metricName: AGENT_LOOP_METRICS.ACTIVE_COUNT,
    });

    const aggregated = result.metrics.get(AGENT_LOOP_METRICS.ACTIVE_COUNT);
    return aggregated ? Math.round(aggregated.value) : 0;
  }

  /**
   * Get average iterations per agent loop
   * @returns Average iteration count
   */
  getAverageIterations(): number {
    const result = this.query({
      metricName: AGENT_LOOP_METRICS.ITERATION_COUNT,
    });

    const aggregated = result.metrics.get(AGENT_LOOP_METRICS.ITERATION_COUNT);
    if (!aggregated || aggregated.value === 0) {
      return 0;
    }

    const executionCount = this.query({
      metricName: AGENT_LOOP_METRICS.EXECUTION_COUNT,
      labels: { status: "started" },
    });

    const countAgg = executionCount.metrics.get(AGENT_LOOP_METRICS.EXECUTION_COUNT);
    const executions = countAgg ? countAgg.value : 1;

    return aggregated.value / executions;
  }

  /**
   * Flush metrics to storage
   * Override to implement custom persistence logic
   */
  async flush(): Promise<void> {
    const flushedCount = this.metricsBuffer.length;

    if (flushedCount > 0) {
      logger.debug("Flushing agent loop metrics", { flushedCount });

      // TODO: Implement actual persistence (e.g., write to database, send to monitoring service)
      // For now, just clear the buffer
      this.metricsBuffer = [];
    }
  }
}
