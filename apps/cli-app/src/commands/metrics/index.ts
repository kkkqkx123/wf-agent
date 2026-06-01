/**
 * Metrics Command Group
 */

import { Command } from "commander";
import { MetricsAdapter } from "../../adapters/metrics-adapter.js";
import { getOutput } from "../../utils/output.js";
import { handleError } from "../../utils/error-handler.js";

const output = getOutput();

/**
 * Create Metrics Command Group
 */
export function createMetricsCommands(): Command {
  const metricsCmd = new Command("metrics").description("View and export metrics data");

  // Workflow metrics command
  metricsCmd
    .command("workflow")
    .description("View workflow execution metrics")
    .option("-w, --workflow-id <id>", "Filter by workflow ID")
    .option("--json", "Output in JSON format")
    .action(async (options: {
      workflowId?: string;
      json?: boolean;
    }) => {
      try {
        const adapter = new MetricsAdapter();
        const metrics = await adapter.getWorkflowMetrics({
          workflowId: options.workflowId,
        });

        if (options.json) {
          output.json(metrics);
        } else {
          output.output("=== Workflow Execution Metrics ===");
          output.output(`Total Executions: ${metrics.totalExecutions}`);
          output.output(`Success Rate: ${(metrics.successRate * 100).toFixed(2)}%`);
          output.output(`Average Duration: ${metrics.avgDuration.toFixed(2)}ms`);
          output.output(`P95 Duration: ${metrics.p95Duration.toFixed(2)}ms`);
          output.output(`P99 Duration: ${metrics.p99Duration.toFixed(2)}ms`);
          
          if (Object.keys(metrics.byVersion).length > 0) {
            output.output("\nExecutions by Version:");
            Object.entries(metrics.byVersion).forEach(([version, count]) => {
              output.output(`  ${version}: ${count}`);
            });
          }
        }
      } catch (error) {
        handleError(error, {
          operation: "get-workflow-metrics",
        });
      }
    });

  // Node template metrics command
  metricsCmd
    .command("node-templates")
    .description("View node template usage metrics")
    .option("-n, --top-n <number>", "Number of top templates to show", parseInt)
    .option("--json", "Output in JSON format")
    .action(async (options: {
      topN?: number;
      json?: boolean;
    }) => {
      try {
        const adapter = new MetricsAdapter();
        const metrics = await adapter.getNodeTemplateMetrics({
          topN: options.topN,
        });

        if (options.json) {
          output.json(metrics);
        } else {
          output.output("=== Node Template Usage Metrics ===");
          output.output(`Total Templates: ${metrics.length}`);
          output.output("");
          
          metrics.forEach((template: any) => {
            output.output(`Template: ${template.templateName}`);
            output.output(`  Type: ${template.nodeType}`);
            output.output(`  Instantiations: ${template.instantiationCount}`);
            output.output("");
          });
        }
      } catch (error) {
        handleError(error, {
          operation: "get-node-template-metrics",
        });
      }
    });

  // Agent metrics command
  metricsCmd
    .command("agents")
    .description("View agent loop metrics")
    .option("-p, --profile-id <id>", "Filter by LLM profile ID")
    .option("--json", "Output in JSON format")
    .action(async (options: {
      profileId?: string;
      json?: boolean;
    }) => {
      try {
        const adapter = new MetricsAdapter();
        const metrics = await adapter.getAgentMetrics({
          profileId: options.profileId,
        });

        if (options.json) {
          output.json(metrics);
        } else {
          output.output("=== Agent Loop Metrics ===");
          output.output(`Total Executions: ${metrics.totalExecutions}`);
          output.output(`Average Iterations: ${metrics.avgIterations.toFixed(2)}`);

          
          if (Object.keys(metrics.byProfile).length > 0) {
            output.output("\nExecutions by Profile:");
            Object.entries(metrics.byProfile).forEach(([profile, count]) => {
              output.output(`  ${profile}: ${count}`);
            });
          }
        }
      } catch (error) {
        handleError(error, {
          operation: "get-agent-metrics",
        });
      }
    });

  // Comprehensive report command
  metricsCmd
    .command("report")
    .description("Generate comprehensive metrics report")
    .option("--json", "Output in JSON format")
    .action(async (options: {
      json?: boolean;
    }) => {
      try {
        const adapter = new MetricsAdapter();
        const report = await adapter.getComprehensiveReport();

        if (options.json) {
          output.json(report);
        } else {
          output.output("=== Comprehensive Metrics Report ===");
          output.output(`Generated at: ${new Date(report.timestamp).toISOString()}`);
          output.output("");
          
          output.output("--- Summary ---");
          output.output(`Total Metrics: ${report.summary.totalMetrics}`);
          output.output("");
          
          output.output("Metrics by Type:");
          Object.entries(report.summary.byType).forEach(([type, count]) => {
            output.output(`  ${type}: ${count}`);
          });
          output.output("");
          
          if (report.topMetrics && report.topMetrics.length > 0) {
            output.output("Top Metrics:");
            report.topMetrics.slice(0, 10).forEach((metric, index) => {
              output.output(`  ${index + 1}. ${metric.metricName}: ${metric.value}`);
            });
            output.output("");
          }
          
          if (report.anomalies && report.anomalies.length > 0) {
            output.output("Anomalies Detected:");
            report.anomalies.forEach(anomaly => {
              output.output(`  [${anomaly.severity.toUpperCase()}] ${anomaly.metricName}: ${anomaly.description}`);
            });
          }
        }
      } catch (error) {
        handleError(error, {
          operation: "get-comprehensive-report",
        });
      }
    });

  // Export metrics command
  metricsCmd
    .command("export")
    .description("Export metrics to file")
    .requiredOption("-f, --format <format>", "Export format (json/prometheus)")
    .option("-o, --output <file>", "Output file path")
    .action(async (options: { format: string; output?: string }) => {
      try {
        const adapter = new MetricsAdapter();
        const result = await adapter.exportMetrics(
          options.format as any,
          options.output,
        );

        if (result.outputFile) {
          output.success(`Metrics exported to: ${result.outputFile}`);
        } else if (result.content) {
          output.output(result.content);
        }
      } catch (error) {
        handleError(error, {
          operation: "export-metrics",
          additionalInfo: { format: options.format },
        });
      }
    });

  return metricsCmd;
}
