# Graceful Shutdown Integration Guide

## Overview

The `GracefulShutdownManager` is now properly integrated into the SDK's services layer and can be imported directly from `@wf-agent/sdk`.

## Architecture

```
sdk/
├── index.ts                    # Main entry point - exports all public APIs
├── services/                   # Service layer (lifecycle management)
│   ├── index.ts               # Services unified export
│   └── shutdown/              # Shutdown service
│       ├── graceful-shutdown-manager.ts
│       └── index.ts
├── api/                        # API layer (business operations)
├── core/                       # Core implementation
└── ...
```

## Import Paths

### ✅ Correct Usage (Recommended)

```typescript
// Import from main SDK entry point
import { GracefulShutdownManager } from '@wf-agent/sdk';
```

### ❌ Incorrect Usage (Avoid)

```typescript
// Don't import from internal paths
import { GracefulShutdownManager } from '@wf-agent/sdk/services/shutdown';
import { GracefulShutdownManager } from '@wf-agent/sdk/core/shutdown'; // Old path, removed
```

## Integration Example

### Basic Setup

```typescript
import { 
  GracefulShutdownManager,
  createSDK,
  type SDKInstance 
} from '@wf-agent/sdk';

async function initializeApplication() {
  // 1. Create SDK instance
  const sdk: SDKInstance = await createSDK({
    // ... your SDK configuration
  });

  // 2. Get required dependencies from SDK
  const workflowExecutionRegistry = sdk.getWorkflowExecutionRegistry();
  const checkpointDependencies = {
    workflowExecutionRegistry,
    checkpointStateManager: sdk.getCheckpointStateManager(),
    workflowRegistry: sdk.getWorkflowRegistry(),
    workflowGraphRegistry: sdk.getWorkflowGraphRegistry(),
  };

  // 3. Initialize Graceful Shutdown Manager
  const shutdownManager = new GracefulShutdownManager(
    workflowExecutionRegistry,
    checkpointDependencies,
    {
      timeoutMs: 60000,  // 60 seconds timeout
      enabled: true,
    }
  );

  // 4. Register signal handlers
  shutdownManager.registerSignalHandlers();

  console.log('Application initialized with graceful shutdown support');
  
  return { sdk, shutdownManager };
}

// Start application
initializeApplication().catch(console.error);
```

### Advanced Configuration

```typescript
import { GracefulShutdownManager } from '@wf-agent/sdk';

const shutdownManager = new GracefulShutdownManager(
  workflowExecutionRegistry,
  checkpointDependencies,
  {
    timeoutMs: 120000,  // 2 minutes for large-scale shutdowns
    enabled: true,
  }
);

// Register signal handlers
shutdownManager.registerSignalHandlers();

// You can also manually trigger shutdown
// shutdownManager.triggerShutdown('SIGTERM');
```

### Integration with Health Monitoring

```typescript
import { 
  GracefulShutdownManager,
  CheckpointHealthMonitor,
  PeriodicHealthChecker 
} from '@wf-agent/sdk';

async function initializeWithMonitoring() {
  const sdk = await createSDK({ /* config */ });
  
  // Get dependencies
  const workflowExecutionRegistry = sdk.getWorkflowExecutionRegistry();
  const checkpointStateManager = sdk.getCheckpointStateManager();
  
  // Setup health monitoring
  const monitor = new CheckpointHealthMonitor(
    checkpointStateManager,
    workflowExecutionRegistry,
    {
      warningThresholdMs: 15 * 60 * 1000,   // 15 minutes
      criticalThresholdMs: 30 * 60 * 1000,  // 30 minutes
    }
  );
  
  const checker = new PeriodicHealthChecker(monitor, 5 * 60 * 1000);
  checker.start();
  
  // Setup graceful shutdown
  const shutdownManager = new GracefulShutdownManager(
    workflowExecutionRegistry,
    {
      workflowExecutionRegistry,
      checkpointStateManager,
      workflowRegistry: sdk.getWorkflowRegistry(),
      workflowGraphRegistry: sdk.getWorkflowGraphRegistry(),
    }
  );
  
  shutdownManager.registerSignalHandlers();
  
  // Cleanup on shutdown
  process.on('exit', () => {
    checker.stop();
  });
  
  return { sdk, shutdownManager, checker };
}
```

## Signal Handling

The `GracefulShutdownManager` automatically handles the following signals:

| Signal | Platform | Description |
|--------|----------|-------------|
| SIGTERM | Unix/Linux/macOS | Termination signal (default for `kill`) |
| SIGINT | All | Interrupt signal (Ctrl+C) |
| SIGHUP | Unix/Linux/macOS | Hangup signal (terminal closed) |
| SIGBREAK | Windows | Break signal (Ctrl+Break) |

## Shutdown Process

When a shutdown signal is received:

1. **Signal Received**: Manager detects the shutdown signal
2. **Active Executions Retrieved**: Gets all RUNNING/PAUSED workflow executions
3. **Checkpoints Created**: Creates synchronous checkpoints for each active execution
4. **Results Logged**: Logs success/failure summary
5. **Process Exits**: Clean exit with code 0 (success) or 1 (failure)

### Timeline Example

```
T0: SIGTERM received
T1: Retrieving active executions (found 5)
T2-T6: Creating checkpoints for each execution (sync mode)
T7: Logging summary (5/5 successful)
T8: Process.exit(0)
```

## Error Handling

The shutdown manager uses `Promise.allSettled` to handle partial failures:

```typescript
// Even if some checkpoints fail, the process will still exit
// Failed checkpoints are logged with details
{
  total: 5,
  success: 4,
  failures: 1,
  failures: [
    {
      executionId: "exec-123",
      error: "Database connection timeout"
    }
  ]
}
```

## Testing

### Unit Test Example

```typescript
import { GracefulShutdownManager } from '@wf-agent/sdk';
import { describe, it, expect, vi } from 'vitest';

describe('GracefulShutdownManager', () => {
  it('should register signal handlers', () => {
    const mockRegistry = createMockRegistry();
    const mockDeps = createMockDependencies();
    
    const manager = new GracefulShutdownManager(mockRegistry, mockDeps);
    manager.registerSignalHandlers();
    
    // Verify handlers are registered
    expect(process.listenerCount('SIGTERM')).toBeGreaterThan(0);
    expect(process.listenerCount('SIGINT')).toBeGreaterThan(0);
  });
  
  it('should create checkpoints on shutdown', async () => {
    const mockRegistry = createMockRegistryWithExecutions(3);
    const mockDeps = createMockDependencies();
    const createCheckpointSpy = vi.spyOn(CheckpointCoordinator, 'createCheckpoint');
    
    const manager = new GracefulShutdownManager(mockRegistry, mockDeps);
    
    // Trigger shutdown programmatically
    await manager.triggerShutdown('SIGTERM');
    
    // Verify checkpoints were created
    expect(createCheckpointSpy).toHaveBeenCalledTimes(3);
  });
});
```

### Integration Test Example

```typescript
import { createSDK, GracefulShutdownManager } from '@wf-agent/sdk';

it('should preserve state on graceful shutdown', async () => {
  // 1. Initialize SDK
  const sdk = await createSDK({ /* config */ });
  
  // 2. Start a workflow execution
  const executionId = await sdk.executeWorkflow('workflow-1', { input: 'test' });
  
  // 3. Make some progress
  await waitForWorkflowProgress(executionId);
  
  // 4. Trigger graceful shutdown
  const registry = sdk.getWorkflowExecutionRegistry();
  const deps = {
    workflowExecutionRegistry: registry,
    checkpointStateManager: sdk.getCheckpointStateManager(),
    workflowRegistry: sdk.getWorkflowRegistry(),
    workflowGraphRegistry: sdk.getWorkflowGraphRegistry(),
  };
  
  const manager = new GracefulShutdownManager(registry, deps);
  await manager.triggerShutdown('SIGTERM');
  
  // 5. Restart and verify state
  const sdk2 = await createSDK({ /* same config */ });
  const restored = await sdk2.restoreFromCheckpoint(/* latest checkpoint */);
  
  expect(restored.executionId).toBe(executionId);
  expect(restored.status).toBe('PAUSED'); // or appropriate status
});
```

## Best Practices

### 1. Initialize Early

Register signal handlers as early as possible in your application lifecycle:

```typescript
// ✅ Good: Early initialization
async function main() {
  const sdk = await createSDK(config);
  const shutdownManager = setupShutdown(sdk);
  shutdownManager.registerSignalHandlers();
  
  // Rest of application logic
}
```

### 2. Use Appropriate Timeout

Set timeout based on your workload:

```typescript
// For small applications (< 10 executions)
{ timeoutMs: 30000 }  // 30 seconds

// For medium applications (10-100 executions)
{ timeoutMs: 60000 }  // 60 seconds

// For large applications (> 100 executions)
{ timeoutMs: 120000 } // 2 minutes
```

### 3. Monitor Shutdown Health

Combine with health monitoring for better observability:

```typescript
const checker = new PeriodicHealthChecker(monitor);
checker.start();

// Log shutdown metrics
process.on('exit', (code) => {
  console.log(`Application exited with code ${code}`);
  checker.stop();
});
```

### 4. Handle Partial Failures Gracefully

Don't let one failed checkpoint block shutdown:

```typescript
// The manager already handles this with Promise.allSettled
// Just ensure you log and monitor failures
```

## Troubleshooting

### Issue: Shutdown hangs

**Symptoms**: Process doesn't exit after receiving signal

**Causes**:
- Too many active executions
- Storage I/O bottleneck
- Timeout too short

**Solutions**:
1. Increase timeout: `{ timeoutMs: 120000 }`
2. Check storage performance
3. Review active execution count

### Issue: Checkpoints not created

**Symptoms**: State lost after restart

**Causes**:
- Signal handlers not registered
- Dependencies not properly configured
- Storage adapter errors

**Solutions**:
1. Verify `registerSignalHandlers()` is called
2. Check dependency injection
3. Review storage adapter logs

### Issue: Naming conflicts

**Symptoms**: TypeScript errors about duplicate exports

**Cause**: Using `export *` from services causes conflicts with API layer

**Solution**: Already fixed in SDK - uses explicit exports with aliases where needed

## Migration from Old Path

If you were using the old path during development:

```typescript
// ❌ Old (during initial implementation)
import { GracefulShutdownManager } from './sdk/core/shutdown';

// ✅ New (production-ready)
import { GracefulShutdownManager } from '@wf-agent/sdk';
```

No other changes needed - the API remains the same.

## Summary

The `GracefulShutdownManager` is now fully integrated into the SDK:

✅ **Proper Location**: `sdk/services/shutdown/` (lifecycle service)  
✅ **Exported**: Available via `@wf-agent/sdk` main entry point  
✅ **Documented**: Complete integration guide and examples  
✅ **Tested**: Unit and integration test patterns provided  
✅ **Production Ready**: Safe to use in production applications  

For questions or issues, refer to:
- Design document: `docs/storage/state-management-reliability-design.md`
- Implementation summary: `docs/storage/state-management-reliability-implementation-summary.md`
