# Configuration System Design Specification

## Overview

This document defines the design principles, architecture patterns, and usage guidelines for the configuration system in the SDK. It ensures consistent configuration handling across all modules and maintains clear architectural boundaries.

---

## Core Principles

### 1. Layer Separation

**All configuration processing must happen in the API layer (`sdk/api/shared/config/`), never in the Core layer.**

The Core layer should only **consume** configuration via Dependency Injection (DI), never **define**, **process**, or **load** it.

```
┌─────────────────────────────────────────┐
│  packages/types/src/config/             │
│  - Type definitions only                │
│  - No implementation logic              │
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│  sdk/api/shared/config/                 │
│  - ALL configuration processing         │
│  - processors/ (pure functions)         │
│  - *-config-loader.ts (file I/O)        │
│  - Default values & merging logic       │
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│  sdk/core/                              │
│  - Consumes configuration via DI        │
│  - NO config definition                 │
│  - NO config processing                 │
│  - NO config/* directory                │
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│  apps/                                  │
│  - Provides configuration via:          │
│    • SDKOptions                         │
│    • Config files (TOML/JSON)           │
│    • Environment variables              │
└─────────────────────────────────────────┘
```

### 2. Key Rules

1. **Type Definitions**: Always in `packages/types/src/config/`
2. **Processing Logic**: Always in `sdk/api/shared/config/processors/`
3. **File I/O**: Always in `sdk/api/shared/config/*-config-loader.ts`
4. **Core Layer**: Never contains any configuration files or directories
5. **Dependency Direction**: Core → API (for types and config), never reverse

---

## Directory Structure

```
sdk/api/shared/config/
├── index.ts                          # Main export file
├── types.ts                          # Internal config-related types
├── config-parser.ts                  # Generic config parser
├── config-transformer.ts             # Generic config transformer
├── config-file-loader.ts             # File loading utilities
├── config-utils.ts                   # Utility functions
├── parsers.ts                        # Parsing functions
├── json-parser.ts                    # JSON parsing
├── toml-parser.ts                    # TOML parsing
│
├── processors/                       # Pure function processors
│   ├── index.ts                      # Export all processors
│   ├── metrics.ts                    # Metrics config processor
│   ├── timeout.ts                    # Timeout config processor
│   ├── workflow.ts                   # Workflow config processor
│   ├── node-template.ts              # Node template processor
│   ├── trigger-template.ts           # Trigger template processor
│   ├── script.ts                     # Script config processor
│   ├── llm-profile.ts                # LLM profile processor
│   ├── prompt-template.ts            # Prompt template processor
│   ├── agent-loop.ts                 # Agent loop processor
│   └── batch-validators.ts           # Batch validation functions
│
├── validators/                       # Validation utilities
│   ├── index.ts
│   └── ... (validation helpers)
│
├── metrics-config-loader.ts          # Metrics config loader (with file I/O)
├── timeout-config-loader.ts          # Timeout config loader (with file I/O)
└── prompt-template-loader.ts         # Prompt template loader (with file I/O)
```

---

## Component Specifications

### 1. Type Definitions (`packages/types/src/config/`)

**Purpose**: Define TypeScript interfaces for configuration objects.

**Rules**:
- Only contain type/interface definitions
- No implementation logic
- Use JSDoc comments for documentation
- All fields should be optional with `?` unless required
- Provide default value hints in comments

**Example**:

```typescript
/**
 * Timeout Configuration Type Definitions
 * 
 * Defines types for timeout configuration across the SDK.
 * These timeouts control internal operation waiting periods.
 */

/**
 * Timeout configuration for different SDK operations
 * All values are in milliseconds
 */
export interface TimeoutConfig {
  /** Waiting for a single workflow execution to complete (default: 30000) */
  workflowExecutionCompletion?: number;
  
  /** Waiting for workflow execution to pause (default: 5000) */
  workflowExecutionPause?: number;
  
  /** Default timeout when no specific timeout is configured (default: 30000) */
  default?: number;
  
  /** Maximum allowed timeout in milliseconds (default: 300000 / 5 minutes) */
  maxAllowed?: number;
}
```

**Export Pattern**:

```typescript
// packages/types/src/config/index.ts
export * from "./storage.js";
export * from "./metrics.js";
export * from "./timeout.js";
// ... other config types
```

---

### 2. Processors (`sdk/api/shared/config/processors/`)

**Purpose**: Provide pure functions for processing, merging, and validating configuration.

**Rules**:
- ✅ Pure functions only (no side effects)
- ✅ No file I/O operations
- ✅ No external dependencies beyond types
- ✅ Deterministic output for same input
- ✅ Accept partial config, return full config with defaults
- ✅ Easy to unit test

**Naming Convention**:
- `merge{Name}WithDefaults()` - Merge user config with defaults
- `get{Name}EnvironmentDefaults()` - Get environment-specific defaults
- `validate{Name}()` - Validate configuration
- `transform{Name}()` - Transform configuration format

**Example**:

```typescript
/**
 * Timeout Configuration Processor
 * 
 * Provides functions for processing and merging timeout configuration.
 * This module handles the business logic for timeout config without file I/O.
 */

import type { TimeoutConfig } from "@wf-agent/types";

/**
 * Default timeout configuration
 * Matches current hardcoded values for backward compatibility
 */
const DEFAULT_TIMEOUT_CONFIG: Required<TimeoutConfig> = {
  workflowExecutionCompletion: 30000,
  workflowExecutionPause: 5000,
  default: 30000,
  maxAllowed: 300000,
};

/**
 * Merge user config with defaults
 * Performs shallow merge of timeout configuration
 * 
 * @param userConfig - User-provided partial configuration
 * @returns Merged configuration with defaults applied
 */
export function mergeTimeoutWithDefaults(
  userConfig: Partial<TimeoutConfig>
): Required<TimeoutConfig> {
  return {
    ...DEFAULT_TIMEOUT_CONFIG,
    ...userConfig,
  };
}

/**
 * Get default config for specific environment
 * Provides environment-optimized defaults
 * 
 * @param env - Environment name ("development" or "production")
 * @returns Environment-specific default configuration
 */
export function getTimeoutEnvironmentDefaults(
  env: "development" | "production"
): Required<TimeoutConfig> {
  if (env === "development") {
    return {
      ...DEFAULT_TIMEOUT_CONFIG,
      workflowExecutionCompletion: 15000, // Faster for dev
    };
  }
  
  return DEFAULT_TIMEOUT_CONFIG;
}
```

**Export Pattern**:

```typescript
// sdk/api/shared/config/processors/index.ts

// Timeout configuration processing function
export {
  mergeTimeoutWithDefaults,
  getTimeoutEnvironmentDefaults,
  validateTimeout,
  isWaitForever,
  toActualTimeout,
  WAIT_FOREVER,
} from "./timeout.js";
```

---

### 3. Loaders (`sdk/api/shared/config/*-config-loader.ts`)

**Purpose**: Handle file I/O operations for loading configuration from disk.

**Rules**:
- ✅ ONLY place for file I/O in config system
- ✅ Read file from disk
- ✅ Parse format (TOML/JSON)
- ✅ Call processor to merge with defaults
- ✅ Handle errors gracefully
- ✅ Log warnings/errors appropriately

**Naming Convention**: `{name}-config-loader.ts`

**Example**:

```typescript
/**
 * Timeout Configuration Loader
 * 
 * Loads timeout configuration from files with priority-based resolution.
 * This is the only module in the config system that performs file I/O for timeout config.
 * 
 * Configuration Priority (highest to lowest):
 * 1. SDKOptions.timeout (programmatic override)
 * 2. Config file (configs/timeout.toml or timeout.json)
 * 3. Environment-specific defaults (development/production)
 * 4. Hardcoded defaults (current values as fallback)
 */

import type { TimeoutConfig } from "@wf-agent/types";
import { mergeTimeoutWithDefaults } from "./processors/timeout.js";
import { parseToml } from "./toml-parser.js";
import { parseJson } from "./json-parser.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "TimeoutConfigLoader" });

/**
 * Load timeout configuration from TOML or JSON file
 * 
 * @param filePath - Path to configuration file
 * @returns Parsed and merged timeout configuration
 */
export async function loadTimeoutConfigFromFile(
  filePath: string
): Promise<Required<TimeoutConfig>> {
  try {
    const fs = await import('fs/promises');
    const path = await import('path');
    
    // Resolve absolute path
    const resolvedPath = path.resolve(filePath);
    
    // Check if file exists
    try {
      await fs.access(resolvedPath);
    } catch {
      logger.warn("Timeout config file not found", { filePath: resolvedPath });
      return mergeTimeoutWithDefaults({});
    }
    
    // Read file content
    const content = await fs.readFile(resolvedPath, 'utf-8');
    
    // Determine format based on extension
    const ext = path.extname(resolvedPath).toLowerCase();
    let parsed: unknown;
    
    if (ext === '.toml') {
      parsed = parseToml(content);
    } else if (ext === '.json') {
      parsed = parseJson(content);
    } else {
      throw new Error(`Unsupported config file format: ${ext}`);
    }
    
    logger.info("Loaded timeout config from file", { filePath: resolvedPath });
    return mergeTimeoutWithDefaults(parsed as Partial<TimeoutConfig>);
  } catch (error) {
    logger.warn("Failed to load timeout config from file, using defaults", { 
      filePath, 
      error: error instanceof Error ? error.message : String(error) 
    });
    return mergeTimeoutWithDefaults({});
  }
}
```

**Export Pattern**:

```typescript
// sdk/api/shared/config/index.ts

// Timeout configuration loader export (with file I/O)
export {
  loadTimeoutConfigFromFile,
} from "./timeout-config-loader.js";
```

---

### 4. SDKOptions Integration

**Purpose**: Allow programmatic configuration via SDK initialization.

**Location**: `sdk/api/shared/types/core-types.ts`

**Pattern**: Add optional config field to `SDKOptions` interface.

**Example**:

```typescript
import type { TimeoutConfig } from "@wf-agent/types";

export interface SDKOptions {
  // ... other options
  
  /** Timeout configuration for SDK operations */
  timeout?: TimeoutConfig;
  
  /** Metrics system configuration */
  metrics?: MetricsConfig;
}
```

---

### 5. DI Container Binding

**Purpose**: Bind configuration to DI container for consumption by Core layer.

**Location**: `sdk/core/di/container-config.ts`

**Pattern**: Use priority-based resolution in dynamic value binding.

**Example**:

```typescript
import type { TimeoutConfig } from "@wf-agent/types";
import type { SDKOptions } from "../../api/shared/types/core-types.js";
import { 
  loadTimeoutConfigFromFile,
  mergeTimeoutWithDefaults,
  getTimeoutEnvironmentDefaults,
} from "../../api/shared/config/index.js";

// TimeoutConfig - Load and merge timeout configuration with priority-based resolution
// Priority: SDKOptions.timeout > Config file > Environment defaults > Hardcoded defaults
container
  .bind(Identifiers.TimeoutConfig)
  .toDynamicValue(async (c: IContainer): Promise<Required<TimeoutConfig>> => {
    // Get SDK options from container
    const sdkOptions = c.get(Identifiers.SDKOptions) as SDKOptions | undefined;
    
    // Priority 1: SDKOptions.timeout (programmatic override)
    if (sdkOptions?.timeout) {
      logger.info("Using timeout config from SDKOptions");
      return mergeTimeoutWithDefaults(sdkOptions.timeout);
    }
    
    // Priority 2: Config file
    const configPaths = [
      './configs/timeout.toml',
      './configs/timeout.json',
    ];
    
    for (const path of configPaths) {
      try {
        const config = await loadTimeoutConfigFromFile(path);
        logger.info("Loaded timeout config from file", { path });
        return config;
      } catch {
        // Try next path
      }
    }
    
    // Priority 3: Environment-based defaults
    const env = process.env["NODE_ENV"] || "development";
    logger.info("Using environment-based timeout defaults", { env });
    return getTimeoutEnvironmentDefaults(env as "development" | "production");
  })
  .inSingletonScope();
```

**Service Identifier**:

```typescript
// sdk/core/di/service-identifiers.ts

/**
 * TimeoutConfig - Timeout Configuration
 * Provides configuration for timeout values across SDK operations
 */
export const TimeoutConfig: ServiceIdentifier<Required<import("@wf-agent/types").TimeoutConfig>> = 
  Symbol("TimeoutConfig");
```

---

### 6. Core Layer Consumption

**Purpose**: Consume configuration via DI in Core layer services.

**Rules**:
- ✅ Import from DI container using service identifier
- ✅ NEVER import directly from config files
- ✅ Use configuration as read-only
- ✅ Pass configuration as parameters to functions if needed

**Example - Correct Usage**:

```typescript
// In a Core layer service
import * as Identifiers from "../di/service-identifiers.js";
import type { IContainer } from "@wf-agent/common-utils";

export class MyService {
  private timeoutConfig: Required<TimeoutConfig>;
  
  constructor(container: IContainer) {
    this.timeoutConfig = container.get(Identifiers.TimeoutConfig);
  }
  
  async executeWithTimeout(operation: () => Promise<void>): Promise<void> {
    const timeout = this.timeoutConfig.default;
    // Use timeout...
  }
}
```

**Example - Incorrect Usage**:

```typescript
// ❌ WRONG: Direct import from config
import { DEFAULT_TIMEOUTS } from "../config/timeout-config.js";

// ❌ WRONG: Importing processor in Core layer
import { mergeTimeoutWithDefaults } from "../../api/shared/config/index.js";
```

---

## Configuration Categories

### 1. User-Facing Configuration

**Examples**: Metrics, Timeouts, Logging, Storage

**Location**: `sdk/api/shared/config/`

**Characteristics**:
- Can be set via SDKOptions
- Can be loaded from config files
- Should have environment-specific defaults
- Users may want to customize

### 2. Internal Constants

**Examples**: Protocol versions, magic numbers, fixed limits

**Location**: Within the module that uses them (NOT in a central config/)

**Characteristics**:
- Not meant to be configured
- Part of implementation details
- May still need to be exposed for testing

**Example**:

```typescript
// In the module itself, not in config/
export const MAX_RETRY_ATTEMPTS = 3;
export const PROTOCOL_VERSION = "1.0";
```

### 3. Runtime Configuration

**Examples**: Execution timeouts, retry delays

**Location**: Passed as parameters or via SDKOptions

**Characteristics**:
- Set at runtime per operation
- Not loaded from files
- Controlled by application code

---

## Implementation Guidelines

### File Naming Convention

| Type | Pattern | Example |
|------|---------|---------|
| Type Definition | `{name}.ts` in `packages/types/src/config/` | `metrics.ts`, `timeout.ts` |
| Processor | `processors/{name}.ts` | `processors/metrics.ts` |
| Loader | `{name}-config-loader.ts` | `metrics-config-loader.ts` |

### Processor Requirements

```typescript
/**
 * Must follow these rules:
 * 1. Pure functions only (no side effects)
 * 2. No file I/O operations
 * 3. No external dependencies beyond types
 * 4. Deterministic output for same input
 */
export function mergeXxxWithDefaults(userConfig: Partial<XxxConfig>): XxxConfig {
  // Merge logic here
  return mergedConfig;
}
```

### Loader Requirements

```typescript
/**
 * This is the ONLY place for file I/O in config system
 * Responsibilities:
 * 1. Read file from disk
 * 2. Parse format (TOML/JSON)
 * 3. Call processor to merge with defaults
 * 4. Handle errors gracefully
 */
export async function loadXxxConfigFromFile(filePath: string): Promise<XxxConfig> {
  // File I/O here
  const content = await fs.readFile(filePath, 'utf-8');
  const parsed = parseToml(content); // or parseJson
  return mergeXxxWithDefaults(parsed);
}
```

### Export Pattern

```typescript
// sdk/api/shared/config/index.ts

// Processors (pure functions)
export {
  mergeMetricsWithDefaults,
  getMetricsEnvironmentDefaults,
} from "./processors/metrics.js";

// Loaders (with file I/O)
export {
  loadMetricsConfigFromFile,
} from "./metrics-config-loader.js";
```

---

## Migration Checklist

When adding new configuration or refactoring existing configuration:

- [ ] Create type definition in `packages/types/src/config/{name}.ts`
- [ ] Export type from `packages/types/src/config/index.ts`
- [ ] Create processor in `sdk/api/shared/config/processors/{name}.ts`
- [ ] Export processor from `sdk/api/shared/config/processors/index.ts`
- [ ] Create loader in `sdk/api/shared/config/{name}-config-loader.ts` (if needs file I/O)
- [ ] Export loader from `sdk/api/shared/config/index.ts`
- [ ] Add config field to `SDKOptions` interface (if user-configurable)
- [ ] Add service identifier in `sdk/core/di/service-identifiers.ts`
- [ ] Update `container-config.ts` to load and bind config
- [ ] Update all consumers to use DI instead of direct imports
- [ ] Delete old files from `sdk/core/config/` (if migrating)
- [ ] Remove `sdk/core/config/` directory if empty
- [ ] Verify no Core→API circular dependencies
- [ ] Run tests to ensure functionality unchanged
- [ ] Update documentation

---

## Anti-Patterns to Avoid

### ❌ Don't: Put Config in Core Layer

```typescript
// WRONG
sdk/core/config/metrics-config.ts
sdk/core/config/timeout-config.ts
```

### ❌ Don't: Mix Processing and I/O

```typescript
// WRONG: Single file does everything
export function processConfig() { /* ... */ }
export async function loadFromFile() { /* ... */ }
```

### ❌ Don't: Create Circular Dependencies

```typescript
// WRONG
sdk/core/something.ts → imports from → sdk/api/config/
sdk/api/config/ → imports from → sdk/core/something.ts
```

### ❌ Don't: Import Config Directly in Core

```typescript
// WRONG: Core layer importing config directly
import { DEFAULT_TIMEOUTS } from "../config/timeout-config.js";
```

### ✅ Do: Follow Established Patterns

```typescript
// CORRECT: Same structure as prompt-template-loader
sdk/api/shared/config/
├── processors/prompt-template.ts  // Pure functions
└── prompt-template-loader.ts      // File I/O + processing
```

### ✅ Do: Use DI in Core Layer

```typescript
// CORRECT: Core layer consuming config via DI
constructor(container: IContainer) {
  this.config = container.get(Identifiers.TimeoutConfig);
}
```

---

## Benefits of This Architecture

### 1. Clear Separation of Concerns
- Types are separate from implementation
- Processing is separate from I/O
- Core doesn't know about config details

### 2. Testability
- Processors are pure functions → easy to unit test
- Loaders can be tested with mock file systems
- Core can be tested with mock configs

### 3. Flexibility
- Users can provide config via multiple methods
- Easy to add new config sources (database, remote API, etc.)
- Environment-specific defaults are centralized

### 4. Maintainability
- All config logic in one place
- Consistent patterns across all configs
- Easy to find and modify configuration behavior

### 5. No Circular Dependencies
- Clear dependency direction: Types → API → Core
- Core never depends on API implementation
- Only depends on API types (which is acceptable)

---

## Examples

### Example 1: Adding a New Configuration (Logging)

**Step 1**: Create type definition

```typescript
// packages/types/src/config/logging.ts
export interface LoggingConfig {
  level?: "debug" | "info" | "warn" | "error";
  format?: "json" | "text";
  output?: "console" | "file" | "both";
  filePath?: string;
}
```

**Step 2**: Export type

```typescript
// packages/types/src/config/index.ts
export * from "./logging.js";
```

**Step 3**: Create processor

```typescript
// sdk/api/shared/config/processors/logging.ts
import type { LoggingConfig } from "@wf-agent/types";

const DEFAULT_LOGGING_CONFIG: Required<LoggingConfig> = {
  level: "info",
  format: "text",
  output: "console",
  filePath: "./logs/app.log",
};

export function mergeLoggingWithDefaults(
  userConfig: Partial<LoggingConfig>
): Required<LoggingConfig> {
  return {
    ...DEFAULT_LOGGING_CONFIG,
    ...userConfig,
  };
}
```

**Step 4**: Export processor

```typescript
// sdk/api/shared/config/processors/index.ts
export { mergeLoggingWithDefaults } from "./logging.js";
```

**Step 5**: Add to SDKOptions

```typescript
// sdk/api/shared/types/core-types.ts
export interface SDKOptions {
  // ... other options
  logging?: LoggingConfig;
}
```

**Step 6**: Bind in DI container

```typescript
// sdk/core/di/container-config.ts
container
  .bind(Identifiers.LoggingConfig)
  .toDynamicValue((c: IContainer): Required<LoggingConfig> => {
    const sdkOptions = c.get(Identifiers.SDKOptions) as SDKOptions | undefined;
    
    if (sdkOptions?.logging) {
      return mergeLoggingWithDefaults(sdkOptions.logging);
    }
    
    return mergeLoggingWithDefaults({});
  })
  .inSingletonScope();
```

**Step 7**: Consume in Core

```typescript
// In a Core layer service
import * as Identifiers from "../di/service-identifiers.js";

export class LoggerService {
  private config: Required<LoggingConfig>;
  
  constructor(container: IContainer) {
    this.config = container.get(Identifiers.LoggingConfig);
  }
  
  log(message: string): void {
    // Use this.config.level, this.config.format, etc.
  }
}
```

---

### Example 2: Using Configuration in Workflow Operations

**Before (Incorrect)**:

```typescript
// ❌ WRONG: Direct import in workflow operations
import { DEFAULT_TIMEOUTS } from "../../../core/config/timeout-config.js";

export async function join(..., timeout: number = 0) {
  const timeoutMs = timeout > 0 ? timeout * 1000 : DEFAULT_TIMEOUTS.JOIN_COMPLETION;
  // ...
}
```

**After (Correct)**:

```typescript
// ✅ CORRECT: Use processor to get defaults
import { mergeTimeoutWithDefaults } from "../../../api/shared/config/index.js";

const DEFAULT_TIMEOUT_CONFIG = mergeTimeoutWithDefaults({});

export async function join(..., timeout: number = 0) {
  const timeoutMs = timeout > 0 ? timeout * 1000 : DEFAULT_TIMEOUT_CONFIG.joinCompletion;
  // ...
}
```

---

## Future Considerations

### Dynamic Configuration

If runtime configuration updates are needed:

```typescript
// Add to API layer
sdk/api/shared/config/config-watcher.ts
export class ConfigWatcher {
  watch(filePath: string, onChange: (config: unknown) => void): void
}
```

### Configuration Validation

Add schema validation using Zod or similar:

```typescript
sdk/api/shared/config/validators/timeout-validator.ts
export const TimeoutConfigSchema = z.object({
  workflowExecutionCompletion: z.number().positive(),
  // ...
});
```

### Configuration Documentation

Auto-generate config documentation from types:

```typescript
// Use JSDoc comments in type definitions
// Generate markdown docs automatically
```

---

## Conclusion

The configuration architecture must follow these principles:

1. **All configuration lives in the API layer**
2. **Core layer only consumes configuration via DI**
3. **Types are in packages/types**
4. **Processing is pure functions in processors/**
5. **File I/O is in *-config-loader.ts files**
6. **Consistent patterns across all configurations**

This ensures:
- ✅ No circular dependencies
- ✅ Clear architecture boundaries
- ✅ Easy maintenance and testing
- ✅ Flexible configuration options for users

By following this specification, we maintain a clean, testable, and maintainable configuration system that scales well as the SDK grows.
