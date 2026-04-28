import { TestHelper } from "../utils/index.js";
import { copyFileSync } from "fs";

/**
 * Workflow test helper class for common workflow testing operations
 */
export class WorkflowTestHelper {
  private helper: TestHelper;

  constructor(helper: TestHelper) {
    this.helper = helper;
  }

  /**
   * Write a workflow fixture file
   */
  async writeWorkflowFixture(filename: string, content: string): Promise<string> {
    return await this.helper.writeFixture(filename, content, "workflows");
  }

  /**
   * Read a workflow fixture file
   */
  readWorkflowFixture(filename: string): string {
    return this.helper.readFixture("workflows", filename);
  }

  /**
   * Check if a workflow fixture exists
   */
  existsWorkflowFixture(filename: string): boolean {
    return this.helper.existsFixture("workflows", filename);
  }

  /**
   * Copy a static workflow fixture to temp directory for test isolation
   * Returns the path to the copied file in temp directory
   */
  copyWorkflowFixtureToTemp(filename: string, targetFilename?: string): string {
    const { resolve } = require("path");
    const sourcePath = this.helper.getFixturePath("workflows", filename);
    const destFilename = targetFilename || filename;
    const destPath = resolve(this.helper.getTempDir(), destFilename);
    copyFileSync(sourcePath, destPath);
    return destPath;
  }

  /**
   * Write a workflow configuration to temp directory
   * Use this for dynamic workflow generation that needs test isolation
   */
  async writeWorkflowToTemp(filename: string, content: string): Promise<string> {
    return await this.helper.writeTempFile(filename, content);
  }

  /**
   * Create a minimal standalone workflow configuration
   */
  createMinimalWorkflow(id: string, name: string): string {
    return `[workflow]
id = "${id}"
name = "${name}"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "end"`;
  }

  /**
   * Create a standalone workflow with LLM node
   */
  createStandaloneWorkflowWithLLM(
    id: string,
    name: string,
    llmProfileId: string = "gpt-4o",
  ): string {
    return `[workflow]
id = "${id}"
name = "${name}"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "llm"
type = "LLM"
config = { profileId = "${llmProfileId}" }

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "llm"

[[edges]]
from = "llm"
to = "end"`;
  }

  /**
   * Create a triggered subworkflow
   */
  createTriggeredSubworkflow(id: string, name: string, llmProfileId: string = "gpt-4o"): string {
    return `[workflow]
id = "${id}"
name = "${name}"
type = "TRIGGERED_SUBWORKFLOW"
version = "1.0.0"

[[nodes]]
id = "start_from_trigger"
type = "START_FROM_TRIGGER"

[[nodes]]
id = "process"
type = "LLM"
config = { profileId = "${llmProfileId}" }

[[nodes]]
id = "continue_from_trigger"
type = "CONTINUE_FROM_TRIGGER"

[[edges]]
from = "start_from_trigger"
to = "process"

[[edges]]
from = "process"
to = "continue_from_trigger"`;
  }

  /**
   * Create a dependent workflow with SUBGRAPH node
   */
  createDependentWorkflow(id: string, name: string, childWorkflowId: string): string {
    return `[workflow]
id = "${id}"
name = "${name}"
type = "DEPENDENT"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "subgraph"
type = "SUBGRAPH"
config = { workflowId = "${childWorkflowId}" }

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "subgraph"

[[edges]]
from = "subgraph"
to = "end"`;
  }

  /**
   * Create a workflow with trigger
   */
  createWorkflowWithTrigger(
    id: string,
    name: string,
    triggerId: string,
    triggerName: string,
  ): string {
    return `[workflow]
id = "${id}"
name = "${name}"
type = "STANDALONE"
version = "1.0.0"

[[triggers]]
id = "${triggerId}"
name = "${triggerName}"
enabled = true

[triggers.condition]
eventType = "NODE_COMPLETED"

[triggers.action]
type = "set_variable"

[triggers.action.parameters]
threadId = "current"

[triggers.action.parameters.variables]
status = "completed"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "end"`;
  }

  /**
   * Create a parameterized workflow
   */
  createParameterizedWorkflow(): string {
    return `[workflow]
id = "{{parameters.workflow_id}}"
name = "{{parameters.workflow_name}}"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "process"
type = "LLM"
config = { profileId = "{{parameters.llm_profile_id}}" }

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "process"

[[edges]]
from = "process"
to = "end"`;
  }

  /**
   * Create an invalid workflow - missing required fields
   */
  createInvalidWorkflowMissingFields(): string {
    return `[workflow]
# 缺少 id
# 缺少 name
type = "STANDALONE"
version = "1.0.0"`;
  }

  /**
   * Create an invalid workflow - invalid node type
   */
  createInvalidWorkflowInvalidNodeType(): string {
    return `[workflow]
id = "invalid-node-type-wf"
name = "Invalid Node Type Workflow"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "INVALID_TYPE"`;
  }

  /**
   * Create an invalid workflow - duplicate node IDs
   */
  createInvalidWorkflowDuplicateNode(): string {
    return `[workflow]
id = "duplicate-node-wf"
name = "Duplicate Node Workflow"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "start"
type = "END"`;
  }

  /**
   * Create an invalid workflow - invalid edge reference
   */
  createInvalidWorkflowInvalidEdge(): string {
    return `[workflow]
id = "invalid-edge-wf"
name = "Invalid Edge Workflow"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[edges]]
from = "start"
to = "nonexistent"`;
  }

  /**
   * Create an invalid workflow - multiple START nodes
   */
  createInvalidWorkflowMultipleStart(): string {
    return `[workflow]
id = "multiple-start-wf"
name = "Multiple Start Workflow"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start1"
type = "START"

[[nodes]]
id = "start2"
type = "START"`;
  }

  /**
   * Create an invalid workflow - multiple END nodes
   */
  createInvalidWorkflowMultipleEnd(): string {
    return `[workflow]
id = "multiple-end-wf"
name = "Multiple End Workflow"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "end1"
type = "END"

[[nodes]]
id = "end2"
type = "END"`;
  }

  /**
   * Create workflow A for circular reference test
   */
  createCircularWorkflowA(): string {
    return `[workflow]
id = "workflow-a"
name = "Workflow A"
type = "DEPENDENT"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "subgraph"
type = "SUBGRAPH"
config = { workflowId = "workflow-b" }

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "subgraph"

[[edges]]
from = "subgraph"
to = "end"`;
  }

  /**
   * Create workflow B for circular reference test
   */
  createCircularWorkflowB(): string {
    return `[workflow]
id = "workflow-b"
name = "Workflow B"
type = "DEPENDENT"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "subgraph"
type = "SUBGRAPH"
config = { workflowId = "workflow-a" }

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "subgraph"

[[edges]]
from = "subgraph"
to = "end"`;
  }

  /**
   * Create child workflow for dependency tests
   */
  createChildWorkflow(): string {
    return `[workflow]
id = "child-wf"
name = "Child Workflow"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "child-start"
type = "START"

[[nodes]]
id = "child-process"
type = "LLM"
config = { llmProfileId = "gpt-4o" }

[[nodes]]
id = "child-end"
type = "END"

[[edges]]
from = "child-start"
to = "child-process"

[[edges]]
from = "child-process"
to = "child-end"`;
  }

  /**
   * Extract workflow ID from CLI output
   */
  extractWorkflowId(output: string, pattern: RegExp = /工作流已注册: ([\w-]+)/): string | null {
    return this.helper.extractId(output, pattern);
  }

  /**
   * Extract workflow ID from show command output
   */
  extractWorkflowIdFromShow(output: string): string | null {
    const match = output.match(/ID: ([\w-]+)/);
    return match ? (match[1] ?? null) : null;
  }

  /**
   * Check if output contains workflow registration success message
   */
  isRegistrationSuccessful(output: string): boolean {
    return output.includes("Workflow is registered");
  }

  /**
   * Check if output contains workflow deletion success message
   */
  isDeletionSuccessful(output: string): boolean {
    return output.includes("Workflow deleted");
  }

  /**
   * Check if output contains error message
   */
  hasErrorMessage(output: string): boolean {
    return output.includes("incorrect") || output.toLowerCase().includes("error");
  }

  /**
   * Extract error message from output
   */
  extractErrorMessage(output: string): string | null {
    const match = output.match(/错误[:：]\s*(.+)/);
    return match ? (match[1]?.trim() ?? null) : null;
  }

  /**
   * Parse workflow list from output
   */
  parseWorkflowList(
    output: string,
  ): Array<{ id: string; name: string; type: string; status: string }> {
    const workflows: Array<{ id: string; name: string; type: string; status: string }> = [];
    const lines = output.split("\n");

    // Skip header lines
    let dataStart = false;
    for (const line of lines) {
      if (line.includes("---")) {
        dataStart = true;
        continue;
      }
      if (dataStart && line.trim()) {
        // Parse workflow line (tab or space separated)
        const parts = line.trim().split(/\s{2,}|\t+/);
        if (parts.length >= 4) {
          workflows.push({
            id: parts[0] ?? "",
            name: parts[1] ?? "",
            type: parts[2] ?? "",
            status: parts[3] ?? "",
          });
        }
      }
    }

    return workflows;
  }
}

/**
 * Create a workflow test helper instance
 */
export function createWorkflowTestHelper(helper: TestHelper): WorkflowTestHelper {
  return new WorkflowTestHelper(helper);
}
