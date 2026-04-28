/**
 * Thread Message Types
 *
 * Messages related to Thread (Graph execution instance) lifecycle and operations.
 */

/**
 * Thread Message Type
 */
export enum ThreadMessageType {
  // Lifecycle
  /** Thread started */
  START = "thread.start",

  /** Thread paused */
  PAUSE = "thread.pause",

  /** Thread resumed */
  RESUME = "thread.resume",

  /** Thread ended */
  END = "thread.end",

  /** Thread cancelled */
  CANCEL = "thread.cancel",

  // Node Execution
  /** Node execution started */
  NODE_START = "thread.node.start",

  /** Node execution completed */
  NODE_END = "thread.node.end",

  /** Node execution failed */
  NODE_ERROR = "thread.node.error",

  /** Node execution skipped */
  NODE_SKIP = "thread.node.skip",

  // Workflow State
  /** Workflow started */
  WORKFLOW_START = "thread.workflow.start",

  /** Workflow ended */
  WORKFLOW_END = "thread.workflow.end",

  /** Workflow checkpoint created */
  WORKFLOW_CHECKPOINT = "thread.workflow.checkpoint",

  // Variable Operations
  /** Variable set */
  VARIABLE_SET = "thread.variable.set",

  /** Variable get */
  VARIABLE_GET = "thread.variable.get",

  // Parallel Execution (Fork/Join)
  /** Fork started */
  FORK_START = "thread.fork.start",

  /** Fork branch started */
  FORK_BRANCH_START = "thread.fork.branch_start",

  /** Fork branch ended */
  FORK_BRANCH_END = "thread.fork.branch_end",

  /** Join waiting for branches */
  JOIN_WAIT = "thread.join.wait",

  /** Join completed */
  JOIN_COMPLETE = "thread.join.complete",

  // Agent Node Call
  /** Agent node called */
  AGENT_CALL = "thread.agent.call",

  /** Agent node returned */
  AGENT_RETURN = "thread.agent.return",

  // Subgraph Call
  /** Subgraph called */
  SUBGRAPH_CALL = "thread.subgraph.call",

  /** Subgraph returned */
  SUBGRAPH_RETURN = "thread.subgraph.return",
}

/**
 * Thread Start Data
 */
export interface ThreadStartData {
  /** Thread ID */
  threadId: string;

  /** Graph ID */
  graphId: string;

  /** Input variables */
  input?: Record<string, unknown>;

  /** Parent thread ID (if subgraph) */
  parentThreadId?: string;
}

/**
 * Thread End Data
 */
export interface ThreadEndData {
  /** Thread ID */
  threadId: string;

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
 * Thread Node Data
 */
export interface ThreadNodeData {
  /** Thread ID */
  threadId: string;

  /** Graph ID */
  graphId: string;

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
 * Thread Variable Data
 */
export interface ThreadVariableData {
  /** Thread ID */
  threadId: string;

  /** Variable name */
  name: string;

  /** Variable value */
  value: unknown;

  /** Previous value (for set) */
  previousValue?: unknown;
}

/**
 * Thread Fork Data
 */
export interface ThreadForkData {
  /** Thread ID */
  threadId: string;

  /** Fork node ID */
  nodeId: string;

  /** Number of branches */
  branchCount: number;

  /** Branch thread IDs */
  branchThreadIds?: string[];
}

/**
 * Thread Fork Branch Data
 */
export interface ThreadForkBranchData {
  /** Parent thread ID */
  parentThreadId: string;

  /** Branch thread ID */
  branchThreadId: string;

  /** Branch index */
  branchIndex: number;

  /** Fork node ID */
  nodeId: string;
}

/**
 * Thread Join Data
 */
export interface ThreadJoinData {
  /** Thread ID */
  threadId: string;

  /** Join node ID */
  nodeId: string;

  /** Waiting branch thread IDs */
  waitingThreadIds?: string[];

  /** Completed branch results */
  branchResults?: Array<Record<string, unknown>>;
}

/**
 * Thread Agent Call Data
 */
export interface ThreadAgentCallData {
  /** Thread ID */
  threadId: string;

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
 * Thread Agent Return Data
 */
export interface ThreadAgentReturnData {
  /** Thread ID */
  threadId: string;

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
 * Thread Subgraph Call Data
 */
export interface ThreadSubgraphCallData {
  /** Parent thread ID */
  parentThreadId: string;

  /** Node ID */
  nodeId: string;

  /** Subgraph thread ID */
  subthreadId: string;

  /** Subgraph graph ID */
  subgraphId: string;

  /** Input variables */
  input?: Record<string, unknown>;
}

/**
 * Thread Subgraph Return Data
 */
export interface ThreadSubgraphReturnData {
  /** Parent thread ID */
  parentThreadId: string;

  /** Node ID */
  nodeId: string;

  /** Subgraph thread ID */
  subthreadId: string;

  /** Output variables */
  output?: Record<string, unknown>;
}
