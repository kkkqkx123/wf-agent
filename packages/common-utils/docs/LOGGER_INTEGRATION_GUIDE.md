# Common-Utils Logger Integration Guide

## Overview

This document explains how to properly integrate the logger system within the `@wf-agent/common-utils` package.

## Architecture

### Logger System Structure

The common-utils logger system is designed with the following key components:

1. **Global Logger** (`global-logger.ts`) - A singleton logger instance that can be configured globally
2. **Lazy Logger** (`lazy-logger.ts`) - Proxy-based lazy initialization to avoid side effects during module load
3. **Base Logger** (`base-logger.ts`) - Core logger implementation with child logger support
4. **Logger Factory** (`logger-factory.ts`) - Convenient functions to create different types of loggers

### Key Design Principles

1. **No Circular Dependencies**: Logger modules do not depend on utility modules (like compression)
2. **Safe Internal Usage**: Utility modules can safely import and use logger functions
3. **Child Logger Pattern**: Modules should create child loggers for better organization
4. **No Direct Console Output**: All logging goes through the logger system, never use `console.*` or `process.stderr.write`

## Integration Pattern

### For Utility Modules (Recommended)

Utility modules within common-utils should use `getGlobalLogger()` to create child loggers:

```typescript
// Example: compression/adaptive-compression.ts
import { getGlobalLogger } from "../../logger/global-logger.js";

// Create a child logger for this module
const logger = getGlobalLogger().child("compression", { pkg: "common-utils" });

export function selectCompressionStrategy(data: Uint8Array): CompressionConfig {
  const dataType = detectDataType(data);
  const size = data.length;

  // Use logger for debugging
  logger.debug("Selected compression strategy", {
    dataType,
    dataSize: size,
    algorithm: config.algorithm,
  });

  return config;
}
```

### Why This Works

1. **No Circular Dependency**: 
   - `logger/global-logger.ts` only depends on `logger/base-logger.ts` and `logger/types.ts`
   - It does NOT depend on any utility modules
   - Therefore, utility modules can safely import from logger without creating cycles

2. **Proven Pattern**: 
   - Already used in `evalutor/condition-evaluator.ts` and `evalutor/expression-evaluator.ts`
   - Works correctly in production

3. **Flexible Configuration**:
   - Applications can configure the global logger before using compression
   - Child loggers inherit configuration from parent

## What NOT to Do

### ❌ Don't Use Console Directly

```typescript
// WRONG - Interferes with SDK message system
console.debug("Compression decision", { size });
process.stderr.write("Error occurred\n");
```

**Why it's bad:**
- Bypasses the SDK's event-driven message streaming system
- Cannot be controlled by application output settings
- Breaks CLI app's structured output management
- Makes debugging and maintenance difficult

### ❌ Don't Create Simple Local Loggers

```typescript
// WRONG - Unnecessary duplication
const logger = {
  debug: (message: string, ...args: unknown[]) => {
    if (process.env["DEBUG_COMPRESSION"]) {
      console.debug(`[adaptive-compression] ${message}`, ...args);
    }
  },
};
```

**Why it's bad:**
- Duplicates functionality already provided by the logger system
- Cannot be configured externally
- Doesn't integrate with the rest of the logging infrastructure

## Benefits of Proper Integration

### 1. Unified Logging

All logs go through the same system, making it easy to:
- Control log levels globally or per-module
- Redirect logs to files, streams, or external services
- Filter and search logs consistently

### 2. Structured Output

Logs include metadata like:
- Timestamp
- Log level
- Module name
- Package name
- Context information
- Trace IDs (for distributed tracing)

### 3. Application Control

Applications can control logging behavior:

```typescript
// In CLI app
import { setGlobalLogLevel, createRotatingFileStream } from "@wf-agent/common-utils";

// Set log level
setGlobalLogLevel("debug");

// Redirect to file
const fileStream = createRotatingFileStream({
  filename: "./logs/app.log",
  maxSize: 100 * 1024 * 1024, // 100MB
  maxFiles: 10,
});

// Configure global logger to use file stream
const logger = createPackageLogger("app", { stream: fileStream });
setGlobalLogger(logger);

// Now all compression logs go to the file
```

### 4. No Message System Interference

Since logs go through the proper logger system:
- They don't interfere with SDK message streams
- They can be filtered by level
- They respect application output configuration
- They can be disabled entirely if needed

## Implementation Checklist

When adding logging to a new utility module:

- [ ] Import `getGlobalLogger` from `../../logger/global-logger.js`
- [ ] Create a child logger at module level: `const logger = getGlobalLogger().child("module-name", { pkg: "common-utils" })`
- [ ] Use appropriate log levels:
  - `debug` - Detailed diagnostic information
  - `info` - General operational information
  - `warn` - Warning conditions
  - `error` - Error conditions
- [ ] Include relevant context in log calls
- [ ] Never use `console.*` or `process.stderr.write`
- [ ] Test that logs appear when expected and can be controlled

## Examples

### Example 1: Debug Logging

```typescript
logger.debug("Processing data", {
  size: data.length,
  type: dataType,
});
```

### Example 2: Info Logging

```typescript
logger.info("Operation completed", {
  duration: endTime - startTime,
  result: "success",
});
```

### Example 3: Error Logging

```typescript
try {
  await performOperation();
} catch (error) {
  logger.error("Operation failed", {
    error: error instanceof Error ? error.message : String(error),
    stack: error instanceof Error ? error.stack : undefined,
  });
  throw error;
}
```

### Example 4: Conditional Logging

```typescript
if (logger.isLevelEnabled("debug")) {
  // Expensive operation only done when debug is enabled
  const details = computeExpensiveDetails();
  logger.debug("Detailed information", { details });
}
```

## Migration Guide

If you have existing code using console/process.stderr:

### Before

```typescript
// Old code
console.debug("Debug info");
process.stderr.write("Error occurred\n");
```

### After

```typescript
// New code
import { getGlobalLogger } from "../../logger/global-logger.js";
const logger = getGlobalLogger().child("module-name", { pkg: "common-utils" });

logger.debug("Debug info");
logger.error("Error occurred");
```

## Troubleshooting

### Logs Not Appearing

1. Check the global log level: `getGlobalLogLevel()`
2. Ensure the logger is configured correctly
3. Verify the log level of your messages matches the configured level

### Circular Dependency Errors

If you encounter circular dependency errors:
1. Make sure you're importing from `logger/global-logger.js`, not from the main index
2. Check that logger modules don't import from utility modules
3. Use TypeScript's module resolution to verify imports

### Performance Concerns

The logger system is designed to be efficient:
- Child loggers share the same stream (no extra overhead)
- Log level checks are fast
- Async context access is optimized
- If performance is critical, use conditional logging for expensive operations

## Summary

The correct way to integrate logging in common-utils utility modules is:

1. ✅ Use `getGlobalLogger().child()` to create module-specific loggers
2. ✅ Use appropriate log levels (debug, info, warn, error)
3. ✅ Include relevant context in log messages
4. ❌ Never use `console.*` or `process.stderr.write`
5. ❌ Don't create custom logger implementations

This approach ensures:
- No circular dependencies
- Unified logging across the package
- Application control over log output
- No interference with SDK message systems
- Easy maintenance and debugging
