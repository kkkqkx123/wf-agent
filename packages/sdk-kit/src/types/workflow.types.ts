/**
 * Workflow-related types for SDK-Kit
 */

/**
 * Node configuration
 */
export interface NodeConfig {
  type: string;
  config?: Record<string, unknown>;
  name?: string;
  description?: string;
}

/**
 * Edge condition for workflow routing
 */
export interface EdgeCondition {
  condition?: string;
  [key: string]: unknown;
}

/**
 * Node in workflow template
 */
export interface Node {
  id: string;
  type: string;
  config: Record<string, unknown>;
  name?: string;
  description?: string;
}

/**
 * Edge in workflow template
 */
export interface Edge {
  from: string;
  to: string;
  condition?: EdgeCondition;
}

/**
 * Workflow template
 */
export interface WorkflowTemplate {
  id: string;
  version: string;
  name?: string;
  description?: string;
  nodes: Node[];
  edges: Edge[];
  metadata?: Record<string, unknown>;
}
