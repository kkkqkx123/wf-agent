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

  // Rule 2: Agent Human Relay request -> TUI + FILE_FUNCTIONAL + FILE_DISPLAY
  {
    name: "agent-human-relay-request",
    match: {
      types: [AgentMessageType.HUMAN_RELAY_REQUEST],
    },
    decision: {
      targets: [OutputTarget.TUI, OutputTarget.FILE_FUNCTIONAL, OutputTarget.FILE_DISPLAY],
      aggregateToParent: true,
      aggregateLevel: "summary",
      notifyParent: true,
    },
    priority: 100,
  },

  // Rule 3: Agent Human Relay response/timeout/cancel -> TUI + FILE_DISPLAY
  {
    name: "agent-human-relay-status",
    match: {
      types: [
        AgentMessageType.HUMAN_RELAY_RESPONSE,
        AgentMessageType.HUMAN_RELAY_TIMEOUT,
        AgentMessageType.HUMAN_RELAY_CANCEL,
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

  // Rule 5: WorkflowExecution node events -> TUI + FILE_DISPLAY
  {
    name: "workflow-execution-node",
    match: {
      types: [WorkflowExecutionMessageType.NODE_START, WorkflowExecutionMessageType.NODE_END],
    },
    decision: {
      targets: [OutputTarget.TUI, OutputTarget.FILE_DISPLAY],
      aggregateToParent: false,
      aggregateLevel: "none",
      notifyParent: false,
    },
    priority: 100,
  },

  // Rule 6: WorkflowExecution fork branch -> FILE_DISPLAY + aggregate
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

  // Rule 7: Subgraph events -> FILE_DISPLAY + aggregate to parent
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

  // Rule 8: Error messages -> TUI + FILE_DISPLAY
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

  // Rule 9: Default -> TUI
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
