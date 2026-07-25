/**
 * Workflow Management Screen
 *
 * Flat list view: navigate workflows with arrow keys.
 * Press Enter to drill into detail view.
 */

import { Container, Text, SelectList } from "../core/index.js";
import type { Screen } from "./screen.js";
import type { TUI } from "../core/tui.js";
import { WorkflowAdapter } from "../../adapters/workflow-adapter.js";
import { WorkflowGraphAdapter } from "../../adapters/workflow-graph-adapter.js";
import { createContextualLogger } from "@wf-agent/sdk/utils";
import { ConfirmModal } from "../modals/index.js";

interface WorkflowItem {
  id: string;
  name: string;
  version: string;
  nodeCount: number;
  status: string;
}

export class WorkflowScreen implements Screen {
  private container: Container;
  private workflowList: SelectList;
  private detailView: Container | null = null;
  private adapter: WorkflowAdapter;
  private graphAdapter: WorkflowGraphAdapter;
  private onBack?: () => void;
  private tui: TUI;
  private logger = createContextualLogger({ component: "WorkflowScreen" });
  private items: WorkflowItem[] = [];
  private selectedId: string | null = null;
  constructor(onBack?: () => void, tui?: TUI) {
    this.onBack = onBack;
    this.tui = tui!;
    this.adapter = new WorkflowAdapter();
    this.graphAdapter = new WorkflowGraphAdapter();
    this.container = new Container();
    this.workflowList = new SelectList([]);
    this.setupLayout();
    this.loadWorkflows();
  }

  private setupLayout() {
    this.container.addChild(this.workflowList);
  }

  private async loadWorkflows() {
    try {
      const workflows = await this.adapter.listWorkflows();
      this.items = [];

      for (const w of workflows) {
        let nodeCount = 0;
        try {
          const nodes = await this.graphAdapter.getNodes(w.id);
          nodeCount = nodes?.length ?? 0;
        } catch { /* ignore */ }

        this.items.push({
          id: w.id,
          name: w.name,
          version: w.version || "1.0",
          nodeCount,
          status: (w as any).status || "idle",
        });
      }

      this.workflowList.setItems(this.items.map(item => ({
        value: item.id,
        label: `${item.name}  v${item.version}  ${item.nodeCount} nodes  ${item.status}`,
        description: "",
      })));

      if (this.items.length > 0) {
        this.workflowList.setSelectedIndex(0);
      }
    } catch (error) {
      this.logger.error("Failed to load workflows", {}, undefined, error as Error);
    }
  }

  private async showDetail(item: WorkflowItem) {
    this.selectedId = item.id;
    this.detailView = new Container();

    try {
      const workflow = await this.adapter.getWorkflow(item.id);
      if (workflow) {
        this.detailView.addChild(new Text(`← Back to Workflows`));
        this.detailView.addChild(new Text(""));
        this.detailView.addChild(new Text(`Name:        ${workflow.name}`));
        this.detailView.addChild(new Text(`Version:     ${workflow.version || "1.0"}`));
        this.detailView.addChild(new Text(`Status:      ${item.status}`));
        this.detailView.addChild(new Text(`Nodes:       ${item.nodeCount}`));
        this.detailView.addChild(new Text(`Created:     ${new Date(workflow.createdAt).toLocaleDateString()}`));
        this.detailView.addChild(new Text(`Description: ${workflow.description || "N/A"}`));
        this.detailView.addChild(new Text(""));
        this.detailView.addChild(new Text("[Enter] view graph  [E] edit  [D] delete  [Esc] back"));
      }
    } catch (error) {
      this.logger.error("Failed to load workflow details", {}, undefined, error as Error);
      this.detailView.addChild(new Text("Failed to load workflow details"));
    }

    this.container.clear();
    this.container.addChild(this.detailView);
    this.tui.requestRender();
  }

  private showList() {
    this.selectedId = null;
    this.detailView = null;
    this.container.clear();
    this.container.addChild(this.workflowList);
    this.tui.requestRender();
  }

  private async deleteSelected() {
    const item = this.items.find(i => i.id === this.selectedId);
    if (!item) return;

    const confirmed = await ConfirmModal.show(this.tui, "Confirm Delete", `Delete "${item.name}"?`);
    if (!confirmed) return;

    try {
      await this.adapter.deleteWorkflow(item.id);
      this.showList();
      await this.loadWorkflows();
    } catch (error) {
      this.logger.error("Failed to delete workflow", {}, undefined, error as Error);
    }
  }

  render(): Container {
    return this.container;
  }

  handleInput(data: string): boolean {
    if (data === "b" || data === "B" || data === "\x1b") {
      if (this.detailView) {
        this.showList();
      } else {
        this.onBack?.();
      }
      return true;
    }

    if (data === "r" || data === "R") {
      if (!this.detailView) this.loadWorkflows();
      return true;
    }

    if (data === "d" || data === "D") {
      if (this.detailView) this.deleteSelected();
      return true;
    }

    if (data === "n" || data === "N") {
      return true;
    }

    if (data === "e" || data === "E") {
      return true;
    }

    if (data === "\r" || data === "\n") {
      if (!this.detailView) {
        const selected = this.workflowList.getSelectedItem();
        const item = this.items.find(i => i.id === selected?.value);
        if (item) this.showDetail(item);
        return true;
      }
    }

    return this.workflowList.handleInput?.(data), true;
  }

  destroy(): void {}
}
