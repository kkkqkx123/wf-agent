/**
 * Node Metrics Collector
 *
 * Tracks node template and execution metrics including:
 * - Template instantiation count
 * - Node execution frequency by type
 * - Execution duration and success rate
 * - Token usage for LLM nodes
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig } from "./types.js";
import { NODE_METRICS, TEMPLATE_METRICS, SUBGRAPH_METRICS } from "./constants.js";
import { PrometheusFormatter, type PrometheusMetric } from "./utils/prometheus-formatter.js";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";

const logger = createContextualLogger({ component: "NodeMetricsCollector" });

export class NodeMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record node template instantiation
   */
  recordTemplateInstantiation(
    templateName: string,
    nodeType: string,
    metadata?: {
      category?: string;
      tags?: string[];
    },
  ): void {
    if (!templateName || !nodeType) {
      logger.warn("recordTemplateInstantiation called with missing parameters");
      return;
    }

    this.incrementCounter(TEMPLATE_METRICS.INSTANTIATION_COUNT, {
      template_name: templateName,
      node_type: nodeType,
      category: metadata?.category || "uncategorized",
    });

    logger.debug("Recorded node template instantiation", { templateName, nodeType });
  }

  /**
   * Record node execution start
   */
  recordNodeExecutionStart(nodeId: string, nodeType: string, workflowId: string): void {
    if (!nodeId || !nodeType || !workflowId) {
      logger.warn("recordNodeExecutionStart called with missing parameters");
      return;
    }

    this.incrementCounter(NODE_METRICS.STARTED_COUNT, {
      node_type: nodeType,
      node_id: nodeId,
      workflow_id: workflowId,
    });
  }

  /**
   * Record node execution completion
   */
  recordNodeExecution(
    nodeId: string,
    nodeType: string,
    workflowId: string,
    result: {
      success: boolean;
      duration: number;
      tokenUsage?: number;
      errorType?: string;
    },
  ): void {
    if (!nodeId || !nodeType || !workflowId) {
      logger.warn("recordNodeExecution called with missing parameters");
      return;
    }

    // Record execution count
    this.incrementCounter(NODE_METRICS.EXECUTION_COUNT, {
      node_type: nodeType,
      node_id: nodeId,
      workflow_id: workflowId,
    });

    // Record duration
    this.observeHistogram(NODE_METRICS.EXECUTION_DURATION, result.duration, {
      node_type: nodeType,
      node_id: nodeId,
    });

    // Record success/failure
    if (result.success) {
      this.incrementCounter(NODE_METRICS.SUCCESS_COUNT, {
        node_type: nodeType,
      });
    } else {
      this.incrementCounter(NODE_METRICS.FAILURE_COUNT, {
        node_type: nodeType,
        error_type: result.errorType || "unknown",
      });
    }

    // For LLM nodes, track token usage
    if (result.tokenUsage !== undefined && nodeType === "LLM") {
      this.observeHistogram(NODE_METRICS.TOKEN_USAGE, result.tokenUsage, {
        node_type: nodeType,
      });
    }

    logger.debug("Recorded node execution", {
      nodeId,
      nodeType,
      workflowId,
      success: result.success,
      duration: result.duration,
    });
  }

  /**
   * Record SUBGRAPH-specific metrics
   * This extends the basic node execution metrics with SUBGRAPH-specific data
   */
  recordSubgraphExecution(
    nodeId: string,
    workflowId: string,
    result: {
      success: boolean;
      duration: number;
      subworkflowId: string;
      depth: number;
      variableInputCount: number;
      variableOutputCount: number;
      errorType?: string;
    },
  ): void {
    if (!nodeId || !workflowId) {
      logger.warn("recordSubgraphExecution called with missing parameters");
      return;
    }

    // Record as regular node execution first
    this.recordNodeExecution(nodeId, "SUBGRAPH", workflowId, {
      success: result.success,
      duration: result.duration,
      errorType: result.errorType,
    });

    // Record SUBGRAPH-specific metrics
    this.incrementCounter(SUBGRAPH_METRICS.EXECUTION_COUNT, {
      subworkflow_id: result.subworkflowId,
      depth: result.depth.toString(),
    });

    // Record nested depth
    this.observeHistogram(SUBGRAPH_METRICS.NESTED_DEPTH, result.depth, {
      subworkflow_id: result.subworkflowId,
    });

    // Record variable counts
    if (result.variableInputCount > 0) {
      this.incrementCounter(
        SUBGRAPH_METRICS.VARIABLE_IMPORT_COUNT,
        {
          subworkflow_id: result.subworkflowId,
        },
        result.variableInputCount,
      );
    }

    if (result.variableOutputCount > 0) {
      this.incrementCounter(
        SUBGRAPH_METRICS.VARIABLE_EXPORT_COUNT,
        {
          subworkflow_id: result.subworkflowId,
        },
        result.variableOutputCount,
      );
    }

    logger.debug("Recorded subgraph execution", {
      nodeId,
      workflowId,
      subworkflowId: result.subworkflowId,
      depth: result.depth,
      variableInputCount: result.variableInputCount,
      variableOutputCount: result.variableOutputCount,
    });
  }

  /**
   * Record FORK-specific metrics
   * This extends the basic node execution metrics with FORK-specific data
   */
  recordForkExecution(
    nodeId: string,
    workflowId: string,
    result: {
      branchCount: number;
      totalDuration: number;
      successCount: number;
      failureCount: number;
      maxBranchDuration: number;
      minBranchDuration: number;
    },
  ): void {
    if (!nodeId || !workflowId) {
      logger.warn("recordForkExecution called with missing parameters");
      return;
    }

    // Record as regular node execution first
    this.recordNodeExecution(nodeId, "FORK", workflowId, {
      success: result.failureCount === 0,
      duration: result.totalDuration,
    });

    // Record FORK-specific metrics
    this.incrementCounter("fork.execution.count", {
      node_id: nodeId,
      branch_count: result.branchCount.toString(),
    });

    // Record branch statistics
    this.observeHistogram("fork.branch.duration", result.maxBranchDuration, {
      node_id: nodeId,
      stat: "max",
    });

    this.observeHistogram("fork.branch.duration", result.minBranchDuration, {
      node_id: nodeId,
      stat: "min",
    });

    // Record success/failure counts
    if (result.successCount > 0) {
      this.incrementCounter(
        "fork.branch.success.count",
        {
          node_id: nodeId,
        },
        result.successCount,
      );
    }

    if (result.failureCount > 0) {
      this.incrementCounter(
        "fork.branch.failure.count",
        {
          node_id: nodeId,
        },
        result.failureCount,
      );
    }

    logger.debug("Recorded fork execution", {
      nodeId,
      workflowId,
      branchCount: result.branchCount,
      successCount: result.successCount,
      failureCount: result.failureCount,
    });
  }

  /**
   * Record individual FORK branch execution metrics
   */
  recordForkBranchExecution(branchResult: {
    nodeId: string;
    forkPathId: string;
    duration: number;
    status: string;
  }): void {
    if (!branchResult.nodeId || !branchResult.forkPathId) {
      logger.warn("recordForkBranchExecution called with missing parameters");
      return;
    }

    // Record branch duration
    this.observeHistogram("fork.branch.execution.duration", branchResult.duration, {
      node_id: branchResult.nodeId,
      fork_path_id: branchResult.forkPathId,
    });

    // Record branch status
    this.incrementCounter("fork.branch.status.count", {
      node_id: branchResult.nodeId,
      fork_path_id: branchResult.forkPathId,
      status: branchResult.status,
    });

    logger.debug("Recorded fork branch execution", {
      nodeId: branchResult.nodeId,
      forkPathId: branchResult.forkPathId,
      duration: branchResult.duration,
      status: branchResult.status,
    });
  }

  /**
   * Get SUBGRAPH execution statistics
   */
  getSubgraphExecutionStats(): Record<
    string,
    {
      totalCount: number;
      successRate: number;
      avgDepth: number;
      totalVariableImports: number;
      totalVariableExports: number;
    }
  > {
    const result: Record<
      string,
      {
        totalCount: number;
        successCount: number;
        totalDepth: number;
        variableImports: number;
        variableExports: number;
      }
    > = {};

    // Get execution counts by subworkflow
    const countResult = this.query({
      metricName: SUBGRAPH_METRICS.EXECUTION_COUNT,
      metricType: "counter",
    });

    const countMetric = countResult.metrics.get(SUBGRAPH_METRICS.EXECUTION_COUNT);
    if (countMetric) {
      for (const [labelKey, labelAgg] of countMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.subworkflow_id) {
            const subworkflowId = labels.subworkflow_id;
            if (!result[subworkflowId]) {
              result[subworkflowId] = {
                totalCount: 0,
                successCount: 0,
                totalDepth: 0,
                variableImports: 0,
                variableExports: 0,
              };
            }
            result[subworkflowId].totalCount += labelAgg.value;
          }
        } catch (error) {
          logger.warn("Failed to parse label key in subgraph execution count metric", {
            labelKey,
            error,
          });
        }
      }
    }

    // Get success counts
    const successResult = this.query({
      metricName: NODE_METRICS.SUCCESS_COUNT,
      metricType: "counter",
    });

    const successMetric = successResult.metrics.get(NODE_METRICS.SUCCESS_COUNT);
    if (successMetric) {
      for (const [labelKey] of successMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.node_type === "SUBGRAPH") {
            // We can't easily map this back to subworkflow_id, so we'll skip for now
            // In a real implementation, you'd want to include subworkflow_id in the success metric labels
          }
        } catch (error) {
          logger.warn("Failed to parse label key in success count metric", { labelKey, error });
        }
      }
    }

    // Get variable import counts
    const importResult = this.query({
      metricName: SUBGRAPH_METRICS.VARIABLE_IMPORT_COUNT,
      metricType: "counter",
    });

    const importMetric = importResult.metrics.get(SUBGRAPH_METRICS.VARIABLE_IMPORT_COUNT);
    if (importMetric) {
      for (const [labelKey, labelAgg] of importMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.subworkflow_id && result[labels.subworkflow_id]) {
            const subgraphData = result[labels.subworkflow_id];
            if (subgraphData) {
              subgraphData.variableImports += labelAgg.value;
            }
          }
        } catch (error) {
          logger.warn("Failed to parse label key in variable import metric", { labelKey, error });
        }
      }
    }

    // Get variable export counts
    const exportResult = this.query({
      metricName: SUBGRAPH_METRICS.VARIABLE_EXPORT_COUNT,
      metricType: "counter",
    });

    const exportMetric = exportResult.metrics.get(SUBGRAPH_METRICS.VARIABLE_EXPORT_COUNT);
    if (exportMetric) {
      for (const [labelKey, labelAgg] of exportMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.subworkflow_id && result[labels.subworkflow_id]) {
            const subgraphData = result[labels.subworkflow_id];
            if (subgraphData) {
              subgraphData.variableExports += labelAgg.value;
            }
          }
        } catch (error) {
          logger.warn("Failed to parse label key in variable export metric", { labelKey, error });
        }
      }
    }

    // Calculate final stats
    const stats: Record<
      string,
      {
        totalCount: number;
        successRate: number;
        avgDepth: number;
        totalVariableImports: number;
        totalVariableExports: number;
      }
    > = {};

    for (const [subworkflowId, data] of Object.entries(result)) {
      stats[subworkflowId] = {
        totalCount: data.totalCount,
        successRate: data.totalCount > 0 ? data.successCount / data.totalCount : 0,
        avgDepth: data.totalCount > 0 ? data.totalDepth / data.totalCount : 0,
        totalVariableImports: data.variableImports,
        totalVariableExports: data.variableExports,
      };
    }

    return stats;
  }

  /**
   * Get most frequently used node templates
   */
  getTopNodeTemplates(limit: number = 10): Array<{
    templateName: string;
    nodeType: string;
    instantiationCount: number;
  }> {
    const result = this.query({
      metricName: TEMPLATE_METRICS.INSTANTIATION_COUNT,
      metricType: "counter",
    });

    const templates: Map<string, { name: string; type: string; count: number }> = new Map();

    const metric = result.metrics.get(TEMPLATE_METRICS.INSTANTIATION_COUNT);
    if (metric) {
      for (const [labelKey, labelAgg] of metric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.template_name && labels.node_type) {
            const key = `${labels.template_name}:${labels.node_type}`;
            templates.set(key, {
              name: labels.template_name,
              type: labels.node_type,
              count: labelAgg.value,
            });
          }
        } catch (error) {
          logger.warn("Failed to parse label key", { labelKey, error });
        }
      }
    }

    return Array.from(templates.values())
      .sort((a, b) => b.count - a.count)
      .slice(0, limit)
      .map(t => ({
        templateName: t.name,
        nodeType: t.type,
        instantiationCount: t.count,
      }));
  }

  /**
   * Get node execution statistics by type
   */
  getNodeExecutionStatsByType(): Record<
    string,
    {
      totalCount: number;
      successRate: number;
      avgDuration: number;
    }
  > {
    const result: Record<
      string,
      { totalCount: number; successCount: number; totalDuration: number }
    > = {};

    // Get counts by type
    const countResult = this.query({
      metricName: NODE_METRICS.EXECUTION_COUNT,
      metricType: "counter",
    });

    const countMetric = countResult.metrics.get(NODE_METRICS.EXECUTION_COUNT);
    if (countMetric) {
      for (const [labelKey, labelAgg] of countMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.node_type) {
            const nodeType = labels.node_type;
            if (!result[nodeType]) {
              result[nodeType] = { totalCount: 0, successCount: 0, totalDuration: 0 };
            }
            result[nodeType].totalCount += labelAgg.value;
          }
        } catch (error) {
          logger.warn("Failed to parse label key in execution count metric", { labelKey, error });
        }
      }
    }

    // Get success counts
    const successResult = this.query({
      metricName: NODE_METRICS.SUCCESS_COUNT,
      metricType: "counter",
    });

    const successMetric = successResult.metrics.get(NODE_METRICS.SUCCESS_COUNT);
    if (successMetric) {
      for (const [labelKey, labelAgg] of successMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.node_type) {
            const nodeType = labels.node_type;
            if (result[nodeType]) {
              result[nodeType].successCount += labelAgg.value;
            }
          }
        } catch (error) {
          logger.warn("Failed to parse label key in success count metric", { labelKey, error });
        }
      }
    }

    // Calculate final stats
    const stats: Record<string, { totalCount: number; successRate: number; avgDuration: number }> =
      {};
    for (const [nodeType, data] of Object.entries(result)) {
      stats[nodeType] = {
        totalCount: data.totalCount,
        successRate: data.totalCount > 0 ? data.successCount / data.totalCount : 0,
        avgDuration: 0, // Would need to query duration histogram
      };
    }

    return stats;
  }

  /**
   * Export node metrics in Prometheus format
   */
  toPrometheus(): string[] {
    const metrics: PrometheusMetric[] = [];

    // Node execution stats by type
    const nodeStats = this.getNodeExecutionStatsByType();
    for (const [nodeType, stats] of Object.entries(nodeStats)) {
      metrics.push({
        name: "node_execution_total",
        type: "counter",
        help: "Total node executions by type",
        samples: [{ labels: { node_type: nodeType }, value: stats.totalCount }],
      });

      metrics.push({
        name: "node_execution_success_rate",
        type: "gauge",
        help: "Node execution success rate by type",
        samples: [{ labels: { node_type: nodeType }, value: stats.successRate }],
      });
    }

    // Top templates
    const topTemplates = this.getTopNodeTemplates(10);
    for (const template of topTemplates) {
      metrics.push({
        name: "node_template_instantiation_total",
        type: "counter",
        help: "Node template instantiation count",
        samples: [
          {
            labels: {
              template_name: template.templateName,
              node_type: template.nodeType,
            },
            value: template.instantiationCount,
          },
        ],
      });
    }

    return metrics.flatMap(m => PrometheusFormatter.formatMetric(m));
  }

  /**
   * Export as JSON
   */
  toJSON(): Record<string, unknown> {
    return {
      type: "node",
      statsByType: this.getNodeExecutionStatsByType(),
      topTemplates: this.getTopNodeTemplates(10),
    };
  }
}
