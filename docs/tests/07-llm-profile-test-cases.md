# LLM Profile 模块测试用例设计

## 1. 模块概述

LLM Profile 模块负责大语言模型配置文件的管理，包括注册、列表、查看、更新、删除等操作。测试重点在于验证 Profile 的配置正确性和使用。

## 2. 测试用例列表

### 2.1 注册 LLM Profile

**测试用例名称**：`llm_profile_register_success`

**测试场景**：从配置文件注册 LLM Profile

**测试步骤**：
1. 准备一个有效的 TOML 配置文件
2. 执行 CLI 命令：`modular-agent llm-profile register <profile-file>`
3. 验证注册成功

**预期 stdout 内容**：
```
正在注册 LLM Profile: openai-gpt4
LLM Profile 已注册: openai-gpt4

Profile 信息
-----------
ID: openai-gpt4
提供商: openai
模型: gpt-4
版本: 1.0.0
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- Profile 已注册到 ProfileRegistry
- 可以被查询和使用

**验证点**：
- 退出码为 0
- stdout 包含 "LLM Profile 已注册"
- 返回有效的 Profile ID

---

### 2.2 列出 LLM Profile

**测试用例名称**：`llm_profile_list_success`

**测试场景**：列出所有已注册的 LLM Profile

**测试步骤**：
1. 预先注册多个 Profile
2. 执行 CLI 命令：`modular-agent llm-profile list`
3. 验证列表输出

**预期 stdout 内容**：
```
ID              提供商    模型        版本      默认
------------------------------------------------------------
openai-gpt4     openai    gpt-4       1.0.0     是
anthropic-claude anthropic claude-3    1.0.0     否
google-gemini   google    gemini-pro  1.0.0     否
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- 显示所有 Profile
- 列表格式正确

---

### 2.3 查看 LLM Profile 详情

**测试用例名称**：`llm_profile_show_details_success`

**测试场景**：查看指定 Profile 的详细信息

**测试步骤**：
1. 预先注册一个 Profile
2. 执行 CLI 命令：`modular-agent llm-profile show openai-gpt4`
3. 验证详情显示

**预期 stdout 内容**：
```
LLM Profile 详情
---------------
ID: openai-gpt4
名称: OpenAI GPT-4
提供商: openai
模型: gpt-4
版本: 1.0.0
默认: 是
创建时间: 2024-01-01T12:00:00.000Z

配置:
  API Key: sk-... (已隐藏)
  Base URL: https://api.openai.com/v1
  Temperature: 0.7
  Max Tokens: 4096
  Top P: 1.0

功能支持:
  流式输出: 是
  函数调用: 是
  多模态: 否
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- 显示完整的 Profile 信息
- API Key 已隐藏

---

### 2.4 更新 LLM Profile

**测试用例名称**：`llm_profile_update_success`

**测试场景**：更新已注册的 Profile 配置

**测试步骤**：
1. 预先注册一个 Profile
2. 准备更新配置文件
3. 执行 CLI 命令：`modular-agent llm-profile update openai-gpt4 --config update.toml`
4. 验证更新成功

**预期 stdout 内容**：
```
正在更新 LLM Profile: openai-gpt4
LLM Profile 已更新: openai-gpt4

更新内容
-------
Temperature: 0.7 -> 0.9
Max Tokens: 4096 -> 8192
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- Profile 配置已更新
- 新配置生效

**验证点**：
- 退出码为 0
- stdout 包含 "LLM Profile 已更新"
- 配置确实被更新

---

### 2.5 设置默认 Profile

**测试用例名称**：`llm_profile_set_default_success`

**测试场景**：设置指定的 Profile 为默认 Profile

**测试步骤**：
1. 预先注册多个 Profile
2. 执行 CLI 命令：`modular-agent llm-profile set-default anthropic-claude`
3. 验证默认设置

**预期 stdout 内容**：
```
正在设置默认 Profile: anthropic-claude
默认 Profile 已设置为: anthropic-claude

之前的默认 Profile: openai-gpt4
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- anthropic-claude 成为默认 Profile
- openai-gpt4 不再是默认 Profile

**验证点**：
- 退出码为 0
- stdout 包含设置确认
- list 命令显示正确的默认 Profile

---

### 2.6 删除 LLM Profile

**测试用例名称**：`llm_profile_delete_success`

**测试场景**：删除指定的 Profile

**测试步骤**：
1. 预先注册一个非默认 Profile
2. 执行 CLI 命令：`modular-agent llm-profile delete google-gemini --force`
3. 验证删除成功

**预期 stdout 内容**：
```
LLM Profile 已删除: google-gemini
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- Profile 已从 ProfileRegistry 中移除
- 无法再查询到该 Profile

**验证点**：
- 退出码为 0
- stdout 包含 "LLM Profile 已删除"
- Profile 确实被删除

---

### 2.7 删除默认 Profile（失败）

**测试用例名称**：`llm_profile_delete_default_failure`

**测试场景**：尝试删除默认 Profile

**测试步骤**：
1. 确认有一个默认 Profile
2. 执行 CLI 命令：`modular-agent llm-profile delete openai-gpt4 --force`
3. 验证错误处理

**预期 stdout 内容**：
```
（空或包含错误摘要）
```

**预期 stderr 内容**：
```
错误：无法删除默认 Profile
请先设置其他 Profile 为默认，然后再删除此 Profile
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码非 0
- stderr 包含明确的错误信息

---

### 2.8 Profile 不存在

**测试用例名称**：`llm_profile_not_found`

**测试场景**：尝试操作不存在的 Profile

**测试步骤**：
1. 执行 CLI 命令：`modular-agent llm-profile show nonexistent-profile`
2. 验证错误处理

**预期 stdout 内容**：
```
（空或包含错误摘要）
```

**预期 stderr 内容**：
```
错误：LLM Profile 不存在: nonexistent-profile
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码非 0
- stderr 包含明确的错误信息

---

### 2.9 测试 Profile 连接

**测试用例名称**：`llm_profile_test_connection_success`

**测试场景**：测试 Profile 的连接是否正常

**测试步骤**：
1. 预先注册一个有效的 Profile
2. 执行 CLI 命令：`modular-agent llm-profile test openai-gpt4`
3. 验证连接测试

**预期 stdout 内容**：
```
正在测试 Profile 连接: openai-gpt4
连接测试成功

测试结果
--------
连接状态: 成功
响应时间: 250ms
API 状态: 正常
模型可用: 是
```

**预期 stderr 内容**：
```
（空）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码为 0
- stdout 显示连接成功

---

### 2.10 测试 Profile 连接（失败）

**测试用例名称**：`llm_profile_test_connection_failure`

**测试场景**：测试无效 Profile 的连接

**测试步骤**：
1. 预先注册一个无效的 Profile（错误的 API Key）
2. 执行 CLI 命令：`modular-agent llm-profile test invalid-profile`
3. 验证错误检测

**预期 stdout 内容**：
```
正在测试 Profile 连接: invalid-profile
连接测试失败

测试结果
--------
连接状态: 失败
错误信息: Authentication failed: Invalid API key
```

**预期 stderr 内容**：
```
（可能包含错误详情）
```

**预期资源状态**：
- 无变化

**验证点**：
- 退出码非 0
- stdout 显示连接失败
- 错误信息明确

---

## 3. 测试数据准备

### 3.1 OpenAI Profile 配置

```toml
# fixtures/llm-profiles/openai-gpt4.toml
[profile]
id = "openai-gpt4"
name = "OpenAI GPT-4"
provider = "openai"
model = "gpt-4"
version = "1.0.0"
default = true

[profile.config]
api_key = "sk-test-key"
base_url = "https://api.openai.com/v1"
temperature = 0.7
max_tokens = 4096
top_p = 1.0

[profile.capabilities]
stream = true
function_calling = true
multimodal = false
```

### 3.2 Anthropic Profile 配置

```toml
# fixtures/llm-profiles/anthropic-claude.toml
[profile]
id = "anthropic-claude"
name = "Anthropic Claude-3"
provider = "anthropic"
model = "claude-3-opus"
version = "1.0.0"
default = false

[profile.config]
api_key = "sk-ant-test-key"
base_url = "https://api.anthropic.com/v1"
temperature = 0.7
max_tokens = 4096
top_p = 1.0

[profile.capabilities]
stream = true
function_calling = true
multimodal = false
```

## 4. 测试执行顺序建议

1. 先测试注册和查询功能
2. 再测试更新和默认设置
3. 然后测试连接测试
4. 最后测试删除和错误处理

## 5. 注意事项

1. 测试连接测试时，使用有效的 API Key 或 Mock
2. 测试删除功能时，验证不能删除默认 Profile
3. 测试更新功能时，验证配置确实被更新
4. 注意 API Key 的安全性，不应该在输出中明文显示
5. 测试多个 Provider 的 Profile 配置