# Type Tests for @wf-agent/types

This directory contains type-level tests using [tsd](https://github.com/SamVerschueren/tsd) to validate the type definitions in the `@wf-agent/types` package.

## Purpose

Type tests ensure that:
- Generic types infer correctly
- Type guards narrow types as expected
- Union types work properly with discriminated unions
- SDK usage patterns are type-safe
- Breaking changes in types are caught early

## Directory Structure

```
__tests__/test-d/
├── node/                          # Node type system tests
│   ├── static-node-types.test-d.ts
│   ├── runtime-node-types.test-d.ts
│   └── node-type-guards.test-d.ts
├── workflow/                      # Workflow template tests
│   ├── workflow-template.test-d.ts
│   └── boundary-config.test-d.ts
├── result/                        # Result type tests
│   └── result-type.test-d.ts
├── errors/                        # Error hierarchy tests
│   └── error-hierarchy.test-d.ts
├── tool/                          # Tool type system tests
│   ├── tool-definition.test-d.ts
│   └── tool-config.test-d.ts
├── agent/                         # Agent execution tests
├── checkpoint/                    # Checkpoint type tests
├── events/                        # Event type tests
└── integration/                   # SDK integration pattern tests
    └── sdk-usage-patterns.test-d.ts
```

## Running Type Tests

```bash
# Run all type tests
pnpm test:type

# Or from the package directory
cd packages/types
pnpm test:type
```

## Writing Type Tests

### Basic Test Structure

```typescript
/**
 * @description Tests for [Module Name]
 * @priority HIGH/MEDIUM/LOW
 */

import { expectType, expectNotType, expectAssignable } from "tsd";
import type { /* Your types */ } from "../../src/index.js";

// Test 1: Basic type inference
declare const value: YourType;
expectType<ExpectedType>(value);

// Test 2: Type guards
if (isSpecificType(value)) {
  expectType<SpecificType>(value);
}

// Test 3: Invalid assignments (should fail if uncommented)
// expectType<WrongType>(value);
```

### Common Test Patterns

#### 1. Generic Type Inference

```typescript
type MyGeneric<T> = { value: T };
declare const genericValue: MyGeneric<string>;
expectType<{ value: string }>(genericValue);
```

#### 2. Discriminated Unions

```typescript
type Node = 
  | { type: "LLM"; config: LLMConfig }
  | { type: "SCRIPT"; config: ScriptConfig };

declare const node: Node;
if (node.type === "LLM") {
  expectType<LLMConfig>(node.config);
}
```

#### 3. Type Guards

```typescript
declare const value: UnionType;
if (isSpecificType(value)) {
  expectType<SpecificType>(value);
  // Should be able to access specific properties
  expectType<string>(value.specificProperty);
}
```

#### 4. Chain Operations

```typescript
const result = someValue
  .andThen((val) => {
    expectType<ExpectedType>(val);
    return anotherValue;
  });
expectType<FinalType>(result);
```

## Priority Levels

### HIGH Priority
Core types used extensively in SDK:
- Node type system (Static/Runtime nodes)
- WorkflowTemplate structure
- Result type (functional error handling)
- Error hierarchy
- Tool type system

### MEDIUM Priority
Important but less frequently used:
- Agent execution types
- Checkpoint types
- Event types
- Message types
- Storage adapter types

### LOW Priority
Simple or auxiliary types:
- Common base types (ID, Timestamp, etc.)
- Config loading types
- Skill types

## SDK Integration Testing

The `integration/sdk-usage-patterns.test-d.ts` file tests real-world SDK usage patterns:

```typescript
// Pattern: Node traversal with type narrowing
function processNodes(workflow: WorkflowTemplate) {
  for (const node of workflow.nodes) {
    if (isStaticLLMNode(node)) {
      // Verify type narrowing works
      expectType<string | undefined>(node.config.prompt);
    }
  }
}

// Pattern: Result error handling
function handleExecution(result: Result<ExecutionOutput>) {
  if (result.isOk()) {
    expectType<ExecutionOutput>(result.value);
  } else {
    expectType<Error>(result.error);
  }
}
```

## Best Practices

1. **Test Public APIs Only**: Focus on exported types that SDK consumers use
2. **Cover Edge Cases**: Test optional fields, union types, and generic constraints
3. **Document Intent**: Use JSDoc comments to explain what each test validates
4. **Keep Tests Minimal**: Each test should verify one specific behavior
5. **Use Realistic Examples**: Mirror actual SDK usage patterns
6. **Test Failure Cases**: Include commented-out code that should fail type checking

## Debugging Type Tests

If a type test fails:

1. Check the error message from `pnpm test:type`
2. Verify the type definition in `src/`
3. Ensure imports are correct (use `.js` extension for ESM)
4. Use `expectNotType` to verify negative cases
5. Add intermediate type assertions to isolate the issue

## Adding New Tests

When adding new types to the package:

1. Create a corresponding `.test-d.ts` file
2. Follow the naming convention: `[module-name].test-d.ts`
3. Include tests for:
   - Basic type construction
   - Generic parameter inference
   - Type guards (if applicable)
   - Integration with related types
4. Update this README if adding a new category

## CI/CD Integration

Type tests run automatically in CI:
```yaml
# In your CI configuration
- name: Type Tests
  run: pnpm test:type
```

This ensures type safety is maintained across all changes.
