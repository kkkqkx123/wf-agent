import type { ToolDescriptionData, ToolParameterDescription } from "@wf-agent/types";

function renderParameter(param: ToolParameterDescription): string {
  const required = param.required ? "required" : "optional";
  const defaultValue = param.defaultValue !== undefined ? `, default: ${param.defaultValue}` : "";
  return `  - ${param.name} (${param.type}, ${required}${defaultValue}): ${param.description}`;
}

function renderParameters(parameters: ToolParameterDescription[]): string {
  if (parameters.length === 0) {
    return "  None";
  }
  return parameters.map(renderParameter).join("\n");
}

function renderTips(tips: string[] | undefined): string {
  if (!tips || tips.length === 0) {
    return "";
  }
  const tipLines = tips.map(tip => `  - ${tip}`).join("\n");
  return `\nTips:\n${tipLines}`;
}

function renderExamples(examples: string[] | undefined): string {
  if (!examples || examples.length === 0) {
    return "";
  }
  const exampleLines = examples.map(example => `  - ${example}`).join("\n");
  return `\nExamples:\n${exampleLines}`;
}

export function renderToolDescription(data: ToolDescriptionData): string {
  const parts: string[] = [
    `Tool: ${data.id}`,
    data.description,
    "",
    "Parameters:",
    renderParameters(data.parameters),
    renderTips(data.tips),
    renderExamples(data.examples),
  ];

  return parts.filter(part => part !== "").join("\n");
}

export function renderToolDescriptionSingleLine(data: ToolDescriptionData): string {
  return `${data.id}: ${data.description}`;
}

export function renderToolDescriptionListItem(data: ToolDescriptionData): string {
  return `- ${data.id}: ${data.description}`;
}

export function renderToolDescriptionTableRow(data: ToolDescriptionData): string {
  return `| ${data.id} | ${data.description} |`;
}

export function renderToolDescriptions(
  tools: ToolDescriptionData[],
  format: "default" | "single-line" | "list" | "table" = "default",
): string {
  switch (format) {
    case "single-line":
      return tools.map(renderToolDescriptionSingleLine).join("\n");
    case "list":
      return tools.map(renderToolDescriptionListItem).join("\n");
    case "table":
      return tools.map(renderToolDescriptionTableRow).join("\n");
    case "default":
    default:
      return tools.map(renderToolDescription).join("\n\n");
  }
}
