# Workflow 资源数量限制修改方案

> 基于代码分析文档 `workflow-module-analysis.md` 和 `agent-execution-instance-analysis.md` 提出的问题

---

## 一、现状分析

### 1.1 Agent 资源限制现状

| 限制项 | 当前值 | 实现位置 |
|--------|--------|----------|
| 并发执行数 | CPU核心数 | `AgentCapacityGate` + `AgentLoopRegistry` |
| 子agent递归深度 | 8层 | `DEFAULT_MAX_SUB_AGENT_DEPTH` |
| 最大迭代次数 | 无全局上限 | `AgentDefinition.config.max_iterations` |
| 最大执行时间 | 无全局上限 | 配置项 |

**存在的问题**：`max_iterations` 无全局硬编码上限，可能导致资源长时间占用。

### 1.2 Workflow 资源限制现状

| 限制项 | 当前值 | 实现位置 |
|--------|--------|----------|
| 循环最大迭代上限 | 10,000次 | `MAX_ITERATIONS_CAP` |
| 全局执行时间 | 无上限 | `max_execution_time` |
| 步数上限 | 无上限 | `max_steps` |
| 死循环保护 | `navigation_count > 节点数×5` | `check_navigation_backstop()` |

**存在的问题**：`navigation_count` 的死循环检测逻辑存在缺陷，详见下文分析。

---

## 二、navigation_count 死循环检测问题分析

### 2.1 当前实现

```rust
// workflow.rs:888-910
fn check_navigation_backstop(&mut self, node_id: &str) -> WorkflowResult<()> {
    // 当进入 LOOP_START 且有活跃循环时，重置计数器
    if self.traversal.get_node(node_id)
        .is_some_and(|n| n.node_type == "LOOP_START")
        && !crate::loop_state::stack(&self.ctx.variables).is_empty()
    {
        self.navigation_count = 0;
    }

    self.navigation_count += 1;
    let max_allowed = self.total_node_count * self.max_navigation_multiplier;
    if self.navigation_count > max_allowed && max_allowed > 0 {
        return Err(WorkflowError::CoordinatorError(...));
    }
    Ok(())
}
```

### 2.2 问题分析

#### 问题1：Loop 节点计数不准确

**场景**：一个包含 `LOOP_START -> A -> B -> LOOP_END` 的循环，`max_iterations = 1000`

- 每次进入 `LOOP_START` 时 `navigation_count` 被重置为 0
- 循环体内的节点（A、B）每次迭代都会使 `navigation_count` 递增
- 如果 `total_node_count = 4`，`max_navigation_multiplier = 5`，则 `max_allowed = 20`
- 循环执行 5 次后就会触发死循环检测，但实际循环是正常的

**结论**：当前的重置逻辑无法正确处理循环场景，因为循环体内的节点计数没有被排除。

#### 问题2：嵌套循环计数叠加

**场景**：外层循环 10 次，内层循环 10 次，每次循环体 2 个节点

- 外层循环体：`LOOP_START -> INNER_LOOP_START -> A -> INNER_LOOP_END -> LOOP_END`
- 外层循环每次迭代，内层循环执行 10 次，每次内层循环体 2 个节点
- `navigation_count` 会快速累积，可能误触发死循环检测

#### 问题3：Embed/Subgraph 的独立性

**现状**：`SubgraphHandler` 会创建新的 `WorkflowCoordinator`，拥有独立的 `navigation_count`，所以 **Embed/Subgraph 的计数已经是独立的**，不受父级影响。

但 Embed 图被预处理展开为扁平图后，其节点会计入父级的 `total_node_count`，这可能导致父级的死循环阈值计算不准确。

#### 问题4：Fork 并行分支的计数干扰

Fork 的每个分支都会创建新的 `WorkflowCoordinator`，各自拥有独立的 `navigation_count`，所以 Fork 分支的计数不会互相干扰。

### 2.3 核心问题总结

| 场景 | 当前行为 | 问题 |
|------|----------|------|
| 正常循环 | 每次进入 LOOP_START 重置 | 循环体内的节点仍然计数，导致阈值过低 |
| 嵌套循环 | 多层重置叠加 | 外层循环的重置可能干扰内层计数 |
| Embed/Subgraph | 独立计数 | 无问题（已独立） |
| Fork 并行 | 独立计数 | 无问题（已独立） |
| 非循环死循环 | 正常检测 | 无问题 |

---

## 三、修改方案

### 3.1 方案概述

将 `navigation_count` 拆分为两个独立的计数器：

| 计数器 | 用途 | 检测目标 |
|--------|------|----------|
| `navigation_count` | 检测 DAG 中的非循环死循环（如错误的边配置导致的无限跳转） | 非循环场景的死循环 |
| `dead_loop_count` | 检测循环节点本身的异常（如 max_iterations 过大导致的长时间运行） | 循环场景的异常 |

### 3.2 详细设计

#### 3.2.1 修改 `WorkflowCoordinator` 结构体

```rust
pub struct WorkflowCoordinator {
    // ... 现有字段 ...

    /// 记录 DAG 非循环路径的导航次数（不包含循环体内的节点）
    navigation_count: u32,

    /// 记录循环节点的执行次数（用于检测循环本身的异常）
    dead_loop_count: u32,

    /// 当前是否处于循环体内（用于区分计数）
    in_loop_body: bool,

    /// 当前循环的迭代计数（用于死循环检测）
    loop_iteration_count: u32,

    // ... 现有字段 ...
}
```

#### 3.2.2 修改 `check_navigation_backstop()` 方法

```rust
fn check_navigation_backstop(&mut self, node_id: &str) -> WorkflowResult<()> {
    let node_type = self.traversal.get_node(node_id)
        .map(|n| n.node_type.as_str())
        .unwrap_or("");

    // 检测循环状态变化
    match node_type {
        "LOOP_START" => {
            // 进入循环：重置导航计数，开始循环计数
            self.navigation_count = 0;
            self.in_loop_body = true;
            self.loop_iteration_count = 0;
        }
        "LOOP_END" => {
            // 离开循环：重置循环计数，恢复导航计数
            self.in_loop_body = false;
            self.loop_iteration_count = 0;
        }
        _ => {}
    }

    if self.in_loop_body {
        // 循环体内：使用循环计数器
        self.loop_iteration_count += 1;
        let max_loop_iterations = self.total_node_count * self.max_navigation_multiplier;
        if self.loop_iteration_count > max_loop_iterations && max_loop_iterations > 0 {
            return Err(WorkflowError::CoordinatorError(format!(
                "Potential infinite loop: loop body executed {} times (max allowed: {})",
                self.loop_iteration_count, max_loop_iterations
            )));
        }
    } else {
        // 非循环路径：使用导航计数器
        self.navigation_count += 1;
        let max_allowed = self.total_node_count * self.max_navigation_multiplier;
        if self.navigation_count > max_allowed && max_allowed > 0 {
            return Err(WorkflowError::CoordinatorError(format!(
                "Infinite loop detected: {} navigations exceeded max {} ({} nodes x {})",
                self.navigation_count, max_allowed,
                self.total_node_count, self.max_navigation_multiplier
            )));
        }
    }

    Ok(())
}
```

#### 3.2.3 修改初始化代码

```rust
fn new_preprocessed(...) -> WorkflowResult<Self> {
    // ... 现有代码 ...
    Ok(Self {
        // ... 现有字段 ...
        navigation_count: 0,
        dead_loop_count: 0,      // 新增
        in_loop_body: false,     // 新增
        loop_iteration_count: 0, // 新增
        // ... 现有字段 ...
    })
}
```

#### 3.2.4 修改 `resume_from()` 方法

需要在恢复检查点时正确恢复 `in_loop_body` 和 `loop_iteration_count` 状态。

### 3.3 Agent 资源限制修改

#### 3.3.1 添加 `max_iterations` 全局上限

```rust
// 在 wf-types 或 wf-config 中定义
pub const AGENT_MAX_ITERATIONS_CAP: u32 = 1000;
```

在 `AgentLoopNodeConfig` 处理时检查：

```rust
let max_iterations = agent_config.and_then(|c| c.max_iterations).unwrap_or(10);
if max_iterations > AGENT_MAX_ITERATIONS_CAP {
    return Err(AgentError::ConfigError(format!(
        "max_iterations ({}) exceeds the allowed limit ({})",
        max_iterations, AGENT_MAX_ITERATIONS_CAP
    )));
}
```

---

## 四、Workflow 其他资源限制修改

### 4.1 添加全局并发预算

当前 `TaskScheduler` 已实现但未被使用。建议：

1. 在 `WorkflowCoordinator` 中引入 `Arc<ConcurrencyGate>` 作为全局并发预算
2. 在 `ForkHandler` 中使用该 gate 控制分支并发数
3. 配置项：`max_concurrent_nodes`（默认值：CPU核心数）

```rust
pub struct WorkflowCoordinator {
    // ... 现有字段 ...
    /// 全局并发预算，控制所有并行节点的总并发数
    concurrency_gate: Option<Arc<ConcurrencyGate>>,
}
```

### 4.2 统一暂停语义

将 `WorkflowExecutionState.status == Paused` 和 `InterruptionState::Pause` 统一为单一真相源：

```rust
// 以 InterruptionState 为唯一暂停真相
pub fn is_paused(&self) -> bool {
    self.interruption.check() == Some(Interruption::Pause)
}
```

### 4.3 修复层级深度追踪

```rust
impl WorkflowExecutionEntity {
    pub fn get_hierarchy_depth(&self) -> u32 {
        // 沿 parent_execution_id 上溯计数
        self.hierarchy_depth.load(Ordering::Relaxed)
    }
}
```

---

## 五、实施计划

### 5.1 阶段划分

| 阶段 | 任务 | 优先级 |
|------|------|--------|
| P0 | 修复 navigation_count 死循环检测逻辑 | 高 |
| P0 | 添加 Agent max_iterations 全局上限 | 高 |
| P1 | 引入全局并发预算（TaskScheduler 接入） | 中 |
| P1 | 统一暂停语义 | 中 |
| P2 | 修复层级深度追踪 | 低 |

### 5.2 测试策略

1. **单元测试**：为新的 `check_navigation_backstop()` 添加测试用例
   - 测试非循环死循环检测
   - 测试循环场景下的正确重置
   - 测试嵌套循环场景
2. **集成测试**：测试包含循环、Fork、Embed 的复杂工作流
3. **回归测试**：确保现有测试通过

---

## 六、风险评估

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 循环计数器阈值设置不当 | 正常循环被误判为死循环 | 使用 `max_iterations` 作为主要限制，死循环检测作为辅助 |
| 并发预算导致性能下降 | 并行任务被过度限制 | 默认值设置合理，并提供配置项调整 |
| 暂停语义统一影响现有代码 | 现有暂停逻辑失效 | 分阶段迁移，保持向后兼容 |

---

## 七、相关文件

- `crates/engine/wf-workflow/src/coordinator/workflow.rs` - navigation_count 实现
- `crates/engine/wf-workflow/src/handler/loop_handler.rs` - 循环处理器
- `crates/engine/wf-workflow/src/loop_state.rs` - 循环状态管理
- `crates/engine/wf-agent/src/registry.rs` - Agent 并发控制
- `crates/engine/wf-agent/src/capacity.rs` - Agent 容量门
