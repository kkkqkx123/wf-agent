# CLI Integration Test Analysis

## Overview

This document analyzes the CLI test mode configuration and execution flow to help developers understand how integration tests work and what issues may occur.

## Test Execution Flow

### 1. Test Entry Point

Tests are run via:
```bash
cd apps/cli-app && pnpm test __tests__/integration/workflows/01-registration.test.ts
```

This uses `vitest` with configuration `vitest.integration.config.mjs`.

### 2. CLIRunner Execution

The `CLIRunner` class (in `__tests__/utils/cli-runner.ts`) spawns a new Node.js process:

```typescript
const child = spawn("node", [this.cliPath, ...args], {
  env: { ...this.defaultEnv, ...options.env },
  cwd: options.cwd,
  stdio: ["pipe", "pipe", "pipe"],
});
```

Key environment variables set for tests:
```typescript
this.defaultEnv = {
  NODE_ENV: "test",
  TEST_MODE: "true",
  LOG_DIR: this.outputDir,
  DISABLE_LOG_TERMINAL: "true",
  DISABLE_SDK_LOGS: "true",
  SDK_LOG_LEVEL: "silent",
};
```

### 3. CLI Initialization Flow

When the CLI starts (`src/index.ts`), the `preAction` hook executes:

1. **Load Configuration** - Uses `ConfigLoader` to load config from file or defaults
2. **Initialize Output** - Creates `CLIOutput` instance for stdout/stderr/log management
3. **Initialize Logger** - Sets up logging system
4. **Initialize SDK Logger** - Configures SDK logging
5. **Initialize Storage Manager** - Sets up storage with `STORAGE_DIR` from env

### 4. Headless Mode Detection

From `src/utils/exit-manager.ts`:

```typescript
export function isHeadlessMode(): boolean {
  return (
    process.env["CLI_MODE"] === "headless" ||
    process.env["HEADLESS"] === "true" ||
    process.env["TEST_MODE"] === "true"  // <-- Tests set this
  );
}
```

When `TEST_MODE=true`, the CLI is in headless mode.

### 5. Exit Behavior

In `src/index.ts`, the `postAction` hook:

```typescript
if (isHeadlessMode()) {
  await ExitManager.exit(0);
}
```

This ensures the CLI exits cleanly after command completion.

## Key Issues and Solutions

### Issue 1: Storage Directory Not Properly Isolated

**Problem**: The `STORAGE_DIR` environment variable is set in `CLIRunner`, but the CLI may not be using it correctly during initialization.

**Solution**: Ensure `STORAGE_DIR` is set before storage manager initialization. The `ConfigLoader.loadWithEnvOverride()` method reads `STORAGE_DIR` and merges it into config.

### Issue 2: Configuration Loading in Tests

**Problem**: The CLI loads configuration from file which may override test-specific settings.

**Solution**: The `CLIRunner` sets `DISABLE_SDK_LOGS=true` and `SDK_LOG_LEVEL=silent` which should override config values. However, the config loading happens in `preAction` which runs after the process starts.

### Issue 3: Output Stream Buffering

**Problem**: When running CLI as subprocess, stdout/stderr may not be fully flushed before the process exits.

**Solution**: The `ExitManager.exit()` method calls `output.ensureDrained()` before exiting, which waits for all output to complete.

### Issue 4: SDK Not Properly Initialized

**Problem**: The SDK requires initialization including TOML parser. If this fails silently, workflow operations may fail.

**Solution**: The `src/index.ts` includes a health check and TOML parser initialization with error handling that doesn't crash the CLI.

## Test Configuration Best Practices

### Environment Variables

| Variable | Purpose | Recommended Value for Tests |
|----------|---------|----------------------------|
| `NODE_ENV` | Node environment | `test` |
| `TEST_MODE` | Enable test mode (headless) | `true` |
| `LOG_DIR` | Log output directory | Test-specific directory |
| `DISABLE_LOG_TERMINAL` | Disable terminal logging | `true` |
| `DISABLE_SDK_LOGS` | Disable SDK logging | `true` |
| `SDK_LOG_LEVEL` | SDK log level | `silent` |
| `STORAGE_DIR` | Storage directory | Test-specific directory |

### Storage Isolation

Each test should use a unique storage directory to ensure isolation. The `TestHelper` class creates isolated directories:

```typescript
this.storageDir = join(outputDir, "../storage", testName);
```

### CLIRunner Configuration

The `CLIRunner` should be configured with:

```typescript
const runner = new CLIRunner(undefined, testOutputDir);
runner.setStorageDir(helper.getStorageDir());
```

## Debugging Test Failures

### Enable Debug Output

Set environment variables in `CLIRunner.defaultEnv`:

```typescript
SDK_LOG_LEVEL: "debug",  // Enable SDK debug logs
```

### Check Output Files

Tests save command output to:
```
__tests__/outputs/<subdir>/<number>_<command>.log
```

### Common Failure Causes

1. **Exit code 1**: Command failed or threw an unhandled error
2. **Exit code 2**: Validation error (see `error-handler.ts`)
3. **Exit code 3**: File operation error
4. **Exit code 4**: API error

## Test Helper Classes

### TestHelper

Provides:
- `getTempDir()` - Temporary files directory
- `getStorageDir()` - Isolated storage directory
- `cleanup()` - Clean up resources after test

### WorkflowTestHelper

Provides:
- `createStandaloneWorkflowWithLLM()` - Create workflow configs
- `copyWorkflowFixtureToTemp()` - Copy fixture files
- `writeWorkflowToTemp()` - Write dynamic workflow configs

### CLIRunner

Provides:
- `run(args, options)` - Execute CLI commands
- `setStorageDir(dir)` - Set storage directory
- Output file saving for debugging

## References

- Exit Manager: `src/utils/exit-manager.ts`
- Error Handler: `src/utils/error-handler.ts`
- CLI Output: `src/utils/output.ts`
- Configuration Loader: `src/config/cli/loader.ts`
- Workflow Adapter: `src/adapters/workflow-adapter.ts`
- Test Utilities: `__tests__/utils/`
