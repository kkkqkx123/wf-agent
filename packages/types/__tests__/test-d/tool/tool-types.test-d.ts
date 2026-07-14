/**
 * @description Tests for Tool Type System
 * @priority HIGH
 * 
 * Validates:
 * - Tool interface structure
 * - ToolConfig union types (Stateless, Stateful, Rest, Builtin)
 * - ToolParameterSchema and ToolProperty
 * - ToolMetadata structure
 * - Type guards (isRestToolConfig, isBuiltinToolConfig, etc.)
 * - ToolSchema for LLM invocation
 */

import { expectType, expectAssignable } from "tsd";
import type {
  Tool,
  ToolType,
  ToolConfig,
  StatelessToolConfig,
  StatefulToolConfig,
  RestToolConfig,
  BuiltinToolConfig,
  ToolParameterSchema,
  ToolProperty,
  ToolMetadata,
  ToolSchema,
  StatefulToolInstance,
  StatefulToolFactory,
  BuiltinToolExecutionContext,
  ToolRuntimeParameters,
  ToolRiskLevel,
  ApprovalCondition,
} from "../../../src/index.js";
import {
  isRestToolConfig,
  isBuiltinToolConfig,
  isStatelessToolConfig,
  isStatefulToolConfig,
} from "../../../src/index.js";

// ============================================================================
// Test 1: Tool interface basic structure
// ============================================================================

declare const tool: Tool;
expectType<string>(tool.id);
expectType<ToolType>(tool.type);
expectType<string>(tool.description);
expectType<ToolParameterSchema>(tool.parameters);
expectType<ToolMetadata | undefined>(tool.metadata);
expectType<ToolConfig | undefined>(tool.config);
expectType<boolean | "before" | "after" | "both" | undefined>(tool.createCheckpoint);
expectType<string | undefined>(tool.checkpointDescriptionTemplate);
expectType<boolean | undefined>(tool.strict);

// ============================================================================
// Test 2: ToolParameterSchema structure
// ============================================================================

declare const paramSchema: ToolParameterSchema;
expectType<"object">(paramSchema.type);
expectType<Record<string, ToolProperty>>(paramSchema.properties);
expectType<string[]>(paramSchema.required);
expectType<boolean | ToolProperty | undefined>(paramSchema.additionalProperties);

// ============================================================================
// Test 3: ToolProperty structure
// ============================================================================

declare const property: ToolProperty;
expectType<"string" | "number" | "integer" | "boolean" | "array" | "object" | "null">(property.type);
expectType<string | undefined>(property.description);
expectType<unknown>(property.default);
expectType<unknown[] | undefined>(property.enum);
expectType<string | undefined>(property.format);
expectType<unknown[] | undefined>(property.examples);

// String constraints
expectType<number | undefined>(property.minLength);
expectType<number | undefined>(property.maxLength);
expectType<string | undefined>(property.pattern);

// Numeric constraints
expectType<number | undefined>(property.minimum);
expectType<number | undefined>(property.maximum);

// Object structure
expectType<Record<string, ToolProperty> | undefined>(property.properties);
expectType<string[] | undefined>(property.required);
expectType<boolean | ToolProperty | undefined>(property.additionalProperties);

// Array structure
expectType<ToolProperty | undefined>(property.items);

// Allow additional JSON Schema fields
expectType<unknown>(property["customField"]);

// ============================================================================
// Test 4: ToolMetadata structure
// ============================================================================

declare const metadata: ToolMetadata;
expectType<string | undefined>(metadata.category);
expectType<string[] | undefined>(metadata.tags);
expectType<string | undefined>(metadata.documentationUrl);
expectType<Record<string, unknown> | undefined>(metadata.customFields);
expectType<ToolRiskLevel | undefined>(metadata.riskLevel);
expectType<boolean | undefined>(metadata.autoApprovable);
expectType<ApprovalCondition[] | undefined>(metadata.approvalConditions);
expectType<boolean | undefined>(metadata.requiresUserInteraction);
expectType<string | undefined>(metadata.interactionType);

// ============================================================================
// Test 5: StatelessToolConfig
// ============================================================================

declare const statelessConfig: StatelessToolConfig;
expectType<(parameters: ToolRuntimeParameters) => Promise<unknown>>(statelessConfig.execute);
expectType<string | undefined>(statelessConfig.version);
expectType<string | undefined>(statelessConfig.description);

// Execute function should accept parameters and return Promise
const result = statelessConfig.execute({ param1: "value1" });
expectType<Promise<unknown>>(result);

// ============================================================================
// Test 6: StatefulToolInstance and StatefulToolFactory
// ============================================================================

declare const instance: StatefulToolInstance;
expectType<(parameters: ToolRuntimeParameters) => Promise<unknown>>(instance.execute);
expectType<(() => void) | undefined>(instance.destroy);

declare const factory: StatefulToolFactory;
expectType<(executionId?: string) => StatefulToolInstance>(factory.create);

const createdInstance = factory.create();
expectType<StatefulToolInstance>(createdInstance);

const createdInstanceWithId = factory.create("exec-123");
expectType<StatefulToolInstance>(createdInstanceWithId);

// ============================================================================
// Test 7: StatefulToolConfig
// ============================================================================

declare const statefulConfig: StatefulToolConfig;
expectType<StatefulToolFactory>(statefulConfig.factory);

// ============================================================================
// Test 8: RestToolConfig
// ============================================================================

declare const restConfig: RestToolConfig;
expectType<string | undefined>(restConfig.baseUrl);
expectType<Record<string, string> | undefined>(restConfig.headers);
expectType<number | undefined>(restConfig.timeout);
expectType<number | undefined>(restConfig.maxRetries);
expectType<number | undefined>(restConfig.retryDelay);

// ============================================================================
// Test 9: BuiltinToolConfig and BuiltinToolExecutionContext
// ============================================================================

declare const builtinConfig: BuiltinToolConfig;
expectType<
  (parameters: ToolRuntimeParameters, context: BuiltinToolExecutionContext) => Promise<unknown>
>(builtinConfig.execute);

declare const builtinContext: BuiltinToolExecutionContext;
expectType<string | undefined>(builtinContext.executionId);
expectType<unknown>(builtinContext.parentExecutionEntity);
expectType<unknown>(builtinContext.executionRegistry);
expectType<unknown>(builtinContext.eventManager);
expectType<unknown>(builtinContext.executionBuilder);
expectType<unknown>(builtinContext.taskQueueManager);
expectType<unknown>(builtinContext.workflowRegistry);
expectType<unknown>(builtinContext.graphRegistry);

// ============================================================================
// Test 10: ToolConfig union type
// ============================================================================

// ToolConfig should be assignable to any of the specific config types
declare const anyConfig: ToolConfig;

// Should be assignable to the union
expectAssignable<ToolConfig>(anyConfig);

// ============================================================================
// Test 11: Type guards narrow config types
// ============================================================================

// Test isRestToolConfig
declare const maybeRestConfig: ToolConfig;
if (isRestToolConfig(maybeRestConfig)) {
  expectType<RestToolConfig>(maybeRestConfig);
  expectType<string | undefined>(maybeRestConfig.baseUrl);
  expectType<Record<string, string> | undefined>(maybeRestConfig.headers);
  expectType<number | undefined>(maybeRestConfig.timeout);
}

// Test isBuiltinToolConfig
declare const maybeBuiltinConfig: ToolConfig;
if (isBuiltinToolConfig(maybeBuiltinConfig)) {
  expectAssignable<BuiltinToolConfig>(maybeBuiltinConfig);
  expectAssignable<
    (parameters: ToolRuntimeParameters, context: BuiltinToolExecutionContext) => Promise<unknown>
  >(maybeBuiltinConfig.execute);
}

// Test isStatelessToolConfig
declare const maybeStatelessConfig: ToolConfig;
if (isStatelessToolConfig(maybeStatelessConfig)) {
  expectType<StatelessToolConfig>(maybeStatelessConfig);
  expectType<(parameters: ToolRuntimeParameters) => Promise<unknown>>(maybeStatelessConfig.execute);
}

// Test isStatefulToolConfig
declare const maybeStatefulConfig: ToolConfig;
if (isStatefulToolConfig(maybeStatefulConfig)) {
  expectType<StatefulToolConfig>(maybeStatefulConfig);
  expectType<StatefulToolFactory>(maybeStatefulConfig.factory);
}

// ============================================================================
// Test 12: ToolSchema for LLM invocation
// ============================================================================

declare const toolSchema: ToolSchema;
expectType<string>(toolSchema.id);
expectType<string>(toolSchema.description);
expectType<ToolParameterSchema>(toolSchema.parameters);

// ============================================================================
// Test 13: Complete Tool construction example
// ============================================================================

// Stateless tool example
const statelessTool: Tool = {
  id: "read_file",
  type: "STATELESS",
  description: "Read file content",
  parameters: {
    type: "object",
    properties: {
      path: {
        type: "string",
        description: "File path to read",
      },
    },
    required: ["path"],
  },
  config: {
    execute: async (_params: ToolRuntimeParameters) => {
      return { content: "file content" };
    },
  },
};

expectType<Tool>(statelessTool);
expectType<ToolType>(statelessTool.type);
expectType<string>(statelessTool.id);

// Rest tool example
const restTool: Tool = {
  id: "api_call",
  type: "REST",
  description: "Make REST API call",
  parameters: {
    type: "object",
    properties: {
      endpoint: {
        type: "string",
        description: "API endpoint",
      },
    },
    required: ["endpoint"],
  },
  config: {
    baseUrl: "https://api.example.com",
    headers: { "Authorization": "Bearer token" },
    timeout: 5000,
  },
};

expectType<Tool>(restTool);
expectType<ToolType>(restTool.type);

// ============================================================================
// Test 14: Optional fields handling
// ============================================================================

declare const minimalTool: Tool;
// All optional fields should be properly typed
expectType<ToolMetadata | undefined>(minimalTool.metadata);
expectType<ToolConfig | undefined>(minimalTool.config);
expectType<boolean | "before" | "after" | "both" | undefined>(minimalTool.createCheckpoint);
expectType<string | undefined>(minimalTool.checkpointDescriptionTemplate);
expectType<boolean | undefined>(minimalTool.strict);

// ============================================================================
// Test 15: ToolProperty nested structure
// ============================================================================

// Object with nested properties
const objectProperty: ToolProperty = {
  type: "object",
  properties: {
    name: {
      type: "string",
      description: "User name",
      minLength: 1,
      maxLength: 100,
    },
    age: {
      type: "integer",
      description: "User age",
      minimum: 0,
      maximum: 150,
    },
    email: {
      type: "string",
      description: "Email address",
      format: "email",
    },
  },
  required: ["name", "age"],
};

expectType<ToolProperty>(objectProperty);
expectType<Record<string, ToolProperty> | undefined>(objectProperty.properties);

// Array with items
const arrayProperty: ToolProperty = {
  type: "array",
  items: {
    type: "string",
    description: "List item",
  },
};

expectType<ToolProperty>(arrayProperty);
expectType<ToolProperty | undefined>(arrayProperty.items);

// Enum values
const enumProperty: ToolProperty = {
  type: "string",
  enum: ["option1", "option2", "option3"],
  description: "Select an option",
};

expectType<ToolProperty>(enumProperty);
expectType<unknown[] | undefined>(enumProperty.enum);
