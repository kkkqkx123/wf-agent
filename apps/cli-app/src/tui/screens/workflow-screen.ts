/**
 * Workflow Management Screen
 * Provides visual interface for workflow CRUD operations
 */

import { Box, Container, Text, SelectList } from "../core/index.js";
import type { Screen } from "./screen.js";
import { WorkflowAdapter } from "../../adapters/workflow-adapter.js";
import { createContextualLogger } from "@wf-agent/sdk/utils";

export class WorkflowScreen implements Screen {
  private container: Container;
  private workflowList: SelectList;
  private detailPanel: Box;
  private logPanel: Box;
  private adapter: WorkflowAdapter;
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

    // Execution log panel
    const logBox = new Box();
    logBox.addChild(new Text("Execution Log:", 1, 0));
    logBox.addChild(this.logPanel);

    this.container.addChild(toolbar);
    this.container.addChild(splitContainer);
    this.container.addChild(logBox);
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
        description: w.description || "",
      }));
      this.workflowList.setItems(items);

      // Auto-select first workflow
      if (items.length > 0) {
        this.selectWorkflow(items[0]!.value);
      }
    } catch (error) {
      this.logger.error("Failed to load workflows", {}, undefined, error as Error);
      this.appendLog("Failed to load workflows", "error");
    }
  }

  private async selectWorkflow(workflowId: string) {
    this.detailPanel.clear();

    try {
      const workflow = await this.adapter.getWorkflow(workflowId);
      if (workflow) {
        this.detailPanel.addChild(new Text(`Name: ${workflow.name}`));
        this.detailPanel.addChild(new Text(`Description: ${workflow.description || "N/A"}`));
        this.detailPanel.addChild(new Text(`Version: ${workflow.version || "1.0"}`));
        this.detailPanel.addChild(
          new Text(`Created: ${new Date(workflow.createdAt).toLocaleDateString()}`),
        );
      }
    } catch (error) {
      this.logger.error("Failed to load workflow details", {}, undefined, error as Error);
      this.detailPanel.addChild(new Text("Failed to load workflow details"));
    }
  }

  render(): Container {
    return this.container;
  }

  handleInput(data: string): boolean {
    if (data === "b" || data === "B") {
      this.onBack?.();
      return true;
    }

    if (data === "r" || data === "R") {
      this.loadWorkflows();
      return true;
    }

    if (this.workflowList.handleInput) {
      this.workflowList.handleInput(data);
      return true;
    }

    return false;
  }

  destroy(): void {
    // Cleanup
  }
}