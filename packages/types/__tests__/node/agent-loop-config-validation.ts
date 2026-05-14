/**
 * Agent Loop Node Configuration Type Validation
 * 
 * This script validates that the refactored types are correctly exported and usable.
 */

import type { AgentLoopNodeConfig } from "../../src/node/configs/agent-loop-configs.js";
import { AgentLoopNodeConfigSchema, isAgentLoopNodeConfig } from "../../src/node/configs/agent-loop-configs-schema.js";

// Test 1: Type checking for inline config mode
const inlineConfigExample: AgentLoopNodeConfig = {
  inlineConfig: {
    profileId: "gpt-4",
    maxIterations: 10,
    initialContextRefs: ["system", "task-spec"],
    availableTools: {
      initial: ["read_file", "write_file"],
    },
  },
};

console.log("✓ Test 1 passed: Inline config type validation");

// Test 2: Type checking for reference mode
const referenceConfigExample: AgentLoopNodeConfig = {
  agentLoopId: "my-complex-agent",
};

console.log("✓ Test 2 passed: Reference config type validation");

// Test 3: Schema validation for valid config
const validConfig = {
  inlineConfig: {
    profileId: "gpt-4",
    maxIterations: 5,
  },
};

const validationResult = AgentLoopNodeConfigSchema.safeParse(validConfig);
if (validationResult.success) {
  console.log("✓ Test 3 passed: Schema validation for valid config");
} else {
  console.error("✗ Test 3 failed:", validationResult.error);
  process.exit(1);
}

// Test 4: Type guard function
if (isAgentLoopNodeConfig(validConfig)) {
  console.log("✓ Test 4 passed: Type guard function works correctly");
} else {
  console.error("✗ Test 4 failed: Type guard returned false for valid config");
  process.exit(1);
}

// Test 5: Invalid config should fail validation
const invalidConfig = {
  inlineConfig: {
    // Missing required profileId
    maxIterations: 5,
  },
};

const invalidResult = AgentLoopNodeConfigSchema.safeParse(invalidConfig);
if (!invalidResult.success) {
  console.log("✓ Test 5 passed: Schema correctly rejects invalid config");
} else {
  console.error("✗ Test 5 failed: Schema should reject config without profileId");
  process.exit(1);
}

console.log("\n✅ All type validation tests passed!");
