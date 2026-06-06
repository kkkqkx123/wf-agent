/**
 * CLI-App Routing Rules
 *
 * Application-specific routing rules for component messages.
 */

import type { RoutingRule } from "@wf-agent/types";
import {
  MessageCategory,
  OutputTarget,
  AgentMessageType,
  WorkflowExecutionMessageType,
} from "@wf-agent/types";

/**
 * Default routing rules for CLI-App
 */
export const CLI_ROUTING_RULES: RoutingRule[] = [
  // Rule 1: Agent LLM stream -> TUI only
  {
    name: "agent-llm-stream",
    match: {
      types: [AgentMessageType.LLM_STREAM],
    },
    decision: {
      targets: [OutputTarget.TUI],
      aggregateToParent: false,
      aggregateLevel: "none",
      notifyParent: false,
    },
    priority: 100,
  },

  // Rule 4: Agent tool call start/end -> TUI (summary)
  {
    name: "agent-tool-call",
    match: {
      types: [AgentMessageType.TOOL_CALL_START, AgentMessageType.TOOL_CALL_END],
    },
    decision: {
      targets: [OutputTarget.TUI],
      aggregateToParent: false,
      aggregateLevel: "none",
      notifyParent: false,
    },
    priority: 100,
  },

  // Rule 4: Agent tool result -> FILE_DISPLAY (detail)
  {
    name: "agent-tool-result",
    match: {
      types: [AgentMessageType.TOOL_RESULT],
    },
    decision: {
      targets: [OutputTarget.FILE_DISPLAY],
      aggregateToParent: true,
      aggregateLevel: "summary",
      notifyParent: false,
    },
    priority: 100,
  },

  // Rule 5: Agent lifecycle events -> TUI + FILE_DISPLAY
  {
    name: "agent-lifecycle",
    match: {
      types: [
        AgentMessageType.AGENT_START,
        AgentMessageType.AGENT_END,
        AgentMessageType.AGENT_PAUSE,
        AgentMessageType.AGENT_RESUME,
        AgentMessageType.AGENT_CANCEL,
      ],
    },
    decision: {
      targets: [OutputTarget.TUI, OutputTarget.FILE_DISPLAY],
      aggregateToParent: true,
      aggregateLevel: "summary",
      notifyParent: true,
    },
    priority: 100,
  },

  // Rule 6: Agent iteration events -> TUI + FILE_DISPLAY
  {
    name: "agent-iteration",
    match: {
      types: [AgentMessageType.ITERATION_START, AgentMessageType.ITERATION_END],
    },
    decision: {
      targets: [OutputTarget.TUI, OutputTarget.FILE_DISPLAY],
      aggregateToParent: true,
      aggregateLevel: "summary",
      notifyParent: false,
    },
    priority: 100,
  },

  // Rule 7: Agent checkpoint events -> TUI + FILE_DISPLAY
  {
    name: "agent-checkpoint",
    match: {
      types: [AgentMessageType.CHECKPOINT_CREATE, AgentMessageType.CHECKPOINT_RESTORE],
    },
    decision: {
      targets: [OutputTarget.TUI, OutputTarget.FILE_DISPLAY],
      aggregateToParent: true,
      aggregateLevel: "summary",
      notifyParent: false,
    },
    priority: 100,
  },

  // Rule 8: WorkflowExecution node events -> TUI + FILE_DISPLAY
  {
    name: "workflow-execution-node",
    match: {
      types: [
        WorkflowExecutionMessageType.NODE_START,
        WorkflowExecutionMessageType.NODE_END,
        WorkflowExecutionMessageType.NODE_ERROR,
        WorkflowExecutionMessageType.NODE_SKIP,
      ],
    },
    decision: {
      targets: [OutputTarget.TUI, OutputTarget.FILE_DISPLAY],
      aggregateToParent: false,
      aggregateLevel: "none",
      notifyParent: false,
    },
    priority: 100,
  },

  // Rule 9: WorkflowExecution lifecycle events -> TUI + FILE_DISPLAY
  {
    name: "workflow-execution-lifecycle",
    match: {
      types: [WorkflowExecutionMessageType.EXECUTION_START, WorkflowExecutionMessageType.EXECUTION_END],
    },
    decision: {
      targets: [OutputTarget.TUI, OutputTarget.FILE_DISPLAY],
      aggregateToParent: true,
      aggregateLevel: "summary",
      notifyParent: true,
    },
    priority: 100,
  },

  // Rule 10: WorkflowExecution fork branch -> FILE_DISPLAY + aggregate
  {
    name: "workflow-execution-fork-branch",
    match: {
      types: [WorkflowExecutionMessageType.FORK_BRANCH_START, WorkflowExecutionMessageType.FORK_BRANCH_END],
    },
    decision: {
      targets: [OutputTarget.FILE_DISPLAY],
      aggregateToParent: true,
      aggregateLevel: "summary",
      notifyParent: false,
    },
    priority: 100,
  },

  // Rule 11: Subgraph events -> FILE_DISPLAY + aggregate to parent
  {
    name: "subgraph-events",
    match: {
      categories: [MessageCategory.SUBGRAPH],
    },
    decision: {
      targets: [OutputTarget.FILE_DISPLAY],
      aggregateToParent: true,
      aggregateLevel: "detail",
      notifyParent: true,
    },
    priority: 100,
  },

  // Rule 12: Error messages -> TUI + FILE_DISPLAY
  {
    name: "error-messages",
    match: {
      levels: ["error", "critical"],
    },
    decision: {
      targets: [OutputTarget.TUI, OutputTarget.FILE_DISPLAY],
      aggregateToParent: true,
      aggregateLevel: "detail",
      notifyParent: true,
    },
    priority: 50, // High priority
  },

  // Rule 13: Default -> TUI
  {
    name: "default",
    match: {},
    decision: {
      targets: [OutputTarget.TUI],
      aggregateToParent: false,
      aggregateLevel: "none",
      notifyParent: false,
    },
    priority: 999,
  },
];
