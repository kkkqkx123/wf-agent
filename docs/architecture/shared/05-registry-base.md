# Shared Registry Base Components

## 1. Overview

The shared registry base components provide reusable storage and registry infrastructure used by both workflow and agent modules.

## 2. ExecutionStore

A generic in-memory store for execution entities:

```
ExecutionStore<T extends { id: ID }>
├── register(entity) → void
│   ├── Validate entity has ID
│   ├── Check for duplicate registration
│   └── Store in Map<ID, T>
│
├── get(id) → T | undefined
│   └── Lookup by ID
│
├── unregister(id) → boolean
│   ├── Remove from Map
│   └── Return true if existed
│
├── has(id) → boolean
│   └── Check existence
│
├── getAll() → T[]
│   └── Return all entities
│
├── query(predicate?) → T[]
│   ├── If predicate: filter entities
│   └── If no predicate: return all
│
├── count() → number
│   └── Return entity count
│
└── clear() → void
    └── Remove all entities
```

## 3. CoordinatorStore

A generic store for state coordinators:

```
CoordinatorStore<T>
├── register(entityId, coordinator) → void
│   └── Store coordinator with entity ID key
│
├── get(entityId) → T | undefined
│   └── Lookup by entity ID
│
├── unregister(entityId) → boolean
│   └── Remove coordinator
│
├── has(entityId) → boolean
│   └── Check existence
│
├── getAll() → T[]
│   └── Return all coordinators
│
└── clear() → void
    └── Remove all coordinators
```

## 4. TaskRegistry

Manages async task lifecycle:

```
TaskRegistry
├── registerTask(task) → string
│   ├── Assign task ID
│   ├── Initialize task state
│   └── Return task ID
│
├── updateTaskStatus(taskId, status) → void
│   ├── Update task status
│   ├── Track timestamps
│   └── Emit task events
│
├── getTask(taskId) → TaskInfo | null
│   └── Query task by ID
│
├── cancelTask(taskId) → boolean
│   ├── Mark task as cancelled
│   └── Return true if cancelled
│
├── listTasks(filter?) → TaskInfo[]
│   └── Query tasks with optional filter
│
├── getTaskStats() → TaskStats
│   └── Aggregate task statistics
│
└── clear() → void
    └── Remove all tasks
```

### TaskManager Interface

```typescript
interface TaskManager {
  cancelTask(taskId: string): Promise<boolean>;
  getTaskStatus(taskId: string): TaskInfo | null;
}
```

## 5. ToolRegistry

Manages tool registration and lookup:

```
ToolRegistry
├── registerTool(tool) → void
│   ├── Validate tool definition
│   ├── Check for duplicate registration
│   └── Store in registry
│
├── getTool(name) → Tool | undefined
│   └── Lookup by name
│
├── getAllTools() → Tool[]
│   └── Return all registered tools
│
├── getAvailableTools(filter?) → Tool[]
│   ├── Apply include/exclude filter
│   └── Return filtered tools
│
├── unregisterTool(name) → boolean
│   └── Remove tool from registry
│
├── hasTool(name) → boolean
│   └── Check tool existence
│
└── clear() → void
    └── Clear all tools
```

## 6. EventRegistry

The central event bus (detailed in [Event System](02-event-system.md)):

```
EventRegistry
├── emit(event) → Promise<void>
├── subscribe(eventType, handler) → Subscription
├── unsubscribe(subscription) → void
├── getEventHistory() → Event[]
└── clear() → void
```

## 7. ExecutionHierarchyRegistry

Manages parent-child execution relationships:

```
ExecutionHierarchyRegistry
├── registerChild(parentId, childId, childType) → void
├── unregisterChild(parentId, childId) → void
├── getChildren(parentId) → ChildExecutionReference[]
├── getParent(childId) → ParentExecutionReference | null
├── getSiblings(executionId) → ChildExecutionReference[]
├── hasChildren(parentId) → boolean
├── hasParent(childId) → boolean
├── getDescendantCount(executionId) → number
├── getAncestors(executionId) → ExecutionReference[]
├── getSubtree(executionId) → ExecutionTreeNode
└── clear() → void
```

## 8. Additional Registries

| Registry | Purpose |
|----------|---------|
| `NodeTemplateRegistry` | Stores node template definitions |
| `HookTemplateRegistry` | Stores hook template definitions |
| `TriggerTemplateRegistry` | Stores trigger template definitions |
| `ScriptRegistry` | Stores script definitions |
| `SkillRegistry` | Stores skill definitions |
| `PromptTemplateRegistry` | Stores prompt template definitions |
| `AgentProfileRegistry` | Stores agent profile definitions |
| `FragmentRegistry` | Stores fragment definitions |

## 9. Registry Utilities

### HierarchyTraversalService

```
HierarchyTraversalService
├── traverseUp(executionId, callback) → void
├── traverseDown(executionId, callback) → void
├── findAncestor(executionId, predicate) → ExecutionReference | null
├── findDescendant(executionId, predicate) → ExecutionReference | null
└── getPathToRoot(executionId) → ExecutionReference[]
```

### RegistryExtensions

```
RegistryExtensions
├── extendRegistry(registry, extensions) → void
├── createNamespacedRegistry(namespace, registry) → Registry
└── createCompositeRegistry(registries) → Registry
```