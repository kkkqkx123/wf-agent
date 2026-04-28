# Checkpoint 模块测试用例设计

## 1. 模块概述

Checkpoint 模块负责检查点的创建、管理、查询和恢复功能。测试重点在于验证检查点的完整性和恢复功能。

## 2. 测试用例列表

### 2.1 创建线程检查点

**测试用例名称**：`checkpoint_create_thread_success`

**测试场景**：为运行中的线程创建检查点

**测试步骤**：
1. 执行一个工作流（运行中）
2. 执行 CLI 命令：`modular-agent checkpoint create <thread-id> --name "test-checkpoint"`
3. 验证检查点创建

**预期 stdout 内容**：
```
正在创建检查点: thread-abc123
检查点已创建: checkpoint-xyz789

检查点信息
----------
ID: checkpoint-xyz789
线程ID: thread-abc123
名称: test-checkpoint
创建时间: 2024-01-01T12:00:00.000Z
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 检查点已创建
- 包含线程的当前状态
- 可以被查询和恢复

**验证点**：
- 退出码为 0
- stdout 包含 "检查点已创建"
- 返回有效的检查点 ID

---

### 2.2 创建 Agent Loop 检查点

**测试用例名称**：`checkpoint_create_agent_loop_success`

**测试场景**：为运行中的 Agent Loop 创建检查点

**测试步骤**：
1. 启动一个 Agent Loop（运行中）
2. 执行 CLI 命令：`modular-agent agent checkpoint <agent-id> --name "agent-checkpoint"`
3. 验证检查点创建

**预期 stdout 内容**：
```
正在创建检查点: agent-loop-abc123
检查点已创建: checkpoint-agent-001

检查点信息
----------
ID: checkpoint-agent-001
Agent Loop ID: agent-loop-abc123
名称: agent-checkpoint
创建时间: 2024-01-01T12:00:00.000Z
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 检查点已创建
- 包含 Agent Loop 的当前状态
- 可以被查询和恢复

**验证点**：
- 退出码为 0
- stdout 包含 "检查点已创建"
- 返回有效的检查点 ID

---

### 2.3 列出线程检查点

**测试用例名称**：`checkpoint_list_thread_success`

**测试场景**：列出指定线程的所有检查点

**测试步骤**：
1. 为一个线程创建多个检查点
2. 执行 CLI 命令：`modular-agent checkpoint list <thread-id>`
3. 验证列表输出

**预期 stdout 内容**：
```
线程 thread-abc123 的检查点列表

ID                  名称              创建时间
-----------------------------------------------------------
checkpoint-xyz789   test-checkpoint   2024-01-01T12:00:00.000Z
checkpoint-abc123   another-checkpoint 2024-01-01T12:01:00.000Z
checkpoint-def456   final-checkpoint  2024-01-01T12:02:00.000Z
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- 显示所有检查点
- 列表格式正确

---

### 2.4 列出 Agent Loop 检查点

**测试用例名称**：`checkpoint_list_agent_loop_success`

**测试场景**：列出指定 Agent Loop 的所有检查点

**测试步骤**：
1. 为一个 Agent Loop 创建多个检查点
2. 执行 CLI 命令：`modular-agent agent list-checkpoints <agent-id>`
3. 验证列表输出

**预期 stdout 内容**：
```
Agent Loop agent-loop-abc123 的检查点列表

ID                  名称              创建时间
-----------------------------------------------------------
checkpoint-agent-001 agent-checkpoint  2024-01-01T12:00:00.000Z
checkpoint-agent-002 mid-checkpoint    2024-01-01T12:01:00.000Z
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- 显示所有检查点
- 列表格式正确

---

### 2.5 查看检查点详情

**测试用例名称**：`checkpoint_show_details_success`

**测试场景**：查看检查点的详细信息

**测试步骤**：
1. 创建一个检查点
2. 执行 CLI 命令：`modular-agent checkpoint show <checkpoint-id>`
3. 验证详情显示

**预期 stdout 内容**：
```
检查点详情
----------
ID: checkpoint-xyz789
类型: thread
资源ID: thread-abc123
名称: test-checkpoint
创建时间: 2024-01-01T12:00:00.000Z
大小: 1024 bytes

状态快照:
  当前节点: process
  执行进度: 2/3
  变量: {"input": "test", "output": "processing"}

元数据:
  版本: 1.0
  描述: Test checkpoint
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- 显示完整的检查点信息
- 状态快照正确

---

### 2.6 从检查点恢复线程

**测试用例名称**：`checkpoint_restore_thread_success`

**测试场景**：从检查点恢复线程

**测试步骤**：
1. 创建一个线程检查点
2. 执行 CLI 命令：`modular-agent checkpoint restore-thread <checkpoint-id>`
3. 验证恢复成功

**预期 stdout 内容**：
```
正在从检查点恢复线程: checkpoint-xyz789
线程已恢复: thread-new-456

恢复的线程信息
--------------
ID: thread-new-456
原线程ID: thread-abc123
检查点ID: checkpoint-xyz789
恢复时间: 2024-01-01T12:05:00.000Z
当前节点: process
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 新线程已创建
- 状态与检查点一致
- 可以继续执行

**验证点**：
- 退出码为 0
- stdout 包含 "线程已恢复"
- 返回新的线程 ID
- 状态与检查点一致

---

### 2.7 从检查点恢复 Agent Loop

**测试用例名称**：`checkpoint_restore_agent_loop_success`

**测试场景**：从检查点恢复 Agent Loop

**测试步骤**：
1. 创建一个 Agent Loop 检查点
2. 执行 CLI 命令：`modular-agent agent restore <checkpoint-id>`
3. 验证恢复成功

**预期 stdout 内容**：
```
正在从检查点恢复: checkpoint-agent-001
Agent Loop 已从检查点恢复: agent-loop-new-789

恢复的 Agent Loop 信息
----------------------
ID: agent-loop-new-789
原 Agent Loop ID: agent-loop-abc123
检查点ID: checkpoint-agent-001
恢复时间: 2024-01-01T12:05:00.000Z
当前迭代: 3
消息数: 5
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 新 Agent Loop 已创建
- 状态与检查点一致
- 可以继续执行

**验证点**：
- 退出码为 0
- stdout 包含 "Agent Loop 已从检查点恢复"
- 返回新的 Agent Loop ID
- 状态与检查点一致

---

### 2.8 删除检查点

**测试用例名称**：`checkpoint_delete_success`

**测试场景**：删除指定的检查点

**测试步骤**：
1. 创建一个检查点
2. 执行 CLI 命令：`modular-agent checkpoint delete <checkpoint-id> --force`
3. 验证删除成功

**预期 stdout 内容**：
```
检查点已删除: checkpoint-xyz789
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 检查点已从存储中移除
- 无法再查询到该检查点

**验证点**：
- 退出码为 0
- stdout 包含 "检查点已删除"
- 检查点确实被删除

---

### 2.9 批量删除检查点

**测试用例名称**：`checkpoint_delete_batch_success`

**测试场景**：批量删除指定资源的所有检查点

**测试步骤**：
1. 为一个资源创建多个检查点
2. 执行 CLI 命令：`modular-agent checkpoint delete-all <thread-id> --force`
3. 验证批量删除成功

**预期 stdout 内容**：
```
正在删除线程 thread-abc123 的所有检查点...
已删除 3 个检查点
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 所有检查点已被删除
- list 命令不再显示检查点

**验证点**：
- 退出码为 0
- stdout 显示删除的数量
- 所有检查点确实被删除

---

### 2.10 检查点不存在

**测试用例名称**：`checkpoint_not_found`

**测试场景**：尝试操作不存在的检查点

**测试步骤**：
1. 执行 CLI 命令：`modular-agent checkpoint show nonexistent-checkpoint-id`
2. 验证错误处理

**预期 stdout 内容**：
```
（空或包含错误摘要）
```

**预期 stderr 内容**：
```
错误：检查点不存在: nonexistent-checkpoint-id
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码非 0
- stderr 包含明确的错误信息

---

### 2.11 检查点完整性验证

**测试用例名称**：`checkpoint_integrity_validation_success`

**测试场景**：验证检查点的完整性

**测试步骤**：
1. 创建一个检查点
2. 执行 CLI 命令：`modular-agent checkpoint validate <checkpoint-id>`
3. 验证完整性检查

**预期 stdout 内容**：
```
正在验证检查点完整性: checkpoint-xyz789
检查点完整性验证通过

验证结果
--------
状态: valid
大小: 1024 bytes
校验和: abc123def456
验证时间: 2024-01-01T12:00:00.000Z
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- stdout 显示验证通过

---

### 2.12 检查点完整性验证失败

**测试用例名称**：`checkpoint_integrity_validation_failure`

**测试场景**：验证损坏的检查点

**测试步骤**：
1. 创建一个检查点
2. 模拟检查点损坏
3. 执行 CLI 命令：`modular-agent checkpoint validate <checkpoint-id>`
4. 验证错误检测

**预期 stdout 内容**：
```
正在验证检查点完整性: checkpoint-xyz789
检查点完整性验证失败

验证结果
--------
状态: invalid
错误: 校验和不匹配
损坏的数据块: 2
```

**预期 stderr 内容**：
```
（可能包含错误详情）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码非 0
- stdout 显示验证失败
- 错误信息明确

---

### 2.13 检查点导出

**测试用例名称**：`checkpoint_export_success`

**测试场景**：导出检查点到文件

**测试步骤**：
1. 创建一个检查点
2. 执行 CLI 命令：`modular-agent checkpoint export <checkpoint-id> --output /path/to/checkpoint.json`
3. 验证导出成功

**预期 stdout 内容**：
```
正在导出检查点: checkpoint-xyz789
检查点已导出到: /path/to/checkpoint.json

导出信息
--------
检查点ID: checkpoint-xyz789
导出格式: JSON
文件大小: 2048 bytes
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 文件已创建
- 包含完整的检查点数据

**验证点**：
- 退出码为 0
- stdout 包含导出路径
- 文件确实被创建

---

### 2.14 检查点导入

**测试用例名称**：`checkpoint_import_success`

**测试场景**：从文件导入检查点

**测试步骤**：
1. 准备一个检查点文件
2. 执行 CLI 命令：`modular-agent checkpoint import /path/to/checkpoint.json`
3. 验证导入成功

**预期 stdout 内容**：
```
正在导入检查点: /path/to/checkpoint.json
检查点已导入: checkpoint-imported-001

导入信息
--------
原检查点ID: checkpoint-xyz789
新检查点ID: checkpoint-imported-001
导入时间: 2024-01-01T12:00:00.000Z
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 检查点已导入
- 可以被查询和恢复

**验证点**：
- 退出码为 0
- stdout 包含新检查点 ID
- 检查点数据正确

---

## 3. 测试数据准备

### 3.1 测试工作流配置

```toml
# fixtures/workflows/checkpoint-workflow.toml
[workflow]
name = "checkpoint-workflow"
description = "用于检查点测试的工作流"

[[workflow.nodes]]
id = "start"
type = "start"

[[workflow.nodes]]
id = "process1"
type = "node"
function = "node:processor1"

[[workflow.nodes]]
id = "process2"
type = "node"
function = "node:processor2"

[[workflow.nodes]]
id = "end"
type = "end"

[[workflow.edges]]
from = "start"
to = "process1"

[[workflow.edges]]
from = "process1"
to = "process2"

[[workflow.edges]]
from = "process2"
to = "end"
```

## 4. 测试执行顺序建议

1. 先测试基础创建和查询功能
2. 再测试恢复功能
3. 然后测试高级功能（导出、导入、验证）
4. 最后测试错误处理

## 5. 注意事项

1. 测试恢复功能时，验证状态完整性
2. 测试删除功能时，确保资源被正确清理
3. 测试导出和导入时，验证数据完整性
4. 注意检查点存储的位置和清理
5. 测试完整性验证时，模拟各种损坏场景