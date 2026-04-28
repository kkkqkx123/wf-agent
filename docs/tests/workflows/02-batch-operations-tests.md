# Workflow 批量操作测试用例

## 测试目标

验证 WorkflowRegistry.batchRegister() 的正确性，包括批量注册、递归加载和文件过滤功能。

## SDK 核心组件

- **WorkflowRegistry** (sdk/graph/services/workflow-registry.ts)
- **ConfigManager** (apps/cli-app/src/config/config-manager.ts)

---

## 2.1 批量注册 - 全部成功

**测试用例名称**: `workflow_batch_register_all_success`

**测试目标**: 验证批量注册的正确性

**测试步骤**:
1. 准备一个包含多个有效工作流配置文件的目录
2. 执行 CLI 命令: `modular-agent workflow register-batch /path/to/workflows`
3. 验证所有工作流正确注册

**预期 stdout 内容**:
```
正在批量注册工作流: /path/to/workflows

扫描到 3 个工作流文件

正在注册: workflow1.toml
工作流已注册: wf-001

正在注册: workflow2.toml
工作流已注册: wf-002

正在注册: workflow3.toml
工作流已注册: wf-003

批量注册完成: 成功 3 个, 失败 0 个
```

**预期 stderr 内容**:
```
(空)
```

**SDK 验证点**:
- 所有工作流正确注册
- 返回成功列表
- 批量操作原子性(可选)

**验证点**:
- 退出码为 0
- 所有工作流可以通过 list 命令查询到
- stdout 显示注册数量

---

## 2.2 批量注册 - 部分失败

**测试用例名称**: `workflow_batch_register_partial_failure`

**测试目标**: 验证批量注册的错误处理

**测试步骤**:
1. 准备一个目录,包含 3 个有效文件和 2 个无效文件
2. 执行 CLI 命令: `modular-agent workflow register-batch /path/to/workflows`
3. 验证部分失败的处理

**预期 stdout 内容**:
```
正在批量注册工作流: /path/to/workflows

扫描到 5 个工作流文件

正在注册: workflow1.toml
工作流已注册: wf-001

正在注册: workflow2.toml
工作流已注册: wf-002

正在注册: invalid1.toml
注册失败: 缺少必需字段: name

正在注册: workflow3.toml
工作流已注册: wf-003

正在注册: invalid2.toml
注册失败: TOML 解析错误

批量注册完成: 成功 3 个, 失败 2 个

失败的文件:
  - invalid1.toml: 缺少必需字段: name
  - invalid2.toml: TOML 解析错误
```

**预期 stderr 内容**:
```
(可能包含警告信息)
```

**SDK 验证点**:
- 有效工作流成功注册
- 无效工作流跳过
- 返回失败列表及原因
- 不影响其他工作流的注册

**验证点**:
- 退出码为 0(即使部分失败)
- 有效的工作流被成功注册
- stdout 列出失败的文件和原因

---

## 2.3 批量注册 - 递归加载

**测试用例名称**: `workflow_batch_register_recursive_success`

**测试目标**: 验证 ConfigManager 的递归加载

**测试步骤**:
1. 准备一个包含子目录的目录结构
2. 子目录中也有工作流文件
3. 执行 CLI 命令: `modular-agent workflow register-batch /path/to/workflows --recursive`
4. 验证递归注册成功

**目录结构**:
```
/path/to/workflows/
├── workflow1.toml
├── workflow2.toml
└── subdirectory/
    ├── workflow3.toml
    └── workflow4.toml
```

**预期 stdout 内容**:
```
正在批量注册工作流: /path/to/workflows (递归模式)

扫描到 4 个工作流文件

正在注册: workflow1.toml
工作流已注册: wf-001

正在注册: workflow2.toml
工作流已注册: wf-002

正在注册: subdirectory/workflow3.toml
工作流已注册: wf-003

正在注册: subdirectory/workflow4.toml
工作流已注册: wf-004

批量注册完成: 成功 4 个, 失败 0 个
```

**预期 stderr 内容**:
```
(空)
```

**SDK 验证点**:
- 子目录文件被正确加载
- 递归深度正确处理
- 文件路径正确解析

**验证点**:
- 退出码为 0
- 包括子目录中的所有工作流都被注册
- stdout 显示注册数量

---

## 2.4 批量注册 - 文件过滤

**测试用例名称**: `workflow_batch_register_with_pattern_success`

**测试目标**: 验证文件模式过滤功能

**测试步骤**:
1. 准备一个包含多种类型文件的目录
2. 执行 CLI 命令: `modular-agent workflow register-batch /path/to/workflows --pattern ".*-test\\.toml"`
3. 验证只有匹配模式的文件被注册

**目录内容**:
```
/path/to/workflows/
├── workflow1.toml
├── workflow-test.toml
├── workflow2-test.toml
└── workflow3.toml
```

**预期 stdout 内容**:
```
正在批量注册工作流: /path/to/workflows (匹配模式: .*-test\.toml)

扫描到 2 个匹配的工作流文件

正在注册: workflow-test.toml
工作流已注册: wf-test-001

正在注册: workflow2-test.toml
工作流已注册: wf-test-002

批量注册完成: 成功 2 个, 失败 0 个
```

**预期 stderr 内容**:
```
(空)
```

**SDK 验证点**:
- 只有匹配文件被加载
- 正则表达式正确匹配
- 不匹配的文件被跳过

**验证点**:
- 退出码为 0
- 只有文件名匹配模式的文件被注册
- 不匹配的文件未被注册
