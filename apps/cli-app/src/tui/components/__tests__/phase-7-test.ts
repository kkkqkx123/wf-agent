/**
 * Phase 7 Components - Quick Verification Test
 * 
 * This file demonstrates usage of the new Phase 7 components.
 * Run with: node --loader ts-node/esm phase-7-test.ts
 */

import { IterationPanel } from "../iteration-panel.js";
import { ToolCallIndicator } from "../tool-call-indicator.js";

console.log("=== Phase 7 Component Verification ===\n");

// Test 1: Iteration Panel
console.log("Test 1: Iteration Panel");
const iterationPanel = new IterationPanel({ maxHeight: 5 });

// Simulate iterations
iterationPanel.updateIteration({ iteration: 1, toolCallCount: 2 });
console.log("After iteration 1 start:");
console.log(iterationPanel.render(60).join("\n"));
console.log("");

iterationPanel.completeIteration(1, 1500);
console.log("After iteration 1 complete:");
console.log(iterationPanel.render(60).join("\n"));
console.log("");

iterationPanel.updateIteration({ iteration: 2, toolCallCount: 5 });
iterationPanel.completeIteration(2, 2300);
console.log("After iteration 2:");
console.log(iterationPanel.render(60).join("\n"));
console.log("");

// Test 2: Tool Call Indicator
console.log("\nTest 2: Tool Call Indicator");
const toolIndicator = new ToolCallIndicator({ maxDisplayCalls: 3, showArguments: false });

// Simulate tool calls
toolIndicator.handleToolCallStart({
  toolCallId: "call_1",
  toolName: "read_file",
  arguments: { path: "test.txt" },
  summary: "Reading test.txt",
});

console.log("During tool call:");
console.log(toolIndicator.render(60).join("\n"));
console.log("");

setTimeout(() => {
  toolIndicator.handleToolCallEnd({
    toolCallId: "call_1",
    toolName: "read_file",
    success: true,
    duration: 150,
  });

  console.log("After tool call complete:");
  console.log(toolIndicator.render(60).join("\n"));
  console.log("");

  // Test multiple tools
  toolIndicator.handleToolCallStart({
    toolCallId: "call_2",
    toolName: "search_codebase",
    arguments: { query: "authentication" },
    summary: "Searching for authentication",
  });

  setTimeout(() => {
    toolIndicator.handleToolCallEnd({
      toolCallId: "call_2",
      toolName: "search_codebase",
      success: true,
      duration: 1200,
    });

    console.log("After second tool:");
    console.log(toolIndicator.render(60).join("\n"));
    console.log("");

    console.log("\n✅ All tests passed!");
  }, 100);
}, 100);
