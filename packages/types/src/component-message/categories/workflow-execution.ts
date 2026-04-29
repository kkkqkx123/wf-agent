/**
 * Workflow Execution Message Types
 *
 * Messages related to WorkflowExecution (Graph execution instance) lifecycle and operations.
 */

/**
 * Workflow Execution Message Type
 */
export enum WorkflowExecutionMessageType {
  // Lifecycle
  /** Workflow execution started */
  START = "workflow.execution.start",

  /** Workflow execution paused */
  PAUSE = "workflow.execution.pause",

  /** Workflow execution resumed */
  RESUME = "workflow.execution.resume",

  /** Workflow execution ended */
  END = "workflow.execution.end",

  /** Workflow execution cancelled */
  CANCEL = "workflow.execution.cancel",

  // Node Execution
  /** Node execution started */
  NODE_START = "workflow.execution.node.start",

  /** Node execution completed */
  NODE_END = "workflow.execution.node.end",

  /** Node execution failed */
  NODE_ERROR = "workflow.execution.node.error",

  /** Node execution skipped */
  NODE_SKIP = "workflow.execution.node.skip",

  // Workflow State
  /** Workflow started */
  WORKFLOW_START = "workflow.execution.workflow.start",

  /** Workflow ended */
  WORKFLOW_END = "workflow.execution.workflow.end",

  /** Workflow checkpoint created */
  WORKFLOW_CHECKPOINT = "workflow.execution.workflow.checkpoint",

  // Variable Operations
  /** Variable set */
  VARIABLE_SET = "workflow.execution.variable.set",

  /** Variable get */
  VARIABLE_GET = "workflow.execution.variable.get",

  // Parallel Execution (Fork/Join)
  /** Fork started */
  FORK_START = "workflow.execution.fork.start",

  /** Fork branch started */
  FORK_BRANCH_START = "workflow.execution.fork.branch_start",

  /** Fork branch ended */
  FORK_BRANCH_END = "workflow.execution.fork.branch_end",

  /** Join waiting for branches */
  JOIN_WAIT = "workflow.execution.join.wait",

  /** Join completed */
  JOIN_COMPLETE = "workflow.execution.join.complete",

  // Agent Node Call
  /** Agent node called */
  AGENT_CALL = "workflow.execution.agent.call",

  /** Agent node returned */
  AGENT_RETURN = "workflow.execution.agent.return",

  // Subgraph Call
  /** Subgraph called */
  SUBGRAPH_CALL = "workflow.execution.subgraph.call",

  /** Subgraph returned */
  SUBGRAPH_RETURN = "workflow.execution.subgraph.return",
}

/**
 * Workflow Execution Start Data
 */
export interface WorkflowExecutionStartData {
  /** Execution ID */
  executionId: string;

  /** Workflow ID */
  workflowId: string;

  /** Input variables */
  input?: Record<string, unknown>;

  /** Parent execution ID (if subgraph) */
  parentExecutionId?: string;
}

/**
 * Workflow Execution End Data
 */
export interface WorkflowExecutionEndData {
  /** Execution ID */
  executionId: string;

  /** Final status */
  status: "completed" | "failed" | "cancelled";

  /** Output variables */
  output?: Record<string, unknown>;

  /** Error message (if failed) */
  error?: string;

  /** Total execution duration in milliseconds */
  duration: number;
}

/**
 * Workflow Execution Node Data
 */
export interface WorkflowExecutionNodeData {
  /** Execution ID */
  executionId: string;

  /** Workflow ID */
  workflowId: string;

  /** Node ID */
  nodeId: string;

  /** Node type */
  nodeType: string;

  /** Node status */
  status: "running" | "completed" | "error" | "skipped";

  /** Execution duration in milliseconds */
  duration?: number;

  /** Node input */
  input?: Record<string, unknown>;

  /** Node output */
  output?: Record<string, unknown>;

  /** Error message (if error) */
  error?: string;
}

/**
 * Workflow Execution Variable Data
 */
export interface WorkflowExecutionVariableData {
  /** Execution ID */
  executionId: string;

  /** Variable name */
  name: string;

  /** Variable value */
  value: unknown;

  /** Previous value (for set) */
  previousValue?: unknown;
}

/**
 * Workflow Execution Fork Data
 */
export interface WorkflowExecutionForkData {
  /** Execution ID */
  executionId: string;

  /** Fork node ID */
  nodeId: string;

  /** Number of branches */
  branchCount: number;

  /** Branch execution IDs */
  branchExecutionIds?: string[];
}

/**
 * Workflow Execution Fork Branch Data
 */
export interface WorkflowExecutionForkBranchData {
  /** Parent execution ID */
  parentExecutionId: string;

  /** Branch execution ID */
  branchExecutionId: string;

  /** Branch index */
  branchIndex: number;

  /** Fork node ID */
  nodeId: string;
}

/**
 * Workflow Execution Join Data
 */
export interface WorkflowExecutionJoinData {
  /** Execution ID */
  executionId: string;

  /** Join node ID */
  nodeId: string;

  /** Waiting branch execution IDs */
  waitingExecutionIds?: string[];

  /** Completed branch results */
  branchResults?: Array<Record<string, unknown>>;
}

/**
 * Workflow Execution Agent Call Data
 */
export interface WorkflowExecutionAgentCallData {
  /** Execution ID */
  executionId: string;

  /** Node ID */
  nodeId: string;

  /** Target agent loop ID */
  targetLoopId: string;

  /** Agent configuration */
  config: {
    /** Agent profile ID */
    profileId: string;

    /** Maximum iterations */
    maxIterations: number;

    /** Tool IDs */
    tools?: string[];
  };
}

/**
 * Workflow Execution Agent Return Data
 */
export interface WorkflowExecutionAgentReturnData {
  /** Execution ID */
  executionId: string;

  /** Node ID */
  nodeId: string;

  /** Agent loop ID */
  loopId: string;

  /** Agent result */
  result: unknown;

  /** Agent status */
  status: "completed" | "failed" | "cancelled";
}

/**
 * Workflow Execution Subgraph Call Data
 */
export interface WorkflowExecutionSubgraphCallData {
  /** Parent execution ID */
  parentExecutionId: string;

  /** Node ID */
  nodeId: string;

  /** Subgraph execution ID */
  subExecutionId: string;

  /** Subgraph workflow ID */
  subworkflowId: string;

  /** Input variables */
  input?: Record<string, unknown>;
}

/**
 * Workflow Execution Subgraph Return Data
 */
export interface WorkflowExecutionSubgraphReturnData {
  /** Parent execution ID */
  parentExecutionId: string;

  /** Node ID */
  nodeId: string;

  /** Subgraph execution ID */
  subExecutionId: string;

  /** Output variables */
  output?: Record<string, unknown>;
}
