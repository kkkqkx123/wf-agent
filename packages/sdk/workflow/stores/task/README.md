# Task Store

This directory contains task-related components for tracking execution task status and
managing triggered sub-workflow execution queues.

## Components

### `TaskRegistry` (`task-registry.ts`)

Global task registry managed by the DI container (singleton per SDK instance).
**Responsibilities:**

- Stores and manages task status throughout the lifecycle (QUEUED, RUNNING, COMPLETED, FAILED, CANCELLED, TIMEOUT)
- Tracks submit/start/complete timestamps, execution results, and error details
- Supports optional persistence via `TaskStorageAdapter` (in-memory by default)
- Implements the `TaskManager` interface for routing task cancellation to the appropriate manager
- Provides cleanup of expired completed/failed tasks

### `TaskQueue` (`task-queue.ts`)

Stateful queue manager held by `TriggeredSubworkflowHandler`, one instance per handler.
**Responsibilities:**

- Manages the queue of `WorkflowExecutionContext`s waiting to execute as triggered sub-workflows
- Coordinates task assignment to `WorkflowExecutionPool`
- Supports both synchronous (`submitSync`) and asynchronous (`submitAsync`) task submission
- Handles completion/failure/cancellation events and registry updates

## Usage in the Codebase

- **`sdk/core/di/container-config.ts`** — Both components registered in the DI container
- **`sdk/core/execution/execution-queue.ts`** — Generic execution queue depends on `TaskRegistry`
- **`sdk/workflow/execution/handlers/triggered-subworkflow-handler.ts`** — Primary user of `TaskQueue` (also implements `TaskManager`)
- **`sdk/api/shared/resources/tasks/task-resource-api.ts`** — Exposes `TaskRegistry` via public API
- **Exported from** `sdk/workflow/stores/index.ts`
