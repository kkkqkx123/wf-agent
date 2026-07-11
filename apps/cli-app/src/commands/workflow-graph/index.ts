/**
 * Workflow Graph Commands
 * Query and analyze workflow graph structure
 */

import { Command } from "commander";
import { WorkflowGraphAdapter } from "../../adapters/workflow-graph-adapter.js";
import { getRouter } from "../../utils/output-router.js";
import { handleError } from "../../utils/error-handler.js";
import {
  formatGraphSummary,
  formatGraphAnalysis,
  formatNodeStats,
  formatEdgeStats,
  formatNodesAscii,
} from "../../utils/formatters/graph-formatters.js";

export function createWorkflowGraphCommands(): Command {
  const graph = new Command("graph").description("Workflow graph structure analysis");

  graph
    .command("show <workflow-id>")
    .description("Display workflow graph structure")
    .option("-f, --format <format>", "Output format (text|json|dot)", "text")
    .option("-m, --metadata", "Include metadata")
    .action(async (workflowId: string, options) => {
      try {
        const adapter = new WorkflowGraphAdapter();
        const summary = await adapter.getGraphSummary(workflowId);

        getRouter().render(summary, {
          type: "detail",
          entity: "workflow_graph",
          format: () => formatGraphSummary(summary),
          message: `Graph structure for workflow: ${workflowId}`,
        });
      } catch (error) {
        handleError(error, {
          operation: "workflow-graph-show",
          additionalInfo: { workflowId, format: options.format },
        });
      }
    });

  graph
    .command("analyze <workflow-id>")
    .description("Analyze workflow graph for cycles, paths, and structure")
    .option("-c, --check-cycles", "Check for cycles (default: true)")
    .option("-p, --find-critical-path", "Find critical path")
    .action(async (workflowId: string) => {
      try {
        const adapter = new WorkflowGraphAdapter();
        const analysis = await adapter.analyzeGraph(workflowId);

        getRouter().render(analysis, {
          type: "detail",
          entity: "workflow_graph_analysis",
          format: () => formatGraphAnalysis(analysis),
          message: `Graph analysis for workflow: ${workflowId}`,
        });
      } catch (error) {
        handleError(error, {
          operation: "workflow-graph-analyze",
          additionalInfo: { workflowId },
        });
      }
    });

  graph
    .command("nodes <workflow-id>")
    .description("List all nodes in workflow graph")
    .option("-t, --type <type>", "Filter by node type (e.g., LLM, CONDITION, FORK)")
    .option("-d, --depth <number>", "Maximum depth to display")
    .action(async (workflowId: string, options) => {
      try {
        const adapter = new WorkflowGraphAdapter();
        const nodes = await adapter.getNodes(workflowId, options.type);
        const edges = await adapter.getEdges(workflowId);

        getRouter().render(nodes, {
          type: "list",
          entity: "workflow_nodes",
          format: () => formatNodesAscii(nodes, edges.length),
          message: `Graph nodes for workflow: ${workflowId}`,
          metadata: {
            nodeCount: nodes.length,
            edgeCount: edges.length,
          },
        });
      } catch (error) {
        handleError(error, {
          operation: "workflow-graph-nodes",
          additionalInfo: { workflowId, type: options.type },
        });
      }
    });

  graph
    .command("stats <workflow-id>")
    .description("Get graph statistics")
    .action(async (workflowId: string) => {
      try {
        const adapter = new WorkflowGraphAdapter();
        const nodeStats = await adapter.getNodeStats(workflowId);
        const edgeStats = await adapter.getEdgeStats(workflowId);

        const combined = {
          nodes: nodeStats,
          edges: edgeStats,
        };

        getRouter().render(combined, {
          type: "detail",
          entity: "workflow_graph_stats",
          format: () =>
            formatNodeStats(nodeStats) + formatEdgeStats(edgeStats),
          message: `Graph statistics for workflow: ${workflowId}`,
        });
      } catch (error) {
        handleError(error, {
          operation: "workflow-graph-stats",
          additionalInfo: { workflowId },
        });
      }
    });

  return graph;
}
