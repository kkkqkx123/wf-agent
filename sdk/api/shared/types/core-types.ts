/**
 * Core API Type Definitions
 * Defines types related to core execution
 */

import type { WorkflowExecutionOptions, PresetsConfig } from "@wf-agent/types";
import type {
  CheckpointStorageAdapter,
  WorkflowStorageAdapter,
  WorkflowExecutionStorageAdapter,
  TaskStorageAdapter,
} from "@wf-agent/storage";
import type { CustomTriggerHandler } from "../../../core/registry/custom-handler-registry.js";

/**
 * SDK Lifecycle Hooks
 * Allows apps to hook into SDK initialization lifecycle
 */
export interface SDKLifecycleHooks {
  /** Called when SDK bootstrap starts */
  onBootstrapStart?: () => void | Promise<void>;
  /** Called when SDK bootstrap completes successfully */
  onBootstrapComplete?: () => void | Promise<void>;
  /** Called when SDK bootstrap fails */
  onBootstrapError?: (error: Error) => void | Promise<void>;
  /** Called when SDK is being destroyed */
  onDestroy?: () => void | Promise<void>;
}

/**
 * MCP Configuration Options
 */
export interface McpConfig {
  /** Whether MCP is enabled globally */
  enabled?: boolean;
  /** Maximum error history size */
  maxErrorHistory?: number;
  /** Connection timeout in milliseconds */
  connectionTimeout?: number;
  /** Debounce delay for config changes in milliseconds */
  configDebounceDelay?: number;
}

/**
 * Human Relay Configuration
 */
export interface HumanRelayConfig {
  /** Default timeout for human relay requests (in milliseconds) */
  defaultTimeout?: number;
  /** Human Relay handler instance */
  handler?: unknown; // Will be typed as HumanRelayHandler when imported
}

/**
 * Logging Configuration
 */
export interface LoggingConfig {
  /** Log level */
  level?: "debug" | "info" | "warn" | "error";
  /** Log format */
  format?: "json" | "text";
  /** Log output destination */
  output?: "console" | "file" | "both";
  /** Log file path (if file output is enabled) */
  filePath?: string;
}

/**
 * Skill Configuration
 */
export interface SkillConfig {
  /** Skill directory paths to scan */
  paths?: string[];
  /** Whether to automatically scan skill directories */
  autoScan?: boolean;
  /** Enable caching for skills */
  cacheEnabled?: boolean;
  /** Cache TTL in milliseconds */
  cacheTTL?: number;
}

/**
 * Event System Configuration
 */
export interface EventSystemConfig {
  /** Maximum listener queue size (prevent memory overflow) */
  maxListenerQueueSize?: number;
  /** Default listener timeout in milliseconds */
  defaultListenerTimeout?: number;
  /** Slow listener threshold in milliseconds (for warnings) */
  slowListenerThreshold?: number;
  /** Enable backpressure control */
  enableBackpressure?: boolean;
  /** Maximum event history size */
  maxEventHistory?: number;
}

/**
 * LLM Profile Configuration
 */
export interface LLMProfileConfig {
  /** Default profile ID to use */
  defaultProfileId?: string;
  /** Pre-register profiles at initialization */
  profiles?: unknown[]; // Will be typed as LLMProfile[] when imported
}

/**
 * Validation Configuration
 */
export interface ValidationConfig {
  /** Enable workflow validation */
  enableWorkflowValidation?: boolean;
  /** Enable node configuration validation */
  enableNodeValidation?: boolean;
  /** Enable graph structure validation */
  enableGraphValidation?: boolean;
  /** Check for cycles in workflow graphs */
  checkCycles?: boolean;
  /** Check reachability in workflow graphs */
  checkReachability?: boolean;
  /** Validate FORK/JOIN pairs */
  checkForkJoin?: boolean;
  /** Validate START/END nodes */
  checkStartEnd?: boolean;
  /** Check for isolated nodes */
  checkIsolatedNodes?: boolean;
  /** Maximum recursion depth for workflow references */
  maxRecursionDepth?: number;
}

/**
 * Workflow Execution Configuration
 */
export interface WorkflowExecutionConfig {
  /** Default execution timeout (in milliseconds) */
  defaultTimeout?: number;
  /** Maximum concurrent workflow executions */
  maxConcurrentExecutions?: number;
  /** Enable execution retry on failure */
  enableRetry?: boolean;
  /** Maximum retry attempts */
  maxRetryAttempts?: number;
}

/**
 * Custom Trigger Handler Configuration
 */
export interface CustomTriggerHandlerConfig {
  /** Map of handler name to handler function */
  handlers?: Record<string, CustomTriggerHandler>;
}

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

/**
 * Graceful Shutdown Configuration
 */
export interface GracefulShutdownConfig {
  /** Whether to enable graceful shutdown (default: true) */
  enabled?: boolean;
  /** Maximum time to wait for all checkpoints during shutdown (milliseconds, default: 15000) */
  timeoutMs?: number;
}

/**
 * SDK Options
 */
export interface SDKOptions {
  /** Whether to enable debug mode */
  debug?: boolean;
  /** Detailed logging configuration */
  logging?: LoggingConfig;
  /** Default timeout period (in milliseconds) */
  defaultTimeout?: number;
  /** Whether to enable checkpoints */
  enableCheckpoints?: boolean;
  /** Checkpoint storage adapter interface (implemented by the application layer) */
  checkpointStorageAdapter?: CheckpointStorageAdapter;
  /** Workflow storage adapter interface (implemented by the application layer) */
  workflowStorageAdapter?: WorkflowStorageAdapter;
  /** Task storage adapter interface (implemented by the application layer) */
  taskStorageAdapter?: TaskStorageAdapter;
  /** Workflow execution storage adapter interface (implemented by the application layer) */
  workflowExecutionStorageAdapter?: WorkflowExecutionStorageAdapter;
  /** Whether to enable verification */
  enableValidation?: boolean;
  /** Detailed validation configuration */
  validation?: ValidationConfig;
  /** Predefined feature options */
  presets?: PresetsConfig;
  /** Lifecycle hooks for SDK initialization */
  hooks?: SDKLifecycleHooks;
  /** MCP configuration */
  mcp?: McpConfig;
  /** Human Relay configuration */
  humanRelay?: HumanRelayConfig;
  /** Skill registry configuration */
  skills?: SkillConfig;
  /** Event system configuration */
  events?: EventSystemConfig;
  /** LLM Profile configuration */
  profiles?: LLMProfileConfig;
  /** Workflow execution configuration */
  workflowExecution?: WorkflowExecutionConfig;
  /** Custom trigger handlers configuration */
  customTriggerHandlers?: Record<string, CustomTriggerHandler>;
  /** Graceful shutdown configuration */
  gracefulShutdown?: GracefulShutdownConfig;
  /** Metrics system configuration */
  metrics?: MetricsConfig;
}

export type { WorkflowExecutionOptions };
