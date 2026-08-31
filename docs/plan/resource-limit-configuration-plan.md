# 资源数量限制可配置化修改方案

> 状态：待评审
> 关联文档：`docs/plan/resource-limit-modification-plan.md`（早期方案，其中 navigation_count 死循环检测缺陷已在当前代码中修复，本文基于当前代码重新盘点）
> 目标：让 Agent 与 Workflow 的资源数量限制全部具备配置化能力，消除硬编码常量与"死配置"

---

## 一、背景与目标

当前代码中的资源数量限制分散在硬编码常量、编程式 builder、配置文件三类载体中，存在以下问题：

1. **硬编码常量不可配置**：`AGENT_MAX_ITERATIONS_CAP`（1000）、`MAX_ITERATIONS_CAP`（10000）、`DEFAULT_MAX_ITERATIONS`（10）、循环默认迭代次数（100）、`max_navigation_multiplier` 默认值（5）均为编译期常量，部署者无法调整。
2. **编程式接口未接线**：Agent 的 `max_concurrent`、`max_sub_agent_depth`、`max_pause_duration`、token 限制只有 builder 方法，`bootstrap.rs` 构建 `AgentLoopExecutor` 时未调用，实际全部走默认值，无法通过配置文件或环境变量控制。
3. **配置字段丢失**：`AgentRuntimeConfig` 中存在 `max_execution_time`、`max_retries`、`execution_timeout`，但 `AgentConfig`（agent 定义）没有对应字段，`transform_to_agent_loop_config` 将这些值硬编码为 `None`。
4. **死配置**：`configs/infrastructure/timeout.toml` 中 `TimeoutConfig` 的字段（`node_completion`、`max_allowed`、`sync_branch_wait` 等）被加载并合并默认值，但引擎没有任何消费者读取它们。

**目标**：建立统一的资源限制配置域，让所有限制参数具备"配置默认值 + 配置覆盖 + 环境变量覆盖"三级能力；Workflow 循环迭代上限默认值保持 10000 不变，但允许部署者配置化调整。

---

## 二、现状盘点

### 2.1 Agent 侧资源限制参数清单

| 参数 | 默认值 | 当前是否可配置 | 现状载体 | 位置 |
|------|--------|----------------|----------|------|
| `max_iterations`（单 agent） | 10 | 是（per-agent） | `AgentDefinition.config.max_iterations` | `agent/config.rs:22` |
| `AGENT_MAX_ITERATIONS_CAP` | 1000 | 否（常量） | 硬编码 | `wf-agent/src/constants.rs:4` |
| `DEFAULT_MAX_ITERATIONS` | 10 | 否（常量） | 硬编码 | `wf-agent/src/constants.rs:7` |
| `max_concurrent` | CPU 核数 | 仅编程式 | `AgentCapacityGate` | `wf-agent/src/capacity.rs`；bootstrap 未接线 |
| `max_sub_agent_depth` | 8 | 仅编程式 | `DEFAULT_MAX_SUB_AGENT_DEPTH` | `wf-agent/src/registry.rs:16` |
| `max_pause_duration` | 无 | 仅编程式 | `AgentLoopEntity::with_max_pause_duration` | `wf-agent/src/entity.rs:153`；调用处均传 `None` |
| `max_execution_time` | 无 | 否（字段被丢弃） | `AgentRuntimeConfig` 有字段，transform 置 None | `wf-config/src/processor/agent_loop.rs:62` |
| `max_retries` | 无 | 否（同上） | 同上 | `agent_loop.rs:63` |
| `execution_timeout` | 无 | 否（同上） | 同上 | `agent_loop.rs:64` |
| `token_limit` / `token_warning_threshold` | 无 | 仅编程式 | `AgentLoopConfig` 字段，默认关闭 | `wf-agent/src/executor.rs:379-381` |

### 2.2 Workflow 侧资源限制参数清单

| 参数 | 默认值 | 当前是否可配置 | 现状载体 | 位置 |
|------|--------|----------------|----------|------|
| `MAX_ITERATIONS_CAP`（循环上限） | 10000 | 否（常量） | 硬编码 | `wf-workflow/src/loop_state.rs:20` |
| LOOP_START `max_iterations` 默认 | 100 | 是（per-node） | node config | `wf-workflow/src/handler/loop_handler.rs:46-49` |
| `max_steps` | 无 | 是（per-exec） | `WorkflowExecutionOptions.max_steps` | `wf-types/workflow_execution/execution.rs:21` |
| `max_execution_time` | 无 | 是（per-exec） | `WorkflowExecutionOptions.max_execution_time` | `execution.rs:25` |
| `node_timeout` | 无 | 是（per-exec/node） | Options / node `timeout_seconds` | `execution.rs:29`、`coordinator/workflow.rs:1021-1022` |
| `max_navigation_multiplier` | 5 | 是（per-exec） | `WorkflowExecutionOptions.max_navigation_multiplier` | `execution.rs:49`、`coordinator/workflow.rs:314` |
| fork `child_execution_timeout` | 0（不限） | 是（per-node） | node config | `wf-workflow/src/handler/fork_join.rs:251` |
| fork `total_branch_timeout` | 0（不限） | 是（per-node） | node config | `fork_join.rs:255` |
| workflow 并发预算 | 无限制 | 否 | 无实现 | - |
| `TimeoutConfig` 各字段 | 30s/60s 等 | 是（配置文件） | `configs/infrastructure/timeout.toml` | 引擎未消费（死配置） |
| `max_allowed`（超时硬顶） | 300000ms | 是（配置文件） | `timeout.toml` | 无消费者 |

### 2.3 配置加载链路现状

`configs/infrastructure/` 通过 `index.json` + `development.json` 的 preset 机制加载，`orchestrator.rs` 中的 `load_infrastructure_configs` 负责读取各 domain 文件并调用 `merge_*_with_defaults` 合并默认值，且支持 `WF_*` 环境变量覆盖（`env.rs`）。**基础设施配置域是完整的，缺口在于资源限制参数没有进入该体系。**

---

## 三、问题分析

### 3.1 可配置性缺口汇总

| 缺口类型 | 涉及参数 | 影响 |
|----------|----------|------|
| 硬编码常量 | 两个迭代上限、默认迭代数、导航乘数默认值 | 无法按部署环境调整（如低资源环境收紧、高信任环境放宽） |
| 字段被丢弃 | `max_execution_time`、`max_retries`、`execution_timeout` | agent 定义声明了限制但运行时完全忽略 |
| 未接线 | `max_concurrent`、`max_sub_agent_depth`、`max_pause_duration`、token 限制 | bootstrap 不调用 builder，全部走默认值 |
| 死配置 | `TimeoutConfig` 全部字段 | 运维修改 `timeout.toml` 不产生任何效果，属于静默失败 |
| 无实现 | workflow 并发预算 | 多个 workflow 可无界并行 |

### 3.2 死配置的根因

`TimeoutConfig` 中定义的等待类超时（`node_completion`、`sync_branch_wait` 等）语义与引擎实际的执行超时（`WorkflowExecutionOptions.node_timeout`、`max_execution_time`）不是同一套体系：前者是 SDK 层的"等待/轮询"超时，后者是执行层的"节点/整体"超时。两套体系之间没有桥接，导致配置文件中的值无法影响执行行为。

### 3.3 上限常量与业务参数的关系

`MAX_ITERATIONS_CAP`（10000）是 LOOP_START 配置的**校验上限**，`loop_handler.rs` 中 LOOP_START 的 `max_iterations` 默认 100 是**业务默认值**。二者语义不同：上限防失控，默认值是缺省行为。可配置化时应分层：业务参数（默认值）放开配置；上限常量保留默认值 10000，仅将"数值本身"暴露为可覆盖的配置项，供部署者按需调整。

---

## 四、修改方案

### 4.1 总体设计：新增 `limits` 配置域

复用既有 infrastructure preset 机制，新增 `configs/infrastructure/limits.toml`，承载 Agent 与 Workflow 的全部资源限制参数。配置模型（`wf_types::config::limits::LimitsConfig`）设计如下：

```toml
# configs/infrastructure/limits.toml

# --- Agent 资源限制 ---
[agent]
max_iterations_cap = 1000        # AGENT_MAX_ITERATIONS_CAP 的可配置化
default_max_iterations = 10      # DEFAULT_MAX_ITERATIONS 的可配置化
max_concurrent = 0               # 0 = 默认按 CPU 核数
max_sub_agent_depth = 8
max_pause_duration_ms = 0        # 0 = 不限制

# --- Workflow 资源限制 ---
[workflow]
loop_max_iterations_cap = 10000  # MAX_ITERATIONS_CAP 的可配置化（默认保留 10000）
loop_default_max_iterations = 100
max_navigation_multiplier = 5
max_concurrent = 0               # 0 = 无并发预算（预留，本期可不实现）

# --- 执行超时默认值（桥接 TimeoutConfig，消除死配置） ---
[execution_defaults]
node_timeout_ms = 30000          # 未在 node/options 配置时的兜底
max_execution_time_ms = 0        # 未在 workflow config 配置时的兜底，0 = 不限制
```

对应新增 Rust 类型：

```rust
// crates/foundation/wf-types/src/config/limits.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LimitsConfig {
    pub agent: Option<AgentLimits>,
    pub workflow: Option<WorkflowLimits>,
    pub execution_defaults: Option<ExecutionDefaults>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AgentLimits {
    pub max_iterations_cap: Option<u32>,
    pub default_max_iterations: Option<u32>,
    pub max_concurrent: Option<u32>,
    pub max_sub_agent_depth: Option<u32>,
    pub max_pause_duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WorkflowLimits {
    pub loop_max_iterations_cap: Option<u32>,
    pub loop_default_max_iterations: Option<u32>,
    pub max_navigation_multiplier: Option<u32>,
    pub max_concurrent: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ExecutionDefaults {
    pub node_timeout_ms: Option<u64>,
    pub max_execution_time_ms: Option<u64>,
}
```

### 4.2 配置加载与合并

1. **新增 processor**：`crates/infra/wf-config/src/processor/limits.rs`，实现 `merge_limits_with_defaults(user: &LimitsConfig) -> LimitsConfig`，所有字段缺省时回落到当前硬编码默认值（1000 / 10 / 0 / 8 / 0 / 10000 / 100 / 5 / 0 / 30000 / 0），保证不配置时行为与现状完全一致。
2. **orchestrator 接线**：在 `AssembledConfig` 中新增 `limits: LimitsConfig` 字段，`load_infrastructure_configs` 增加 `limits` 域解析，`development.json` 的 `files` 映射增加 `"limits": "./limits.toml"`。
3. **环境变量覆盖**：沿用 `env.rs` 的 `WF_*` 机制，为关键限制项提供环境变量（如 `WF_AGENT_MAX_CONCURRENT`、`WF_WORKFLOW_LOOP_MAX_ITERATIONS_CAP`）。
4. **校验**：`validate_limits_config` 拒绝非法组合（如 `max_iterations_cap == 0`、`loop_max_iterations_cap < 1`、`max_navigation_multiplier == 0`），fail-fast 启动。

### 4.3 Agent 侧修改

#### 4.3.1 常量可配置化

`wf-agent/src/constants.rs` 的常量改为运行时解析：`AgentLoopExecutor` 构建时接收一个 `AgentLimits`（或独立参数），`max_iterations_cap`、`default_max_iterations` 不再直接引用常量。常量保留为 fallback 默认值，供未接线的测试路径使用。

```rust
// 示意：executor 持有解析后的限制值
pub struct AgentLoopExecutor {
    max_iterations_cap: u32,      // 来自 limits 配置，默认 1000
    max_iterations: u32,          // 来自 limits 配置，默认 10（原 DEFAULT_MAX_ITERATIONS）
    max_sub_agent_depth: u32,     // 来自 limits 配置，默认 8
    // ...
}
```

`validation.rs` 中的 `AGENT_MAX_ITERATIONS_CAP` 引用改为读取 executor/registry 上的 `max_iterations_cap`。

#### 4.3.2 bootstrap 接线

`bootstrap.rs` 构建 `AgentLoopExecutor` 时，从 `config.limits.agent` 读取并调用：

```rust
wf_agent::executor::AgentLoopExecutor::new(gateway, registry)
    .with_shared_registry(agent_registry.clone())
    .with_max_concurrent(limits.agent.max_concurrent.unwrap_or(0) as usize)
    .with_max_sub_agent_depth(limits.agent.max_sub_agent_depth.unwrap_or(8))
    // max_iterations_cap / default_max_iterations 随 executor 传入
```

`max_concurrent` 为 0 时保持现有按 CPU 核数推导的默认行为。

#### 4.3.3 `AgentConfig` 扩展执行限制字段

`AgentDefinition.config`（`wf-types/src/agent/config.rs`）新增字段，并同步 `transform_to_agent_loop_config`、`validate_agent_definition`、测试构造点：

```rust
pub struct AgentConfig {
    // ...现有字段...
    pub max_execution_time: Option<u64>,   // 整体挂钟超时（ms）
    pub max_retries: Option<u32>,          // LLM 调用失败重试次数
    pub execution_timeout: Option<u64>,    // 单次 LLM 调用超时（ms）
    pub max_pause_duration: Option<u64>,   // 暂停超时（ms），0 = 不限制
    pub token_limit: Option<u32>,          // 会话 token 上限
    pub token_warning_threshold: Option<u32>,
    pub enable_token_tracking: Option<bool>,
}
```

transform 将上述字段透传到 `AgentRuntimeConfig`，消除 `agent_loop.rs:62-64` 的硬编码 `None`。这些字段同时在 `validate_agent_definition` 中做范围校验（如 `max_execution_time == 0` 产生警告，语义为关闭限制）。

#### 4.3.4 `max_pause_duration` 生效链路

`with_max_pause_duration` 目前所有调用处均传 `None`。修改 `wf-runtime` 中 agent 执行组装处（`trigger_listener/agent_runner.rs`、`execution_callback.rs` 等），从「agent 定义配置 → 未配置时取 limits 全局默认」两级解析。

### 4.4 Workflow 侧修改

#### 4.4.1 循环上限可配置化（默认保留 10000）

`wf-workflow/src/loop_state.rs` 的 `MAX_ITERATIONS_CAP` 保持为常量 `10000`（供未接线的默认路径与测试使用），但 LOOP_START 校验时改为读取运行时注入的 `loop_max_iterations_cap`（来自 `limits.workflow`，默认 10000）。注入方式：`WorkflowExecutionOptions` 或 `WorkflowExecutor` 构造参数持有该值，`LoopStartHandler` 通过执行上下文读取。

#### 4.4.2 消除死配置：桥接 `TimeoutConfig` 与执行选项

`WorkflowExecutionOptions` 组装时（`wf-runtime` 侧或 `WorkflowExecutor::execute_workflow` 内部），对未显式指定的字段填充来自配置的默认值：

- `node_timeout` 未设置且节点未声明 `timeout_seconds` 时，回落 `limits.execution_defaults.node_timeout_ms`（默认 30000）。
- `max_execution_time` 未设置时，回落 `limits.execution_defaults.max_execution_time_ms`（默认 0 = 不限制，与现状一致）。

`TimeoutConfig` 中的 SDK 等待类超时（`node_completion`、`sync_branch_wait` 等）若在引擎中确无消费点，在本次改动中明确标注为"预留字段"，并在文档与配置注释中说明，避免运维误改后产生预期偏差；后续接入对应等待逻辑时再消费。

#### 4.4.3 workflow 并发预算（可选，本期列为预留）

引入 `AgentCapacityGate` 同构的 `WorkflowCapacityGate`，控制并行 workflow 执行数（`limits.workflow.max_concurrent`，0 = 不限制）。本期方案保留设计但默认关闭，避免引入行为变化。

### 4.5 配置优先级

```
代码默认值（常量） < limits.toml < 环境变量 WF_* < 显式调用参数（API/WorkflowExecutionOptions/agent 定义）
```

即：显式传入的参数优先级最高，环境变量次之，配置文件再次，代码常量兜底。现有 `merge_*_with_defaults` 的"缺省回落"模式与之兼容。

---

## 五、参数可配置性总览（方案落地后）

### 5.1 Agent

| 参数 | 配置默认值 | 配置键 | 环境变量 | 更高优先级覆盖 |
|------|-----------|--------|----------|----------------|
| 迭代上限 | 1000 | `limits.agent.max_iterations_cap` | `WF_AGENT_MAX_ITERATIONS_CAP` | 无（全局硬顶） |
| 默认迭代数 | 10 | `limits.agent.default_max_iterations` | `WF_AGENT_DEFAULT_MAX_ITERATIONS` | agent 定义 `max_iterations` |
| 并发执行数 | CPU 核数 | `limits.agent.max_concurrent` | `WF_AGENT_MAX_CONCURRENT` | - |
| 子 agent 深度 | 8 | `limits.agent.max_sub_agent_depth` | `WF_AGENT_MAX_SUB_AGENT_DEPTH` | - |
| 暂停超时 | 不限制 | `limits.agent.max_pause_duration_ms` | `WF_AGENT_MAX_PAUSE_DURATION_MS` | agent 定义 `max_pause_duration` |
| 执行时间 | 不限制 | `limits.execution_defaults.max_execution_time_ms` | `WF_EXEC_MAX_EXECUTION_TIME_MS` | agent 定义 `max_execution_time` |
| 单次调用超时 | 不限制 | -（沿用现有） | - | agent 定义 `execution_timeout` |
| token 上限 | 不限制 | 扩展 `limits.agent` 预留 | 预留 | agent 定义 `token_limit` |

### 5.2 Workflow

| 参数 | 配置默认值 | 配置键 | 环境变量 | 更高优先级覆盖 |
|------|-----------|--------|----------|----------------|
| 循环迭代上限 | 10000（保留现状） | `limits.workflow.loop_max_iterations_cap` | `WF_WORKFLOW_LOOP_MAX_ITERATIONS_CAP` | 无（全局硬顶） |
| 循环默认迭代数 | 100 | `limits.workflow.loop_default_max_iterations` | `WF_WORKFLOW_LOOP_DEFAULT_MAX_ITERATIONS` | LOOP_START `max_iterations` |
| 导航乘数 | 5 | `limits.workflow.max_navigation_multiplier` | `WF_WORKFLOW_MAX_NAVIGATION_MULTIPLIER` | `WorkflowExecutionOptions.max_navigation_multiplier` |
| 节点超时兜底 | 30000ms | `limits.execution_defaults.node_timeout_ms` | `WF_EXEC_NODE_TIMEOUT_MS` | options / node 配置 |
| 整体超时兜底 | 不限制 | `limits.execution_defaults.max_execution_time_ms` | `WF_EXEC_MAX_EXECUTION_TIME_MS` | options / workflow config |
| 并发预算 | 不限制（预留） | `limits.workflow.max_concurrent` | 预留 | - |

---

## 六、实施步骤

| 步骤 | 任务 | 涉及文件 | 优先级 |
|------|------|----------|--------|
| 1 | 新增 `LimitsConfig` 类型 | `wf-types/src/config/limits.rs`、`wf-types/src/config/mod.rs` | 高 |
| 2 | 新增 limits processor（merge + validate） | `wf-config/src/processor/limits.rs` | 高 |
| 3 | orchestrator 接线、`development.json` 注册、示例配置 | `wf-config/src/orchestrator.rs`、`configs/infrastructure/limits.toml`、`development.json` | 高 |
| 4 | Agent 常量可配置化 + bootstrap 接线 | `wf-agent/src/constants.rs`、`wf-agent/src/validation.rs`、`wf-agent/src/executor.rs`、`wf-runtime/src/bootstrap.rs` | 高 |
| 5 | `AgentConfig` 扩展执行限制字段并透传 | `wf-types/src/agent/config.rs`、`wf-config/src/processor/agent_loop.rs`、示例 agent 配置 | 中 |
| 6 | `max_pause_duration` 两级解析 | `wf-runtime/src/trigger_listener/agent_runner.rs`、`execution_callback.rs` | 中 |
| 7 | Workflow 循环上限运行时注入（默认 10000） | `wf-workflow/src/loop_state.rs`、`loop_handler.rs`、`execution_context.rs` | 中 |
| 8 | 桥接执行超时默认值（消除死配置） | `wf-workflow/src/coordinator/workflow.rs`、`wf-runtime` 侧 options 组装 | 中 |
| 9 | workflow 并发预算（预留项，可选） | 新增 `WorkflowCapacityGate` | 低 |
| 10 | 测试：默认值回归 + 配置覆盖 + 环境变量覆盖 | 各 crate 测试 | 高 |

### 测试策略

1. **回归保障**：不提供任何配置时，所有默认值必须与现状逐位一致（1000 / 10 / 8 / 10000 / 100 / 5 / 30s），现有测试不改动即通过。
2. **配置覆盖测试**：为每个限制参数提供"配置生效"与"环境变量生效"的单元测试。
3. **校验测试**：非法配置（上限为 0、乘数为 0）在启动时 fail-fast。
4. **集成测试**：覆盖"配置收紧迭代上限 → LOOP 超限被拒"、"配置节点超时兜底 → 未声明超时的节点被限制"两条链路。

---

## 七、风险与兼容性

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 常量可配置化后未接线路径行为漂移 | 测试/工具路径依赖常量 | 常量保留为默认值兜底，配置解析失败回落到常量 |
| `AgentConfig` 新增字段破坏既有 agent 定义解析 | 旧配置兼容 | 全部字段 `Option` + `skip_serializing_if`，缺省行为不变 |
| workflow 并发预算引入行为变化 | 并行任务被限制 | 默认 0 = 不限制，预留项本期不默认开启 |
| 超时兜底值改变节点行为 | 未声明超时的节点突然被限制 | 默认值与现状一致（30000 为当前 node 兜底语义），仅桥接不改语义 |
| `TimeoutConfig` 字段继续无消费 | 运维困惑 | 配置注释明确标注预留字段与生效范围 |

---

## 八、明确保留现状的项

1. **Workflow 循环迭代上限默认值 10000**：保持为默认值不变，仅暴露可覆盖配置键。
2. **Agent 迭代上限默认值 1000**：保持默认值不变，仅暴露可覆盖配置键。
3. **`max_steps` 语义**（`completed_nodes.len()` 唯一节点计数）：本期不调整语义，如后续需要"节点执行次数"语义另行评审。
4. **navigation_count / loop_navigation_count 双计数器**：当前实现已修复早期方案的缺陷，本期不改动其逻辑。
