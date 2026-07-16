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
import { AGENT_LOOP_METRICS, PROTOCOL_METRICS } from "./constants.js";
import { PrometheusFormatter, type PrometheusMetric } from "./utils/prometheus-formatter.js";

/**
 * Agent Loop-specific metric collector
 * Extends BaseMetricCollector with agent loop-specific convenience methods
 */
export class AgentLoopMetricsCollector extends BaseMetricCollector {
  private activeCount: number = 0;

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

    this.activeCount += 1;
    this.setGauge(AGENT_LOOP_METRICS.ACTIVE_COUNT, this.activeCount, {
      agent_loop_id: agentLoopId,
    });
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

    // Record success/failure as separate counters
    if (success) {
      this.incrementCounter(`${AGENT_LOOP_METRICS.SUCCESS_RATE}.success`, {
        agent_loop_id: agentLoopId,
      });
    } else {
      this.incrementCounter(`${AGENT_LOOP_METRICS.SUCCESS_RATE}.failure`, {
        agent_loop_id: agentLoopId,
      });
    }

    // Decrement active count using gauge
    this.activeCount = Math.max(0, this.activeCount - 1);
    this.setGauge(AGENT_LOOP_METRICS.ACTIVE_COUNT, this.activeCount, {
      agent_loop_id: agentLoopId,
    });
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
  recordError(agentLoopId: string, errorType: string, iteration?: number): void {
    this.incrementCounter(AGENT_LOOP_METRICS.ERROR_COUNT, {
      agent_loop_id: agentLoopId,
      error_type: errorType,
      iteration: iteration?.toString() || "unknown",
    });
  }

  /**
   * Record protocol locked at execution start.
   *
   * Tracks the tool call format that was resolved and locked for this execution.
   * The `source` indicates where the format came from: "definition", "profile", or "default".
   *
   * @param format The locked tool call format (e.g. "native", "xml", "json_wrapped")
   * @param agentLoopId Agent Loop ID
   * @param profileId Profile ID used for resolution
   * @param source Source of the protocol: "definition" | "profile" | "default"
   */
  recordProtocolLocked(
    format: string,
    agentLoopId: string,
    profileId: string,
    source: "definition" | "profile" | "default",
  ): void {
    this.incrementCounter(PROTOCOL_METRICS.LOCKED_COUNT, {
      format,
      agent_loop_id: agentLoopId,
      profile_id: profileId,
      source,
    });
  }

  /**
   * Record protocol violation detected.
   *
   * A violation occurs when the locked tool call format differs from
   * the format requested by the current LLM profile or request.
   *
   * @param lockedFormat The locked format
   * @param attemptedFormat The format that was attempted
   * @param policy The policy applied ("ignore", "warn", "fail", "auto_convert")
   * @param agentLoopId Agent Loop ID
   * @param profileId Profile ID that triggered the violation
   */
  recordProtocolViolation(
    lockedFormat: string,
    attemptedFormat: string,
    policy: string,
    agentLoopId: string,
    profileId: string,
  ): void {
    this.incrementCounter(PROTOCOL_METRICS.VIOLATION_COUNT, {
      locked_format: lockedFormat,
      attempted_format: attemptedFormat,
      policy,
      agent_loop_id: agentLoopId,
      profile_id: profileId,
    });
  }

  /**
   * Record cross-boundary protocol conversion.
   *
   * A conversion occurs when execution crosses from one context to another
   * (e.g., sub-agent spawn, workflow fork) and the protocols differ.
   *
   * @param fromFormat Source protocol format
   * @param toFormat Target protocol format
   * @param boundaryType Type of boundary crossed
   * @param agentLoopId Agent Loop ID
   */
  recordProtocolConversion(
    fromFormat: string,
    toFormat: string,
    boundaryType: "sub_agent" | "workflow_fork" | "triggered_subworkflow" | "workflow_to_agent",
    agentLoopId: string,
  ): void {
    this.incrementCounter(PROTOCOL_METRICS.CONVERSION_COUNT, {
      from_format: fromFormat,
      to_format: toFormat,
      boundary_type: boundaryType,
      agent_loop_id: agentLoopId,
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
    return this.activeCount;
  }

  /**
   * Get average iterations per agent loop
   * Uses histogram data when available, falls back to gauge/counter ratio
   * @returns Average iteration count
   */
  getAverageIterations(): number {
    const executionCount = this.query({
      metricName: AGENT_LOOP_METRICS.EXECUTION_COUNT,
      labels: { status: "started" },
    });

    const countAgg = executionCount.metrics.get(AGENT_LOOP_METRICS.EXECUTION_COUNT);
    const executions = countAgg ? countAgg.value : 0;

    if (executions === 0) {
      return 0;
    }

    const iterationGauge = this.query({
      metricName: AGENT_LOOP_METRICS.ITERATION_COUNT,
    });

    const iterAgg = iterationGauge.metrics.get(AGENT_LOOP_METRICS.ITERATION_COUNT);
    const totalIterations = iterAgg ? iterAgg.value : 0;

    return totalIterations / executions;
  }

  /**
   * Export as Prometheus format
   */
  toPrometheus(): string[] {
    const result = this.query({});
    const metrics: PrometheusMetric[] = [];

    // Extract totals
    let totalExecutions = 0;
    let totalErrors = 0;
    let totalPauses = 0;
    let totalResumes = 0;
    let successCount = 0;
    let failureCount = 0;
    let protocolLockedCount = 0;
    let protocolViolationCount = 0;
    let protocolConversionCount = 0;

    for (const [metricName, aggregated] of result.metrics.entries()) {
      switch (metricName) {
        case AGENT_LOOP_METRICS.EXECUTION_COUNT:
          totalExecutions += aggregated.value;
          break;
        case AGENT_LOOP_METRICS.ERROR_COUNT:
          totalErrors += aggregated.value;
          break;
        case AGENT_LOOP_METRICS.PAUSE_COUNT:
          totalPauses += aggregated.value;
          break;
        case AGENT_LOOP_METRICS.RESUME_COUNT:
          totalResumes += aggregated.value;
          break;
        default:
          if (metricName === `${AGENT_LOOP_METRICS.SUCCESS_RATE}.success`) {
            successCount += aggregated.value;
          } else if (metricName === `${AGENT_LOOP_METRICS.SUCCESS_RATE}.failure`) {
            failureCount += aggregated.value;
          } else if (metricName === PROTOCOL_METRICS.LOCKED_COUNT) {
            protocolLockedCount += aggregated.value;
          } else if (metricName === PROTOCOL_METRICS.VIOLATION_COUNT) {
            protocolViolationCount += aggregated.value;
          } else if (metricName === PROTOCOL_METRICS.CONVERSION_COUNT) {
            protocolConversionCount += aggregated.value;
          }
          break;
      }
    }

    // Total executions
    metrics.push({
      name: "agent_loop_execution_total",
      type: "counter",
      help: "Total agent loop executions",
      samples: [{ value: totalExecutions }],
    });

    // Active count
    metrics.push({
      name: "agent_loop_active_count",
      type: "gauge",
      help: "Active agent loops",
      samples: [{ value: this.activeCount }],
    });

    // Success/failure counts
    metrics.push({
      name: "agent_loop_success_total",
      type: "counter",
      help: "Successful agent loop executions",
      samples: [{ value: successCount }],
    });

    if (failureCount > 0) {
      metrics.push({
        name: "agent_loop_failure_total",
        type: "counter",
        help: "Failed agent loop executions",
        samples: [{ value: failureCount }],
      });
    }

    // Errors
    if (totalErrors > 0) {
      metrics.push({
        name: "agent_loop_error_total",
        type: "counter",
        help: "Total agent loop errors",
        samples: [{ value: totalErrors }],
      });
    }

    // Pauses
    if (totalPauses > 0) {
      metrics.push({
        name: "agent_loop_pause_total",
        type: "counter",
        help: "Total agent loop pauses",
        samples: [{ value: totalPauses }],
      });
    }

    // Resumes
    if (totalResumes > 0) {
      metrics.push({
        name: "agent_loop_resume_total",
        type: "counter",
        help: "Total agent loop resumes",
        samples: [{ value: totalResumes }],
      });
    }

    // Protocol metrics
    if (protocolLockedCount > 0) {
      metrics.push({
        name: "agent_loop_protocol_locked_total",
        type: "counter",
        help: "Total tool call protocol locks",
        samples: [{ value: protocolLockedCount }],
      });
    }
    if (protocolViolationCount > 0) {
      metrics.push({
        name: "agent_loop_protocol_violation_total",
        type: "counter",
        help: "Total tool call protocol violations",
        samples: [{ value: protocolViolationCount }],
      });
    }
    if (protocolConversionCount > 0) {
      metrics.push({
        name: "agent_loop_protocol_conversion_total",
        type: "counter",
        help: "Total cross-boundary protocol conversions",
        samples: [{ value: protocolConversionCount }],
      });
    }

    // Format all metrics
    return metrics.flatMap(m => PrometheusFormatter.formatMetric(m));
  }

  /**
   * Export as JSON
   */
  toJSON(): Record<string, unknown> {
    return {
      type: "agent_loop",
      activeAgentLoops: this.getActiveAgentLoops(),
      averageIterations: this.getAverageIterations(),
    };
  }
}
