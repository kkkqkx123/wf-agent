# 配置模块使用指南

## 概述

配置模块提供了从外部配置文件（TOML/JSON）加载和解析多种配置类型的功能，包括：

- **工作流定义**（WorkflowDefinition）
- **节点模板**（NodeTemplate）
- **触发器模板**（TriggerTemplate）
- **脚本**（Script）

该模块支持参数化配置、配置验证和自动转换为对应的类型定义。

## 设计原则

- **配置验证**：直接使用 `sdk/core/validation` 中的 `WorkflowValidator`，不实现新的验证逻辑
- **职责分离**：`api/config` 层只负责配置文件的解析和转换，验证逻辑由 `core/validation` 提供
- **类型统一**：验证结果使用 `Result<T, ValidationError[]>` 类型，与核心模块保持一致
- **统一管理**：通过 `ConfigManager` 提供统一的配置管理接口

## 核心组件

### 1. 纯函数式解析接口（推荐使用）

提供无状态的纯函数式配置解析接口：

- **parseWorkflow()**：解析工作流配置
- **parseNodeTemplate()**：解析节点模板配置
- **parseTriggerTemplate()**：解析触发器模板配置
- **parseScript()**：解析脚本配置
- **parseBatchWorkflows()**：批量解析工作流配置
- **parseBatchNodeTemplates()**：批量解析节点模板配置
- **parseBatchTriggerTemplates()**：批量解析触发器模板配置
- **parseBatchScripts()**：批量解析脚本配置

### 2. 配置验证函数

提供单个和批量配置验证功能：

- **validateWorkflowConfig()**：验证单个工作流配置
- **validateNodeTemplateConfig()**：验证单个节点模板配置
- **validateTriggerTemplateConfig()**：验证单个触发器模板配置
- **validateScriptConfig()**：验证单个脚本配置
- **validateBatchWorkflows()**：批量验证工作流配置
- **validateBatchNodeTemplates()**：批量验证节点模板配置
- **validateBatchTriggerTemplates()**：批量验证触发器模板配置
- **validateBatchScripts()**：批量验证脚本配置

### 4. ConfigParser

主配置解析器，整合了TOML/JSON解析、验证和转换功能。

### 5. TomlParser

TOML格式解析器（需要安装 `@iarna/toml` 或 `toml` 包）。

### 6. JsonParser

JSON格式解析器。

### 7. ConfigTransformer

配置转换器，将配置文件格式转换为WorkflowDefinition，处理参数替换和边引用更新。

### 8. WorkflowValidator

工作流验证器（来自 `sdk/core/validation`），负责验证工作流定义的有效性。

## 使用方式

### 方式1：通过WorkflowBuilder加载配置

```typescript
import { WorkflowBuilder } from "@modular-agent/sdk";

// 从配置文件内容加载
const builder = WorkflowBuilder.fromConfig(
  configContent,
  "json", // 或 'toml'
  { model: "gpt-4" }, // 可选的运行时参数
);

// 从配置文件路径加载
const builder = await WorkflowBuilder.fromConfigFile("./workflows/chat-workflow.json", {
  model: "gpt-4",
});

// 构建工作流定义
const workflowDef = builder.build();
```

### 方式2：通过ConfigurationAPI加载配置

```typescript
import { ConfigurationAPI } from "@modular-agent/sdk";

const configAPI = new ConfigurationAPI();

// 加载并注册工作流
const workflowId = await configAPI.loadAndRegisterWorkflow("./workflows/chat-workflow.json", {
  model: "gpt-4",
});

// 仅加载工作流定义（不注册）
const workflowDef = await configAPI.loadWorkflowDefinition("./workflows/chat-workflow.json", {
  model: "gpt-4",
});

// 验证配置文件
const validationResult = await configAPI.validateConfigFile("./workflows/chat-workflow.json");

if (validationResult.isErr()) {
  console.error("配置验证失败:", validationResult.error);
}
```

### 方式3：直接使用ConfigParser

```typescript
import { ConfigParser, ConfigFormat } from "@modular-agent/sdk";

const parser = new ConfigParser();

// 解析并转换配置
const workflowDef = parser.parseAndTransform(configContent, ConfigFormat.JSON, { model: "gpt-4" });

// 从文件加载
const workflowDef = await parser.loadAndTransform("./workflows/chat-workflow.json", {
  model: "gpt-4",
});

// 导出工作流为配置文件
await parser.saveWorkflow(workflowDef, "./workflows/exported.json");
```

### 方式4：使用ConfigManager统一管理配置

```typescript
import { configManager } from "@modular-agent/sdk";

// 加载工作流配置
const workflow = await configManager.workflows.loadAndTransform("./workflows/chat-workflow.json", {
  model: "gpt-4",
});

// 加载并注册节点模板
const nodeTemplate = await configManager.nodeTemplates.loadAndRegister(
  "./templates/node-templates/llm-node.json",
);

// 加载并注册触发器模板
const triggerTemplate = await configManager.triggerTemplates.loadAndRegister(
  "./templates/trigger-templates/error-alert.json",
);

// 加载并注册脚本
const script = await configManager.scripts.loadAndRegister("./scripts/data-fetch.json");

// 从目录批量加载所有配置
const configs = await configManager.loadFromDirectory("./configs", {
  workflows: true,
  nodeTemplates: true,
  triggerTemplates: true,
  scripts: true,
});

console.log("加载的工作流:", configs.workflows);
console.log("加载的节点模板:", configs.nodeTemplates);
console.log("加载的触发器模板:", configs.triggerTemplates);
console.log("加载的脚本:", configs.scripts);

// 导出配置到目录
await configManager.exportToDirectory("./exported", {
  workflows: [workflow],
  nodeTemplates: [nodeTemplate],
  triggerTemplates: [triggerTemplate],
  scripts: [script],
});

// 获取配置摘要
const summary = configManager.getSummary();
console.log("配置摘要:", summary);
```

## 配置文件格式

### JSON格式示例

```json
{
  "id": "example-chat",
  "name": "示例聊天工作流",
  "description": "一个简单的聊天工作流示例",
  "version": "1.0.0",
  "createdAt": 0,
  "updatedAt": 0,
  "nodes": [
    {
      "id": "start",
      "type": "START",
      "name": "开始",
      "config": {},
      "outgoingEdgeIds": [],
      "incomingEdgeIds": []
    },
    {
      "id": "chat_llm",
      "type": "LLM",
      "name": "聊天LLM",
      "config": {
        "profileId": "{{parameters.model}}",
        "prompt": "请回答用户的问题：{{input.message}}"
      },
      "outgoingEdgeIds": [],
      "incomingEdgeIds": []
    },
    {
      "id": "end",
      "type": "END",
      "name": "结束",
      "config": {},
      "outgoingEdgeIds": [],
      "incomingEdgeIds": []
    }
  ],
  "edges": [
    {
      "id": "edge-1",
      "sourceNodeId": "start",
      "targetNodeId": "chat_llm",
      "type": "DEFAULT"
    },
    {
      "id": "edge-2",
      "sourceNodeId": "chat_llm",
      "targetNodeId": "end",
      "type": "DEFAULT"
    }
  ]
}
```

### TOML格式示例

```toml
id = "example-chat"
name = "示例聊天工作流"
description = "一个简单的聊天工作流示例"
version = "1.0.0"
createdAt = 0
updatedAt = 0

[[nodes]]
id = "start"
type = "START"
name = "开始"
config = {}
outgoingEdgeIds = []
incomingEdgeIds = []

[[nodes]]
id = "chat_llm"
type = "LLM"
name = "聊天LLM"

[nodes.config]
profileId = "{{parameters.model}}"
prompt = "请回答用户的问题：{{input.message}}"

outgoingEdgeIds = []
incomingEdgeIds = []

[[nodes]]
id = "end"
type = "END"
name = "结束"
config = {}
outgoingEdgeIds = []
incomingEdgeIds = []

[[edges]]
id = "edge-1"
sourceNodeId = "start"
targetNodeId = "chat_llm"
type = "DEFAULT"

[[edges]]
id = "edge-2"
sourceNodeId = "chat_llm"
targetNodeId = "end"
type = "DEFAULT"
```

### 节点模板配置文件示例

```json
{
  "name": "gpt4-llm",
  "type": "LLM",
  "description": "GPT-4 LLM节点模板",
  "config": {
    "profileId": "gpt-4",
    "prompt": "请回答用户的问题：{{input.message}}"
  },
  "metadata": {
    "category": "llm",
    "tags": ["gpt-4", "chat"],
    "author": "system"
  },
  "createdAt": 0,
  "updatedAt": 0
}
```

### 触发器模板配置文件示例

```json
{
  "name": "error-alert",
  "description": "错误告警触发器",
  "condition": {
    "eventType": "NODE_FAILED",
    "nodeId": "*"
  },
  "action": {
    "type": "SEND_NOTIFICATION",
    "config": {
      "message": "节点执行失败：{{nodeId}}",
      "channel": "email"
    }
  },
  "enabled": true,
  "maxTriggers": 0,
  "metadata": {
    "category": "alert",
    "tags": ["error", "notification"]
  },
  "createdAt": 0,
  "updatedAt": 0
}
```

### 脚本配置文件示例

```json
{
  "id": "data-fetch-script",
  "name": "data-fetch",
  "type": "PYTHON",
  "description": "从API获取数据",
  "content": "import requests\n\ndef fetch_data(url):\n    response = requests.get(url)\n    return response.json()\n\nresult = fetch_data('{{parameters.url}}')\nprint(result)",
  "options": {
    "timeout": 30000,
    "retries": 3,
    "retryDelay": 1000,
    "environment": {
      "PYTHONPATH": "/usr/local/lib/python3.9/site-packages"
    }
  },
  "metadata": {
    "category": "data",
    "tags": ["api", "fetch"],
    "author": "system"
  },
  "enabled": true
}
```

## 参数化配置

配置文件支持参数化，使用 `{{parameters.xxx}}` 语法引用参数：

```json
{
  "id": "example",
  "name": "示例工作流",
  "version": "1.0.0",
  "createdAt": 0,
  "updatedAt": 0,
  "nodes": [
    {
      "id": "llm",
      "type": "LLM",
      "name": "LLM节点",
      "config": {
        "profileId": "{{parameters.model}}"
      },
      "outgoingEdgeIds": [],
      "incomingEdgeIds": []
    }
  ],
  "edges": []
}
```

在加载时提供参数值：

```typescript
const workflowDef = parser.parseAndTransform(
  configContent,
  ConfigFormat.JSON,
  { model: "gpt-4-turbo" }, // 替换占位符
);
```

## 配置验证

配置验证使用 `WorkflowValidator`（来自 `sdk/core/validation`），会检查以下内容：

1. **基本信息验证**：
   - id、name、version 等必需字段
   - createdAt、updatedAt 时间戳

2. **节点验证**：
   - 必须包含一个START节点（普通工作流）
   - 必须包含至少一个END节点（普通工作流）
   - 触发子工作流必须包含START_FROM_TRIGGER和CONTINUE_FROM_TRIGGER节点
   - 节点ID不能重复
   - 节点类型必须有效
   - 节点配置根据类型进行验证

3. **边验证**：
   - 边ID不能重复
   - 边的源节点和目标节点必须存在
   - 边类型必须有效

4. **引用完整性验证**：
   - 边引用的节点必须存在
   - 节点的边引用必须一致

5. **配置验证**：
   - 工作流配置（timeout、maxSteps、retryPolicy等）

6. **触发器验证**：
   - 触发器配置的有效性

7. **自引用验证**：
   - 检测SUBGRAPH和START_FROM_TRIGGER节点的自引用问题

8. **工具配置验证**：
   - availableTools配置
   - LLM节点的dynamicTools配置

验证结果使用 `Result<WorkflowDefinition, ValidationError[]>` 类型：

```typescript
const validationResult = parser.validate(parsedConfig);

if (validationResult.isOk()) {
  console.log("验证通过", validationResult.value);
} else {
  console.error("验证失败:", validationResult.error);
  validationResult.error.forEach(err => {
    console.error(`  - ${err.message} (${err.field})`);
  });
}
```

## 高级功能

### 批量加载工作流

```typescript
const workflowIds = await configAPI.batchLoadAndRegisterWorkflows([
  "./workflows/workflow1.json",
  "./workflows/workflow2.json",
  "./workflows/workflow3.json",
]);
```

### 批量加载节点模板

```typescript
const nodeTemplates = await configManager.nodeTemplates.loadBatchAndRegister([
  "./templates/node-templates/llm-node.json",
  "./templates/node-templates/code-node.json",
  "./templates/node-templates/variable-node.json",
]);
```

### 批量加载触发器模板

```typescript
const triggerTemplates = await configManager.triggerTemplates.loadBatchAndRegister([
  "./templates/trigger-templates/error-alert.json",
  "./templates/trigger-templates/completion-notification.json",
]);
```

### 批量加载脚本

```typescript
const scripts = await configManager.scripts.loadBatchAndRegister([
  "./scripts/data-fetch.json",
  "./scripts/data-process.json",
  "./scripts/data-save.json",
]);
```

### 批量验证配置

```typescript
import {
  validateBatchWorkflows,
  validateBatchNodeTemplates,
  validateBatchTriggerTemplates,
  validateBatchScripts,
} from "@modular-agent/sdk/api/config";

// 批量验证工作流配置
const workflowConfigs = [workflowConfig1, workflowConfig2, workflowConfig3];
const validationResult = validateBatchWorkflows(workflowConfigs);

if (validationResult.isOk()) {
  console.log("所有工作流配置验证通过", validationResult.value);
} else {
  console.error("部分工作流配置验证失败:", validationResult.error);
  validationResult.error.forEach((errors, index) => {
    console.error(`配置 ${index + 1} 错误:`, errors);
  });
}

// 批量验证节点模板配置
const nodeTemplateConfigs = [nodeTemplateConfig1, nodeTemplateConfig2];
const nodeTemplateResult = validateBatchNodeTemplates(nodeTemplateConfigs);

// 批量验证触发器模板配置
const triggerTemplateConfigs = [triggerTemplateConfig1, triggerTemplateConfig2];
const triggerTemplateResult = validateBatchTriggerTemplates(triggerTemplateConfigs);

// 批量验证脚本配置
const scriptConfigs = [scriptConfig1, scriptConfig2, scriptConfig3];
const scriptResult = validateBatchScripts(scriptConfigs);
```

### 从目录批量加载所有配置

```typescript
// 加载所有类型的配置
const configs = await configManager.loadFromDirectory("./configs");

// 只加载特定类型的配置
const configs = await configManager.loadFromDirectory("./configs", {
  workflows: true,
  nodeTemplates: false,
  triggerTemplates: true,
  scripts: false,
});

// 使用文件名模式过滤
const configs = await configManager.loadFromDirectory("./configs", {
  filePattern: /^prod-.*\.json$/,
});
```

### 导出工作流为配置文件

```typescript
// 导出到文件
await configAPI.exportWorkflowToConfig("workflow-id", "./exported.json");

// 导出为内容字符串
const content = configAPI.exportWorkflowToConfigContent("workflow-id", ConfigFormat.JSON);

// 使用ConfigManager导出
await configManager.workflows.exportToFile(workflowDef, "./exported/workflow.json");
```

### 导出节点模板为配置文件

```typescript
await configManager.nodeTemplates.exportToFile(nodeTemplate, "./exported/node-template.json");
```

### 导出触发器模板为配置文件

```typescript
await configManager.triggerTemplates.exportToFile(
  triggerTemplate,
  "./exported/trigger-template.json",
);
```

### 导出脚本为配置文件

```typescript
await configManager.scripts.exportToFile(script, "./exported/script.json");
```

### 批量导出配置到目录

```typescript
await configManager.exportToDirectory("./exported", {
  workflows: [workflow1, workflow2],
  nodeTemplates: [nodeTemplate1, nodeTemplate2],
  triggerTemplates: [triggerTemplate1],
  scripts: [script1, script2],
});
```

### 搜索配置

```typescript
// 搜索节点模板
const nodeTemplates = configManager.nodeTemplates.searchTemplates("llm");

// 搜索触发器模板
const triggerTemplates = configManager.triggerTemplates.searchTemplates("error");

// 搜索脚本
const scripts = configManager.scripts.searchScripts("fetch");
```

### 获取配置摘要

```typescript
const summary = configManager.getSummary();
console.log("配置摘要:", summary);
// 输出:
// {
//   workflows: { count: 0, parser: 'parseWorkflow' },
//   nodeTemplates: { count: 5, parser: 'parseNodeTemplate' },
//   triggerTemplates: { count: 3, parser: 'parseTriggerTemplate' },
//   scripts: { count: 10, parser: 'parseScript' }
// }
```

## 依赖安装

如果需要使用TOML格式，请安装TOML解析库：

```bash
pnpm add @iarna/toml
# 或
pnpm add toml
```

## 注意事项

1. **TOML序列化**：TOML解析器不支持序列化(TOML是主配置文件，如果需要来回转换统一使用json格式)，导出时请使用JSON格式
2. **参数替换**：参数替换是递归的，会替换所有嵌套对象中的占位符
3. **边引用**：转换后会自动更新节点的边引用（outgoingEdgeIds和incomingEdgeIds）
4. **验证结果**：验证结果使用 `Result` 类型，需要使用 `isOk()` 和 `isErr()` 方法判断
5. **验证逻辑**：所有验证逻辑由 `sdk/core/validation` 提供，确保与核心模块的一致性

## 架构说明

### 模块职责

- **sdk/api/config**：
  - 配置文件解析（JSON/TOML）
  - 参数替换
  - 配置转换（边引用更新）
  - 提供配置文件处理的入口
  - 支持多种配置类型：工作流、节点模板、触发器模板、脚本

- **sdk/core/validation**：
  - 工作流定义验证
  - 节点配置验证
  - 边引用验证
  - 图结构验证

- **sdk/core/services**：
  - 节点模板注册表（NodeTemplateRegistry）
  - 触发器模板注册表（TriggerTemplateRegistry）
  - 脚本服务（CodeService）

### 数据流

```
配置文件 → ConfigLoader → ConfigValidator → 注册表/返回对象
   ↓            ↓              ↓              ↓
 解析        验证          注册/转换        完成
```

### 目录结构

```
sdk/api/config/
├── validators/                   # 配置验证器
│   ├── base-validator.ts         # 基础验证器抽象类
│   ├── workflow-validator.ts     # 工作流配置验证器
│   ├── node-template-validator.ts # 节点模板配置验证器
│   ├── trigger-template-validator.ts # 触发器模板配置验证器
│   ├── script-validator.ts       # 脚本配置验证器
│   └── batch-validators.ts       # 批量验证函数
├── config-parser.ts              # 通用配置解析器
├── config-transformer.ts         # 通用配置转换器
├── parsers.ts                    # 纯函数式配置解析接口
├── types.ts                      # 类型定义
├── json-parser.ts                # JSON解析器
├── toml-parser.ts                # TOML解析器
└── index.ts                      # 入口文件
```

## 示例

完整示例请参考：

- [`sdk/api/config/__tests__/config-parser.test.ts`](./__tests__/config-parser.test.ts)
- [`sdk/api/config/__tests__/example-workflow.json`](./__tests__/example-workflow.json)
