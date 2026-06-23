/**
 * CLI Formatters
 * Formatting functions for CLI output
 */

import { getFormatter, Formatter } from "./formatter.js";
import type {
  WorkflowExecution,
  WorkflowExecutionResult,
  NodeTemplate,
  TriggerTemplate,
  NodeTemplateSummary,
  TriggerTemplateSummary,
  Checkpoint,
  LLMProfile,
  Script,
  Tool,
  Trigger,
  Message,
  Skill,
  AgentLoopResult,
  BaseEvent,
} from "@wf-agent/types";

// Get global formatter instance
function getGlobalFormatter(): Formatter {
  return getFormatter();
}

// Type alias for workflow summary objects (includes metadata like name, status, createdAt)
// Also supports template types for unified formatting
type WorkflowSummary = (
  | WorkflowExecution
  | WorkflowExecutionResult
  | NodeTemplate
  | TriggerTemplate
  | NodeTemplateSummary
  | TriggerTemplateSummary
  // Generic workflow-like objects from adapters (which may have broader type constraints)
  | ({ id?: string; type?: string; status?: string } & Record<string, unknown>)
) & {
  name?: string;
  status?: string;
  createdAt?: string | number;
};

// ============================================
// Workflow Formatters
// ============================================

export function formatWorkflow(workflow: WorkflowSummary, options?: { verbose?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (options?.verbose) {
    return formatter.json(workflow);
  }
  return formatter.workflow(workflow);
}

export function formatWorkflowList(
  workflows: WorkflowSummary[],
  options?: { table?: boolean; verbose?: boolean },
): string {
  const formatter = getGlobalFormatter();
  if (workflows.length === 0) {
    return "No workflow was found.";
  }

  if (options?.table) {
    const headers = ["ID", "Name", "Status", "Creation time"];
    const rows = workflows.map(w => {
      const id = 'id' in w ? w.id : (w as WorkflowExecutionResult).executionId;
      return [
        id?.substring(0, 8) || "N/A",
        w.name || "unnamed",
        w.status || "unknown",
        String(w.createdAt || "N/A"),
      ];
    });
    return formatter.table(headers, rows);
  }

  return workflows.map(w => formatWorkflow(w, options)).join("\n");
}

// ============================================
// Workflow Execution Formatters
// ============================================

export function formatWorkflowExecution(execution: WorkflowSummary, options?: { verbose?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (options?.verbose) {
    return formatter.json(execution);
  }
  return formatter.workflowExecution(execution);
}

export function formatWorkflowExecutionList(
  executions: WorkflowSummary[],
  options?: { table?: boolean; verbose?: boolean },
): string {
  const formatter = getGlobalFormatter();
  if (executions.length === 0) {
    return "No workflow execution found.";
  }

  if (options?.table) {
    const headers = ["Execution ID", "Workflow ID", "Status", "Creation time"];
    const rows = executions.map(e => {
      const id = 'id' in e ? e.id : (e as WorkflowExecutionResult).executionId;
      const workflowId = 'workflowId' in e ? String(e.workflowId ?? "") : undefined;
      return [
        String(id ?? "").substring(0, 8) || "N/A",
        workflowId?.substring(0, 8) || "N/A",
        e.status || "unknown",
        String(e.createdAt || "N/A"),
      ];
    });
    return formatter.table(headers, rows);
  }

  return executions.map(e => formatWorkflowExecution(e, options)).join("\n");
}

// ============================================
// Checkpoint Formatters
// ============================================

// Type alias for checkpoint with createdAt field
type CheckpointWithMetadata = Checkpoint & {
  createdAt?: string | number;
};

export function formatCheckpoint(checkpoint: CheckpointWithMetadata, options?: { verbose?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (options?.verbose) {
    return formatter.json(checkpoint);
  }
  return `${checkpoint.id || "N/A"} - ${checkpoint.executionId || "N/A"} - ${checkpoint.createdAt || "N/A"}`;
}

export function formatCheckpointList(checkpoints: CheckpointWithMetadata[], options?: { table?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (checkpoints.length === 0) {
    return "No checkpoints found.";
  }

  if (options?.table) {
    const headers = ["Checkpoint ID", "Execution ID", "Creation time"];
    const rows = checkpoints.map(c => [
      c.id?.substring(0, 8) || "N/A",
      c.executionId?.substring(0, 8) || "N/A",
      String(c.createdAt || "N/A"),
    ]);
    return formatter.table(headers, rows);
  }

  return checkpoints.map(c => formatCheckpoint(c)).join("\n");
}

// ============================================
// LLM Profile Formatters
// ============================================

export function formatLLMProfile(profile: LLMProfile, options?: { verbose?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (options?.verbose) {
    return formatter.json(profile);
  }

  const provider = profile.provider || "N/A";
  const model = profile.model || "N/A";
  const baseUrl = profile.baseUrl || "default";

  return `${profile.name || "unnamed"} (${profile.id || "N/A"}) - ${provider} - ${model} - ${baseUrl}`;
}

export function formatLLMProfileList(profiles: LLMProfile[], options?: { table?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (profiles.length === 0) {
    return "No LLM Profile found.";
  }

  if (options?.table) {
    const headers = ["ID", "Name", "Provider", "Model", "Basic URL"];
    const rows = profiles.map(p => [
      p.id?.substring(0, 8) || "N/A",
      p.name || "unnamed",
      p.provider || "N/A",
      p.model || "N/A",
      p.baseUrl || "default",
    ]);
    return formatter.table(headers, rows);
  }

  return profiles.map(p => formatLLMProfile(p)).join("\n");
}

// ============================================
// Script Formatters
// ============================================

// Type alias for script with language field
type ScriptWithLanguage = Script & {
  language?: string;
};

export function formatScript(script: ScriptWithLanguage, options?: { verbose?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (options?.verbose) {
    return formatter.json(script);
  }

  const language = script.language || "N/A";

  return `${script.name || "unnamed"} (${script.id || "N/A"}) - ${language}`;
}

export function formatScriptList(scripts: ScriptWithLanguage[], options?: { table?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (scripts.length === 0) {
    return "No script found.";
  }

  if (options?.table) {
    const headers = ["ID", "name", "multilingualism", "descriptive"];
    const rows = scripts.map(s => [
      s.id?.substring(0, 8) || "N/A",
      s.name || "unnamed",
      s.language || "N/A",
      s.description || "-",
    ]);
    return formatter.table(headers, rows);
  }

  return scripts.map(s => formatScript(s)).join("\n");
}

// ============================================
// Tool Formatters
// ============================================

// Type alias for tool with name field
type ToolWithName = Tool & {
  name?: string;
};

export function formatTool(tool: ToolWithName, options?: { verbose?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (options?.verbose) {
    return formatter.json(tool);
  }

  const type = tool.type || "N/A";
  return `${tool.name || "unnamed"} (${tool.id || "N/A"}) - ${type}`;
}

export function formatToolList(tools: ToolWithName[], options?: { table?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (tools.length === 0) {
    return "No tool was found.";
  }

  if (options?.table) {
    const headers = ["ID", "Name", "Type", "Description"];
    const rows = tools.map(t => [
      t.id?.substring(0, 8) || "N/A",
      t.name || "unnamed",
      t.type || "N/A",
      t.description || "-",
    ]);
    return formatter.table(headers, rows);
  }

  return tools.map(t => formatTool(t)).join("\n");
}

// ============================================
// Trigger Formatters
// ============================================

// Type alias for trigger with type and executionId fields
type TriggerWithMetadata = Trigger & {
  type?: string;
  executionId?: string;
};

export function formatTrigger(trigger: TriggerWithMetadata, options?: { verbose?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (options?.verbose) {
    return formatter.json(trigger);
  }

  const status = trigger.status || "unknown";
  const type = trigger.type || "N/A";

  return `${trigger.id || "N/A"} - ${status} - ${type} - ${trigger.executionId || "N/A"}`;
}

export function formatTriggerList(triggers: TriggerWithMetadata[], options?: { table?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (triggers.length === 0) {
    return "No trigger was found.";
  }

  if (options?.table) {
    const headers = ["Trigger ID", "Type", "Status", "Execution ID"];
    const rows = triggers.map(t => [
      t.id?.substring(0, 8) || "N/A",
      t.type || "N/A",
      t.status || "unknown",
      t.executionId?.substring(0, 8) || "N/A",
    ]);
    return formatter.table(headers, rows);
  }

  return triggers.map(t => formatTrigger(t)).join("\n");
}

// ============================================
// Message Formatters
// ============================================

// Helper function to extract text content from MessageContent
function getMessageText(content: Message): string {
  if (typeof content.content === "string") {
    return content.content;
  }
  // For array content, try to extract text from first text element
  if (Array.isArray(content.content)) {
    const textElement = content.content.find(item => item.type === "text");
    return textElement?.text || "";
  }
  return "";
}

export function formatMessage(message: Message, options?: { verbose?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (options?.verbose) {
    return formatter.json(message);
  }

  const role = message.role || "N/A";
  const content = getMessageText(message);
  const preview = content.length > 50 ? content.substring(0, 50) + "..." : content;

  return `${role}: ${preview}`;
}

export function formatMessageList(messages: Message[], options?: { table?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (messages.length === 0) {
    return "No message found.";
  }

  if (options?.table) {
    const headers = ["Role", "Content preview", "Time"];
    const rows = messages.map(m => {
      const content = getMessageText(m);
      const preview = content.length > 30 ? content.substring(0, 30) + "..." : content;
      return [m.role || "N/A", preview, String(m.timestamp || "N/A")];
    });
    return formatter.table(headers, rows);
  }

  return messages.map(m => formatMessage(m)).join("\n");
}

// ============================================
// Variable Formatters
// ============================================

export function formatVariable(name: string, value: unknown, options?: { verbose?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (options?.verbose) {
    return formatter.json({ name, value });
  }

  const valueStr = typeof value === "object" ? JSON.stringify(value, null, 2) : String(value);
  return formatter.keyValue(name, valueStr);
}

export function formatVariableList(
  variables: Record<string, unknown>,
  options?: { table?: boolean },
): string {
  const formatter = getGlobalFormatter();
  const entries = Object.entries(variables);

  if (entries.length === 0) {
    return "No variable was found.";
  }

  if (options?.table) {
    const headers = ["Variable name", "Value", "Type"];
    const rows = entries.map(([name, value]) => [
      name,
      typeof value === "object" ? JSON.stringify(value).substring(0, 30) + "..." : String(value),
      typeof value,
    ]);
    return formatter.table(headers, rows);
  }

  return entries.map(([name, value]) => formatVariable(name, value)).join("\n");
}

// ============================================
// Event Formatters
// ============================================

export function formatEvent(event: BaseEvent, options?: { verbose?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (options?.verbose) {
    return formatter.json(event);
  }

  const type = event.type || "N/A";
  const timestamp = event.timestamp || "N/A";
  const executionId = event.executionId || "N/A";

  return `${type} - ${timestamp} - ${executionId.substring(0, 8)}`;
}

export function formatEventList(events: BaseEvent[], options?: { table?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (events.length === 0) {
    return "No event found.";
  }

  if (options?.table) {
    const headers = ["Type", "Time", "Execution ID", "Workflow ID"];
    const rows = events.map(e => [
      e.type || "N/A",
      String(e.timestamp || "N/A"),
      e.executionId?.substring(0, 8) || "N/A",
      e.workflowId?.substring(0, 8) || "N/A",
    ]);
    return formatter.table(headers, rows);
  }

  return events.map(e => formatEvent(e)).join("\n");
}

// ============================================
// Agent Loop Formatters
// ============================================

// Type alias for agent loop with additional fields
type AgentLoopWithMetadata = AgentLoopResult & {
  id?: string;
  status?: string;
  currentIteration?: number;
  content?: string;
};

export function formatAgentLoop(agentLoop: AgentLoopWithMetadata, options?: { verbose?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (options?.verbose) {
    return formatter.json(agentLoop);
  }

  const id = agentLoop.id || "N/A";
  const status = agentLoop.status || agentLoop.success ? "completed" : "failed";
  const iterations = agentLoop.iterations ?? agentLoop.currentIteration ?? 0;
  const toolCallCount = agentLoop.toolCallCount ?? 0;

  let content = "";
  if (agentLoop.content) {
    content =
      agentLoop.content.length > 50
        ? agentLoop.content.substring(0, 50) + "..."
        : agentLoop.content;
  }

  return `${id} - ${status} - Iterations: ${iterations} - Tool calls: ${toolCallCount}${content ? `\n  Result: ${content}` : ""}`;
}

export function formatAgentLoopList(agentLoops: AgentLoopWithMetadata[], options?: { table?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (agentLoops.length === 0) {
    return "No Agent Loop found.";
  }

  if (options?.table) {
    const headers = ["ID", "Status", "Number of iterations", "Tool invocation"];
    const rows = agentLoops.map(al => [
      al.id?.substring(0, 8) || "N/A",
      al.status || "unknown",
      String(al.iterations ?? al.currentIteration ?? 0),
      String(al.toolCallCount ?? 0),
    ]);
    return formatter.table(headers, rows);
  }

  return agentLoops.map(al => formatAgentLoop(al)).join("\n");
}

// ============================================
// Skill Formatters
// ============================================

// Type alias for skill with version and description fields
type SkillWithMetadata = Skill & {
  name?: string;
  version?: string;
  description?: string;
};

export function formatSkill(skill: SkillWithMetadata, options?: { verbose?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (options?.verbose) {
    return formatter.json(skill);
  }

  const name = skill.name || "N/A";
  const version = skill.version || "N/A";
  const description = skill.description || "-";

  return `${name} (${version}) - ${description}`;
}

export function formatSkillList(skills: SkillWithMetadata[], options?: { table?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (skills.length === 0) {
    return "Skill not found.";
  }

  if (options?.table) {
    const headers = ["Name", "Version", "Description"];
    const rows = skills.map(s => [s.name || "N/A", s.version || "N/A", s.description || "-"]);
    return formatter.table(headers, rows);
  }

  return skills.map(s => formatSkill(s)).join("\n");
}
