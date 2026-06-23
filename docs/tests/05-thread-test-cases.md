# Thread 模块测试用例设计

## 1. 模块概述

Thread 模块负责工作流执行线程（Thread）的管理，包括执行、暂停、恢复、取消等操作。测试重点在于验证线程的生命周期管理和事件订阅功能。

## 2. 测试用例列表

### 2.1 执行线程（简单工作流）

**测试用例名称**：`thread_execute_simple_workflow_success`

**测试场景**：执行一个简单的工作流，创建并运行线程

**测试步骤**：
1. 预先注册一个简单工作流
2. 执行 CLI 命令：`modular-agent thread execute <workflow-id> --params '{"input": "test"}'`
3. 等待执行完成
4. 验证执行结果

**预期 stdout 内容**：
```
正在执行工作流: wf-simple-001
线程已创建: thread-abc123
[节点执行] start
[节点执行] process
[节点执行] end
线程执行完成

线程结果
--------
ID: thread-abc123
工作流: wf-simple-001
状态: completed
执行时长: 1500ms
节点执行数: 3
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 线程已创建并执行完成
- 工作流的所有节点按顺序执行
- 线程状态为 completed

**验证点**：
- 退出码为 0
- stdout 包含 "线程执行完成"
- stdout 显示线程 ID 和状态
- 所有节点都被执行

---

### 2.2 执行线程（带参数）

**测试用例名称**：`thread_execute_with_parameters_success`

**测试场景**：执行工作流并传递参数

**测试步骤**：
1. 预先注册一个需要参数的工作流
2. 准备参数 JSON
3. 执行 CLI 命令：`modular-agent thread execute <workflow-id> --params '{"name": "test", "value": 123}'`
4. 验证参数传递

**预期 stdout 内容**：
```
正在执行工作流: wf-param-001
线程已创建: thread-def456
[节点执行] start
[节点执行] process (name=test, value=123)
[节点执行] end
线程执行完成

线程结果
--------
ID: thread-def456
工作流: wf-param-001
状态: completed
执行时长: 2000ms
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 参数正确传递给工作流
- 节点可以使用传递的参数

**验证点**：
- 退出码为 0
- 参数被正确使用

---

### 2.3 暂停线程

**测试用例名称**：`thread_pause_success`

**测试场景**：暂停正在执行的线程

**测试步骤**：
1. 执行一个长时间运行的工作流
2. 在执行过程中暂停
3. 执行 CLI 命令：`modular-agent thread pause <thread-id>`
4. 验证暂停成功

**预期 stdout 内容**：
```
正在暂停线程: thread-abc123
线程已暂停: thread-abc123
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 线程状态从 running 变为 paused
- 执行被中断
- 可以恢复执行

**验证点**：
- 退出码为 0
- stdout 包含 "线程已暂停"
- status 命令显示状态为 paused

---

### 2.4 恢复线程

**测试用例名称**：`thread_resume_success`

**测试场景**：恢复已暂停的线程

**测试步骤**：
1. 执行并暂停一个线程
2. 执行 CLI 命令：`modular-agent thread resume <thread-id>`
3. 验证恢复成功

**预期 stdout 内容**：
```
正在恢复线程: thread-abc123
[节点执行]继续执行...
线程执行完成

线程结果
--------
ID: thread-abc123
工作流: wf-simple-001
状态: completed
执行时长: 3500ms
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 线程状态从 paused 变为 running
- 线程继续执行并完成

**验证点**：
- 退出码为 0
- stdout 包含 "线程执行完成"
- 线程从暂停点继续执行

---

### 2.5 取消线程

**测试用例名称**：`thread_cancel_success`

**测试场景**：取消正在执行的线程

**测试步骤**：
1. 执行一个长时间运行的工作流
2. 在执行过程中取消
3. 执行 CLI 命令：`modular-agent thread cancel <thread-id>`
4. 验证取消成功

**预期 stdout 内容**：
```
正在取消线程: thread-abc123
线程已取消: thread-abc123

线程结果
--------
ID: thread-abc123
工作流: wf-simple-001
状态: cancelled
执行时长: 1500ms
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 线程状态从 running 变为 cancelled
- 执行被终止
- 已执行的节点结果被保留

**验证点**：
- 退出码为 0
- stdout 包含 "线程已取消"
- 最终状态为 cancelled

---

### 2.6 查看线程状态

**测试用例名称**：`thread_status_success`

**测试场景**：查看线程的运行状态

**测试步骤**：
1. 执行一个工作流
2. 执行 CLI 命令：`modular-agent thread status <thread-id>`
3. 验证状态显示

**预期 stdout 内容**：
```
线程状态
--------
ID: thread-abc123
工作流: wf-simple-001
状态: running
当前节点: process
进度: 2/3
开始时间: 2024-01-01T12:00:00.000Z
执行时长: 1200ms
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- 显示正确的状态信息
- 包含进度信息

---

### 2.7 查看线程详情

**测试用例名称**：`thread_show_details_success`

**测试场景**：查看线程的详细信息

**测试步骤**：
1. 执行一个工作流（已完成）
2. 执行 CLI 命令：`modular-agent thread show <thread-id>`
3. 验证详情显示

**预期 stdout 内容**：
```
线程详情
--------
ID: thread-abc123
工作流: wf-simple-001
状态: completed
创建时间: 2024-01-01T12:00:00.000Z
开始时间: 2024-01-01T12:00:00.000Z
结束时间: 2024-01-01T12:00:01.500Z
执行时长: 1500ms

执行历史:
  1. start (类型: start, 状态: completed, 耗时: 50ms)
  2. process (类型: node, 状态: completed, 耗时: 1400ms)
  3. end (类型: end, 状态: completed, 耗时: 50ms)

变量:
  - input: test
  - output: processed
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- 显示完整的执行历史
- 显示所有变量

---

### 2.8 列出所有线程

**测试用例名称**：`thread_list_all_success`

**测试场景**：列出所有线程

**测试步骤**：
1. 创建多个不同状态的线程
2. 执行 CLI 命令：`modular-agent thread list`
3. 验证列表输出

**预期 stdout 内容**：
```
ID              工作流          状态      进度    执行时长
-------------------------------------------------------------
thread-abc123   wf-simple-001   running   2/3    1200ms
thread-def456   wf-simple-001   paused    1/3    500ms
thread-ghi789   wf-simple-001   completed 3/3    1500ms
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- 显示所有线程
- 列表格式正确

---

### 2.9 列出运行中的线程

**测试用例名称**：`thread_list_running_success`

**测试场景**：仅列出运行中的线程

**测试步骤**：
1. 创建多个不同状态的线程
2. 执行 CLI 命令：`modular-agent thread list --running`
3. 验证过滤正确

**预期 stdout 内容**：
```
ID              工作流          状态      进度    执行时长
-------------------------------------------------------------
thread-abc123   wf-simple-001   running   2/3    1200ms
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- 只显示 running 状态的线程

---

### 2.10 订阅线程事件

**测试用例名称**：`thread_subscribe_events_success`

**测试场景**：订阅线程事件，实时接收执行事件

**测试步骤**：
1. 执行 CLI 命令：`modular-agent thread subscribe <thread-id>`
2. 观察事件流
3. 验证事件接收

**预期 stdout 内容**：
```
正在订阅线程事件: thread-abc123
[事件] thread_started: { threadId: "thread-abc123", workflowId: "wf-simple-001" }
[事件] node_started: { nodeId: "start", nodeType: "start" }
[事件] node_completed: { nodeId: "start", status: "completed", duration: 50 }
[事件] node_started: { nodeId: "process", nodeType: "node" }
[事件] node_completed: { nodeId: "process", status: "completed", duration: 1400 }
[事件] node_started: { nodeId: "end", nodeType: "end" }
[事件] node_completed: { nodeId: "end", status: "completed", duration: 50 }
[事件] thread_completed: { threadId: "thread-abc123", status: "completed", duration: 1500 }
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0（Ctrl+C 退出订阅）
- 接收到所有事件
- 事件顺序正确

---

### 2.11 取消订阅线程事件

**测试用例名称**：`thread_unsubscribe_events_success`

**测试场景**：取消订阅线程事件

**测试步骤**：
1. 订阅线程事件
2. 执行 CLI 命令：`modular-agent thread unsubscribe <thread-id>`
3. 验证取消订阅

**预期 stdout 内容**：
```
已取消订阅线程事件: thread-abc123
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 订阅被取消
- 不再接收事件

**验证点**：
- 退出码为 0
- stdout 包含取消订阅确认

---

### 2.12 工作流不存在

**测试用例名称**：`thread_execute_workflow_not_found`

**测试场景**：尝试执行不存在的工作流

**测试步骤**：
1. 执行 CLI 命令：`modular-agent thread execute nonexistent-wf-id`
2. 验证错误处理

**预期 stdout 内容**：
```
（空或包含错误摘要）
```

**预期 stderr 内容**：
```
错误：工作流不存在: nonexistent-wf-id
```

**预期资源状态**：
- 无线程被创建

**验证点**：
- 退出码非 0
- stderr 包含明确的错误信息

---

### 2.13 线程不存在

**测试用例名称**：`thread_not_found`

**测试场景**：尝试操作不存在的线程

**测试步骤**：
1. 执行 CLI 命令：`modular-agent thread status nonexistent-thread-id`
2. 验证错误处理

**预期 stdout 内容**：
```
（空或包含错误摘要）
```

**预期 stderr 内容**：
```
错误：线程不存在: nonexistent-thread-id
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码非 0
- stderr 包含明确的错误信息

---

### 2.14 节点执行失败

**测试用例名称**：`thread_node_execution_failure`

**测试场景**：线程执行过程中节点失败

**测试步骤**：
1. 预先注册一个包含可能失败节点的工作流
2. 执行 CLI 命令：`modular-agent thread execute <workflow-id>`
3. 验证错误处理

**预期 stdout 内容**：
```
正在执行工作流: wf-error-001
线程已创建: thread-error-123
[节点执行] start
[节点执行] process
[节点失败] error_node: Error: Node execution failed
线程执行失败

线程结果
--------
ID: thread-error-123
工作流: wf-error-001
状态: failed
执行时长: 800ms
失败节点: error_node
错误信息: Error: Node execution failed
```

**预期 stderr 内容**：
```
（可能包含错误详情）
```

**预期资源状态**：
- 线程状态为 failed
- 错误信息被记录

**验证点**：
- 退出码非 0
- stdout 显示失败信息
- 最终状态为 failed

---

### 2.15 条件路由测试

**测试用例名称**：`thread_conditional_routing_success`

**测试场景**：测试工作流的条件路由功能

**测试步骤**：
1. 预先注册一个包含条件路由的工作流
2. 执行 CLI 命令：`modular-agent thread execute <workflow-id> --params '{"condition": "true"}'`
3. 验证路由正确

**预期 stdout 内容**：
```
正在执行工作流: wf-routing-001
线程已创建: thread-routing-123
[节点执行] start
[条件判断] condition_node: true
[路由到] path_a
[节点执行] node_a
[节点执行] end
线程执行完成

线程结果
--------
ID: thread-routing-123
工作流: wf-routing-001
状态: completed
执行时长: 2000ms
选择的路径: path_a
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 条件路由正确执行
- 只执行了符合条件的路径

**验证点**：
- 退出码为 0
- stdout 显示路由信息
- 正确的路径被执行

---

### 2.16 并行执行测试

**测试用例名称**：`thread_parallel_execution_success`

**测试场景**：测试工作流的并行执行功能

**测试步骤**：
1. 预先注册一个包含并行节点的工作流
2. 执行 CLI 命令：`modular-agent thread execute <workflow-id>`
3. 验证并行执行

**预期 stdout 内容**：
```
正在执行工作流: wf-parallel-001
线程已创建: thread-parallel-123
[节点执行] start
[并行执行] task1, task2, task3
[节点完成] task2 (耗时: 800ms)
[节点完成] task1 (耗时: 1000ms)
[节点完成] task3 (耗时: 1200ms)
[节点执行] end
线程执行完成

线程结果
--------
ID: thread-parallel-123
工作流: wf-parallel-001
状态: completed
执行时长: 1300ms
并行节点数: 3
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 并行节点同时执行
- 总执行时间小于各节点时间之和

**验证点**：
- 退出码为 0
- stdout 显示并行执行信息
- 执行时间符合并行特征

---

## 3. 测试数据准备

### 3.1 简单工作流配置

```toml
# fixtures/workflows/simple-thread-workflow.toml
[workflow]
name = "simple-thread-workflow"
description = "简单线程工作流"

[[workflow.nodes]]
id = "start"
type = "start"

[[workflow.nodes]]
id = "process"
type = "node"
function = "node:processor"

[[workflow.nodes]]
id = "end"
type = "end"

[[workflow.edges]]
from = "start"
to = "process"

[[workflow.edges]]
from = "process"
to = "end"
```

### 3.2 带参数的工作流配置

```toml
# fixtures/workflows/param-thread-workflow.toml
[workflow]
name = "param-thread-workflow"
description = "带参数的线程工作流"

[[workflow.nodes]]
id = "start"
type = "start"

[[workflow.nodes]]
id = "process"
type = "node"
function = "node:processor"

[workflow.nodes.config]
input_param = "${name}"
value_param = "${value}"

[[workflow.nodes]]
id = "end"
type = "end"

[[workflow.edges]]
from = "start"
to = "process"

[[workflow.edges]]
from = "process"
to = "end"
```

### 3.3 条件路由工作流配置

```toml
# fixtures/workflows/routing-thread-workflow.toml
[workflow]
name = "routing-thread-workflow"
description = "条件路由工作流"

[[workflow.nodes]]
id = "start"
type = "start"

[[workflow.nodes]]
id = "condition_node"
type = "condition"
function = "condition:check_value"

[[workflow.nodes]]
id = "node_a"
type = "node"
function = "node:process_a"

[[workflow.nodes]]
id = "node_b"
type = "node"
function = "node:process_b"

[[workflow.nodes]]
id = "end"
type = "end"

[[workflow.edges]]
from = "start"
to = "condition_node"

[[workflow.edges]]
from = "condition_node"
to = "node_a"
condition = "path_a"

[[workflow.edges]]
from = "condition_node"
to = "node_b"
condition = "path_b"

[[workflow.edges]]
from = "node_a"
to = "end"

[[workflow.edges]]
from = "node_b"
to = "end"
```

## 4. 测试执行顺序建议

1. 先测试基础执行功能
2. 再测试状态管理（暂停、恢复、取消）
3. 然后测试高级功能（事件订阅、条件路由、并行执行）
4. 最后测试错误处理

## 5. 注意事项

1. 测试暂停和恢复时，需要确保线程状态正确转换
2. 测试事件订阅时，可能需要处理异步事件流
3. 测试并行执行时，注意执行时间的验证
4. 测试条件路由时，验证只执行符合条件的路径
5. 注意清理测试创建的线程，避免影响其他测试