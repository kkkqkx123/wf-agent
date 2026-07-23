/**
 * Workflow Management Screen
 * Provides visual interface for workflow CRUD operations
 */

import { Box, Container, Text, SelectList } from "../core/index.js";
import type { Screen } from "./screen.js";
import type { Component, TUI, OverlayHandle } from "../core/tui.js";
import { WorkflowAdapter } from "../../adapters/workflow-adapter.js";
import { WorkflowGraphAdapter } from "../../adapters/workflow-graph-adapter.js";
import { createContextualLogger } from "@wf-agent/sdk/utils";

/**
 * A simple Box-shaped component that renders fixed text lines.
 * Used for confirmation overlays.
 */
class ConfirmBox implements Component {
  private lines: string[];
  constructor(lines: string[]) {
    this.lines = lines;
  }
  render(_width: number): string[] {
    return this.lines;
  }
  invalidate(): void {}
}

export class WorkflowScreen implements Screen {
  private container: Container;
  private workflowList: SelectList;
  private detailPanel: Box;
  private graphPanel: Box;
  private logPanel: Box;
  private adapter: WorkflowAdapter;
  private graphAdapter: WorkflowGraphAdapter;
  private onBack?: () => void;
  private tui: TUI;
  private logger = createContextualLogger({ component: "WorkflowScreen" });

  /** Currently selected workflow id */
  private selectedWorkflowId: string | null = null;

  constructor(onBack?: () => void, tui?: TUI) {
    this.onBack = onBack;
    this.tui = tui!;
    this.adapter = new WorkflowAdapter();
    this.graphAdapter = new WorkflowGraphAdapter();
    this.container = new Container();
    this.workflowList = new SelectList([]);
    this.detailPanel = new Box();
    this.graphPanel = new Box();
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

    // Graph preview panel
    const graphBox = new Box();
    graphBox.addChild(new Text("Graph Preview:", 1, 0));
    graphBox.addChild(this.graphPanel);

    splitContainer.addChild(listBox);
    splitContainer.addChild(detailBox);
    splitContainer.addChild(graphBox);

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
    this.selectedWorkflowId = workflowId;
    this.detailPanel.clear();
    this.graphPanel.clear();

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

      // Load graph preview
      await this.loadGraphPreview(workflowId);
    } catch (error) {
      this.logger.error("Failed to load workflow details", {}, undefined, error as Error);
      this.detailPanel.addChild(new Text("Failed to load workflow details"));
    }
  }

  /**
   * Load and display graph structure as an indented tree.
   */
  private async loadGraphPreview(workflowId: string) {
    this.graphPanel.clear();

    try {
      const [nodes, edges] = await Promise.all([
        this.graphAdapter.getNodes(workflowId),
        this.graphAdapter.getEdges(workflowId),
      ]);

      if (!nodes || nodes.length === 0) {
        this.graphPanel.addChild(new Text("  (empty graph)", 1, 0));
        return;
      }

      // Display nodes as an indented list with edge counts
      let startNode: string | null = null;
      for (const node of nodes as Array<{ id: string; type?: string; label?: string }>) {
        const nodeLabel = node.label || node.type || node.id;
        if (!startNode) startNode = node.id;

        // Count outgoing edges
        const outEdges = (edges as unknown as Array<{ sourceNodeId: string; targetNodeId: string; condition?: string }>)
          ?.filter((e) => e.sourceNodeId === node.id) ?? [];

        const edgeCount = outEdges.length;
        const connections = edgeCount > 0 ? ` -> ${outEdges.map((e) => e.targetNodeId).join(", ")}` : "";
        this.graphPanel.addChild(
          new Text(`  ${node.id}: ${nodeLabel}${connections}`, 1, 0),
        );
      }

      // Show edge count summary
      if (edges && (edges as Array<unknown>).length > 0) {
        this.graphPanel.addChild(
          new Text(`  Total edges: ${(edges as Array<unknown>).length}`, 1, 0),
        );
      }
    } catch {
      this.graphPanel.addChild(new Text("  (graph preview unavailable)", 1, 0));
    }
  }

  /**
   * Show a confirmation overlay before a destructive operation.
   */
  private async confirmOperation(
    action: string,
    workflowName: string,
  ): Promise<boolean> {
    return new Promise<boolean>((resolve) => {
      const overlayLines = [
        `┌─ Confirm ${action} ─────────────────────────────┐`,
        `│                                                  │`,
        `│  Are you sure you want to ${action}:              │`,
        `│    "${workflowName}"                               │`,
        `│                                                  │`,
        `│  Press Enter to confirm, Esc to cancel.          │`,
        `│                                                  │`,
        `└──────────────────────────────────────────────────┘`,
      ];

      const confirmComponent = new ConfirmBox(overlayLines);
      const overlayHandle: OverlayHandle = this.tui.showOverlay(confirmComponent, {
        anchor: "center",
        width: 60,
      });

      // Register a one-shot input handler on the TUI to capture confirm/cancel
      const originalOnInput = this.tui.onInput;
      this.tui.onInput = (data: string): boolean => {
        const kb = require("../core/keybindings.js").getKeybindings();

        if (kb.matches(data, "tui.select.confirm")) {
          overlayHandle.hide();
          this.tui.onInput = originalOnInput;
          resolve(true);
          return true;
        }

        if (kb.matches(data, "tui.select.cancel")) {
          overlayHandle.hide();
          this.tui.onInput = originalOnInput;
          resolve(false);
          return true;
        }

        return false;
      };
      this.tui.requestRender();
    });
  }

  /**
   * Delete the currently selected workflow after confirmation.
   */
  private async deleteSelectedWorkflow() {
    if (!this.selectedWorkflowId) {
      this.appendLog("No workflow selected", "error");
      return;
    }

    try {
      const workflow = await this.adapter.getWorkflow(this.selectedWorkflowId);
      const name = workflow?.name || this.selectedWorkflowId;

      const confirmed = await this.confirmOperation("delete", name);
      if (!confirmed) {
        this.appendLog("Delete cancelled");
        return;
      }

      await this.adapter.deleteWorkflow(this.selectedWorkflowId);
      this.appendLog(`Workflow "${name}" deleted`);
      this.selectedWorkflowId = null;
      await this.loadWorkflows();
    } catch (error) {
      this.logger.error("Failed to delete workflow", {}, undefined, error as Error);
      this.appendLog("Failed to delete workflow", "error");
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

    // N — new workflow
    if (data === "n" || data === "N") {
      this.appendLog("New workflow: integration pending");
      return true;
    }

    // E — edit selected workflow
    if (data === "e" || data === "E") {
      if (!this.selectedWorkflowId) {
        this.appendLog("No workflow selected", "error");
      } else {
        this.appendLog("Edit workflow: integration pending");
      }
      return true;
    }

    // D — delete selected workflow (with confirmation)
    if (data === "d" || data === "D") {
      this.deleteSelectedWorkflow();
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
