# CLI Adapters Unit Tests

This directory contains unit tests for the CLI adapters that encapsulate SDK API calls.

## Test Files

### Core Adapters (Recently Improved)

1. **workflow-execution-adapter.test.ts** - Tests for workflow execution operations
   - Execute, pause, resume, stop workflows
   - List and get execution details
   - Delete executions

2. **user-interaction-adapter.test.ts** - Tests for user interaction configuration management
   - CRUD operations for configurations
   - Enable/disable configurations
   - Filter and search

3. **variable-adapter.test.ts** - Tests for variable management
   - Get/set/delete variables
   - List variables by execution
   - Get variable definitions

4. **skill-adapter.test.ts** - Tests for skill registry operations
   - Initialize and scan skills
   - List and get skill details
   - Load content and resources
   - Match skills by query

5. **message-adapter.test.ts** - Tests for message management
   - List and get messages
   - Filter by execution ID
   - Get message statistics
   - Normalize messages (deprecated)

## Running Tests

```bash
# Run all adapter tests
pnpm test src/adapters/__tests__

# Run specific adapter test
pnpm test src/adapters/__tests__/workflow-execution-adapter.test.ts

# Run with coverage
pnpm test src/adapters/__tests__ --coverage
```

## Test Structure

Each test file follows this pattern:

```typescript
describe("AdapterName", () => {
  let adapter: AdapterClass;
  let mockSdk: any;
  let mockApi: any;

  beforeEach(() => {
    // Setup mocks
    // Create adapter instance
  });

  describe("methodName", () => {
    it("should perform operation successfully", async () => {
      // Mock successful response
      // Call adapter method
      // Assert expectations
    });

    it("should handle failure", async () => {
      // Mock failure response
      // Call adapter method
      // Assert error is thrown
    });
  });
});
```

## Mock Strategy

Tests use Vitest's mocking capabilities to:

1. **Mock SDK Instance**: Replace the real SDK with a mock instance
2. **Mock API Methods**: Stub SDK API methods to return controlled responses
3. **Mock Output**: Prevent actual console output during tests
4. **Test Both Success and Failure Paths**: Ensure proper error handling

## Key Testing Principles

1. **Isolation**: Each test is independent and doesn't rely on other tests
2. **Mocking**: All external dependencies are mocked
3. **Coverage**: Test both success and failure scenarios
4. **Type Safety**: Use TypeScript types to catch errors at compile time
5. **Clear Assertions**: Use descriptive expect statements

## Adding New Tests

When adding tests for a new adapter:

1. Create a new test file: `adapter-name.test.ts`
2. Follow the existing test structure
3. Mock all SDK dependencies
4. Test all public methods
5. Include both success and failure cases
6. Run tests to ensure they pass

## Integration with CI/CD

These unit tests are part of the CI/CD pipeline and run automatically on:
- Pull requests
- Push to main branch
- Before deployment

## Related Documentation

- [CLI Adapters Improvement Plan](../../docs/cli-adapters-improvement-plan.md)
- [TODO CLI Adapters](../../docs/TODO-cli-adapters.md)
- [SDK API Reference](../../../sdk/docs/api-reference.md)
