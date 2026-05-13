/**
 * MetricsAggregator Usage Example
 * 
 * This example demonstrates how to use the MetricsAggregator for
 * cross-execution event statistics and monitoring.
 */

import { MetricsAggregator, type MetricsSummary } from "../metrics-aggregator.js";

async function metricsAggregatorExample() {
  console.log("=== MetricsAggregator Example ===\n");

  // Create a metrics aggregator
  const aggregator = new MetricsAggregator({
    bufferSize: 100,
    enablePeriodicSummaries: true,
    summaryInterval: 2000, // 2 seconds
  });

  // Subscribe to periodic summaries
  const unsubscribe = aggregator.onSummary((summary: MetricsSummary) => {
    console.log("\n📊 Periodic Summary:");
    console.log(`   Total Events: ${summary.totalEvents}`);
    console.log(`   Active Executions: ${summary.activeExecutions}`);
    console.log(`   Event Types: ${summary.byEventType.size}`);
    
    for (const [eventType, stat] of summary.byEventType.entries()) {
      console.log(`   - ${eventType}: ${stat.count} occurrences`);
    }
  }, { interval: 2000 });

  // Simulate events from multiple executions
  console.log("Recording events from multiple executions...\n");

  // Execution 1: Multiple NODE_COMPLETED events
  for (let i = 0; i < 5; i++) {
    aggregator.record({
      executionId: "exec-1",
      eventType: "NODE_COMPLETED",
      timestamp: Date.now(),
      metadata: { nodeId: `node-${i}` },
    });
  }

  // Execution 2: Different event types
  aggregator.record({
    executionId: "exec-2",
    eventType: "WORKFLOW_STARTED",
    timestamp: Date.now(),
  });

  aggregator.record({
    executionId: "exec-2",
    eventType: "NODE_COMPLETED",
    timestamp: Date.now(),
    metadata: { nodeId: "node-A" },
  });

  aggregator.record({
    executionId: "exec-2",
    eventType: "NODE_COMPLETED",
    timestamp: Date.now(),
    metadata: { nodeId: "node-B" },
  });

  // Execution 3: TOOL_EXECUTED events
  for (let i = 0; i < 3; i++) {
    aggregator.record({
      executionId: "exec-3",
      eventType: "TOOL_EXECUTED",
      timestamp: Date.now(),
      metadata: { toolName: `tool-${i}` },
    });
  }

  // Wait a bit to see periodic summaries
  await new Promise(resolve => setTimeout(resolve, 3000));

  // Query specific statistics
  console.log("\n\n🔍 Querying Specific Statistics:\n");

  const nodeCompletedStats = aggregator.getStatistics("NODE_COMPLETED");
  if (nodeCompletedStats) {
    console.log("NODE_COMPLETED Statistics:");
    console.log(`  Total Count: ${nodeCompletedStats.count}`);
    console.log(`  First Seen: ${new Date(nodeCompletedStats.firstSeen || 0).toISOString()}`);
    console.log(`  Last Seen: ${new Date(nodeCompletedStats.lastSeen).toISOString()}`);
    console.log("  By Execution:");
    for (const [execId, count] of nodeCompletedStats.byExecution.entries()) {
      console.log(`    - ${execId}: ${count}`);
    }
  }

  // Get complete summary
  console.log("\n📈 Complete Metrics Summary:");
  const summary = aggregator.generateSummary();
  console.log(`Total Events: ${summary.totalEvents}`);
  console.log(`Active Executions: ${summary.activeExecutions}`);

  // Cleanup one execution
  console.log("\n🧹 Cleaning up execution 'exec-1'...");
  const cleanedCount = aggregator.cleanupExecution("exec-1");
  console.log(`Cleaned ${cleanedCount} event type entries`);

  // Check updated statistics
  const updatedStats = aggregator.getStatistics("NODE_COMPLETED");
  if (updatedStats) {
    console.log(`\nNODE_COMPLETED count after cleanup: ${updatedStats.count}`);
    console.log("Remaining executions:");
    for (const [execId, count] of updatedStats.byExecution.entries()) {
      console.log(`  - ${execId}: ${count}`);
    }
  }

  // Stop periodic summaries
  console.log("\n⏹️  Stopping periodic summaries...");
  unsubscribe();
  aggregator.stopPeriodicSummaries();

  // Dispose aggregator
  console.log("\n🗑️  Disposing aggregator...");
  aggregator.dispose();

  console.log("\n✅ Example completed!");
}

// Run the example
metricsAggregatorExample().catch(console.error);
