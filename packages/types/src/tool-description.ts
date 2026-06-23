export interface ToolParameterDescription {
  name: string;
  type: string;
  required: boolean;
  description: string;
  defaultValue?: unknown;
}

export interface ToolDescriptionData {
  id: string;
  type: "STATELESS" | "STATEFUL";
  category?:
    | "filesystem"
    | "shell"
    | "memory"
    | "code"
    | "http"
    | "workflow"
    | "agent"
    | "integration"
    | "interaction";
  description: string;
  parameters: ToolParameterDescription[];
  tips?: string[];
  examples?: string[];
}