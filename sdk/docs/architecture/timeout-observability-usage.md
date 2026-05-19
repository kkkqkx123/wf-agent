# Timeout Observability Usage Guide

This guide demonstrates how to use the enhanced timeout observability features in the SDK.

## Overview

The timeout system now provides comprehensive observability through:

1. **Metrics Collection**: Real-time metrics via `TimeoutMetricsCollector`
2. **Event Emission**: Lifecycle events for timeout registration, expiration, cancellation, and warnings
3. **Diagnostic APIs**: Tools for monitoring and debugging active timeouts
4. **Prometheus Export**: Support for external monitoring systems

## Quick Start

### 1. Initialize with Observability

```typescript
import { MetricsRegistry } from '@wf-agent/sdk/core/metrics/metrics-registry.js';
import { TimeoutRegistry } from '@wf-agent/sdk/core/registry/timeout-registry.js';
import { EventRegistry } from '@wf-agent/sdk/core/registry/event-registry.js';

// Create registries
const eventRegistry = new EventRegistry();
const metricsRegistry = new MetricsRegistry({
  timeoutMetrics: {
    bufferSize: 100,
    flushInterval: 5000,
  },
});

const timeoutRegistry = new TimeoutRegistry(
  {},
  {
    metricsCollector: metricsRegistry.getTimeoutCollector(),
    eventRegistry,
  }
);
```

### 2. Register Timeouts with Tags

```typescript
const executionId = 'exec-123';

// Register a timeout with standard tag
const handle = timeoutRegistry.register(executionId, {
  id: 'llm-call-001',
  duration: 30000, // 30 seconds
  tag: 'llm-call', // Standard tag for categorization
  onTimeout: async () => {
    console.log('LLM call timed out!');
  },
  metadata: {
    model: 'gpt-4',
    node: 'agent-node',
  },
});
```

### 3. Monitor via Metrics

```typescript
// Get timeout collector
const timeoutCollector = metricsRegistry.getTimeoutCollector();

// Generate summary
const summary = timeoutCollector.generateSummary();
console.log('Active timeouts:', summary.totalActive);
console.log('Expired count:', summary.totalExpired);
console.log('Timeout rate:', summary.timeoutRate);

// Query specific metrics
const result = timeoutCollector.query({
  metricName: 'timeout.registration.count',
  labels: { tag: 'llm-call' },
});
```

### 4. Subscribe to Events

```typescript
const emitter = eventRegistry.getEmitter(executionId);

// Listen for timeout registrations
emitter.on('TIMEOUT_REGISTERED', (event) => {
  console.log('Timeout registered:', {
    id: event.timeoutId,
    duration: event.duration,
    tag: event.tag,
  });
});

// Listen for timeout expirations
emitter.on('TIMEOUT_EXPIRED', (event) => {
  console.log('Timeout expired:', {
    id: event.timeoutId,
    actualDuration: event.actualDuration,
    configuredDuration: event.configuredDuration,
  });
});

// Listen for warnings
emitter.on('TIMEOUT_WARNING', (event) => {
  console.log('Timeout warning:', {
    id: event.timeoutId,
    remainingTime: event.remainingTime,
  });
});
```

### 5. Use Diagnostic APIs

```typescript
// Get all active timeouts
const activeTimeouts = timeoutRegistry.getActiveTimeouts();
console.log('Active timeouts:', activeTimeouts.length);

// Filter by tag
const llmTimeouts = timeoutRegistry.getActiveTimeouts({ tag: 'llm-call' });

// Filter by category
const workflowTimeouts = timeoutRegistry.getActiveTimeouts({ category: 'workflow' });

// Find stuck timeouts (>90% elapsed)
const stuckTimeouts = timeoutRegistry.findStuckTimeouts(90);
stuckTimeouts.forEach((timeout) => {
  console.warn(`Stuck timeout: ${timeout.timeoutId} (${timeout.progressPercent.toFixed(1)}%)`);
});

// Export for Prometheus
const prometheusData = timeoutRegistry.exportForMonitoring();
console.log('Gauges:', prometheusData.gauges);
console.log('Counters:', prometheusData.counters);
```

## Standard Tags

Use these standardized tags for consistent categorization:

### LLM Timeouts
- `llm-call`: Single LLM call
- `llm-stream`: LLM streaming response
- `llm-retry`: LLM retry attempt

### Tool Timeouts
- `tool-execution`: Generic tool execution
- `tool-shell`: Shell command execution
- `tool-api`: API call tool

### Workflow Timeouts
- `workflow-execution`: Overall workflow execution
- `workflow-pause`: Pause state monitoring
- `workflow-node`: Node execution

### Interruption Timeouts
- `interruption-hook`: Interruption hook execution
- `interruption-cleanup`: Interruption cleanup operations

### User Interaction Timeouts
- `user-input`: Waiting for user input
- `user-approval`: Waiting for user approval

## Metrics Reference

### Counters
- `timeout.registration.count`: Number of timeouts registered
- `timeout.expiration.count`: Number of timeouts that expired
- `timeout.cancellation.count`: Number of timeouts cancelled
- `timeout.warning.count`: Number of warnings emitted
- `timeout.registered.total`: Total registered (cumulative)
- `timeout.expired.total`: Total expired (cumulative)
- `timeout.cancelled.total`: Total cancelled (cumulative)

### Gauges
- `timeout.active.count`: Current number of active timeouts
- `timeout.executions.active`: Number of active executions
- `timeout.active.by_tag`: Active timeouts by tag
- `timeout.active.by_category`: Active timeouts by category
- `timeout.warning.remaining_time`: Remaining time when warning emitted

### Histograms
- `timeout.duration.configured`: Configured timeout durations
- `timeout.duration.actual`: Actual timeout durations (for expired timeouts)

## Labels

All metrics support the following labels:

- `tag`: The timeout tag (e.g., 'llm-call', 'tool-execution')
- `execution_id`: The execution ID
- `reason`: Cancellation reason ('user', 'interrupted', 'cleanup')

## Best Practices

### 1. Always Use Standard Tags

```typescript
// ✅ Good
timeoutRegistry.register(executionId, {
  id: 'my-timeout',
  duration: 30000,
  tag: 'llm-call', // Standard tag
});

// ❌ Bad
timeoutRegistry.register(executionId, {
  id: 'my-timeout',
  duration: 30000,
  tag: 'my-custom-tag', // Non-standard
});
```

### 2. Add Meaningful Metadata

```typescript
timeoutRegistry.register(executionId, {
  id: 'llm-call-001',
  duration: 30000,
  tag: 'llm-call',
  metadata: {
    model: 'gpt-4',
    node: 'agent-node',
    temperature: 0.7,
  },
});
```

### 3. Monitor Stuck Timeouts

```typescript
// Periodically check for stuck timeouts
setInterval(() => {
  const stuck = timeoutRegistry.findStuckTimeouts(90);
  if (stuck.length > 0) {
    console.warn('Detected stuck timeouts:', stuck);
    // Take corrective action
  }
}, 60000); // Check every minute
```

### 4. Clean Up Properly

```typescript
// When execution ends
timeoutRegistry.cleanup(executionId);

// On application shutdown
timeoutRegistry.dispose();
metricsRegistry.dispose();
```

## Troubleshooting

### High Timeout Rate

If `timeoutRate` is high (>0.5), investigate:

```typescript
const summary = timeoutCollector.generateSummary();
if (summary.timeoutRate > 0.5) {
  console.warn('High timeout rate detected!');
  
  // Check which tags have most timeouts
  Object.entries(summary.byTag).forEach(([tag, count]) => {
    console.log(`${tag}: ${count} timeouts`);
  });
  
  // Find stuck timeouts
  const stuck = timeoutRegistry.findStuckTimeouts(80);
  console.log('Potentially stuck:', stuck);
}
```

### Memory Leak Detection

Monitor active timeouts over time:

```typescript
let previousCount = 0;
setInterval(() => {
  const stats = timeoutRegistry.getStats();
  if (stats.totalTimeouts > previousCount * 1.5) {
    console.error('Possible memory leak: timeout count growing rapidly');
  }
  previousCount = stats.totalTimeouts;
}, 300000); // Check every 5 minutes
```

## Integration with External Monitoring

### Prometheus

```typescript
// Export metrics in Prometheus format
const collector = metricsRegistry.getTimeoutCollector();
const prometheusLines = collector.toPrometheus();
console.log(prometheusLines.join('\n'));
```

Example output:
```
# HELP timeout_active_total Current active timeouts
# TYPE timeout_active_total gauge
timeout_active_total 5

# HELP timeout_registered_total Total registered timeouts
# TYPE timeout_registered_total counter
timeout_registered_total{tag="llm-call"} 10
timeout_registered_total{tag="tool-execution"} 15
```

### Custom Dashboard

```typescript
// Build custom dashboard data
function buildDashboardData() {
  const stats = timeoutRegistry.getStats();
  const summary = timeoutCollector.generateSummary();
  const stuck = timeoutRegistry.findStuckTimeouts(90);
  
  return {
    overview: {
      activeTimeouts: stats.totalTimeouts,
      activeExecutions: stats.activeExecutions,
      timeoutRate: summary.timeoutRate,
    },
    breakdown: {
      byTag: stats.byTag,
      byCategory: stats.byCategory,
    },
    alerts: {
      stuckTimeouts: stuck.length,
      stuckDetails: stuck,
    },
  };
}
```

## Advanced Usage

### Custom Tag Categories

You can extend the standard tags with your own prefixes:

```typescript
import { TIMEOUT_TAG_PREFIXES } from '@wf-agent/sdk/core/types/timeout-tags.js';

// Your custom prefix
const CUSTOM_PREFIX = 'custom';

// Use it consistently
timeoutRegistry.register(executionId, {
  id: 'my-timeout',
  duration: 30000,
  tag: `${CUSTOM_PREFIX}-my-operation`,
});
```

### Batch Operations

```typescript
// Cancel all LLM timeouts across all executions
timeoutRegistry.cancelByTag('llm-call');

// Cancel all workflow timeouts
timeoutRegistry.cancelByTag('workflow-execution');
```

### Historical Analysis

While the current implementation focuses on real-time monitoring, you can integrate with storage:

```typescript
// Periodically save metrics to database
setInterval(async () => {
  const stats = timeoutRegistry.getStats();
  await metricsStorage.save({
    timestamp: Date.now(),
    activeTimeouts: stats.totalTimeouts,
    expiredCount: stats.timedOutCount,
    cancelledCount: stats.cancelledCount,
    byTag: stats.byTag,
  });
}, 60000); // Every minute
```

## Summary

The enhanced timeout observability provides:

✅ **Real-time metrics** via TimeoutMetricsCollector  
✅ **Lifecycle events** for complete visibility  
✅ **Diagnostic APIs** for debugging and monitoring  
✅ **Standard tags** for consistent categorization  
✅ **Prometheus support** for external monitoring  
✅ **Best practices** for reliable operation  

Use these features to build robust monitoring, alerting, and debugging capabilities for your timeout management.
