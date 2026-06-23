/**
 * SDK Interface Types - Type definitions for SDK compatibility checking
 *
 * These types define the contract that the underlying SDK must satisfy.
 * Used for validation and type safety without requiring hard dependencies on SDK implementation.
 *
 * Phase 1 enhancement:
 * - Stricter type definitions
 * - Better SDK feature detection
 * - Clearer API contracts
 */

/**
 * SDK Feature support detection
 */
export type SDKFeature =
  | 'checkpoints'
  | 'events'
  | 'streaming'
  | 'undo'
  | 'cancellation';

/**
 * Result type returned by SDK methods
 */
export interface SDKResult<T, E = Error> {
  success: boolean;
  data?: T;
  error?: E;
}

/**
 * Workflow template type
 */
export interface WorkflowTemplate {
  id: string;
  name?: string;
  description?: string;
  nodes: WorkflowNode[];
  edges: WorkflowEdge[];
  metadata?: Record<string, unknown>;
}

/**
 * Workflow node type
 */
export interface WorkflowNode {
  id: string;
  type: string;
  name?: string;
  description?: string;
  config?: Record<string, unknown>;
}

/**
 * Workflow edge type
 */
export interface WorkflowEdge {
  from: string;
  to: string;
  condition?: string;
}

/**
 * Workflow registry interface
 */
export interface WorkflowRegistry {
  create(template: WorkflowTemplate): Promise<SDKResult<string>>;
  get(id: string): Promise<SDKResult<WorkflowTemplate>>;
  update(id: string, template: Partial<WorkflowTemplate>): Promise<SDKResult<void>>;
  delete(id: string): Promise<SDKResult<void>>;
  list(filter?: Record<string, unknown>): Promise<SDKResult<WorkflowTemplate[]>>;
}

/**
 * Workflow execution registry interface
 */
export interface WorkflowExecutionRegistry {
  query(options: QueryOptions): Promise<SDKResult<ExecutionRecord[]>>;
}

/**
 * Query options interface
 */
export interface QueryOptions {
  workflowId?: string;
  status?: string;
  limit?: number;
  offset?: number;
  filters?: Record<string, unknown>;
}

/**
 * Execution record interface
 */
export interface ExecutionRecord {
  executionId: string;
  workflowId: string;
  status: string;
  input?: Record<string, unknown>;
  output?: Record<string, unknown>;
  error?: string;
  startTime: number;
  endTime?: number;
}

/**
 * SDK dependencies interface
 */
export interface SDKDependencies {
  [key: string]: unknown;
}

/**
 * SDK factory interface
 */
export interface SDKFactory {
  getDependencies(): SDKDependencies;
  getWorkflowRegistry(): WorkflowRegistry;
  getWorkflowExecutionRegistry(): WorkflowExecutionRegistry;
}

/**
 * Base command interface
 * Represents a command that can be executed by the SDK
 */
export interface BaseCommand {
  type: string;
  [key: string]: unknown;
}

/**
 * ExecuteWorkflowCommand constructor interface
 */
export interface ExecuteWorkflowCommandConstructor {
  new(
    config: ExecuteWorkflowConfig,
    dependencies: SDKDependencies
  ): ExecuteWorkflowCommand;
}

/**
 * ExecuteWorkflowCommand interface
 */
export interface ExecuteWorkflowCommand {
  execute?(): Promise<any>;
}

/**
 * ExecuteWorkflowConfig interface
 */
export interface ExecuteWorkflowConfig {
  workflowId: string;
  options?: Record<string, unknown>;
  [key: string]: unknown;
}

/**
 * Main SDK interface (Phase 1 enhancement)
 *
 * Provides a clearer contract for SDK implementations with:
 * - Required version property
 * - Strongly-typed executeCommand method
 * - SDKFactory access
 * - Required ExecuteWorkflowCommand
 * - Feature detection support
 */
export interface SDK {
  /** SDK version string (required, format: semver) */
  readonly version: string;

  /** Execute a command and return structured result */
  executeCommand(command: BaseCommand): Promise<SDKResult<any>>;

  /** Get SDK factory for accessing registries and dependencies */
  getFactory(): SDKFactory;

  /** Execute workflow command constructor (required) */
  readonly ExecuteWorkflowCommand: ExecuteWorkflowCommandConstructor;

  /** Alternative API namespace for command constructors */
  api?: {
    readonly ExecuteWorkflowCommand: ExecuteWorkflowCommandConstructor;
  };

  /** Check if SDK supports a specific feature */
  supports?(feature: SDKFeature): boolean;

  /** SDK options/configuration (optional) */
  options?: Record<string, unknown>;
}

