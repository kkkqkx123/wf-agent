# Terminal Service Design Document

## Overview

This document analyzes the current shell execution implementations and proposes a unified Terminal Service architecture to manage persistent terminal sessions with multi-shell support, working directory management, and environment variable maintenance.

## Current Implementation Analysis

### 1. backend-shell (`sdk/resources/predefined/tools/stateful/shell/backend-shell`)

**Current Implementation:**
- Simple backend process management using Node.js `child_process.spawn`
- Platform-based shell selection: Windows uses `cmd.exe`, others use `/bin/sh`
- Basic session management via shell ID
- Output collection through line-by-line monitoring

**Limitations:**
| Issue | Current Behavior | Impact |
|-------|------------------|--------|
| Fixed Shell Type | Windows: `cmd.exe`, Others: `/bin/sh` | Cannot use bash/powershell/git-bash/wsl on demand |
| No Working Directory Management | Uses `process.cwd()` | Cannot maintain session-specific pwd |
| No Environment Variable Support | Inherits `process.env` | Cannot customize environment per session |
| No Session Reuse | Creates new process each time | Resource inefficiency |
| No Persistence | In-memory only | Lost on restart |

### 2. run-shell (`sdk/resources/predefined/tools/stateless/shell/run-shell`)

**Current Implementation:**
- Stateless, synchronous shell command execution
- Platform-based shell selection: Windows uses `powershell.exe`, others use `/bin/bash`
- Timeout control with `TimeoutController`

**Limitations:**
- No session management (stateless by design)
- Fixed shell type per platform
- No working directory or environment variable control

### 3. Reference Implementation: roo-code (`ref/roo-code/src/integrations/terminal`)

**Key Features:**
- **Multi-shell support**: bash, zsh, fish, pwsh, cmd, powershell
- **Terminal Registry**: Singleton pattern for terminal lifecycle management
- **Session Reuse**: Intelligent terminal reuse based on working directory and task ID
- **Shell Integration**: VSCode shell integration for accurate output capture
- **Environment Variable Management**: Custom environment configuration per shell type
- **Output Processing**: Stream-based, throttled output with compression

**Architecture Highlights:**
```
TerminalRegistry (Singleton)
├── terminals: RooTerminal[]
├── getOrCreateTerminal(cwd, taskId, provider)
├── releaseTerminalsForTask(taskId)
└── cleanup()

RooTerminal (Interface)
├── busy, running, taskId
├── getCurrentWorkingDirectory()
├── runCommand(command, callbacks)
├── getUnretrievedOutput()
└── shellExecutionComplete(exitDetails)

RooTerminalProcess (EventEmitter)
├── command, isHot
├── run(command), continue(), abort()
└── getUnretrievedOutput()
```

## Proposed Architecture

### Directory Structure

```
sdk/core/services/terminal/
├── index.ts                      # Unified export
├── types.ts                      # Type definitions
├── terminal-service.ts           # Main service class
├── terminal-registry.ts          # Session registry (singleton)
├── terminal-session.ts           # Session abstraction
├── shell-detector.ts             # Shell type detection
├── shell-executor.ts             # Command execution logic
└── __tests__/
    ├── terminal-service.test.ts
    ├── terminal-registry.test.ts
    └── shell-detector.test.ts
```

### Core Types

```typescript
// types.ts

/**
 * Supported shell types
 */
export type ShellType =
  | 'bash'
  | 'zsh'
  | 'fish'
  | 'sh'
  | 'cmd'
  | 'powershell'
  | 'pwsh'
  | 'git-bash'
  | 'wsl';

/**
 * Terminal session configuration
 */
export interface TerminalSessionOptions {
  /** Shell type to use */
  shellType?: ShellType;
  /** Working directory */
  cwd?: string;
  /** Environment variables */
  env?: Record<string, string>;
  /** Whether to enable auto-approval for commands */
  enableAutoApproval?: boolean;
  /** Session timeout in milliseconds */
  timeout?: number;
}

/**
 * Terminal session state
 */
export interface TerminalSession {
  /** Unique session identifier */
  sessionId: string;
  /** Shell type */
  shellType: ShellType;
  /** Current working directory */
  cwd: string;
  /** Environment variables */
  env: Record<string, string>;
  /** Session status */
  status: 'idle' | 'busy' | 'terminated';
  /** Creation timestamp */
  createdAt: number;
  /** Last activity timestamp */
  lastActiveAt: number;
  /** Associated task ID (optional) */
  taskId?: string;
}

/**
 * Command execution options
 */
export interface ExecuteOptions {
  /** Command timeout in milliseconds */
  timeout?: number;
  /** Whether to check auto-approval */
  checkApproval?: boolean;
  /** Working directory override */
  cwd?: string;
  /** Environment variable override */
  env?: Record<string, string>;
}

/**
 * Command execution result
 */
export interface ExecuteResult {
  /** Whether execution succeeded */
  success: boolean;
  /** Standard output */
  stdout: string;
  /** Standard error */
  stderr: string;
  /** Exit code */
  exitCode: number;
  /** Error message if failed */
  error?: string;
  /** Session ID (for stateful execution) */
  sessionId?: string;
}

/**
 * Output retrieval options
 */
export interface OutputOptions {
  /** Regex filter pattern */
  filter?: string;
  /** Whether to include all output (not just new) */
  all?: boolean;
}
```

### Terminal Service Interface

```typescript
// terminal-service.ts

/**
 * Terminal Service
 * 
 * Provides unified terminal session management with:
 * - Multi-shell support (bash, zsh, fish, pwsh, cmd, powershell, git-bash, wsl)
 * - Working directory management
 * - Environment variable maintenance
 * - Session reuse and lifecycle management
 * - Integration with auto-approval service
 */
export class TerminalService {
  /**
   * Create a new terminal session
   */
  async createSession(options?: TerminalSessionOptions): Promise<TerminalSession>;

  /**
   * Get an existing session
   */
  getSession(sessionId: string): TerminalSession | undefined;

  /**
   * Get or create a session for a specific working directory
   * Implements intelligent session reuse
   */
  async getOrCreateSession(
    cwd: string,
    options?: TerminalSessionOptions
  ): Promise<TerminalSession>;

  /**
   * Execute a command in a session
   */
  async executeInSession(
    sessionId: string,
    command: string,
    options?: ExecuteOptions
  ): Promise<ExecuteResult>;

  /**
   * Execute a one-off command (stateless)
   */
  async executeOneOff(
    command: string,
    options?: TerminalSessionOptions & ExecuteOptions
  ): Promise<ExecuteResult>;

  /**
   * Get output from a session
   */
  async getOutput(
    sessionId: string,
    options?: OutputOptions
  ): Promise<string>;

  /**
   * Terminate a session
   */
  async terminateSession(sessionId: string): Promise<ExecuteResult>;

  /**
   * Release sessions associated with a task
   */
  releaseSessionsForTask(taskId: string): void;

  /**
   * Cleanup all sessions
   */
  cleanup(): Promise<void>;
}
```

### Shell Detector

```typescript
// shell-detector.ts

/**
 * Detects available shells on the current system
 */
export class ShellDetector {
  /**
   * Get the default shell for the current platform
   */
  getDefaultShell(): ShellType;

  /**
   * Check if a shell is available on the system
   */
  async isShellAvailable(shellType: ShellType): Promise<boolean>;

  /**
   * Get the executable path for a shell type
   */
  getShellPath(shellType: ShellType): string;

  /**
   * Get shell arguments for executing a command
   */
  getShellArgs(shellType: ShellType, command: string): string[];

  /**
   * Get all available shells on the system
   */
  async getAvailableShells(): Promise<ShellType[]>;
}
```

### Terminal Registry

```typescript
// terminal-registry.ts

/**
 * Terminal Registry (Singleton)
 * 
 * Manages all terminal sessions with:
 * - Session lifecycle management
 * - Intelligent session reuse
 * - Task-based session association
 */
export class TerminalRegistry {
  private static instance: TerminalRegistry;
  private sessions: Map<string, TerminalSession> = new Map();

  /**
   * Get singleton instance
   */
  static getInstance(): TerminalRegistry;

  /**
   * Register a new session
   */
  register(session: TerminalSession): void;

  /**
   * Get a session by ID
   */
  get(sessionId: string): TerminalSession | undefined;

  /**
   * Find an available session matching criteria
   */
  findAvailable(cwd: string, taskId?: string): TerminalSession | undefined;

  /**
   * Release a session
   */
  release(sessionId: string): void;

  /**
   * Release all sessions for a task
   */
  releaseForTask(taskId: string): void;

  /**
   * Cleanup all sessions
   */
  cleanup(): void;
}
```

## Integration with Existing Components

### 1. Integration with auto-approval

```typescript
// In terminal-service.ts executeInSession()

async executeInSession(
  sessionId: string,
  command: string,
  options?: ExecuteOptions
): Promise<ExecuteResult> {
  const session = this.getSession(sessionId);
  if (!session) {
    throw new Error(`Session not found: ${sessionId}`);
  }

  // Check auto-approval if enabled
  if (options?.checkApproval !== false) {
    const decision = getCommandDecision(
      command,
      this.config.allowedCommands ?? [],
      this.config.deniedCommands
    );

    if (decision === 'auto_deny') {
      return {
        success: false,
        stdout: '',
        stderr: 'Command denied by auto-approval',
        exitCode: -1,
        error: 'Command denied by auto-approval',
      };
    }

    if (decision === 'ask_user') {
      // Emit event for user approval
      // This will be handled by the tool executor
    }
  }

  // Execute command...
}
```

### 2. Refactoring backend-shell

```typescript
// backend-shell/handler.ts (refactored)

import { TerminalService } from '@wf-agent/core-services/terminal';

export function createBackendShellFactory() {
  const terminalService = TerminalService.getInstance();

  return () => ({
    execute: async (params: Record<string, unknown>): Promise<ShellOutputResult> => {
      const { command, shell_type, cwd, env } = params as {
        command: string;
        shell_type?: ShellType;
        cwd?: string;
        env?: Record<string, string>;
      };

      // Create or reuse session
      const session = await terminalService.getOrCreateSession(cwd ?? process.cwd(), {
        shellType: shell_type,
        env: env,
      });

      // Execute command
      const result = await terminalService.executeInSession(session.sessionId, command);

      return {
        success: result.success,
        content: result.stdout || result.stderr,
        stdout: result.stdout,
        stderr: result.stderr,
        exitCode: result.exitCode,
        shellId: session.sessionId,
        error: result.error,
      };
    },

    cleanup: () => {
      terminalService.cleanup();
    },
  });
}
```

### 3. Refactoring run-shell

```typescript
// run-shell/handler.ts (refactored)

import { TerminalService } from '@wf-agent/core-services/terminal';

export function createRunShellHandler(config?: RunShellConfig) {
  const terminalService = TerminalService.getInstance();

  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    const { command, timeout, shell_type, cwd, env } = params as {
      command: string;
      timeout?: number;
      shell_type?: ShellType;
      cwd?: string;
      env?: Record<string, string>;
    };

    // Execute one-off command
    const result = await terminalService.executeOneOff(command, {
      shellType: shell_type,
      cwd: cwd,
      env: env,
      timeout: timeout ? timeout * 1000 : config?.maxTimeout ?? 120000,
    });

    return {
      success: result.success,
      content: result.stdout || result.stderr || '(no output)',
      stdout: result.stdout,
      stderr: result.stderr,
      exitCode: result.exitCode,
      error: result.error,
    };
  };
}
```

## Implementation Phases

### Phase 1: Core Service Foundation

1. Create `sdk/core/services/terminal/` directory
2. Implement `types.ts` with all type definitions
3. Implement `ShellDetector` for multi-shell support
4. Implement `TerminalRegistry` for session management
5. Add unit tests for core components

### Phase 2: Service Implementation

1. Implement `TerminalService` main class
2. Implement `ShellExecutor` for command execution
3. Implement `TerminalSession` abstraction
4. Add integration with `auto-approval` service
5. Add comprehensive tests

### Phase 3: Tool Refactoring

1. Refactor `backend-shell` to use `TerminalService`
2. Refactor `run-shell` to use `TerminalService`
3. Update tool schemas to support new options
4. Add integration tests

### Phase 4: Enhancement

1. Add output processing optimization (streaming, compression)
2. Add session persistence (optional)
3. Add shell integration support (optional, for VSCode extension)
4. Performance optimization and monitoring

## Benefits

### 1. Architecture Benefits

- **Single Responsibility**: Terminal management is isolated in a dedicated service
- **Reusability**: Multiple tools share the same terminal management logic
- **Testability**: Service can be tested independently
- **Maintainability**: Centralized terminal logic for easier debugging

### 2. Functional Benefits

- **Multi-shell Support**: Users can choose bash/powershell/git-bash/wsl etc.
- **Working Directory Management**: Session-specific pwd with cd support
- **Environment Variable Maintenance**: Custom environment per session
- **Session Reuse**: Efficient resource utilization
- **Security**: Integration with auto-approval for command safety

### 3. Extensibility Benefits

- **Plugin Support**: Easy to add new shell types
- **Event-driven**: EventEmitter for extensibility
- **Configuration**: Flexible configuration options

## Comparison with roo-code

| Feature | roo-code | Proposed Terminal Service |
|---------|----------|---------------------------|
| Multi-shell Support | ✅ bash/zsh/fish/pwsh/cmd/powershell | ✅ bash/zsh/fish/sh/cmd/powershell/pwsh/git-bash/wsl |
| Session Management | ✅ TerminalRegistry | ✅ TerminalRegistry |
| Session Reuse | ✅ cwd + taskId based | ✅ cwd + taskId based |
| Working Directory | ✅ Shell integration | ✅ Explicit cwd parameter |
| Environment Variables | ✅ Shell-specific config | ✅ Per-session config |
| Output Processing | ✅ Stream + compression | ✅ Line-based (extensible) |
| Auto-approval | ❌ Not integrated | ✅ Integrated |
| VSCode Dependency | ✅ Required | ❌ Independent |
| Persistence | ❌ In-memory | ⚠️ Optional |

## Conclusion

Creating a dedicated Terminal Service is strongly recommended for the following reasons:

1. **Architectural Soundness**: Follows single responsibility principle and modular design
2. **Functional Requirements**: Supports multi-shell, working directory, and environment variable management
3. **Reusability**: Multiple tools can share terminal management capabilities
4. **Maintainability**: Centralized lifecycle management for easier debugging
5. **Security**: Better integration with auto-approval mechanism
6. **No External Dependencies**: Unlike roo-code, this service is independent of VSCode

The proposed architecture draws from roo-code's proven design while adapting it for the SDK's service-oriented architecture and adding auto-approval integration.
