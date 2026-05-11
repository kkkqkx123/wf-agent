# Cross-Platform Process Signal Handling Analysis

## Overview

This document analyzes the challenges and solutions for implementing cross-platform graceful shutdown mechanisms in Node.js applications, with a focus on Windows vs Unix signal handling differences.

## Problem Statement

The current `GracefulShutdownManager` implementation in `sdk/services/shutdown/graceful-shutdown-manager.ts` has significant limitations on Windows platforms:

1. **SIGTERM is not supported on Windows**: Windows does not natively support Unix-style signals
2. **Limited signal emulation**: Only SIGINT, SIGBREAK, and SIGHUP are emulated
3. **Inconsistent behavior**: Different signals behave differently across platforms
4. **No unified abstraction**: Application code must handle platform-specific logic

## Platform Differences

### Unix/Linux/macOS

**Supported Signals:**
- `SIGTERM` (15): Termination request - can be caught and handled gracefully
- `SIGINT` (2): Interrupt from keyboard (Ctrl+C) - can be caught
- `SIGHUP` (1): Hangup detected - typically when terminal closes
- `SIGQUIT` (3): Quit from keyboard - can be caught
- Many other signals available

**Characteristics:**
- All signals can be registered via `process.on()`
- Asynchronous operations can complete before exit
- Clean process termination possible
- Standard POSIX behavior

### Windows

**Supported Signals (Emulated):**
- `SIGINT` (2): Ctrl+C - **fully supported**
- `SIGBREAK` (21): Ctrl+Break - **Windows-specific**
- `SIGHUP` (1): Console window close - **gives ~10 seconds cleanup time**

**NOT Supported:**
- `SIGTERM`: Cannot be received by Node.js processes on Windows
- `SIGKILL`, `SIGSEGV`, etc.: Not available
- Most Unix signals are ignored

**Characteristics:**
- Limited to 3 signals maximum
- `process.kill()` can send limited signals to child processes
- When console window closes, Windows gives approximately 10 seconds before forced termination
- No equivalent to Unix `kill -SIGTERM <pid>`

## Key Findings from Node.js Documentation

### Signal Reception on Windows

From Node.js libuv documentation:

> On Windows, reception of some signals is emulated to provide cross-platform compatibility:
> - **SIGINT**: Delivered when user presses CTRL+C (similar to Unix), but NOT generated when terminal raw mode is enabled
> - **SIGBREAK**: Delivered when user presses CTRL+BREAK
> - **SIGHUP**: Generated when user closes the console window, giving the program approximately **10 seconds** to perform cleanup before Windows terminates it unconditionally

### Unsupported Signals on Windows

> On Windows, watchers for signals such as SIGILL, SIGABRT, SIGFPE, SIGSEGV, SIGTERM, and SIGKILL can be successfully created, but these signals are **never actually received** by the application.

### process.kill() Limitations

On Windows, `process.kill(pid, signal)` only supports:
- `SIGKILL`
- `SIGTERM` 
- `SIGINT`
- `SIGQUIT`

However, sending these signals will **unconditionally terminate** the target process without allowing graceful shutdown handlers to run.

## Current Implementation Issues

### Issue 1: SIGTERM Handler Registration

```typescript
// Current code - INEFFECTIVE on Windows
process.on("SIGTERM", () => this.handleShutdown("SIGTERM"));
```

On Windows, this handler will **never be called** because SIGTERM cannot be received.

### Issue 2: False Sense of Cross-Platform Support

The current implementation suggests full cross-platform support but actually provides:
- ✅ Full support on Unix (all 4 signals work)
- ⚠️ Partial support on Windows (only SIGINT, SIGBREAK, SIGHUP work)
- ❌ SIGTERM completely broken on Windows

### Issue 3: Missing Windows-Specific Mechanisms

Windows requires different approaches for graceful shutdown:
1. **Console events**: `SetConsoleCtrlHandler` API (not directly accessible in Node.js)
2. **WM_CLOSE message**: For GUI applications
3. **Service control**: For Windows Services
4. **Named pipes/IPC**: For inter-process communication

## Recommended Solutions

### Solution 1: Platform-Aware Signal Registration (Recommended)

Create a utility that registers appropriate handlers based on platform:

```typescript
// packages/common-utils/src/utils/process/platform-signals.ts

export interface PlatformSignalHandler {
  register(handler: (signal: string) => Promise<void>): void;
  unregister(): void;
}

export function createPlatformSignalHandler(): PlatformSignalHandler {
  const isWindows = process.platform === 'win32';
  const handlers: Array<[string, () => void]> = [];

  return {
    register(handler) {
      if (isWindows) {
        // Windows: Register only supported signals
        const winSignals: Array<'SIGINT' | 'SIGBREAK'> = ['SIGINT', 'SIGBREAK'];
        
        for (const signal of winSignals) {
          const listener = async () => {
            await handler(signal);
          };
          process.on(signal, listener);
          handlers.push([signal, listener]);
        }
        
        // Note: SIGHUP on Windows means console close
        // It's automatically handled but gives only 10 seconds
        const sighupListener = async () => {
          await handler('SIGHUP');
        };
        process.on('SIGHUP', sighupListener);
        handlers.push(['SIGHUP', sighupListener]);
      } else {
        // Unix: Register all standard signals
        const unixSignals = ['SIGTERM', 'SIGINT', 'SIGHUP'];
        
        for (const signal of unixSignals) {
          const listener = async () => {
            await handler(signal);
          };
          process.on(signal, listener);
          handlers.push([signal, listener]);
        }
      }
    },

    unregister() {
      for (const [signal, listener] of handlers) {
        process.removeListener(signal, listener);
      }
      handlers.length = 0;
    }
  };
}
```

### Solution 2: Unified Shutdown Interface

```typescript
// packages/common-utils/src/utils/process/graceful-shutdown.ts

export interface GracefulShutdownOptions {
  timeoutMs?: number;
  onShutdown: (signal: string, deadline: Date) => Promise<void>;
}

export class GracefulShutdownController {
  private handler: PlatformSignalHandler;
  private isShuttingDown = false;

  constructor(private options: GracefulShutdownOptions) {
    this.handler = createPlatformSignalHandler();
  }

  start(): void {
    this.handler.register(async (signal) => {
      if (this.isShuttingDown) {
        console.warn(`Shutdown already in progress, ignoring ${signal}`);
        return;
      }

      this.isShuttingDown = true;
      const timeout = this.options.timeoutMs ?? 15000;
      const deadline = new Date(Date.now() + timeout);

      try {
        await Promise.race([
          this.options.onShutdown(signal, deadline),
          new Promise((_, reject) => 
            setTimeout(() => reject(new Error(`Shutdown timeout after ${timeout}ms`)), timeout)
          )
        ]);
        
        console.log('Graceful shutdown completed');
        process.exit(0);
      } catch (error) {
        console.error('Graceful shutdown failed:', error);
        process.exit(1);
      }
    });
  }

  stop(): void {
    this.handler.unregister();
  }
}
```

### Solution 3: Enhanced GracefulShutdownManager

Update the existing manager to use platform-aware utilities:

```typescript
// sdk/services/shutdown/graceful-shutdown-manager.ts (updated)

import { createPlatformSignalHandler } from '@wf-agent/common-utils/utils/process/platform-signals';

export class GracefulShutdownManager {
  private signalHandler: ReturnType<typeof createPlatformSignalHandler>;

  constructor(...) {
    this.signalHandler = createPlatformSignalHandler();
  }

  registerSignalHandlers(): void {
    if (!this.config.enabled) {
      logger.info("Graceful shutdown is disabled");
      return;
    }

    logger.info("Registering graceful shutdown signal handlers", {
      platform: process.platform,
      supportedSignals: process.platform === 'win32' 
        ? ['SIGINT', 'SIGBREAK', 'SIGHUP']
        : ['SIGTERM', 'SIGINT', 'SIGHUP']
    });

    this.signalHandler.register(async (signal) => {
      await this.handleShutdown(signal as ShutdownSignal);
    });

    logger.info("Signal handlers registered successfully");
  }
}
```

## Implementation Plan

### Phase 1: Create Common Utilities Package

Location: `packages/common-utils/src/utils/process/`

Files to create:
1. `platform-signals.ts` - Platform-aware signal registration
2. `graceful-shutdown.ts` - Unified shutdown controller
3. `process-utils.ts` - General process utilities (exit codes, platform detection)
4. `index.ts` - Module exports

### Phase 2: Update GracefulShutdownManager

Refactor `sdk/services/shutdown/graceful-shutdown-manager.ts` to:
1. Use platform-aware signal handlers
2. Log which signals are actually registered
3. Handle Windows-specific timeout constraints (10-second limit on SIGHUP)
4. Provide better error messages for unsupported operations

### Phase 3: Add Documentation

Create usage examples showing:
1. How to use the utilities in different applications
2. Platform-specific considerations
3. Testing strategies for both platforms
4. Migration guide from old implementation

## Best Practices

### 1. Always Check Platform

```typescript
const isWindows = process.platform === 'win32';
if (isWindows) {
  // Use Windows-appropriate strategies
} else {
  // Use Unix strategies
}
```

### 2. Set Reasonable Timeouts

- **Unix**: 15-30 seconds is reasonable
- **Windows**: Maximum 10 seconds for SIGHUP (console close)
- Consider shorter timeouts for critical paths

### 3. Prioritize Critical Operations

During shutdown, prioritize:
1. Save state/checkpoints
2. Close database connections
3. Flush logs
4. Release resources

Skip non-critical operations if time is running out.

### 4. Test on Both Platforms

Never assume Unix behavior applies to Windows. Test:
- Signal reception
- Timeout behavior
- Cleanup completion
- Exit codes

### 5. Use Exit Codes Properly

```typescript
// Success
process.exit(0);

// Graceful shutdown failure
process.exit(1);

// Signal-based exit codes (Unix convention)
// 128 + signal_number
process.exit(143); // SIGTERM (128 + 15)
process.exit(130); // SIGINT (128 + 2)
```

## Alternative Approaches

### Approach A: IPC-Based Shutdown

For multi-process architectures, use inter-process communication:

```typescript
// Parent process sends shutdown command via IPC
child.send({ type: 'SHUTDOWN', signal: 'SIGTERM' });

// Child process receives and handles
process.on('message', (msg) => {
  if (msg.type === 'SHUTDOWN') {
    performGracefulShutdown(msg.signal);
  }
});
```

**Pros:**
- Works consistently across platforms
- Can target specific processes
- Supports complex shutdown sequences

**Cons:**
- Requires IPC setup
- More complex architecture
- Additional failure modes

### Approach B: File-Based Coordination

Use a shared file or lock to coordinate shutdown:

```typescript
// Watch for shutdown trigger file
fs.watchFile('.shutdown-trigger', () => {
  performGracefulShutdown();
});
```

**Pros:**
- Platform-independent
- Simple to implement
- Works across different processes

**Cons:**
- Slower response time
- File system dependency
- Race conditions possible

### Approach C: HTTP Health Check Endpoint

Expose an endpoint that triggers shutdown:

```typescript
app.post('/shutdown', async (req, res) => {
  res.send('Shutting down...');
  await gracefulShutdown();
  process.exit(0);
});
```

**Pros:**
- Works in containerized environments
- Can be triggered remotely
- Standard in cloud deployments

**Cons:**
- Requires HTTP server
- Security considerations
- Not suitable for CLI apps

## Conclusion

The current `GracefulShutdownManager` implementation has fundamental limitations on Windows due to the platform's lack of native signal support. The recommended approach is to:

1. **Create platform-aware utilities** in `@wf-agent/common-utils`
2. **Abstract signal handling** behind a unified interface
3. **Document platform differences** clearly
4. **Test thoroughly** on both Windows and Unix systems

This approach provides:
- ✅ True cross-platform compatibility
- ✅ Clear documentation of limitations
- ✅ Flexible extension points
- ✅ Consistent developer experience

## References

- [Node.js Process API](https://nodejs.org/api/process.html)
- [libuv Signal Handling](https://docs.libuv.org/en/v1.x/signal.html)
- [Windows Console Control Handlers](https://learn.microsoft.com/en-us/windows/console/setconsolectrlhandler)
- [POSIX Signal Specification](https://pubs.opengroup.org/onlinepubs/9699919799/basedefs/signal.h.html)

## Related Files

- `sdk/services/shutdown/graceful-shutdown-manager.ts` - Current implementation
- `packages/common-utils/src/utils/signal/` - Existing signal utilities
- `ref/roo-code/src/integrations/terminal/__tests__/TerminalProcessExec.*.spec.ts` - Platform-specific test examples
