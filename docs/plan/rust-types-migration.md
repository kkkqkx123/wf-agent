# wf-types Rust 迁移计划

## 模块系统设计原则

### 现代 Rust 模块约定

参考 `wf-storage` 和 `mod-refactor.sh` 脚本，项目使用现代 Rust 模块系统：

1. **顶层模块**: `src/<name>.rs` (不是 `src/<name>/mod.rs`)
2. **子模块**: 当模块需要进一步拆分时，使用 `src/<name>/<submodule>.rs`
3. **模块声明**: 在 `lib.rs` 或父模块中用 `pub mod <name>;` 声明
4. **不使用 `include!`**: 使用标准 `mod` 系统

### 目录结构规则

```
src/
├── lib.rs              # 模块入口，声明所有顶层模块
├── common.rs           # 顶层模块 (基础类型)
├── condition.rs        # 顶层模块
├── agent.rs            # 顶层模块 (Agent 域)
├── agent/
│   ├── mod.rs          # 子模块入口 (因为 agent 有多个子文件)
│   ├── definition.rs   # 子模块文件
│   ├── static_config.rs
│   └── tool_config.rs
├── checkpoint.rs       # 顶层模块
├── checkpoint/
│   ├── mod.rs
│   ├── base.rs
│   ├── agent.rs        # 子模块 (checkpoint 的 agent 子域)
│   ├── agent/
│   │   ├── mod.rs
│   │   ├── snapshot.rs
│   │   └── config.rs
│   └── workflow.rs
│   └── workflow/
│       ├── mod.rs
│       ├── snapshot.rs
│       └── config.rs
```

---

## 完整目录结构

```
crates/wf-types/src/
├── lib.rs                          # 模块声明入口
│
├── common.rs                       # 基础类型
├── condition.rs                    # 条件类型
├── environment.rs                  # 环境类型
├── dynamic_context.rs              # 动态上下文
├── fragment.rs                     # 提示片段
├── hook.rs                         # 通用 Hook
├── http.rs                         # HTTP 类型
├── prompt_template.rs              # 提示模板
├── registry_options.rs             # 注册选项
├── result.rs                       # Result 类型
├── skill.rs                        # Skill 类型
├── todo.rs                         # TODO 类型
├── tool_description.rs             # 工具描述
├── user_config.rs                  # 用户配置
│
├── agent.rs                        # Agent 域入口
├── agent/
│   ├── mod.rs                      # 子模块声明
│   ├── definition.rs               # AgentLoopDefinition, AgentTemplate
│   ├── static_config.rs            # AgentHookStatic, AgentTriggerStatic
│   └── tool_config.rs              # AgentToolConfig
│
├── agent_execution.rs              # Agent Execution 域入口
├── agent_execution/
│   ├── mod.rs
│   ├── types.rs                    # AgentLoopStatus, ToolCallRecord
│   ├── context.rs                  # AgentLoopRuntimeConfig
│   ├── definition.rs               # AgentLoopExecution
│   ├── event.rs                    # AgentStreamEvent
│   ├── hooks.rs                    # AgentHookType
│   └── triggers.rs                 # AgentTrigger
│
├── checkpoint.rs                   # Checkpoint 域入口
├── checkpoint/
│   ├── mod.rs
│   ├── base.rs                     # CheckpointStateBase
│   ├── error_handling.rs           # CheckpointError
│   ├── execution_events.rs         # ExecutionErrorRecord
│   ├── metrics.rs                  # CheckpointMetrics
│   ├── variable_state.rs           # CheckpointVariableState
│   ├── version.rs                  # CheckpointFormatVersion
│   ├── agent.rs                    # Agent Checkpoint 子域
│   ├── agent/
│   │   ├── mod.rs
│   │   ├── snapshot.rs             # AgentLoopStateSnapshot
│   │   ├── config.rs               # AgentLoopCheckpointConfig
│   │   └── checkpoint.rs           # AgentLoopDelta
│   ├── workflow.rs                 # Workflow Checkpoint 子域
│   └── workflow/
│       ├── mod.rs
│       ├── snapshot.rs             # WorkflowExecutionStateSnapshot
│       ├── config.rs               # WorkflowCheckpointConfig
│       └── checkpoint.rs           # CheckpointDelta
│
├── config.rs                       # Config 域
├── config/
│   ├── mod.rs
│   ├── schemas.rs                  # StorageConfig, CompressionConfig
│   ├── metrics.rs                  # MetricsConfig
│   ├── timeout.rs                  # TimeoutConfig
│   └── output.rs                   # OutputConfig
│
├── errors.rs                       # Errors 域入口
├── errors/
│   ├── mod.rs
│   ├── base.rs                     # WfError, ErrorSeverity
│   ├── execution_errors.rs
│   ├── validation_errors.rs
│   ├── tool_errors.rs
│   ├── storage_errors.rs
│   ├── network_errors.rs
│   ├── resource_errors.rs
│   └── serialized_error.rs
│
├── events.rs                       # Events 域入口
├── events/
│   ├── mod.rs
│   ├── base.rs                     # BaseEvent, EventType
│   ├── workflow_execution_events.rs
│   ├── node_events.rs
│   ├── tool_events.rs
│   ├── conversation_events.rs
│   ├── checkpoint_events.rs
│   ├── agent_events.rs
│   ├── interaction_events.rs
│   ├── subgraph_events.rs
│   ├── system_events.rs
│   ├── skill_events.rs
│   ├── async_completion_events.rs
│   ├── attempt_completion_events.rs
│   └── type_guards.rs
│
├── execution.rs                    # Execution 域
├── execution/
│   ├── mod.rs
│   ├── hierarchy.rs                # ExecutionHierarchy
│   ├── failure_policy.rs           # FailurePolicy, RetryPolicy
│   └── workflow_execution_mode.rs
│
├── interaction.rs                  # Interaction 域
├── interaction/
│   ├── mod.rs
│   ├── user_interaction.rs
│   ├── tool_approval.rs
│   └── followup_question.rs
│
├── interruption.rs                 # Interruption 域
├── interruption/
│   ├── mod.rs
│   └── execution_context.rs
│
├── llm.rs                          # LLM 域入口
├── llm/
│   ├── mod.rs
│   ├── state.rs                    # LLMProvider
│   ├── profile.rs                  # LLMProfile
│   ├── request.rs                  # ChatRequest
│   ├── response.rs                 # ChatResponse
│   ├── tool_call_format.rs         # ToolCallFormat
│   ├── protocol_config.rs          # ToolCallProtocolConfig
│   ├── message_stream_events.rs    # MessageStreamEvent
│   ├── client.rs                   # LLMClient (trait)
│   ├── execution_config.rs
│   └── usage.rs                    # TokenUsageStats
│
├── message.rs                      # Message 域入口
├── message/
│   ├── mod.rs
│   ├── message.rs                  # Message, MessageRole
│   ├── message_context.rs
│   ├── message_operations.rs
│   ├── message_array.rs
│   ├── batch_snapshot.rs
│   ├── message_mark_map.rs
│   └── batch_management_operation.rs
│
├── node.rs                         # Node 域入口
├── node/
│   ├── mod.rs
│   ├── shared.rs                   # NodeIdentity, NodeExecutionConfig
│   ├── static.rs                   # StaticNode, 21 node types
│   ├── runtime.rs                  # RuntimeNode
│   ├── hooks.rs
│   ├── properties.rs
│   └── configs.rs                  # Node Configs 入口
│   └── configs/
│       ├── mod.rs
│       ├── control.rs              # RouteNodeConfig
│       ├── variable.rs
│       ├── fork_join.rs
│       ├── loop.rs
│       ├── script.rs
│       ├── llm.rs
│       ├── tool_visibility.rs
│       ├── interaction.rs
│       ├── context.rs
│       ├── variable_operation.rs
│       ├── subgraph.rs
│       ├── embed_graph.rs
│       ├── sync.rs
│       └── agent_loop.rs
│
├── script.rs                       # Script 域入口
├── script/
│   ├── mod.rs
│   ├── script.rs                   # Script
│   ├── argument.rs
│   ├── executor.rs
│   ├── flow.rs
│   ├── interactive.rs
│   ├── sandbox.rs
│   └── security.rs
│
├── storage.rs                      # Storage traits
├── storage/
│   ├── mod.rs
│   ├── checkpoint.rs
│   ├── task.rs
│   ├── workflow.rs
│   ├── workflow_execution.rs
│   ├── agent_loop.rs
│   ├── agent_profile.rs
│   ├── tool.rs
│   ├── script.rs
│   ├── trigger.rs
│   ├── node_template.rs
│   ├── hook_template.rs
│   └── file_checkpoint.rs
│
├── tool.rs                         # Tool 域入口
├── tool/
│   ├── mod.rs
│   ├── state.rs                    # ToolType
│   ├── definition.rs               # Tool
│   ├── static_config.rs            # ToolProperty, ToolMetadata
│   ├── runtime_config.rs
│   ├── execution.rs
│   ├── approval.rs
│   ├── risk_level.rs
│   ├── file_permission.rs
│   ├── mcp_approval.rs
│   └── mcp_connection.rs
│
├── trigger.rs                      # Trigger 域入口
├── trigger/
│   ├── mod.rs
│   ├── definition.rs
│   ├── config.rs
│   ├── execution.rs
│   ├── state.rs
│   └── template.rs
│
├── workflow.rs                     # Workflow 域入口
├── workflow/
│   ├── mod.rs
│   ├── definition.rs               # WorkflowTemplate
│   ├── config.rs
│   ├── edge.rs
│   ├── boundary_config.rs
│   ├── node_template.rs
│   ├── hook_template.rs
│   └── tool_config.rs
│
├── workflow_execution.rs           # Workflow Execution 域入口
└── workflow_execution/
    ├── mod.rs
    ├── status.rs
    ├── definition.rs
    ├── execution.rs
    ├── context.rs
    ├── variables.rs
    ├── graph_structure.rs
    └── history.rs
```

---

## lib.rs 模板

```rust
// ============ 基础类型 ============
pub mod common;
pub mod condition;
pub mod environment;
pub mod dynamic_context;
pub mod fragment;
pub mod hook;
pub mod http;
pub mod prompt_template;
pub mod registry_options;
pub mod result;
pub mod skill;
pub mod todo;
pub mod tool_description;
pub mod user_config;

// ============ 域模块 ============
pub mod agent;
pub mod agent_execution;
pub mod checkpoint;
pub mod config;
pub mod errors;
pub mod events;
pub mod execution;
pub mod interaction;
pub mod interruption;
pub mod llm;
pub mod message;
pub mod node;
pub mod script;
pub mod storage;
pub mod tool;
pub mod trigger;
pub mod workflow;
pub mod workflow_execution;

// ============ 重导出常用类型 ============
// 可选：为了方便，可以重导出一些常用类型到 crate root
pub use common::*;
pub use condition::*;
// ...
```

---

## 子模块 mod.rs 模板

```rust
// agent/mod.rs
pub mod definition;
pub mod static_config;
pub mod tool_config;

// 可选：重导出到父模块
pub use definition::*;
pub use static_config::*;
pub use tool_config::*;
```

---

## 文件对应关系 (TS → Rust)

| Rust 模块 | TS 源文件 | 说明 |
|-----------|-----------|------|
| `common.rs` | `common.ts`, `hook.ts` | Id, Timestamp, VariableDefinition, HookType |
| `condition.rs` | `condition.ts` | Condition, ExpressionCondition |
| `environment.rs` | `environment.ts` | WorkspaceInfo, EnvironmentInfo |
| `dynamic_context.rs` | `dynamic-context.ts` | DynamicContextConfig, DynamicRuntimeContext |
| `fragment.rs` | `fragment.ts` | SystemPromptFragment |
| `http.rs` | `http.ts` | HttpRequestOptions, HttpResponse |
| `prompt_template.rs` | `prompt-template.ts` | PromptTemplate, PromptVariableDefinition |
| `registry_options.rs` | `registry-options.ts` | RegisterOptions, UnregisterOptions |
| `result.rs` | `result.ts` | Result<T,E> enum |
| `skill.rs` | `skill.ts` | Skill, SkillMetadata |
| `todo.rs` | `todo.ts` | TodoItem, TodoStatus |
| `tool_description.rs` | `tool-description.ts` | ToolDescriptionData |
| `user_config.rs` | `user-config.ts` | PinnedFileItem, SkillConfigItem |
| `agent/definition.rs` | `agent/definition.ts` | AgentLoopDefinition, AgentTemplate |
| `agent/static_config.rs` | `agent/static-config.ts` | AgentHookStatic, AgentTriggerStatic |
| `agent/tool_config.rs` | `agent/tool-config.ts` | AgentToolConfig |
| `agent_execution/types.rs` | `agent-execution/types.ts` | AgentLoopStatus, ToolCallRecord |
| `agent_execution/context.rs` | `agent-execution/context.ts` | AgentLoopRuntimeConfig |
| `agent_execution/definition.rs` | `agent-execution/definition.ts` | AgentLoopExecution |
| `agent_execution/event.rs` | `agent-execution/event.ts` | AgentStreamEvent |
| `agent_execution/hooks.rs` | `agent-execution/hooks.ts` | AgentHookType |
| `agent_execution/triggers.rs` | `agent-execution/triggers.ts` | AgentTrigger |
| `checkpoint/base.rs` | `checkpoint/base.ts` | CheckpointStateBase, CheckpointTrigger |
| `checkpoint/error_handling.rs` | `checkpoint/error-handling.ts` | CheckpointError |
| `checkpoint/execution_events.rs` | `checkpoint/execution-events.ts` | ExecutionErrorRecord |
| `checkpoint/metrics.rs` | `checkpoint/metrics.ts` | CheckpointCreationMetrics |
| `checkpoint/variable_state.rs` | `checkpoint/variable-state.ts` | CheckpointVariableState |
| `checkpoint/version.rs` | `checkpoint/version.ts` | CheckpointFormatVersion |
| `checkpoint/agent/snapshot.rs` | `checkpoint/agent/snapshot.ts` | AgentLoopStateSnapshot |
| `checkpoint/agent/config.rs` | `checkpoint/agent/config.ts` | AgentLoopCheckpointConfig |
| `checkpoint/agent/checkpoint.rs` | `checkpoint/agent/checkpoint.ts` | AgentLoopDelta |
| `checkpoint/workflow/snapshot.rs` | `checkpoint/workflow/snapshot.ts` | WorkflowExecutionStateSnapshot |
| `checkpoint/workflow/config.rs` | `checkpoint/workflow/config.ts` | WorkflowCheckpointConfig |
| `checkpoint/workflow/checkpoint.rs` | `checkpoint/workflow/checkpoint.ts` | CheckpointDelta |
| `config/schemas.rs` | `config/schemas.ts` | StorageConfig, CompressionConfig |
| `config/metrics.rs` | `config/metrics-schema.ts` | MetricsConfig |
| `config/timeout.rs` | `config/timeout-schema.ts` | TimeoutConfig |
| `config/output.rs` | `config/output.ts` | OutputConfig |
| `errors/base.rs` | `errors/base.ts` | WfError, ErrorSeverity |
| `errors/execution_errors.rs` | `errors/execution-errors.ts` | ExecutionError |
| `errors/validation_errors.rs` | `errors/validation-errors.ts` | ValidationError |
| `errors/tool_errors.rs` | `errors/tool-errors.ts` | ToolError |
| `errors/storage_errors.rs` | `errors/other-errors.ts` | StorageError |
| `errors/network_errors.rs` | `errors/network-errors.ts` | NetworkError |
| `errors/resource_errors.rs` | `errors/resource-errors.ts` | ResourceError |
| `errors/serialized_error.rs` | `errors/serialized-error.ts` | SerializedError |
| `events/base.rs` | `events/base.ts` | BaseEvent, EventType |
| `events/workflow_execution_events.rs` | `events/workflow-execution-events.ts` | WorkflowExecutionStartedEvent |
| `events/node_events.rs` | `events/node-events.ts` | NodeStartedEvent |
| `events/tool_events.rs` | `events/tool-events.ts` | ToolCallStartedEvent |
| `events/conversation_events.rs` | `events/conversation-events.ts` | MessageAddedEvent |
| `events/checkpoint_events.rs` | `events/checkpoint-events.ts` | CheckpointCreatedEvent |
| `events/agent_events.rs` | `events/agent-events.ts` | AgentStartedEvent |
| `events/interaction_events.rs` | `events/interaction-events.ts` | ToolApprovalRequestedEvent |
| `events/subgraph_events.rs` | `events/subgraph-events.ts` | SubgraphStartedEvent |
| `events/system_events.rs` | `events/system-events.ts` | TokenLimitExceededEvent |
| `events/skill_events.rs` | `events/skill-events.ts` | SkillLoadStartedEvent |
| `events/async_completion_events.rs` | `events/async-completion-events.ts` | AsyncCompletionRegisteredEvent |
| `events/attempt_completion_events.rs` | `events/attempt-completion-events.ts` | AttemptCompletionEvent |
| `events/type_guards.rs` | `events/type-guards.ts` | Type guard functions |
| `execution/hierarchy.rs` | `execution/hierarchy.ts` | ExecutionHierarchy |
| `execution/failure_policy.rs` | `execution/failure-policy.ts` | FailurePolicy, RetryPolicy |
| `execution/workflow_execution_mode.rs` | `execution/workflow-execution-mode.ts` | WorkflowExecutionMode |
| `interaction/user_interaction.rs` | `interaction/user-interaction.ts` | UserInteractionRequest |
| `interaction/tool_approval.rs` | `interaction/tool-approval.ts` | ToolApprovalRequestData |
| `interaction/followup_question.rs` | `interaction/followup-question.ts` | FollowupQuestion |
| `interruption/execution_context.rs` | `interruption/execution-context.ts` | ExecutionDomainContext |
| `llm/state.rs` | `llm/state.ts` | LLMProvider |
| `llm/profile.rs` | `llm/profile.ts` | LLMProfile |
| `llm/request.rs` | `llm/request.ts` | ChatRequest |
| `llm/response.rs` | `llm/response.ts` | ChatResponse, LLMUsage |
| `llm/tool_call_format.rs` | `llm/tool-call-format.ts` | ToolCallFormat |
| `llm/protocol_config.rs` | `llm/protocol-config.ts` | ToolCallProtocolConfig |
| `llm/message_stream_events.rs` | `llm/message-stream-events.ts` | MessageStreamEvent |
| `llm/client.rs` | `llm/client.ts` | LLMClient (trait) |
| `llm/execution_config.rs` | `llm/execution-config.ts` | LLMExecutionConfig |
| `llm/usage.rs` | `llm/usage.ts` | TokenUsageStats |
| `message/message.rs` | `message/message.ts` | Message, MessageRole |
| `message/message_context.rs` | `message/message-context.ts` | MessageContext |
| `message/message_operations.rs` | `message/message-operations.ts` | MessageOperationConfig |
| `message/message_array.rs` | `message/message-array.ts` | MessageArrayState |
| `message/batch_snapshot.rs` | `message/batch-snapshot.ts` | BatchSnapshot |
| `message/message_mark_map.rs` | `message/message-mark-map.ts` | MessageMarkMap |
| `message/batch_management_operation.rs` | `message/batch-management-operation.ts` | BatchManagementOperation |
| `node/shared.rs` | `node/shared-node-types.ts` | NodeIdentity, NodeExecutionConfig |
| `node/static.rs` | `node/static-node-types.ts` | StaticNode, 21 node types |
| `node/runtime.rs` | `node/runtime-node-types.ts` | RuntimeNode |
| `node/hooks.rs` | `node/hooks.ts` | NodeHook |
| `node/properties.rs` | `node/properties.ts` | NodeProperty |
| `node/configs/control.rs` | `node/configs/control-configs.ts` | RouteNodeConfig |
| `node/configs/variable.rs` | `node/configs/variable-configs.ts` | VariableNodeConfig |
| `node/configs/fork_join.rs` | `node/configs/fork-join-configs.ts` | ForkNodeConfig |
| `node/configs/loop.rs` | `node/configs/loop-configs.ts` | LoopStartNodeConfig |
| `node/configs/script.rs` | `node/configs/script-configs.ts` | ScriptNodeConfig |
| `node/configs/llm.rs` | `node/configs/llm-configs.ts` | LLMNodeConfig |
| `node/configs/tool_visibility.rs` | `node/configs/tool-visibility-configs.ts` | ToolVisibilityNodeConfig |
| `node/configs/interaction.rs` | `node/configs/interaction-configs.ts` | UserInteractionNodeConfig |
| `node/configs/context.rs` | `node/configs/context-configs.ts` | ContextProcessorNodeConfig |
| `node/configs/variable_operation.rs` | `node/configs/variable-operation-configs.ts` | VariableOperationConfig |
| `node/configs/subgraph.rs` | `node/configs/subgraph-configs.ts` | SubgraphNodeConfig |
| `node/configs/embed_graph.rs` | `node/configs/embed-graph-configs.ts` | EmbedGraphNodeConfig |
| `node/configs/sync.rs` | `node/configs/sync-configs.ts` | SyncNodeConfig |
| `node/configs/agent_loop.rs` | `node/configs/agent-loop-configs.ts` | AgentLoopNodeConfig |
| `script/script.rs` | `script/script.ts` | Script, ScriptExecutionOptions |
| `script/argument.rs` | `script/script-argument.ts` | ScriptArgument |
| `script/executor.rs` | `script/script-executor.ts` | ScriptExecutorConfig |
| `script/flow.rs` | `script/script-flow.ts` | ScriptFlow |
| `script/interactive.rs` | `script/script-interactive.ts` | InteractiveScriptConfig |
| `script/sandbox.rs` | `script/script-sandbox.ts` | SandboxConfig, SandboxPolicy |
| `script/security.rs` | `script/script-security.ts` | ScriptRiskLevel |
| `storage/checkpoint.rs` | `storage/checkpoint-storage.ts` | CheckpointStorage (trait) |
| `storage/task.rs` | `storage/task-storage.ts` | TaskStorage (trait) |
| `storage/workflow.rs` | `storage/workflow-storage.ts` | WorkflowStorage (trait) |
| `storage/workflow_execution.rs` | `storage/workflow-execution-storage.ts` | WorkflowExecutionStorage |
| `storage/agent_loop.rs` | `storage/agent-loop-storage.ts` | AgentLoopStorage (trait) |
| `storage/agent_profile.rs` | `storage/agent-profile-storage.ts` | AgentProfileStorage |
| `storage/tool.rs` | `storage/tool-storage.ts` | ToolStorage (trait) |
| `storage/script.rs` | `storage/script-storage.ts` | ScriptStorage (trait) |
| `storage/trigger.rs` | `storage/trigger-storage.ts` | TriggerStorage (trait) |
| `storage/node_template.rs` | `storage/node-template-storage.ts` | NodeTemplateStorage |
| `storage/hook_template.rs` | `storage/hook-template-storage.ts` | HookTemplateStorage |
| `storage/file_checkpoint.rs` | `storage/file-checkpoint.ts` | FileCheckpointStorage |
| `tool/state.rs` | `tool/state.ts` | ToolType |
| `tool/definition.rs` | `tool/definition.ts` | Tool |
| `tool/static_config.rs` | `tool/static-config.ts` | ToolProperty, ToolMetadata |
| `tool/runtime_config.rs` | `tool/runtime-config.ts` | StatelessToolConfig |
| `tool/execution.rs` | `tool/execution.ts` | ToolCall, ToolExecutionResult |
| `tool/approval.rs` | `tool/approval.ts` | ToolApprovalOptions |
| `tool/risk_level.rs` | `tool/risk-level.ts` | ToolRiskLevel |
| `tool/file_permission.rs` | `tool/file-permission.ts` | FilePermissionSettings |
| `tool/mcp_approval.rs` | `tool/mcp-approval.ts` | McpApprovalSettings |
| `tool/mcp_connection.rs` | `tool/mcp-connection.ts` | McpServerConfig |
| `trigger/definition.rs` | `trigger/definition.ts` | TriggerDefinition |
| `trigger/config.rs` | `trigger/config.ts` | TriggerCondition, TriggerAction |
| `trigger/execution.rs` | `trigger/execution.ts` | TriggerExecutionResult |
| `trigger/state.rs` | `trigger/state.ts` | TriggerStatus |
| `trigger/template.rs` | `trigger/template.ts` | TriggerTemplate |
| `workflow/definition.rs` | `workflow/definition.ts` | WorkflowTemplate |
| `workflow/config.rs` | `workflow/config.ts` | WorkflowConfig |
| `workflow/edge.rs` | `workflow/edge.ts` | Edge |
| `workflow/boundary_config.rs` | `workflow/boundary-config.ts` | WorkflowStartConfig |
| `workflow/node_template.rs` | `workflow/node-template.ts` | NodeTemplate |
| `workflow/hook_template.rs` | `workflow/hook-template.ts` | HookTemplate |
| `workflow/tool_config.rs` | `workflow/tool-config.ts` | AvailableTools |
| `workflow_execution/status.rs` | `workflow-execution/status.ts` | WorkflowExecutionStatus |
| `workflow_execution/definition.rs` | `workflow-execution/definition.ts` | WorkflowExecution |
| `workflow_execution/execution.rs` | `workflow-execution/execution.ts` | WorkflowExecutionOptions |
| `workflow_execution/context.rs` | `workflow-execution/context.ts` | ForkJoinContext |
| `workflow_execution/variables.rs` | `workflow-execution/variables.ts` | VariableDefinition |
| `workflow_execution/graph_structure.rs` | `workflow-execution/graph-structure.ts` | WorkflowGraphStructure |
| `workflow_execution/history.rs` | `workflow-execution/history.ts` | NodeExecutionResult |

---

## 实现注意事项

### 1. 模块依赖顺序

Rust 模块可以互相引用，不需要特定的 include 顺序。但要注意：
- 避免循环依赖
- 使用 `crate::module::Type` 引用其他模块的类型
- 或使用 `super::Type` 引用父模块类型
- 或使用 `use` 语句导入

### 2. 泛型处理

```rust
// TS: export type Result<T, E = Error> = Ok<T> | Err<E>;
// Rust:
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Result<T, E = serde_json::Value> {
    Ok(T),
    Err(E),
}
```

### 3. Trait vs Struct

- TS interface (数据结构) → Rust `struct` / `enum`
- TS interface (行为契约) → Rust `trait`
- TS type alias → Rust `type`

### 4. Zod Schema 不迁移

TS 的 `*-schema.ts` 文件包含 Zod 验证 schema，Rust 中：
- 用 `serde` 进行序列化/反序列化
- 验证逻辑在应用层实现
- 不需要迁移 schema 文件

### 5. 类型守卫不迁移

TS 的 `is*()` 类型守卫函数在 Rust 中：
- 用 `if let` 模式匹配替代
- 或用 `matches!` 宏
- `events/type_guards.rs` 可以保留，用函数实现

---

## 分阶段实现计划

### Phase 1: 基础结构 (P0)
- [ ] 创建 `lib.rs` 模块声明
- [ ] 创建基础类型模块 (`common.rs`, `condition.rs`, `environment.rs`, etc.)
- [ ] 创建错误模块 (`errors/`)
- [ ] 创建事件基础 (`events/base.rs`)

### Phase 2: 核心执行模型 (P0)
- [ ] 实现 Node 模块 (`node/`)
- [ ] 实现 Workflow 模块 (`workflow/`)
- [ ] 实现 Workflow Execution 模块 (`workflow_execution/`)

### Phase 3: Agent 系统 (P0)
- [ ] 实现 Agent 模块 (`agent/`)
- [ ] 实现 Agent Execution 模块 (`agent_execution/`)

### Phase 4: Checkpoint 系统 (P1)
- [ ] 实现 Checkpoint 基础 (`checkpoint/base.rs`, etc.)
- [ ] 实现 Agent Checkpoint (`checkpoint/agent/`)
- [ ] 实现 Workflow Checkpoint (`checkpoint/workflow/`)

### Phase 5: Tool 系统 (P1)
- [ ] 实现 Tool 基础 (`tool/`)
- [ ] 实现 Tool Approval (`tool/approval.rs`, `tool/risk_level.rs`)
- [ ] 实现 MCP (`tool/mcp_approval.rs`, `tool/mcp_connection.rs`)

### Phase 6: LLM 和 Message (P1)
- [ ] 实现 LLM 模块 (`llm/`)
- [ ] 实现 Message 模块 (`message/`)

### Phase 7: Script 系统 (P2)
- [ ] 实现 Script 模块 (`script/`)

### Phase 8: 辅助模块 (P2)
- [ ] 实现 Trigger (`trigger/`)
- [ ] 实现 Storage traits (`storage/`)
- [ ] 实现 Config (`config/`)
- [ ] 实现 Interaction (`interaction/`)
- [ ] 实现 Interruption (`interruption/`)

### Phase 9: 事件系统完善 (P2)
- [ ] 实现所有事件类型 (`events/*`)
- [ ] 实现 Type Guards (`events/type_guards.rs`)

---

## 验证清单

- [ ] `cargo check` 通过
- [ ] `cargo clippy` 无警告
- [ ] `cargo fmt` 格式化
- [ ] 所有类型与 TS 源对应
- [ ] 模块声明正确 (`pub mod xxx;`)
- [ ] 无循环依赖
- [ ] 目录结构与 TS 对应
- [ ] 子模块使用 `mod.rs` 正确
