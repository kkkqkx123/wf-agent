# Workflow Execution Instance

This document describes the structure, creation flow, and lifecycle management of WorkflowExecution as a workflow execution instance.

## Overview

WorkflowExecution is an execution instance of a Workflow, containing runtime data and state. WorkflowExecutionEntity is the entity wrapper for WorkflowExecution, providing data access interfaces and state management.

## Core Types

### WorkflowExecution

**Location**: `packages/types/src/workflow/definition.ts`

```typescript
interface WorkflowExecution {
  // Basic identifiers
  id: ID;                                    // Unique identifier for the execution instance
  workflowId: ID;                            // Associated Workflow ID
  workflowVersion: Version;                  // Workflow version
  
  // Execution status
  currentNodeId: ID;                         // Current executing node ID
  
  // Graph structure reference
  graph: WorkflowGraph;                      // Preprocessed graph (reference)
  
  // Variable management
  variables: WorkflowExecutionVariable[];    // Variable array (for persistence)
  variableScopes: VariableScopes;            // Four-layer scope variable storage
  
  // Input/Output
  input: Record<string, unknown>;            // Input data
  output: Record<string, unknown>;           // Output data
  
  // Execution history
  nodeResults: NodeExecutionResult[];        // Node execution results
  errors: unknown[];                         // Error information
  
  // Context data
  contextData?: Record<string, unknown>;     // Context data
  
  // Execution type and relationships
  executionType?: WorkflowExecutionType;     // Execution type
  forkJoinContext?: ForkJoinContext;         // Fork/Join context
  triggeredSubworkflowContext?: TriggeredSubworkflowContext;  // Triggered subworkflow context
}
```

### WorkflowExecutionType

```typescript
enum WorkflowExecutionType {
  MAIN = "MAIN",                           // Main execution
  FORK_JOIN = "FORK_JOIN",                 // Fork/Join child execution
  TRIGGERED_SUBWORKFLOW = "TRIGGERED_SUBWORKFLOW"  // Triggered subworkflow
}
```

### WorkflowExecutionStatus

**Location**: `packages/types/src/workflow/status.ts`

```typescript
enum WorkflowExecutionStatus {
  CREATED = "CREATED",       // Created
  RUNNING = "RUNNING",       // Running
  PAUSED = "PAUSED",         // Paused
  COMPLETED = "COMPLETED",   // Completed
  FAILED = "FAILED",         // Failed
  CANCELLED = "CANCELLED",   // Cancelled
  TIMEOUT = "TIMEOUT"        // Timeout
}
```

### VariableScopes

**Location**: `packages/types/src/workflow/scopes.ts`

```typescript
interface VariableScopes {
  global: Record<string, unknown>;      // Global scope
  execution: Record<string, unknown>;   // Execution scope
  local: Record<string, unknown>[];     // Local scope stack
  loop: Record<string, unknown>[];      // Loop scope stack
}
```

### NodeExecutionResult

**Location**: `packages/types/src/workflow/history.ts`

```typescript
interface NodeExecutionResult {
  nodeId: ID;                           // Node ID
  nodeType: NodeType;                   // Node type
  status: ExecutionStatus;              // Execution status
  step: number;                         // Execution step
  startTime: Timestamp;                 // Start time
  endTime: Timestamp;                   // End time
  executionTime: number;                // Execution duration
  error?: unknown;                      // Error information
  output?: unknown;                     // Output data
}
```

---

## WorkflowExecutionEntity

### Responsibilities

WorkflowExecutionEntity is the entity wrapper for WorkflowExecution, providing:
- Data access interfaces (getter/setter)
- Runtime state management
- Multiple state manager instances

**Location**: `sdk/workflow/entities/workflow-execution-entity.ts`

### Core Structure

```typescript
class WorkflowExecutionEntity {
  readonly id: string;                           // Execution ID
  
  private readonly workflowExecution: WorkflowExecution;  // WorkflowExecution data object
  readonly state: WorkflowExecutionState;        // Runtime state
  private readonly executionState: ExecutionState;  // Subgraph execution stack
  readonly messageHistoryManager: MessageHistory;   // Message history
  readonly variableStateManager: VariableState;     // Variable state
  
  abortController?: AbortController;             // Stop controller
  conversationManager?: ConversationSession;     // Conversation session
  triggerManager?: unknown;                      // Trigger management
  toolVisibilityCoordinator?: unknown;           // Tool visibility coordination
  
  private childAgentLoopIds: Set<string>;        // Child AgentLoop IDs
}
```

### Main Methods

#### Basic Property Access

```typescript
getWorkflowId(): string {
  return this.workflowExecution.workflowId;
}

getStatus(): WorkflowExecutionStatus {
  return this.state.status;
}

setStatus(status: WorkflowExecutionStatus): void {
  this.state.status = status;
}

getCurrentNodeId(): string {
  return this.workflowExecution.currentNodeId;
}

setCurrentNodeId(nodeId: string): void {
  this.workflowExecution.currentNodeId = nodeId;
}

getInput(): Record<string, unknown> {
  return this.workflowExecution.input;
}

getOutput(): Record<string, unknown> {
  return this.workflowExecution.output;
}

setOutput(output: Record<string, unknown>): void {
  this.workflowExecution.output = output;
}
```

#### Execution Result Management

```typescript
addNodeResult(result: NodeExecutionResult): void {
  this.workflowExecution.nodeResults.push(result);
}

getNodeResults(): NodeExecutionResult[] {
  return this.workflowExecution.nodeResults;
}

getErrors(): unknown[] {
  return this.workflowExecution.errors;
}
```

#### Variable Management

```typescript
getVariable(name: string): unknown {
  return this.variableStateManager.getVariable(name);
}

setVariable(name: string, value: unknown): void {
  this.variableStateManager.setVariable(name, value);
}

getAllVariables(): Record<string, unknown> {
  return this.variableStateManager.getAllVariables();
}

deleteVariable(name: string): boolean {
  return this.variableStateManager.deleteVariable(name);
}
```

#### Message Management

```typescript
addMessage(message: LLMMessage): void {
  this.messageHistoryManager.addMessage(message);
  if (this.conversationManager) {
    this.conversationManager.addMessage(message);
  }
}

getMessages(): LLMMessage[] {
  return this.messageHistoryManager.getMessages();
}

getRecentMessages(count: number): LLMMessage[] {
  return this.messageHistoryManager.getRecentMessages(count);
}
```

#### Interrupt Control

```typescript
pause(): void {
  this.state.pause();
}

resume(): void {
  this.state.resume();
}

stop(): void {
  this.state.cancel();
  this.abort();
}

shouldPause(): boolean {
  return this.state.shouldPause();
}

shouldStop(): boolean {
  return this.state.shouldStop();
}

interrupt(type: "PAUSE" | "STOP"): void {
  this.state.interrupt(type);
  if (type === "STOP") {
    this.abort();
  }
}
```

#### Subgraph Execution Stack

```typescript
enterSubgraph(workflowId: ID, parentWorkflowId: ID, input: unknown): void {
  this.executionState.enterSubgraph(workflowId, parentWorkflowId, input);
}

exitSubgraph(): void {
  this.executionState.exitSubgraph();
}

getCurrentSubgraphContext(): SubgraphContext | null {
  return this.executionState.getCurrentSubgraphContext();
}

getSubgraphStack(): SubgraphContext[] {
  return this.executionState.getSubgraphStack();
}
```

#### Child Execution Management

```typescript
registerChildExecution(childExecutionId: ID): void {
  if (!this.workflowExecution.triggeredSubworkflowContext) {
    this.workflowExecution.triggeredSubworkflowContext = {
      parentExecutionId: "",
      childExecutionIds: [],
      triggeredSubworkflowId: "",
    };
  }
  if (!this.workflowExecution.triggeredSubworkflowContext.childExecutionIds.includes(childExecutionId)) {
    this.workflowExecution.triggeredSubworkflowContext.childExecutionIds.push(childExecutionId);
  }
}

getChildExecutionIds(): ID[] {
  return this.workflowExecution.triggeredSubworkflowContext?.childExecutionIds || [];
}
```

---

## WorkflowExecution Creation Flow

### WorkflowExecutionBuilder

**Location**: `sdk/workflow/execution/factories/workflow-execution-builder.ts`

```typescript
class WorkflowExecutionBuilder {
  async build(workflowId: string, options: WorkflowExecutionOptions = {}): Promise<WorkflowExecutionEntity> {
    // 1. Get preprocessed graph from GraphRegistry
    const workflowGraph = this.getGraphRegistry().get(workflowId);
    
    if (!workflowGraph) {
      throw new ExecutionError(`Workflow '${workflowId}' not found or not preprocessed`);
    }
    
    // 2. Build WorkflowExecutionEntity from preprocessed graph
    const workflowExecutionEntity = await this.buildFromWorkflowGraph(workflowGraph, options);
    
    return workflowExecutionEntity;
  }
  
  private async buildFromWorkflowGraph(
    workflowGraph: WorkflowGraph,
    options: WorkflowExecutionOptions = {}
  ): Promise<WorkflowExecutionEntity> {
    // 1. Validate preprocessed graph
    if (!workflowGraph.nodes || workflowGraph.nodes.size === 0) {
      throw new RuntimeValidationError("Preprocessed graph must have at least one node");
    }
    
    const startNode = Array.from(workflowGraph.nodes.values()).find(n => n.type === "START");
    if (!startNode) {
      throw new RuntimeValidationError("Preprocessed graph must have a START node");
    }
    
    // 2. Create WorkflowExecution object
    const executionId = generateId();
    const workflowExecution: WorkflowExecution = {
      id: executionId,
      workflowId: workflowGraph.workflowId,
      workflowVersion: workflowGraph.workflowVersion,
      currentNodeId: startNode.id,
      graph: workflowGraph,
      variables: [],
      variableScopes: {
        global: {},
        execution: {},
        local: [],
        loop: [],
      },
      input: options.input || {},
      output: {},
      nodeResults: [],
      errors: [],
      executionType: "MAIN",
    };
    
    // 3. Initialize variables
    const variableCoordinator = this.getVariableCoordinator();
    variableCoordinator.initializeFromWorkflow(workflowGraph.variables || []);
    
    // 4. Create ExecutionState
    const executionState = new ExecutionState();
    
    // 5. Create ConversationSession
    const conversationManager = new ConversationSession({
      eventManager: this.getEventManager(),
      executionId: workflowExecution.id,
      workflowId: workflowGraph.workflowId,
    });
    
    // 6. Create WorkflowExecutionEntity
    const workflowExecutionEntity = new WorkflowExecutionEntity(workflowExecution, executionState, undefined, conversationManager);
    
    return workflowExecutionEntity;
  }
}
```

### Creation Flow Diagram

```
WorkflowExecutionBuilder.build(workflowId, options)
            ↓
GraphRegistry.get(workflowId)
            ↓
WorkflowGraph
            ↓
    [Validate Preprocessed Graph]
            ├─ Check nodes exist
            ├─ Check START node
            └─ Check END node
            ↓
    [Create WorkflowExecution Object]
            ├─ Generate executionId
            ├─ Set workflowId
            ├─ Set currentNodeId = startNode.id
            ├─ Reference WorkflowGraph
            ├─ Initialize variableScopes
            └─ Set input
            ↓
    [Initialize Components]
            ├─ VariableCoordinator.initializeFromWorkflow()
            ├─ new ExecutionState()
            └─ new ConversationSession()
            ↓
new WorkflowExecutionEntity(workflowExecution, executionState, ...)
            ↓
WorkflowExecutionRegistry.register(workflowExecutionEntity)
```

---

## WorkflowExecution Execution Flow

### WorkflowExecutor

**Location**: `sdk/workflow/execution/executors/workflow-executor.ts`

```typescript
class WorkflowExecutor {
  async executeWorkflow(workflowExecutionEntity: WorkflowExecutionEntity): Promise<WorkflowExecutionResult> {
    const executionId = workflowExecutionEntity.id;
    const workflowId = workflowExecutionEntity.getWorkflowId();
    
    // 1. Verify workflow graph exists
    const workflowGraph = this.graphRegistry.get(workflowId);
    if (!workflowGraph) {
      throw new Error(`Graph not found for workflow: ${workflowId}`);
    }
    
    // 2. Create WorkflowExecutionCoordinator
    const workflowExecutionCoordinator = this.workflowExecutionCoordinatorFactory.create(workflowExecutionEntity);
    
    // 3. Execute workflow
    const result = await workflowExecutionCoordinator.execute();
    
    return result;
  }
}
```

### WorkflowExecutionCoordinator

**Location**: `sdk/workflow/execution/coordinators/workflow-execution-coordinator.ts`

```typescript
class WorkflowExecutionCoordinator {
  async execute(): Promise<WorkflowExecutionResult> {
    const executionId = this.workflowExecutionEntity.id;
    const startTime = this.workflowExecutionEntity.getStartTime();
    
    // Execution loop
    while (true) {
      // 1. Check interrupt status
      if (this.interruptionManager.shouldPause()) {
        throw new WorkflowExecutionInterruptedException("Workflow execution paused", "PAUSE", ...);
      }
      
      if (this.interruptionManager.shouldStop()) {
        throw new WorkflowExecutionInterruptedException("Workflow execution stopped", "STOP", ...);
      }
      
      // 2. Get current node
      const currentNodeId = this.workflowExecutionEntity.getCurrentNodeId();
      if (!currentNodeId) break;
      
      const graphNode = this.navigator.getGraph().getNode(currentNodeId);
      if (!graphNode) break;
      
      // 3. Execute node
      const result = await this.nodeExecutionCoordinator.executeNode(
        this.workflowExecutionEntity,
        currentNode
      );
      
      // 4. Record result
      this.workflowExecutionEntity.addNodeResult(result);
      
      // 5. Move to next node
      if (result.status === "COMPLETED") {
        const nextNode = this.navigator.getNextNode(currentNodeId);
        if (nextNode && nextNode.nextNodeId) {
          this.workflowExecutionEntity.setCurrentNodeId(nextNode.nextNodeId);
        } else {
          break;
        }
      } else {
        break;
      }
    }
    
    // 6. Build execution result
    const endTime = this.workflowExecutionEntity.getEndTime() || Date.now();
    const executionTime = endTime - (startTime || Date.now());
    
    return {
      executionId,
      output: this.workflowExecutionEntity.getOutput(),
      executionTime,
      nodeResults: this.workflowExecutionEntity.getNodeResults(),
      metadata: {
        status: this.workflowExecutionEntity.getStatus(),
        startTime: startTime || Date.now(),
        endTime,
        executionTime,
        nodeCount: this.workflowExecutionEntity.getNodeResults().length,
        errorCount: this.workflowExecutionEntity.getErrors().length,
      },
    };
  }
}
```

### Execution Flow Diagram

```
WorkflowExecutor.executeWorkflow(workflowExecutionEntity)
            ↓
WorkflowExecutionCoordinator.execute()
            ↓
    [Execution Loop]
            ├─ Check interrupt status
            │   ├─ shouldPause() → Throw interrupt exception
            │   └─ shouldStop() → Throw interrupt exception
            ├─ Get current node
            │   └─ currentNodeId = workflowExecutionEntity.getCurrentNodeId()
            ├─ NodeExecutionCoordinator.executeNode()
            │   ├─ Trigger NODE_STARTED event
            │   ├─ Create checkpoint (optional)
            │   ├─ Execute BEFORE_EXECUTE Hook
            │   ├─ Execute node logic
            │   ├─ Execute AFTER_EXECUTE Hook
            │   ├─ Create checkpoint (optional)
            │   └─ Trigger NODE_COMPLETED event
            ├─ Record execution result
            │   └─ workflowExecutionEntity.addNodeResult(result)
            └─ Move to next node
                └─ workflowExecutionEntity.setCurrentNodeId(nextNodeId)
            ↓
    [Build Execution Result]
            └─ WorkflowExecutionResult
```

---

## WorkflowExecution Lifecycle Management

### WorkflowLifecycleCoordinator

**Location**: `sdk/workflow/execution/coordinators/workflow-lifecycle-coordinator.ts`

```typescript
class WorkflowLifecycleCoordinator {
  // Execute workflow
  async execute(workflowId: string, options: WorkflowExecutionOptions = {}): Promise<WorkflowExecutionResult> {
    // Step 1: Build WorkflowExecutionEntity
    const workflowExecutionEntity = await this.workflowExecutionBuilder.build(workflowId, options);
    
    // Step 2: Register WorkflowExecutionEntity
    this.workflowExecutionRegistry.register(workflowExecutionEntity);
    
    // Step 3: Start workflow execution
    await this.workflowStateTransitor.startWorkflowExecution(workflowExecutionEntity);
    
    // Step 4: Execute workflow
    const result = await this.workflowExecutor.executeWorkflow(workflowExecutionEntity);
    
    // Step 5: Update status based on execution result
    if (result.metadata?.status === "COMPLETED") {
      await this.workflowStateTransitor.completeWorkflowExecution(workflowExecutionEntity, result);
    } else {
      await this.workflowStateTransitor.failWorkflowExecution(workflowExecutionEntity, error);
    }
    
    return result;
  }
  
  // Pause workflow execution
  async pauseWorkflowExecution(executionId: string): Promise<void> {
    const workflowExecutionEntity = this.workflowExecutionRegistry.get(executionId);
    
    // 1. Request pause
    workflowExecutionEntity.interrupt("PAUSE");
    
    // 2. Delegate state transition
    await this.workflowStateTransitor.pauseWorkflowExecution(workflowExecutionEntity);
  }
  
  // Resume workflow execution
  async resumeWorkflowExecution(executionId: string): Promise<WorkflowExecutionResult> {
    const workflowExecutionEntity = this.workflowExecutionRegistry.get(executionId);
    
    // 1. Delegate state transition
    await this.workflowStateTransitor.resumeWorkflowExecution(workflowExecutionEntity);
    
    // 2. Reset interrupt status
    workflowExecutionEntity.resetInterrupt();
    
    // 3. Continue execution
    return await this.workflowExecutor.executeWorkflow(workflowExecutionEntity);
  }
  
  // Stop workflow execution
  async stopWorkflowExecution(executionId: string): Promise<void> {
    const workflowExecutionEntity = this.workflowExecutionRegistry.get(executionId);
    
    // 1. Request stop
    workflowExecutionEntity.interrupt("STOP");
    
    // 2. Delegate state transition
    await this.workflowStateTransitor.cancelWorkflowExecution(workflowExecutionEntity, "user_requested");
    
    // 3. Cascade cancel child executions
    await this.workflowStateTransitor.cascadeCancel(executionId);
    
    // 4. Cleanup child AgentLoops
    await this.cleanupChildAgentLoops(executionId);
  }
}
```

### WorkflowStateTransitor

**Location**: `sdk/workflow/execution/coordinators/workflow-state-transitor.ts`

Responsible for atomic state transitions:

```typescript
class WorkflowStateTransitor {
  // Start workflow execution
  async startWorkflowExecution(workflowExecutionEntity: WorkflowExecutionEntity): Promise<void> {
    validateTransition(workflowExecutionEntity.id, previousStatus, "RUNNING");
    workflowExecutionEntity.setStatus("RUNNING");
    await emit(this.eventManager, buildWorkflowExecutionStartedEvent(workflowExecutionEntity));
    await emit(this.eventManager, buildWorkflowExecutionStateChangedEvent(...));
  }
  
  // Pause workflow execution
  async pauseWorkflowExecution(workflowExecutionEntity: WorkflowExecutionEntity): Promise<void> {
    validateTransition(workflowExecutionEntity.id, currentStatus, "PAUSED");
    workflowExecutionEntity.setStatus("PAUSED");
    await emit(this.eventManager, buildWorkflowExecutionPausedEvent(workflowExecutionEntity));
  }
  
  // Resume workflow execution
  async resumeWorkflowExecution(workflowExecutionEntity: WorkflowExecutionEntity): Promise<void> {
    validateTransition(workflowExecutionEntity.id, currentStatus, "RUNNING");
    workflowExecutionEntity.setStatus("RUNNING");
    await emit(this.eventManager, buildWorkflowExecutionResumedEvent(workflowExecutionEntity));
  }
  
  // Complete workflow execution
  async completeWorkflowExecution(workflowExecutionEntity: WorkflowExecutionEntity, result: WorkflowExecutionResult): Promise<void> {
    validateTransition(workflowExecutionEntity.id, previousStatus, "COMPLETED");
    workflowExecutionEntity.setStatus("COMPLETED");
    workflowExecutionEntity.state.complete();
    this.graphConversationSession.cleanup();
    await emit(this.eventManager, buildWorkflowExecutionCompletedEvent(workflowExecutionEntity, result));
  }
  
  // Fail workflow execution
  async failWorkflowExecution(workflowExecutionEntity: WorkflowExecutionEntity, error: Error): Promise<void> {
    validateTransition(workflowExecutionEntity.id, previousStatus, "FAILED");
    workflowExecutionEntity.setStatus("FAILED");
    workflowExecutionEntity.state.fail(error);
    await emit(this.eventManager, buildWorkflowExecutionFailedEvent(...));
  }
  
  // Cancel workflow execution
  async cancelWorkflowExecution(workflowExecutionEntity: WorkflowExecutionEntity, reason?: string): Promise<void> {
    validateTransition(workflowExecutionEntity.id, currentStatus, "CANCELLED");
    workflowExecutionEntity.setStatus("CANCELLED");
    workflowExecutionEntity.state.cancel();
    await emit(this.eventManager, buildWorkflowExecutionCancelledEvent(workflowExecutionEntity, reason));
  }
}
```

### State Transition Diagram

```
         ┌─────────┐
         │ CREATED │
         └────┬────┘
              │ startWorkflowExecution()
              ↓
         ┌─────────┐
    ┌───→│ RUNNING │←───┐
    │    └────┬────┘    │
    │         │         │
    │    pauseWorkflowExecution()  │ resumeWorkflowExecution()
    │         │         │
    │         ↓         │
    │    ┌─────────┐    │
    └────┤ PAUSED  ├────┘
         └────┬────┘
              │
              │ stopWorkflowExecution()
              ↓
         ┌───────────┐
         │ CANCELLED │
         └───────────┘
              
         ┌───────────┐
         │ COMPLETED │  (Normal completion)
         └───────────┘
              
         ┌───────────┐
         │  FAILED   │  (Execution failure)
         └───────────┘
```

---

## WorkflowExecutionRegistry

### Responsibilities

WorkflowExecutionRegistry is responsible for storing and querying WorkflowExecutionEntity instances.

**Location**: `sdk/workflow/stores/workflow-execution-registry.ts`

```typescript
class WorkflowExecutionRegistry {
  private workflowExecutionEntities: Map<string, WorkflowExecutionEntity> = new Map();
  
  // Register
  register(workflowExecutionEntity: WorkflowExecutionEntity): void {
    this.workflowExecutionEntities.set(workflowExecutionEntity.id, workflowExecutionEntity);
  }
  
  // Get
  get(executionId: string): WorkflowExecutionEntity | null {
    return this.workflowExecutionEntities.get(executionId) || null;
  }
  
  // Delete
  delete(executionId: string): void {
    this.workflowExecutionEntities.delete(executionId);
  }
  
  // Get all
  getAll(): WorkflowExecutionEntity[] {
    return Array.from(this.workflowExecutionEntities.values());
  }
  
  // Query by status
  getByStatus(status: WorkflowExecutionStatus): WorkflowExecutionEntity[] {
    return this.getAll().filter(entity => entity.getStatus() === status);
  }
  
  // Get running executions
  getRunning(): WorkflowExecutionEntity[] {
    return this.getByStatus("RUNNING");
  }
  
  // Get paused executions
  getPaused(): WorkflowExecutionEntity[] {
    return this.getByStatus("PAUSED");
  }
  
  // Cleanup completed executions
  cleanupCompleted(): number {
    const completedIds = this.getCompleted().map(e => e.id);
    for (const id of completedIds) {
      this.delete(id);
    }
    return completedIds.length;
  }
}
```

---

## Design Principles

### 1. Separation of Data and Behavior

- WorkflowExecution: Pure data object
- WorkflowExecutionEntity: Data encapsulation and access
- WorkflowExecutor: Execution logic
- WorkflowLifecycleCoordinator: Lifecycle management

### 2. State Encapsulation

- WorkflowExecutionEntity holds multiple state managers
- Data accessed through getter/setter
- State managers separate concerns

### 3. Stateless Executors

- WorkflowExecutor does not hold state
- All state passed through WorkflowExecutionEntity
- Facilitates testing and concurrency

### 4. Event-Driven

- State transitions trigger events
- Components communicate via events
- Supports external listeners

---

## Related Documentation

- [Overall Data Flow](./README.md)
- [Workflow Definition and Management](./workflow-definition.md)
- [Graph Preprocessing](./graph-preprocessing.md)
