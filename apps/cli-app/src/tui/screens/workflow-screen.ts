/**
 * Workflow Management Screen
 * Provides visual interface for workflow CRUD operations
 */

import { Box, Container, Text, SelectList } from "../core/index.js";
import type { Screen } from "./screen.js";
import { WorkflowAdapter } from "../../adapters/workflow-adapter.js";

export class WorkflowScreen implements Screen {
  private container: Container;
  private workflowList: SelectList;
  private detailPanel: Box;
  private adapter: WorkflowAdapter;
  private currentWorkflowId?: string;
  private onBack?: () => void;

  constructor(onBack?: () => void) {
    this.onBack = onBack;
    this.adapter = new WorkflowAdapter();
    this.container = new Container();
    this.workflowList = new SelectList([]);
    this.detailPanel = new Box();
    
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

    this.container.addChild(toolbar);
    this.container.addChild(splitContainer);
  }

  private async loadWorkflows() {
    try {
      const workflows = await this.adapter.listWorkflows();
      const items = workflows.map((w: any) => ({
        value: w.id,
        label: w.name,
        description: `${w.version || "v1.0"} | ${w.status}`,
      }));
      
      this.workflowList.setItems(items);
      
      // Set up selection handler
      this.workflowList.onSelect = async (item) => {
        await this.showWorkflowDetail(item.value);
      };
    } catch (error) {
      console.error("Failed to load workflows:", error);
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
        ...(workflow.nodes || []).map((n: any) => `  - ${n.id} (${n.type})`),
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
        new Text(`Error loading workflow: ${error instanceof Error ? error.message : String(error)}`, 1, 0)
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
      console.log("New workflow - to be implemented");
      return true;
    }
    
    if (data === "d" || data === "D" && this.currentWorkflowId) {
      // TODO: Implement delete with confirmation
      console.log("Delete workflow - to be implemented");
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
    // Cleanup if needed
  }
}
