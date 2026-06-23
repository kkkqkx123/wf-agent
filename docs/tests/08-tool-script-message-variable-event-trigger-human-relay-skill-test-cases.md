# Tool、Script、Message、Variable、Event、Trigger、Human Relay、Skill 模块测试用例设计

## 1. Tool 模块

### 1.1 注册工具

**测试用例名称**：`tool_register_success`

**测试场景**：从配置文件注册工具

**测试步骤**：
1. 准备一个有效的工具配置文件
2. 执行 CLI 命令：`modular-agent tool register <tool-file>`
3. 验证注册成功

**预期 stdout 内容**：
```
正在注册工具: get_file
工具已注册: get_file

工具信息
-------
ID: get_file
名称: 获取文件
类型: native
版本: 1.0.0
```

**预期 stderr 内容**：
```
（空）
```

**验证点**：
- 退出码为 0
- stdout 包含 "工具已注册"

---

### 1.2 列出工具

**测试用例名称**：`tool_list_success`

**测试场景**：列出所有已注册的工具

**预期 stdout 内容**：
```
ID          名称      类型      版本
-----------------------------------
get_file    获取文件  native    1.0.0
write_file  写入文件  native    1.0.0
read_file   读取文件  native    1.0.0
```

**验证点**：
- 退出码为 0
- 显示所有工具

---

### 1.3 执行工具

**测试用例名称**：`tool_execute_success`

**测试场景**：执行指定工具

**预期 stdout 内容**：
```
正在执行工具: get_file
工具执行成功

执行结果
--------
状态: success
返回值: { "content": "file content", "encoding": "utf-8" }
执行时长: 50ms
```

**验证点**：
- 退出码为 0
- 显示执行结果

---

## 2. Script 模块

### 2.1 注册脚本

**测试用例名称**：`script_register_success`

**测试场景**：从文件注册脚本

**预期 stdout 内容**：
```
正在注册脚本: calculate.js
脚本已注册: script-calc-001

脚本信息
-------
ID: script-calc-001
名称: 计算脚本
语言: JavaScript
版本: 1.0.0
```

**验证点**：
- 退出码为 0

---

### 2.2 执行脚本

**测试用例名称**：`script_execute_success`

**预期 stdout 内容**：
```
正在执行脚本: script-calc-001
脚本执行成功

执行结果
--------
状态: success
返回值: 42
执行时长: 100ms
```

**验证点**：
- 退出码为 0

---

### 2.3 批量测试脚本

**测试用例名称**：`script_test_batch_success`

**预期 stdout 内容**：
```
正在批量测试脚本...

测试结果
--------
总计: 3
通过: 2
失败: 1

通过:
  - calculate.js
  - process.js

失败:
  - error.js: ReferenceError: x is not defined
```

**验证点**：
- 退出码可能非 0（如果有失败）

---

## 3. Message 模块

### 3.1 发送消息

**测试用例名称**：`message_send_success`

**预期 stdout 内容**：
```
正在发送消息到线程: thread-abc123
消息已发送

消息信息
-------
ID: msg-xyz789
线程ID: thread-abc123
角色: user
内容: Hello, world!
时间: 2024-01-01T12:00:00.000Z
```

**验证点**：
- 退出码为 0

---

### 3.2 列出消息

**测试用例名称**：`message_list_success`

**预期 stdout 内容**：
```
线程 thread-abc123 的消息列表

ID              角色      内容                    时间
------------------------------------------------------------
msg-xyz789      user      Hello, world!           2024-01-01T12:00:00.000Z
msg-abc123      assistant Hi there!               2024-01-01T12:00:01.000Z
```

**验证点**：
- 退出码为 0

---

### 3.3 导出消息

**测试用例名称**：`message_export_success`

**预期 stdout 内容**：
```
正在导出消息: thread-abc123
消息已导出到: /path/to/messages.json

导出信息
--------
消息数量: 5
文件大小: 2048 bytes
```

**验证点**：
- 退出码为 0

---

## 4. Variable 模块

### 4.1 设置变量

**测试用例名称**：`variable_set_success`

**预期 stdout 内容**：
```
正在设置变量: thread-abc123
变量已设置: user_name

变量信息
-------
名称: user_name
值: John Doe
类型: string
```

**验证点**：
- 退出码为 0

---

### 4.2 获取变量

**测试用例名称**：`variable_get_success`

**预期 stdout 内容**：
```
变量信息
-------
名称: user_name
值: John Doe
类型: string
创建时间: 2024-01-01T12:00:00.000Z
更新时间: 2024-01-01T12:00:00.000Z
```

**验证点**：
- 退出码为 0

---

### 4.3 列出变量

**测试用例名称**：`variable_list_success`

**预期 stdout 内容**：
```
线程 thread-abc123 的变量列表

名称          值              类型
---------------------------------------
user_name     John Doe        string
user_age      30              number
is_active     true            boolean
```

**验证点**：
- 退出码为 0

---

## 5. Event 模块

### 5.1 分发事件

**测试用例名称**：`event_dispatch_success`

**预期 stdout 内容**：
```
正在分发事件: custom-event
事件已分发

事件信息
-------
类型: custom-event
数据: { "key": "value" }
时间: 2024-01-01T12:00:00.000Z
订阅者数量: 2
```

**验证点**：
- 退出码为 0

---

### 5.2 查询事件历史

**测试用例名称**：`event_history_success`

**预期 stdout 内容**：
```
事件历史记录

类型              数据                    时间
------------------------------------------------------------
custom-event      {"key":"value"}        2024-01-01T12:00:00.000Z
node-executed     {"nodeId":"process"}   2024-01-01T12:00:01.000Z
```

**验证点**：
- 退出码为 0

---

### 5.3 查看事件统计

**测试用例名称**：`event_stats_success`

**预期 stdout 内容**：
```
事件统计
--------
总事件数: 100
按类型:
  custom-event: 50
  node-executed: 30
  thread-completed: 20

今日事件数: 25
平均每秒事件数: 0.5
```

**验证点**：
- 退出码为 0

---

## 6. Trigger 模块

### 6.1 注册触发器

**测试用例名称**：`trigger_register_success`

**预期 stdout 内容**：
```
正在注册触发器: daily-report
触发器已注册: trigger-daily-001

触发器信息
---------
ID: trigger-daily-001
名称: 每日报表触发器
类型: schedule
状态: enabled
```

**验证点**：
- 退出码为 0

---

### 6.2 启用触发器

**测试用例名称**：`trigger_enable_success`

**预期 stdout 内容**：
```
正在启用触发器: trigger-daily-001
触发器已启用

触发器信息
---------
ID: trigger-daily-001
状态: enabled
启用时间: 2024-01-01T12:00:00.000Z
```

**验证点**：
- 退出码为 0

---

### 6.3 禁用触发器

**测试用例名称**：`trigger_disable_success`

**预期 stdout 内容**：
```
正在禁用触发器: trigger-daily-001
触发器已禁用

触发器信息
---------
ID: trigger-daily-001
状态: disabled
禁用时间: 2024-01-01T12:00:00.000Z
```

**验证点**：
- 退出码为 0

---

## 7. Human Relay 模块

### 7.1 查询待处理请求

**测试用例名称**：`human_relay_pending_success`

**预期 stdout 内容**：
```
待处理的人工中继请求

ID              类型              创建时间
------------------------------------------------------------
relay-abc123    approval          2024-01-01T12:00:00.000Z
relay-def456    input             2024-01-01T12:01:00.000Z
```

**验证点**：
- 退出码为 0

---

### 7.2 处理请求（批准）

**测试用例名称**：`human_relay_approve_success`

**预期 stdout 内容**：
```
正在处理请求: relay-abc123
请求已批准

处理结果
--------
请求ID: relay-abc123
操作: approve
响应: { "approved": true, "comment": "Good to go" }
处理时间: 2024-01-01T12:05:00.000Z
```

**验证点**：
- 退出码为 0

---

### 7.3 处理请求（拒绝）

**测试用例名称**：`human_relay_reject_success`

**预期 stdout 内容**：
```
正在处理请求: relay-abc123
请求已拒绝

处理结果
--------
请求ID: relay-abc123
操作: reject
原因: Insufficient information
处理时间: 2024-01-01T12:05:00.000Z
```

**验证点**：
- 退出码为 0

---

## 8. Skill 模块

### 8.1 注册 Skill

**测试用例名称**：`skill_register_success`

**预期 stdout 内容**：
```
正在注册 Skill: data-analysis
Skill 已注册: skill-data-001

Skill 信息
---------
ID: skill-data-001
名称: 数据分析 Skill
类型: analysis
版本: 1.0.0
```

**验证点**：
- 退出码为 0

---

### 8.2 列出 Skill

**测试用例名称**：`skill_list_success`

**预期 stdout 内容**：
```
ID              名称              类型      版本
--------------------------------------------------
skill-data-001  数据分析 Skill    analysis  1.0.0
skill-code-002  代码生成 Skill    code      1.0.0
```

**验证点**：
- 退出码为 0

---

### 8.3 执行 Skill

**测试用例名称**：`skill_execute_success`

**预期 stdout 内容**：
```
正在执行 Skill: skill-data-001
Skill 执行成功

执行结果
--------
状态: success
输出: { "result": "analysis complete", "data": [...] }
执行时长: 2500ms
```

**验证点**：
- 退出码为 0

---

## 9. 通用错误处理

### 9.1 资源不存在

**预期 stderr 内容**：
```
错误：资源不存在: <resource-id>
```

**验证点**：
- 退出码非 0

---

### 9.2 参数验证失败

**预期 stderr 内容**：
```
错误：参数验证失败
详情：缺少必需参数: <param-name>
```

**验证点**：
- 退出码非 0

---

## 10. 测试数据准备

### 10.1 工具配置

```toml
# fixtures/tools/get-file.toml
[tool]
id = "get_file"
name = "获取文件"
type = "native"
version = "1.0.0"

[tool.parameters]
type = "object"
properties = { path = { type = "string" } }
required = ["path"]
```

### 10.2 脚本文件

```javascript
// fixtures/scripts/calculate.js
module.exports = async function(input) {
  return input.a + input.b;
};
```

### 10.3 触发器配置

```toml
# fixtures/triggers/daily-report.toml
[trigger]
id = "daily-report"
name = "每日报表触发器"
type = "schedule"
enabled = true

[trigger.schedule]
cron = "0 0 * * *"
timezone = "Asia/Shanghai"

[trigger.action]
type = "execute_workflow"
workflow_id = "report-workflow"
```

## 11. 测试执行顺序建议

1. 先测试注册和查询功能
2. 再测试执行功能
3. 然后测试更新和删除
4. 最后测试错误处理

## 12. 注意事项

1. 测试工具执行时，确保工具环境正确
2. 测试脚本执行时，注意脚本语言的依赖
3. 测试事件功能时，验证事件订阅和分发
4. 测试触发器功能时，注意定时触发和条件触发
5. 测试 Human Relay 时，模拟人工交互流程
6. 测试 Skill 时，验证 Skill 的输入输出
7. 注意清理测试创建的资源