# Agent Loop 模块测试用例设计

## 1. 模块概述

Agent Loop 模块负责智能体循环的创建、执行、管理、暂停、恢复、停止等操作。测试重点在于验证智能体循环的完整生命周期管理，包括同步执行、流式执行、检查点等功能。

## 2. 测试用例列表

### 2.1 同步执行 Agent Loop（基础）

**测试用例名称**：`agent_run_sync_basic_success`

**测试场景**：同步执行一个简单的 Agent Loop，不使用工具

**测试步骤**：
1. 执行 CLI 命令：`modular-agent agent run --profile DEFAULT --system-prompt "You are a helpful assistant" --input '{"message": "Hello"}'`
2. 等待执行完成
3. 验证执行结果

**预期 stdout 内容**：
```
正在执行 Agent Loop...
Agent Loop 执行完成

Agent Loop 结果
--------------
迭代次数: 1
工具调用次数: 0
最终状态: completed
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- Agent Loop 执行完成
- 消息历史包含用户消息和助手回复
- 无残留的运行中实例

**验证点**：
- 退出码为 0
- stdout 包含 "Agent Loop 执行完成"
- stdout 显示迭代次数和工具调用次数
- 最终状态为 completed

---

### 2.2 同步执行 Agent Loop（使用工具）

**测试用例名称**：`agent_run_sync_with_tools_success`

**测试场景**：同步执行 Agent Loop，使用可用工具

**测试步骤**：
1. 执行 CLI 命令：`modular-agent agent run --profile DEFAULT --system-prompt "You are a helpful assistant" --tools "get_file,write_file" --input '{"message": "Create a file named test.txt with content Hello"}'`
2. 等待执行完成
3. 验证工具调用

**预期 stdout 内容**：
```
正在执行 Agent Loop...
[工具调用] write_file
[工具完成] tool-call-001
Agent Loop 执行完成

Agent Loop 结果
--------------
迭代次数: 2
工具调用次数: 1
最终状态: completed
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- Agent Loop 执行完成
- 工具被正确调用
- 文件被成功创建

**验证点**：
- 退出码为 0
- stdout 显示工具调用信息
- 工具调用次数正确
- 工具执行结果正确

---

### 2.3 流式执行 Agent Loop

**测试用例名称**：`agent_run_stream_success`

**测试场景**：流式执行 Agent Loop，实时显示执行过程

**测试步骤**：
1. 执行 CLI 命令：`modular-agent agent run --stream --profile DEFAULT --system-prompt "You are a helpful assistant" --input '{"message": "What is 2+2?"}'`
2. 观察流式输出
3. 验证流式事件

**预期 stdout 内容**：
```
正在执行 Agent Loop...
[迭代 1 开始]
助手: Let me calculate that for you.
[工具调用] calculator
[工具完成] tool-call-001
助手: The answer is 4.
[迭代 1 完成]
Agent Loop 执行完成

Agent Loop 结果
--------------
迭代次数: 1
工具调用次数: 1
最终状态: completed
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- Agent Loop 执行完成
- 流式事件正确触发

**验证点**：
- 退出码为 0
- stdout 包含流式事件标记
- 事件顺序正确
- 最终状态正确

---

### 2.4 异步启动 Agent Loop

**测试用例名称**：`agent_start_async_success`

**测试场景**：异步启动 Agent Loop，立即返回 ID

**测试步骤**：
1. 执行 CLI 命令：`modular-agent agent start --profile DEFAULT --system-prompt "You are a helpful assistant" --input '{"message": "Hello"}'`
2. 获取返回的 Agent Loop ID
3. 验证异步启动

**预期 stdout 内容**：
```
正在启动 Agent Loop...

Agent Loop 已启动
  ID: agent-loop-abc123

使用 'modular-agent agent status agent-loop-abc123' 查看状态
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- Agent Loop 已创建并开始执行
- Agent Loop 状态为 running
- 可以通过 ID 查询状态

**验证点**：
- 退出码为 0
- stdout 包含 "Agent Loop 已启动"
- stdout 包含有效的 Agent Loop ID
- 可以通过 status 命令查询到运行状态

---

### 2.5 暂停 Agent Loop

**测试用例名称**：`agent_pause_success`

**测试场景**：暂停正在运行的 Agent Loop

**测试步骤**：
1. 异步启动一个 Agent Loop（使用较长的 max-iterations）
2. 等待一段时间
3. 执行 CLI 命令：`modular-agent agent pause <agent-id>`
4. 验证暂停成功

**预期 stdout 内容**：
```
正在暂停 Agent Loop: agent-loop-abc123
Agent Loop 已暂停: agent-loop-abc123
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- Agent Loop 状态从 running 变为 paused
- 执行被暂停，可以恢复

**验证点**：
- 退出码为 0
- stdout 包含 "Agent Loop 已暂停"
- status 命令显示状态为 paused

---

### 2.6 恢复 Agent Loop

**测试用例名称**：`agent_resume_success`

**测试场景**：恢复已暂停的 Agent Loop

**测试步骤**：
1. 启动并暂停一个 Agent Loop
2. 执行 CLI 命令：`modular-agent agent resume <agent-id>`
3. 验证恢复成功

**预期 stdout 内容**：
```
正在恢复 Agent Loop: agent-loop-abc123
Agent Loop 已恢复: agent-loop-abc123

Agent Loop 结果
--------------
迭代次数: 5
工具调用次数: 2
最终状态: completed
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- Agent Loop 状态从 paused 变为 running
- Agent Loop 继续执行并完成

**验证点**：
- 退出码为 0
- stdout 包含 "Agent Loop 已恢复"
- stdout 显示执行结果
- 最终状态正确

---

### 2.7 停止 Agent Loop

**测试用例名称**：`agent_stop_success`

**测试场景**：停止正在运行的 Agent Loop

**测试步骤**：
1. 异步启动一个 Agent Loop（使用较长的 max-iterations）
2. 等待一段时间
3. 执行 CLI 命令：`modular-agent agent stop <agent-id>`
4. 验证停止成功

**预期 stdout 内容**：
```
正在停止 Agent Loop: agent-loop-abc123
Agent Loop 已停止: agent-loop-abc123
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- Agent Loop 状态从 running 变为 stopped
- 执行被终止

**验证点**：
- 退出码为 0
- stdout 包含 "Agent Loop 已停止"
- status 命令显示状态为 stopped

---

### 2.8 查看状态

**测试用例名称**：`agent_status_success`

**测试场景**：查看 Agent Loop 的运行状态

**测试步骤**：
1. 异步启动一个 Agent Loop
2. 执行 CLI 命令：`modular-agent agent status <agent-id>`
3. 验证状态显示

**预期 stdout 内容**：
```
Agent Loop 状态:
  ID: agent-loop-abc123
  状态: running
  当前迭代: 3
  最大迭代: 10
  工具调用次数: 1
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- stdout 显示正确的状态信息
- 包含 ID、状态、迭代次数等信息

---

### 2.9 查看详情

**测试用例名称**：`agent_show_details_success`

**测试场景**：查看 Agent Loop 的详细信息

**测试步骤**：
1. 执行一个 Agent Loop（已完成）
2. 执行 CLI 命令：`modular-agent agent show <agent-id>`
3. 验证详情显示

**预期 stdout 内容**：
```
Agent Loop 详情
--------------
ID: agent-loop-abc123
状态: completed
创建时间: 2024-01-01T12:00:00.000Z
结束时间: 2024-01-01T12:00:05.000Z

执行统计:
  迭代次数: 5
  工具调用次数: 2
  总时长: 5000ms

配置:
  Profile: DEFAULT
  系统提示词: You are a helpful assistant
  最大迭代: 10
  工具: get_file, write_file

变量:
  - input: Hello
  - output: Hi there!
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- stdout 显示完整的详细信息
- 信息准确无误

---

### 2.10 列出所有 Agent Loop

**测试用例名称**：`agent_list_all_success`

**测试场景**：列出所有 Agent Loop 实例

**测试步骤**：
1. 创建多个 Agent Loop（不同状态）
2. 执行 CLI 命令：`modular-agent agent list`
3. 验证列表输出

**预期 stdout 内容**：
```
ID                  状态      当前迭代    工具调用
-------------------------------------------------------
agent-loop-abc123   running   3          1
agent-loop-def456   paused    2          0
agent-loop-ghi789   completed 5          2
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- stdout 包含所有 Agent Loop
- 列表格式正确

---

### 2.11 列出运行中的 Agent Loop

**测试用例名称**：`agent_list_running_success`

**测试场景**：仅列出运行中的 Agent Loop

**测试步骤**：
1. 创建多个不同状态的 Agent Loop
2. 执行 CLI 命令：`modular-agent agent list --running`
3. 验证过滤正确

**预期 stdout 内容**：
```
ID                  状态      当前迭代    工具调用
-------------------------------------------------------
agent-loop-abc123   running   3          1
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- 只显示 running 状态的 Agent Loop

---

### 2.12 创建检查点

**测试用例名称**：`agent_checkpoint_create_success`

**测试场景**：为运行中的 Agent Loop 创建检查点

**测试步骤**：
1. 启动一个 Agent Loop
2. 执行 CLI 命令：`modular-agent agent checkpoint <agent-id> --name "test-checkpoint"`
3. 验证检查点创建

**预期 stdout 内容**：
```
正在创建检查点: agent-loop-abc123
检查点已创建: checkpoint-xyz789
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 检查点已创建
- 检查点包含 Agent Loop 的当前状态

**验证点**：
- 退出码为 0
- stdout 包含 "检查点已创建"
- 返回有效的检查点 ID

---

### 2.13 从检查点恢复

**测试用例名称**：`agent_checkpoint_restore_success`

**测试场景**：从检查点恢复 Agent Loop

**测试步骤**：
1. 创建一个检查点
2. 执行 CLI 命令：`modular-agent agent restore <checkpoint-id>`
3. 验证恢复成功

**预期 stdout 内容**：
```
正在从检查点恢复: checkpoint-xyz789
Agent Loop 已从检查点恢复: agent-loop-new123
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 新的 Agent Loop 实例已创建
- 状态与检查点一致
- 可以继续执行

**验证点**：
- 退出码为 0
- stdout 包含 "Agent Loop 已从检查点恢复"
- 返回新的 Agent Loop ID
- 状态与检查点一致

---

### 2.14 克隆 Agent Loop

**测试用例名称**：`agent_clone_success`

**测试场景**：克隆现有的 Agent Loop

**测试步骤**：
1. 创建一个 Agent Loop
2. 执行 CLI 命令：`modular-agent agent clone <agent-id>`
3. 验证克隆成功

**预期 stdout 内容**：
```
正在克隆 Agent Loop: agent-loop-abc123
Agent Loop 已克隆
  新 ID: agent-loop-clone456
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 新的 Agent Loop 已创建
- 配置和状态与原 Agent Loop 相同
- 两个实例独立运行

**验证点**：
- 退出码为 0
- stdout 包含 "Agent Loop 已克隆"
- 返回新的 Agent Loop ID
- 克隆的实例配置正确

---

### 2.15 清理已完成的 Agent Loop

**测试用例名称**：`agent_cleanup_success`

**测试场景**：清理已完成的 Agent Loop 实例

**测试步骤**：
1. 创建多个已完成的 Agent Loop
2. 执行 CLI 命令：`modular-agent agent cleanup`
3. 验证清理成功

**预期 stdout 内容**：
```
已清理 3 个完成的 Agent Loop
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 已完成的 Agent Loop 已被移除
- 运行中和暂停的 Agent Loop 保留

**验证点**：
- 退出码为 0
- stdout 显示清理的数量
- list 命令不再显示已清理的实例

---

### 2.16 查看消息历史

**测试用例名称**：`agent_messages_success`

**测试场景**：查看 Agent Loop 的消息历史

**测试步骤**：
1. 执行一个 Agent Loop
2. 执行 CLI 命令：`modular-agent agent messages <agent-id>`
3. 验证消息历史显示

**预期 stdout 内容**：
```
1. [user] Hello
2. [assistant] Hi there!
3. [user] What is 2+2?
4. [assistant] The answer is 4.
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- stdout 显示所有消息
- 消息顺序正确
- 包含 role 和 content

---

### 2.17 查看消息历史（详细模式）

**测试用例名称**：`agent_messages_verbose_success`

**测试场景**：以详细模式查看消息历史

**测试步骤**：
1. 执行一个 Agent Loop
2. 执行 CLI 命令：`modular-agent agent messages <agent-id> --verbose`
3. 验证详细消息显示

**预期 stdout 内容**：
```
[
  {
    "role": "user",
    "content": "Hello",
    "timestamp": "2024-01-01T12:00:00.000Z"
  },
  {
    "role": "assistant",
    "content": "Hi there!",
    "timestamp": "2024-01-01T12:00:01.000Z"
  },
  {
    "role": "user",
    "content": "What is 2+2?",
    "timestamp": "2024-01-01T12:00:02.000Z"
  },
  {
    "role": "assistant",
    "content": "The answer is 4.",
    "timestamp": "2024-01-01T12:00:03.000Z"
  }
]
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- 输出为 JSON 格式
- 包含完整消息信息

---

### 2.18 查看变量

**测试用例名称**：`agent_variables_success`

**测试场景**：查看 Agent Loop 的变量

**测试步骤**：
1. 执行一个 Agent Loop
2. 执行 CLI 命令：`modular-agent agent variables <agent-id>`
3. 验证变量显示

**预期 stdout 内容**：
```
input = Hello
output = Hi there!
iteration_count = 1
tool_calls = 0
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- 显示所有变量
- 变量值正确

---

### 2.19 设置变量

**测试用例名称**：`agent_set_variable_success`

**测试场景**：设置 Agent Loop 的变量

**测试步骤**：
1. 启动一个 Agent Loop
2. 执行 CLI 命令：`modular-agent agent set-var <agent-id> custom_var "custom value"`
3. 验证变量设置

**预期 stdout 内容**：
```
变量已设置: custom_var
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 变量已添加到 Agent Loop
- 可以通过 variables 命令查询到

**验证点**：
- 退出码为 0
- stdout 包含 "变量已设置"
- 变量确实被设置

---

### 2.20 删除 Agent Loop

**测试用例名称**：`agent_delete_success`

**测试场景**：删除 Agent Loop 实例

**测试步骤**：
1. 创建一个 Agent Loop
2. 执行 CLI 命令：`modular-agent agent delete <agent-id> --force`
3. 验证删除成功

**预期 stdout 内容**：
```
Agent Loop 资源已清理: agent-loop-abc123
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- Agent Loop 已从注册表中移除
- 相关资源已清理

**验证点**：
- 退出码为 0
- stdout 包含 "Agent Loop 资源已清理"
- 无法再查询到该 Agent Loop

---

### 2.21 最大迭代次数限制

**测试用例名称**：`agent_max_iterations_limit`

**测试场景**：验证 Agent Loop 在达到最大迭代次数后停止

**测试步骤**：
1. 执行 CLI 命令：`modular-agent agent run --max-iterations 3 --input '{"message": "Continue asking questions"}'`
2. 验证迭代次数限制

**预期 stdout 内容**：
```
正在执行 Agent Loop...
[迭代 1 完成]
[迭代 2 完成]
[迭代 3 完成]
Agent Loop 执行完成

Agent Loop 结果
--------------
迭代次数: 3
工具调用次数: 0
最终状态: stopped
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- Agent Loop 在第3次迭代后停止
- 最终状态为 stopped

**验证点**：
- 退出码为 0
- 迭代次数正好是 3
- 最终状态为 stopped（非 completed）

---

### 2.22 错误处理

**测试用例名称**：`agent_error_handling`

**测试场景**：验证 Agent Loop 对错误的处理

**测试步骤**：
1. 执行一个可能产生错误的 Agent Loop
2. 验证错误处理

**预期 stdout 内容**：
```
正在执行 Agent Loop...
[工具调用] error_tool
[工具失败] tool-call-001: Tool execution failed
Agent Loop 执行完成

Agent Loop 结果
--------------
迭代次数: 1
工具调用次数: 1
最终状态: error
```

**预期 stderr 内容**：
```
（可能包含错误详情）
```

**预期资源状态**：
- Agent Loop 因错误停止
- 错误信息被记录

**验证点**：
- 退出码可能非 0（取决于错误严重程度）
- stdout 显示工具失败信息
- 最终状态为 error

---

### 2.23 Agent Loop 不存在

**测试用例名称**：`agent_not_found`

**测试场景**：尝试操作不存在的 Agent Loop

**测试步骤**：
1. 执行 CLI 命令：`modular-agent agent status nonexistent-id`
2. 验证错误处理

**预期 stdout 内容**：
```
（空或包含错误摘要）
```

**预期 stderr 内容**：
```
错误：Agent Loop 不存在: nonexistent-id
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码非 0
- stderr 包含明确的错误信息

---

## 3. 测试数据准备

### 3.1 简单 Agent Loop 配置

```toml
# fixtures/agent-loops/simple-agent.toml
[agent]
profile_id = "DEFAULT"
system_prompt = "You are a helpful assistant"
max_iterations = 10

[agent.tools]
enabled = []

[agent.variables]
input = "Hello"
```

### 3.2 多工具 Agent Loop 配置

```toml
# fixtures/agent-loops/multi-tool-agent.toml
[agent]
profile_id = "DEFAULT"
system_prompt = "You are a file management assistant"
max_iterations = 20

[agent.tools]
enabled = ["get_file", "write_file", "read_file", "delete_file"]

[agent.variables]
working_dir = "/tmp"
```

### 3.3 长时间运行 Agent Loop 配置

```toml
# fixtures/agent-loops/long-running-agent.toml
[agent]
profile_id = "DEFAULT"
system_prompt = "You are a research assistant"
max_iterations = 100

[agent.tools]
enabled = ["search_web", "summarize_text"]
```

## 4. 测试执行顺序建议

1. 先测试基础执行功能（同步执行、异步启动）
2. 再测试状态管理（暂停、恢复、停止）
3. 然后测试高级功能（检查点、克隆）
4. 最后测试错误处理和边界情况

## 5. 注意事项

1. 测试异步操作时，可能需要添加等待时间或轮询状态
2. 测试清理功能时，确保资源确实被清理
3. 测试检查点功能时，验证状态完整性
4. 注意测试超时，长时间运行的测试需要设置合理的超时时间
5. 使用 Mock LLM Profile 避免依赖外部服务