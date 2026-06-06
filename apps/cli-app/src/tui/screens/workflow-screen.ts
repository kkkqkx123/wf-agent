/**
 * Workflow Management Screen
 * Provides visual interface for workflow CRUD operations
 */

import { Box, Container, Text, SelectList } from "../core/index.js";
import type { Screen } from "./screen.js";
import { WorkflowAdapter } from "../../adapters/workflow-adapter.js";
import { MessageCategory, WorkflowExecutionMessageType } from "@wf-agent/types";
import type { BaseComponentMessage, WorkflowExecutionNodeData } from "@wf-agent/types";
import { createContextualLogger } from "@wf-agent/sdk/utils";

export class WorkflowScreen implements Screen {
  private container: Container;
  private workflowList: SelectList;
  private detailPanel: Box;
  private logPanel: Box;
  private adapter: WorkflowAdapter;
  private currentWorkflowId?: string;
  private onBack?: () => void;
  private logger = createContextualLogger({ component: "WorkflowScreen" });

  constructor(onBack?: () => void) {
    this.onBack = onBack;
    this.adapter = new WorkflowAdapter();
    this.container = new Container();
    this.workflowList = new SelectList([]);
    this.detailPanel = new Box();
    this.logPanel = new Box();

    this.setupLayout();
    this.loadWorkflows();
  }

  private setupLayout() {
    // Toolbar
    const toolbar = new Box();
    toolbar.addChild(new Text("[N]ew  [E]dit  [D]elete  [R]efresh  [B]ack", 1, 0));

    // Split layout: left list, right details
    const splitContainer = new Container();

    // Workflow list
    const listBox = new Box();
    listBox.addChild(new Text("Workflows:", 1, 0));
    listBox.addChild(this.workflowList);

    // Detail panel
    const detailBox = new Box();
    detailBox.addChild(new Text("Workflow Details:", 1, 0));
    detailBox.addChild(this.detailPanel);

    splitContainer.addChild(listBox);
    splitContainer.addChild(detailBox);

    // Execution log panel (separate from detail panel to avoid log loss on CRUD operations)
    const logBox = new Box();
    logBox.addChild(new Text("Execution Log:", 1, 0));
    logBox.addChild(this.logPanel);

    this.container.addChild(toolbar);
    this.container.addChild(splitContainer);
    this.container.addChild(logBox);
  }

  /**
   * Screen message handler - called by TUI dispatcher for WORKFLOW_EXECUTION messages.
   */
  onMessage(message: BaseComponentMessage): void {
    if (message.category !== MessageCategory.WORKFLOW_EXECUTION) return;

    const nodeTypes: string[] = [
      WorkflowExecutionMessageType.NODE_START,
      WorkflowExecutionMessageType.NODE_END,
      WorkflowExecutionMessageType.NODE_ERROR,
      WorkflowExecutionMessageType.NODE_SKIP,
    ] as string[];

    if (nodeTypes.includes(message.type as string)) {
      this.handleNodeMessage(message);
    } else if (
      message.type === WorkflowExecutionMessageType.EXECUTION_START ||
      message.type === WorkflowExecutionMessageType.EXECUTION_END
    ) {
      this.handleWorkflowMessage(message);
    }
  }

  /**
   * Handle node execution messages
   */
  private handleNodeMessage(message: BaseComponentMessage) {
    const data = message.data as WorkflowExecutionNodeData;

    switch (message.type) {
      case WorkflowExecutionMessageType.NODE_START:
        this.appendLog(`Node started: ${data.nodeId} (${data.nodeType})`, "system");
        break;

      case WorkflowExecutionMessageType.NODE_END:
        this.appendLog(`Node completed: ${data.nodeId} (${data.duration}ms)`, "system");
        break;

      case WorkflowExecutionMessageType.NODE_ERROR:
        this.appendLog(`Node error: ${data.nodeId} - ${data.error}`, "error");
        break;

      case WorkflowExecutionMessageType.NODE_SKIP:
        this.appendLog(`Node skipped: ${data.nodeId}`, "system");
        break;
    }
  }

  /**
   * Handle workflow lifecycle messages
   */
  private handleWorkflowMessage(message: BaseComponentMessage) {
    if (message.type === WorkflowExecutionMessageType.EXECUTION_START) {
      this.appendLog("Workflow execution started", "system");
    } else if (message.type === WorkflowExecutionMessageType.EXECUTION_END) {
      this.appendLog("Workflow execution ended", "system");
    }
  }

  /**
   * Append log entry to detail panel
   */
  private appendLog(message: string, type: "system" | "error" = "system") {
    const timestamp = new Date().toLocaleTimeString();
    const icon = type === "error" ? "❌" : "ℹ️";
    const logEntry = `[${timestamp}] ${icon} ${message}`;

    this.logPanel.addChild(new Text(logEntry, 1, 0));
  }

  private async loadWorkflows() {
    try {
      const workflows = await this.adapter.listWorkflows();
      const items = workflows.map(w => ({
        value: w.id,
        label: w.name,
        description: `${w.version || "v1.0"} | ${w.status}`,
      }));

      this.workflowList.setItems(items);

      // Set up selection handler
      this.workflowList.onSelect = async item => {
        await this.showWorkflowDetail(item.value);
      };
    } catch (error) {
      this.logger.error(
        "Failed to load workflows",
        {},
        {
          error: error instanceof Error ? error.message : String(error),
        },
      );
    }
  }

  private async showWorkflowDetail(id: string) {
    try {
      this.currentWorkflowId = id;
      const workflow = await this.adapter.getWorkflow(id);

      // Clear and update detail panel
      this.detailPanel.clear();

      // Format workflow details as plain text (no Markdown)
      const details = [
        `ID: ${workflow.id}`,
        `Name: ${workflow.name}`,
        `Version: ${workflow.version || "1.0"}`,
        `Description: ${workflow.description || "N/A"}`,
        "",
        "Nodes:",
        ...(workflow.nodes || []).map(n => `  - ${n.id} (${n.type})`),
        "",
        `Created: ${workflow.createdAt || "N/A"}`,
        `Updated: ${workflow.updatedAt || "N/A"}`,
      ];

      details.forEach(line => {
        this.detailPanel.addChild(new Text(line, 1, 0));
      });
    } catch (error) {
      this.detailPanel.clear();
      this.detailPanel.addChild(
        new Text(
          `Error loading workflow: ${error instanceof Error ? error.message : String(error)}`,
          1,
          0,
        ),
      );
    }
  }

  render(): Container {
    return this.container;
  }

  handleInput(data: string): boolean {
    // Handle toolbar shortcuts
    if (data === "b" || data === "B") {
      this.onBack?.();
      return true;
    }

    if (data === "r" || data === "R") {
      this.loadWorkflows();
      return true;
    }

    if (data === "n" || data === "N") {
      // TODO: Implement new workflow dialog
      this.logger.info("New workflow - to be implemented");
      return true;
    }

    if (data === "d" || (data === "D" && this.currentWorkflowId)) {
      // TODO: Implement delete with confirmation
      this.logger.info("Delete workflow - to be implemented");
      return true;
    }

    // Delegate to workflow list
    if (this.workflowList.handleInput) {
      this.workflowList.handleInput(data);
      return true;
    }

    return false;
  }

  destroy(): void {
    // No-op: subscriptions are managed by the TUI app
  }
}
