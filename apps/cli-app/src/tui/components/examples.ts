/**
 * Phase 2 Component Usage Examples
 * 
 * This file demonstrates how to use the basic components implemented in Phase 2.
 */

import { ProcessTerminal } from "../core/terminal.js";
import { Container, TUI } from "../core/tui.js";
import { Text, Box, Spacer, SelectList, Input, Loader } from "../core/index.js";

/**
 * Example 1: Simple text display with styling
 */
export function exampleSimpleText() {
  const terminal = new ProcessTerminal();
  const tui = new TUI(terminal);
  const container = new Container();

  // Create text with different padding
  const title = new Text("Welcome to CLI-App TUI", 0, 2);
  const description = new Text(
    "This is a demonstration of the Text component.\nIt supports multi-line text and automatic wrapping.",
    1,
    1
  );

  container.addChild(title);
  container.addChild(description);

  tui.addChild(container);
  tui.start();
}

/**
 * Example 2: Box container with children
 */
export function exampleBoxContainer() {
  const terminal = new ProcessTerminal();
  const tui = new TUI(terminal);
  const container = new Container();

  // Create a box with border-like appearance using background
  const panel = new Box(2, 1);
  
  panel.addChild(new Text("Panel Title", 0, 0));
  panel.addChild(new Spacer(1));
  panel.addChild(new Text("This content is inside a box container with padding.", 0, 0));

  container.addChild(panel);
  tui.addChild(container);
  tui.start();
}

/**
 * Example 3: Interactive select list
 */
export function exampleSelectList() {
  const terminal = new ProcessTerminal();
  const tui = new TUI(terminal);
  const container = new Container();

  const menuItems = [
    { value: "dashboard", label: "Dashboard", description: "View system overview" },
    { value: "workflows", label: "Workflows", description: "Manage workflows" },
    { value: "agents", label: "Agents", description: "Monitor agent loops" },
    { value: "settings", label: "Settings", description: "Configure application" },
  ];

  const menu = new SelectList(menuItems, 5);
  
  menu.onSelect = (item) => {
    console.log(`Selected: ${item.label}`);
    // Navigate to selected screen
  };

  container.addChild(new Text("Main Menu", 0, 1));
  container.addChild(menu);

  tui.addChild(container);
  tui.start();
}

/**
 * Example 4: Text input with placeholder
 */
export function exampleTextInput() {
  const terminal = new ProcessTerminal();
  const tui = new TUI(terminal);
  const container = new Container();

  const input = new Input("Enter your name...");
  
  input.onSubmit = (value) => {
    console.log(`Submitted: ${value}`);
    // Process the input
  };

  container.addChild(new Text("Please enter your name:", 0, 1));
  container.addChild(input);

  tui.addChild(container);
  tui.start();
}

/**
 * Example 5: Loading indicator
 */
export function exampleLoader() {
  const terminal = new ProcessTerminal();
  const tui = new TUI(terminal);
  const container = new Container();

  // Create loader with custom colors (using chalk or similar)
  const spinnerColor = (str: string) => `\x1b[36m${str}\x1b[0m`; // Cyan
  const messageColor = (str: string) => `\x1b[33m${str}\x1b[0m`; // Yellow

  const loader = new Loader(
    tui,
    spinnerColor,
    messageColor,
    "Processing your request..."
  );

  container.addChild(loader);
  tui.addChild(container);
  tui.start();

  // Simulate async operation
  setTimeout(() => {
    loader.setMessage("Almost done...");
  }, 2000);

  setTimeout(() => {
    loader.stop();
    // Show completion message
  }, 4000);
}

/**
 * Example 6: Complex layout combining multiple components
 */
export function exampleComplexLayout() {
  const terminal = new ProcessTerminal();
  const tui = new TUI(terminal);
  const container = new Container();

  // Header
  const header = new Box(1, 1);
  header.addChild(new Text("Application Dashboard", 0, 0));
  
  // Status panel
  const statusPanel = new Box(2, 1);
  statusPanel.addChild(new Text("System Status", 0, 0));
  statusPanel.addChild(new Spacer(1));
  statusPanel.addChild(new Text("• Agents Running: 3", 0, 0));
  statusPanel.addChild(new Text("• Active Threads: 12", 0, 0));
  statusPanel.addChild(new Text("• Memory Usage: 45%", 0, 0));

  // Quick actions menu
  const actionsMenu = new SelectList([
    { value: "new-agent", label: "New Agent", description: "Start a new agent loop" },
    { value: "view-logs", label: "View Logs", description: "Check system logs" },
    { value: "refresh", label: "Refresh", description: "Update dashboard" },
  ], 3);

  // Input for quick commands
  const commandInput = new Input("Type a command...");
  commandInput.onSubmit = (cmd) => {
    console.log(`Executing: ${cmd}`);
  };

  // Assemble layout
  container.addChild(header);
  container.addChild(new Spacer(1));
  container.addChild(statusPanel);
  container.addChild(new Spacer(1));
  container.addChild(actionsMenu);
  container.addChild(new Spacer(1));
  container.addChild(commandInput);

  tui.addChild(container);
  tui.start();
}

/**
 * Example 7: Filtering and searching in SelectList
 */
export function exampleFiltering() {
  const terminal = new ProcessTerminal();
  const tui = new TUI(terminal);
  const container = new Container();

  const workflows = [
    { value: "wf-001", label: "Data Processing Pipeline", description: "ETL workflow" },
    { value: "wf-002", label: "Code Review Assistant", description: "Automated code review" },
    { value: "wf-003", label: "Deployment Automation", description: "CI/CD pipeline" },
    { value: "wf-004", label: "Database Migration", description: "Schema updates" },
    { value: "wf-005", label: "API Testing Suite", description: "Integration tests" },
  ];

  const workflowList = new SelectList(workflows, 5);

  // Add search input
  const searchInput = new Input("Search workflows...");
  searchInput.onSubmit = (query) => {
    workflowList.setFilter(query);
  };

  container.addChild(new Text("Workflow Manager", 0, 1));
  container.addChild(searchInput);
  container.addChild(new Spacer(1));
  container.addChild(workflowList);

  tui.addChild(container);
  tui.start();
}

// Export all examples
export const examples = {
  simpleText: exampleSimpleText,
  boxContainer: exampleBoxContainer,
  selectList: exampleSelectList,
  textInput: exampleTextInput,
  loader: exampleLoader,
  complexLayout: exampleComplexLayout,
  filtering: exampleFiltering,
};
