/**
 * Display Formatter
 *
 * Pure functions that convert component messages into display-agnostic entries.
 * Both file and TUI renderers consume this intermediate format,
 * eliminating duplicate switch/case logic across output targets.
 */

import type { BaseComponentMessage } from "@wf-agent/types";
import {
  AgentMessageType,
  WorkflowExecutionMessageType,
  SystemMessageType,
} from "@wf-agent/types";
import type {
  AgentToolResultData,
  WorkflowExecutionNodeData,
  AgentIterationData,
  WorkflowExecutionForkBranchData,
  AgentStartData,
  AgentEndData,
  AgentLLMStreamData,
  AgentToolCallData,
  AgentToolEndData,
  CheckpointCreateData,
  CheckpointRestoreData,
} from "@wf-agent/types";

/**
 * Display-agnostic entry produced by the formatter.
 * Both file output and TUI screens consume this format.
 */
export interface DisplayEntry {
  /** Short heading */
  title: string;
  /** One-line summary */
  summary: string;
  /** Optional detailed body */
  detail?: string;
  /** Severity / importance level */
  level: "info" | "success" | "warning" | "error";
  /** Event timestamp */
  timestamp: number;
  /** Categorisation tags (e.g. "agent", "iteration", "tool", "checkpoint") */
  tags?: string[];
  /** Arbitrary structured data for rich renderers */
  metadata?: Record<string, unknown>;
}

/**
 * Convert a component message to a display entry.
 * Returns `null` when the message type is not recognised.
 */
export function formatMessage(message: BaseComponentMessage): DisplayEntry | null {
  switch (message.type) {
    /* ── Agent lifecycle ────────────────────────────────── */
    case AgentMessageType.AGENT_START:
      return formatAgentStart(message);
    case AgentMessageType.AGENT_END:
      return formatAgentEnd(message);

    /* ── Iteration ──────────────────────────────────────── */
    case AgentMessageType.ITERATION_START:
      return formatIterationStart(message);
    case AgentMessageType.ITERATION_END:
      return formatIterationEnd(message);

    /* ── LLM stream ─────────────────────────────────────── */
    case AgentMessageType.LLM_STREAM:
      return formatLLMStream(message);

    /* ── Tool calls ─────────────────────────────────────── */
    case AgentMessageType.TOOL_CALL_START:
      return formatToolCallStart(message);
    case AgentMessageType.TOOL_CALL_END:
      return formatToolCallEnd(message);
    case AgentMessageType.TOOL_RESULT:
      return formatToolResult(message);

    /* ── Checkpoint ─────────────────────────────────────── */
    case AgentMessageType.CHECKPOINT_CREATE:
      return formatCheckpointCreate(message);
    case AgentMessageType.CHECKPOINT_RESTORE:
      return formatCheckpointRestore(message);

    /* ── Node execution ─────────────────────────────────── */
    case WorkflowExecutionMessageType.NODE_START:
      return formatNodeStart(message);
    case WorkflowExecutionMessageType.NODE_END:
      return formatNodeEnd(message);
    case WorkflowExecutionMessageType.NODE_ERROR:
      return formatNodeError(message);

    /* ── Fork / join ────────────────────────────────────── */
    case WorkflowExecutionMessageType.FORK_BRANCH_START:
      return formatForkBranchStart(message);
    case WorkflowExecutionMessageType.FORK_BRANCH_END:
      return formatForkBranchEnd(message);

    /* ── System ─────────────────────────────────────────── */
    case SystemMessageType.ERROR:
      return formatSystemError(message);

    default:
      return null;
  }
}

/* ------------------------------------------------------------------ */
/*  Agent lifecycle                                                     */
/* ------------------------------------------------------------------ */

function formatAgentStart(message: BaseComponentMessage): DisplayEntry {
  const data = message.data as AgentStartData;
  return {
    title: "Agent Started",
    summary: `Agent "${data.agentId}" started`,
    level: "info",
    timestamp: message.timestamp,
    tags: ["agent", "lifecycle"],
    metadata: { agentId: data.agentId, loopId: data.loopId },
  };
}

function formatAgentEnd(message: BaseComponentMessage): DisplayEntry {
  const data = message.data as AgentEndData;
  return {
    title: "Agent Ended",
    summary: `Agent ended: ${data.status} (${data.totalIterations} iterations, ${data.duration}ms)`,
    level: data.status === "completed" ? "success" : "error",
    timestamp: message.timestamp,
    tags: ["agent", "lifecycle"],
    metadata: { status: data.status, totalIterations: data.totalIterations, duration: data.duration },
  };
}

/* ------------------------------------------------------------------ */
/*  Iteration                                                          */
/* ------------------------------------------------------------------ */

function formatIterationStart(message: BaseComponentMessage): DisplayEntry {
  const data = message.data as AgentIterationData;
  return {
    title: `Iteration ${data.iteration} Started`,
    summary: `Iteration ${data.iteration} started`,
    level: "info",
    timestamp: message.timestamp,
    tags: ["agent", "iteration"],
    metadata: { iteration: data.iteration },
  };
}

function formatIterationEnd(message: BaseComponentMessage): DisplayEntry {
  const data = message.data as AgentIterationData;
  return {
    title: `Iteration ${data.iteration} Completed`,
    summary: `Iteration ${data.iteration} completed${data.duration ? ` (${data.duration}ms)` : ""}`,
    level: "success",
    timestamp: message.timestamp,
    tags: ["agent", "iteration"],
    metadata: { iteration: data.iteration, duration: data.duration },
  };
}

/* ------------------------------------------------------------------ */
/*  LLM stream                                                         */
/* ------------------------------------------------------------------ */

function formatLLMStream(message: BaseComponentMessage): DisplayEntry {
  const data = message.data as AgentLLMStreamData;
  return {
    title: "LLM Response",
    summary: data.chunk ?? "",
    level: "info",
    timestamp: message.timestamp,
    tags: ["agent", "llm"],
    metadata: { chunk: data.chunk, isComplete: data.isComplete },
  };
}

/* ------------------------------------------------------------------ */
/*  Tool calls                                                         */
/* ------------------------------------------------------------------ */

function formatToolCallStart(message: BaseComponentMessage): DisplayEntry {
  const data = message.data as AgentToolCallData;
  return {
    title: `Tool Call: ${data.toolName}`,
    summary: `Calling tool "${data.toolName}"`,
    level: "info",
    timestamp: message.timestamp,
    tags: ["agent", "tool"],
    metadata: { toolName: data.toolName, toolCallId: data.toolCallId, arguments: data.arguments },
  };
}

function formatToolCallEnd(message: BaseComponentMessage): DisplayEntry {
  const data = message.data as AgentToolEndData;
  return {
    title: `Tool Result: ${data.toolName}`,
    summary: `${data.success ? "✓" : "✗"} Tool "${data.toolName}" completed (${data.duration}ms)`,
    level: data.success ? "success" : "error",
    timestamp: message.timestamp,
    tags: ["agent", "tool"],
    metadata: { toolName: data.toolName, success: data.success, duration: data.duration },
  };
}

function formatToolResult(message: BaseComponentMessage): DisplayEntry {
  const data = message.data as AgentToolResultData;
  return {
    title: `Tool Result: ${data.toolName}`,
    summary: `Tool "${data.toolName}" execution completed`,
    level: "info",
    timestamp: message.timestamp,
    tags: ["agent", "tool"],
    metadata: { toolName: data.toolName },
  };
}

/* ------------------------------------------------------------------ */
/*  Checkpoint                                                         */
/* ------------------------------------------------------------------ */

function formatCheckpointCreate(message: BaseComponentMessage): DisplayEntry {
  const data = message.data as CheckpointCreateData;
  return {
    title: `Checkpoint Created: ${data.checkpointId}`,
    summary: `Checkpoint "${data.checkpointId}" created`,
    level: "info",
    timestamp: message.timestamp,
    tags: ["checkpoint"],
    metadata: { checkpointId: data.checkpointId, entityType: data.entityType },
  };
}

function formatCheckpointRestore(message: BaseComponentMessage): DisplayEntry {
  const data = message.data as CheckpointRestoreData;
  return {
    title: `Checkpoint Restored: ${data.checkpointId}`,
    summary: `Checkpoint "${data.checkpointId}" restored`,
    level: "warning",
    timestamp: message.timestamp,
    tags: ["checkpoint"],
    metadata: { checkpointId: data.checkpointId, entityType: data.entityType },
  };
}

/* ------------------------------------------------------------------ */
/*  Node execution                                                     */
/* ------------------------------------------------------------------ */

function formatNodeStart(message: BaseComponentMessage): DisplayEntry {
  const data = message.data as WorkflowExecutionNodeData;
  return {
    title: `Node Started: ${data.nodeId}`,
    summary: `Node "${data.nodeId}" (${data.nodeType}) started execution`,
    level: "info",
    timestamp: message.timestamp,
    tags: ["workflow", "node"],
    metadata: { nodeId: data.nodeId, nodeType: data.nodeType },
  };
}

function formatNodeEnd(message: BaseComponentMessage): DisplayEntry {
  const data = message.data as WorkflowExecutionNodeData;
  return {
    title: `Node Completed: ${data.nodeId}`,
    summary: `Node "${data.nodeId}" execution completed${data.duration ? ` (${data.duration}ms)` : ""}`,
    level: "success",
    timestamp: message.timestamp,
    tags: ["workflow", "node"],
    metadata: { nodeId: data.nodeId, duration: data.duration },
  };
}

function formatNodeError(message: BaseComponentMessage): DisplayEntry {
  const data = message.data as WorkflowExecutionNodeData;
  return {
    title: `Node Error: ${data.nodeId}`,
    summary: `Node "${data.nodeId}" failed`,
    detail: data.error ?? "Unknown error",
    level: "error",
    timestamp: message.timestamp,
    tags: ["workflow", "node", "error"],
    metadata: { nodeId: data.nodeId, error: data.error },
  };
}

/* ------------------------------------------------------------------ */
/*  Fork / join                                                        */
/* ------------------------------------------------------------------ */

function formatForkBranchStart(message: BaseComponentMessage): DisplayEntry {
  const data = message.data as WorkflowExecutionForkBranchData;
  return {
    title: `Fork Branch Started: ${data.branchIndex}`,
    summary: `Fork branch ${data.branchIndex} started`,
    level: "info",
    timestamp: message.timestamp,
    tags: ["workflow", "fork"],
    metadata: { branchIndex: data.branchIndex },
  };
}

function formatForkBranchEnd(message: BaseComponentMessage): DisplayEntry {
  const data = message.data as WorkflowExecutionForkBranchData;
  return {
    title: `Fork Branch Completed: ${data.branchIndex}`,
    summary: `Fork branch ${data.branchIndex} completed`,
    level: "success",
    timestamp: message.timestamp,
    tags: ["workflow", "fork"],
    metadata: { branchIndex: data.branchIndex },
  };
}

/* ------------------------------------------------------------------ */
/*  System                                                             */
/* ------------------------------------------------------------------ */

function formatSystemError(message: BaseComponentMessage): DisplayEntry {
  const data = message.data as { message?: string };
  return {
    title: "Error",
    summary: data.message ?? "Unknown error",
    level: "error",
    timestamp: message.timestamp,
    tags: ["error"],
    metadata: { error: data.message },
  };
}