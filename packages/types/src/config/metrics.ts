/**
 * Metrics Configuration Type Definitions
 * 
 * Defines types for the metrics system configuration.
 * These types are used across SDK layers for metrics collection and reporting.
 */

/**
 * Individual Metric Collector Configuration
 */
export interface MetricCollectorConfig {
  /** Buffer size for batching metrics before flush */
  bufferSize?: number;
  /** Flush interval in milliseconds */
  flushInterval?: number;
  /** Enable periodic reporting at collector level */
  enablePeriodicReporting?: boolean;
}

/**
 * Metrics System Configuration
 */
export interface MetricsConfig {
  /** Workflow execution metrics */
  workflowMetrics?: MetricCollectorConfig;
  /** Node execution metrics */
  nodeMetrics?: MetricCollectorConfig;
  /** Agent iteration metrics */
  agentMetrics?: MetricCollectorConfig;
  /** Event processing metrics */
  eventMetrics?: MetricCollectorConfig;
  /** Tool invocation metrics */
  toolMetrics?: MetricCollectorConfig;
  /** Token usage metrics */
  tokenMetrics?: MetricCollectorConfig;
  /** Template rendering metrics */
  templateMetrics?: MetricCollectorConfig;
  /** Configuration operation metrics */
  configMetrics?: MetricCollectorConfig;
  /** Error occurrence metrics */
  errorMetrics?: MetricCollectorConfig;
  /** Resource utilization metrics */
  resourceMetrics?: MetricCollectorConfig;
  /** Agent loop metrics */
  agentLoopMetrics?: MetricCollectorConfig;
  
  /** Enable unified periodic reporting across all collectors */
  enablePeriodicReporting?: boolean;
  /** Reporting interval in milliseconds (default: 60000) */
  reportingInterval?: number;
  
  /** Enable/disable metrics collection globally */
  enabled?: boolean;
}
