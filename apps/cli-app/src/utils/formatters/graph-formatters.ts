/**
 * Workflow Graph Formatters
 * Format graph query results for CLI output
 */

import type {
  WorkflowGraphSummary,
  GraphNodeStats,
  GraphEdgeStats,
} from "@wf-agent/sdk/api";

export function formatGraphSummary(summary: WorkflowGraphSummary): string {
  const lines: string[] = [];

  lines.push("");
  lines.push(`📊 Graph Summary: ${summary.workflowId}`);
  lines.push("");

  lines.push(`Nodes: ${summary.nodeCount}`);
  lines.push(`Edges: ${summary.edgeCount}`);

  if ((summary as any).nodeStats) {
    lines.push("");
    lines.push("Node Types:");
    const stats = (summary as any).nodeStats;
    for (const [type, count] of Object.entries(stats)) {
      lines.push(`  • ${type}: ${count}`);
    }
  }

  lines.push("");
  return lines.join("\n");
}

export function formatGraphAnalysis(analysis: any): string {
  const lines: string[] = [];

  lines.push("");
  lines.push("🔍 Graph Analysis");
  lines.push("");

  if (analysis.hasCycles) {
    lines.push("⚠️ Cycles Detected");
    if (analysis.cycles) {
      lines.push("Cycles:");
      for (const cycle of analysis.cycles) {
        lines.push(`  • ${cycle}`);
      }
    }
    lines.push("");
  } else {
    lines.push("✓ No cycles detected");
  }

  if (analysis.isDAG !== false) {
    lines.push("✓ Valid DAG (Directed Acyclic Graph)");
  }

  if (analysis.criticalPath) {
    lines.push("");
    lines.push("Critical Path:");
    lines.push(`  Length: ${analysis.criticalPath.length}`);
    lines.push(`  Estimated Duration: ${analysis.criticalPath.duration}ms`);
  }

  if (analysis.sourceNodes) {
    lines.push("");
    lines.push(`Source Nodes: ${analysis.sourceNodes.length}`);
    for (const node of analysis.sourceNodes.slice(0, 5)) {
      lines.push(`  • ${node}`);
    }
  }

  if (analysis.sinkNodes) {
    lines.push("");
    lines.push(`Sink Nodes: ${analysis.sinkNodes.length}`);
    for (const node of analysis.sinkNodes.slice(0, 5)) {
      lines.push(`  • ${node}`);
    }
  }

  lines.push("");
  return lines.join("\n");
}

export function formatNodeStats(stats: GraphNodeStats): string {
  const lines: string[] = [];

  lines.push("");
  lines.push("Node Statistics:");
  lines.push(`  Total: ${stats.total}`);
  lines.push("");
  lines.push("By Type:");

  for (const [type, count] of Object.entries(stats.byType)) {
    lines.push(`  • ${type}: ${count}`);
  }
  lines.push("");

  return lines.join("\n");
}

export function formatEdgeStats(stats: GraphEdgeStats): string {
  const lines: string[] = [];

  lines.push("");
  lines.push("Edge Statistics:");
  lines.push(`  Total: ${stats.total}`);
  lines.push("");
  lines.push("By Type:");

  for (const [type, count] of Object.entries(stats.byType)) {
    lines.push(`  • ${type}: ${count}`);
  }
  lines.push("");

  return lines.join("\n");
}

export function formatNodesAscii(nodes: any[], edgeCount: number): string {
  const lines: string[] = [];

  lines.push("");
  lines.push("Nodes Graph Preview:");
  lines.push(`[Total Nodes: ${nodes.length}] [Total Edges: ${edgeCount}]`);
  lines.push("");

  const byType: Record<string, any[]> = {};
  for (const node of nodes) {
    const type = (node as any).type || "unknown";
    if (!byType[type]) byType[type] = [];
    byType[type].push(node);
  }

  for (const [type, typeNodes] of Object.entries(byType)) {
    lines.push(`${type} (${typeNodes.length}):`);
    for (const node of typeNodes.slice(0, 3)) {
      lines.push(`  ├─ ${(node as any).id || (node as any).name}`);
    }
    if (typeNodes.length > 3) {
      lines.push(`  └─ ... and ${typeNodes.length - 3} more`);
    }
  }

  lines.push("");
  return lines.join("\n");
}
