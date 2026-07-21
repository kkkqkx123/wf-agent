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
    return formatWorkflowVerbose(workflow, formatter);
  }
  return formatter.workflow(workflow);
}

function formatWorkflowVerbose(workflow: WorkflowSummary, _formatter: Formatter): string {
  const lines: string[] = [];

  const id = 'id' in workflow ? workflow.id : (workflow as WorkflowExecutionResult).executionId;
  lines.push(`ID: ${id ?? "N/A"}`);
  lines.push(`Name: ${workflow.name ?? "Unnamed"}`);

  const type = 'type' in workflow ? (workflow as Record<string, unknown>)['type'] : undefined;
  if (type !== undefined) {
    lines.push(`Type: ${String(type)}`);
  }

  const version = 'version' in workflow ? (workflow as Record<string, unknown>)['version'] : undefined;
  if (version !== undefined) {
    lines.push(`Version: ${String(version)}`);
  }

  const description = 'description' in workflow ? (workflow as Record<string, unknown>)['description'] : undefined;
  if (description !== undefined && description !== "") {
    lines.push(`Description: ${String(description)}`);
  }

  lines.push(`Status: ${workflow.status ?? "unknown"}`);
  lines.push(`Creation time: ${String(workflow.createdAt ?? "N/A")}`);

  const updatedAt = 'updatedAt' in workflow ? (workflow as Record<string, unknown>)['updatedAt'] : undefined;
  if (updatedAt !== undefined && updatedAt !== "") {
    lines.push(`Update time: ${String(updatedAt)}`);
  }

  const nodes = 'nodes' in workflow ? (workflow as Record<string, unknown>)['nodes'] : undefined;
  if (Array.isArray(nodes)) {
    lines.push(`Number of nodes: ${nodes.length}`);
  }

  const edges = 'edges' in workflow ? (workflow as Record<string, unknown>)['edges'] : undefined;
  if (Array.isArray(edges)) {
    lines.push(`Number of sides: ${edges.length}`);
  }

  const triggers = 'triggers' in workflow ? (workflow as Record<string, unknown>)['triggers'] : undefined;
  if (Array.isArray(triggers)) {
    lines.push(`Number of triggers: ${triggers.length}`);
  }

  return lines.join("\n");
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
    const headers = ["ID", "Name", "Type", "Version", "Status", "Creation time"];
    const rows = workflows.map(w => {
      const id = 'id' in w ? w.id : (w as WorkflowExecutionResult).executionId;
      const type = 'type' in w ? String((w as Record<string, unknown>)['type'] ?? "") : "";
      const version = 'version' in w ? String((w as Record<string, unknown>)['version'] ?? "") : "";
      return [
        id?.substring(0, 8) || "N/A",
        w.name || "unnamed",
        type,
        version,
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

// ============================================
// Trigger Template Formatters
// ============================================

export function formatTriggerTemplate(template: TriggerTemplate, options?: { verbose?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (options?.verbose) {
    return formatter.json(template);
  }

  const name = template.name || "N/A";
  const description = template.description || "-";
  const conditionType = template.condition?.eventType || "N/A";
  const actionType = template.action?.type || "N/A";

  return `${name} - ${description} (${conditionType} -> ${actionType})`;
}

export function formatTriggerTemplateList(
  templates: TriggerTemplateSummary[],
  options?: { table?: boolean },
): string {
  const formatter = getGlobalFormatter();
  if (templates.length === 0) {
    return "No trigger template was found.";
  }

  if (options?.table) {
    const headers = ["Name", "Description", "Category", "Created At"];
    const rows = templates.map(t => [
      t.name,
      t.description?.substring(0, 40) || "-",
      t.category || "-",
      String(t.createdAt).substring(0, 10),
    ]);
    return formatter.table(headers, rows);
  }

  return templates.map(t => formatTriggerTemplateSummary(t)).join("\n");
}

export function formatTriggerTemplateSummary(
  summary: TriggerTemplateSummary,
  options?: { verbose?: boolean },
): string {
  const formatter = getGlobalFormatter();
  if (options?.verbose) {
    return formatter.json(summary);
  }

  const name = summary.name || "N/A";
  const description = summary.description || "-";
  const category = summary.category ? ` [${summary.category}]` : "";

  return `${name}${category} - ${description}`;
}

// ============================================
// Plugin Formatters
// ============================================

export function formatPlugin(plugin: Record<string, unknown>, options?: { verbose?: boolean; json?: boolean }): string {
  const formatter = getGlobalFormatter();
  if (options?.json || options?.verbose) {
    return formatter.json(plugin);
  }

  const manifest = plugin["manifest"] as Record<string, unknown> | undefined;
  const name = (manifest?.["name"] as string) || (manifest?.["id"] as string) || "N/A";
  const version = (manifest?.["version"] as string) || "N/A";
  const status = (plugin["status"] as string) || "unknown";
  const description = (manifest?.["description"] as string) || "-";

  return `${name} (${version}) - ${status} - ${description}`;
}

export function formatPluginList(
  plugins: Record<string, unknown>[],
  options?: { table?: boolean; verbose?: boolean },
): string {
  const formatter = getGlobalFormatter();
  if (plugins.length === 0) {
    return "No plugin was found.";
  }

  if (options?.table) {
    const headers = ["ID", "Name", "Version", "Status", "Description"];
    const rows = plugins.map(p => {
      const manifest = p["manifest"] as Record<string, unknown> | undefined;
      return [
        (manifest?.["id"] as string)?.substring(0, 16) || "N/A",
        (manifest?.["name"] as string) || "N/A",
        (manifest?.["version"] as string) || "N/A",
        (p["status"] as string) || "unknown",
        ((manifest?.["description"] as string) || "-").substring(0, 40),
      ];
    });
    return formatter.table(headers, rows);
  }

  return plugins.map(p => formatPlugin(p, { verbose: options?.verbose })).join("\n");
}
