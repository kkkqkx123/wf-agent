# SDK Config 模块重构说明

## 重构概述

本次重构将 `sdk/api/config` 模块从有状态设计改造为完全无状态的设计，明确了 SDK 层和应用层的职责边界。SDK 层完全不涉及文件 I/O，只提供纯配置解析接口。

## 设计原则

### SDK 层 (`sdk/api/config`)

- **完全无状态**：所有函数都是纯函数，不持有任何配置数据
- **仅提供解析接口**：接收配置内容字符串，返回解析后的配置对象
- **不涉及文件 I/O**：文件读取由应用层负责
- **不操作注册表**：配置注册由应用层负责
- **平台无关**：可在任何环境（浏览器、Node.js、边缘计算）中使用

### 应用层 (`apps/*`)

- **文件 I/O**：负责配置文件的读取和写入
- **配置管理**：负责配置状态管理和生命周期控制
- **业务逻辑**：根据应用需求组织配置加载流程
- **错误处理**：处理文件系统相关的错误

## 主要变更

### 1. 移除 ConfigManager

**之前**：有状态的单例类，包含文件扫描和加载逻辑

```typescript
export class ConfigManager {
  async loadFromDirectory(directory: string): Promise<LoadFromDirectoryResult> {
    // SDK 层处理文件 I/O
  }
}

export const configManager = new ConfigManager();
```

**现在**：完全移除，文件 I/O 由应用层处理

### 2. 纯函数式解析接口

**之前**：使用有状态的 Loader 类

```typescript
export class NodeTemplateLoader extends BaseConfigLoader {
  parseTemplate(content: string, format: ConfigFormat): NodeTemplate {
    const config = this.parseFromContent(content, format);
    return config.config as NodeTemplate;
  }
}
```

**现在**：使用纯函数式接口

```typescript
export function parseNodeTemplate(content: string, format: ConfigFormat): NodeTemplate {
  const config = nodeTemplateParser.parse(content, format, ConfigType.NODE_TEMPLATE);
  return config.config as NodeTemplate;
}
```

### 3. 导出变更

**移除的导出**：

- `ConfigManager` 类
- `configManager` 单例实例
- `BaseConfigLoader` 抽象类
- `WorkflowLoader` - 工作流加载器类
- `NodeTemplateLoader` - 节点模板加载器类
- `TriggerTemplateLoader` - 触发器模板加载器类
- `ScriptLoader` - 脚本加载器类
- `scanConfigFiles()` - 文件扫描函数
- `scanConfigFilesFromDirectories()` - 多目录扫描函数
- `exportConfigsToDirectory()` - 文件导出函数
- `loadJsonFromFile()` - JSON 文件读取
- `saveJsonToFile()` - JSON 文件保存

**保留的导出**：

- `ConfigParser` - 配置解析器
- `ConfigTransformer` - 配置转换器
- `parseWorkflow()` - 工作流解析函数
- `parseNodeTemplate()` - 节点模板解析函数
- `parseTriggerTemplate()` - 触发器模板解析函数
- `parseScript()` - 脚本解析函数
- `parseBatchWorkflows()` - 批量工作流解析函数
- `parseBatchNodeTemplates()` - 批量节点模板解析函数
- `parseBatchTriggerTemplates()` - 批量触发器模板解析函数
- `parseBatchScripts()` - 批量脚本解析函数
- `validateWorkflowConfig()` - 验证工作流配置
- `validateNodeTemplateConfig()` - 验证节点模板配置
- `validateTriggerTemplateConfig()` - 验证触发器模板配置
- `validateScriptConfig()` - 验证脚本配置
- `validateBatchWorkflows()` - 批量验证工作流配置
- `validateBatchNodeTemplates()` - 批量验证节点模板配置
- `validateBatchTriggerTemplates()` - 批量验证触发器模板配置
- `validateBatchScripts()` - 批量验证脚本配置
- `parseJson()` / `stringifyJson()` - JSON 解析工具
- `parseToml()` - TOML 解析工具

## 使用示例

### 应用层配置管理（完整示例）

```typescript
import { sdk } from "@modular-agent/sdk";
import { parseWorkflow, parseNodeTemplate, ConfigFormat } from "@modular-agent/sdk/api/config";
import * as fs from "fs/promises";
import * as path from "path";

class AppConfigManager {
  private detectFormat(filePath: string): ConfigFormat {
    const ext = path.extname(filePath).toLowerCase();
    return ext === ".toml" ? ConfigFormat.TOML : ConfigFormat.JSON;
  }

  async loadAndRegisterConfigs(configDir: string): Promise<void> {
    // 1. 应用层扫描目录
    const files = await fs.readdir(configDir);
    const workflowFiles = files.filter(
      f => (f.endsWith(".json") || f.endsWith(".toml")) && !f.includes("template"),
    );

    // 2. 应用层读取文件内容
    for (const file of workflowFiles) {
      const filePath = path.join(configDir, file);
      const content = await fs.readFile(filePath, "utf-8");
      const format = this.detectFormat(filePath);

      // 3. SDK 层解析配置内容（纯函数）
      const workflow = parseWorkflow(content, format);

      // 4. 应用层注册配置
      await sdk.workflows.register(workflow);
    }
  }
}
```

### 直接使用 SDK 解析器

```typescript
import { ConfigParser } from "@modular-agent/sdk/api/config";

const parser = new ConfigParser();

// 解析配置内容（应用层提供内容）
const content = '{"id": "test", "nodes": []}';
const parsedConfig = parser.parse(content, ConfigFormat.JSON);

// 验证配置
const validationResult = parser.validate(parsedConfig);

// 转换为工作流定义
const workflow = parser.transformToWorkflow(parsedConfig.config);
```

### 批量解析配置

```typescript
import { parseBatchNodeTemplates, ConfigFormat } from "@modular-agent/sdk/api/config";

// 应用层读取多个文件内容
const contents = ['{"name": "template1", ...}', '{"name": "template2", ...}'];
const formats = [ConfigFormat.JSON, ConfigFormat.JSON];

// SDK 层批量解析（纯函数）
const templates = parseBatchNodeTemplates(contents, formats);

// 应用层注册
templates.forEach(template => {
  sdk.nodeTemplates.register(template);
});
```

### 批量验证配置

```typescript
import { validateBatchWorkflows, validateBatchNodeTemplates } from "@modular-agent/sdk/api/config";

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
```

## 迁移指南

### 如果之前使用了 `configManager.loadFromDirectory()`

**旧代码**：

```typescript
import { configManager } from "@modular-agent/sdk/api/config";

const result = await configManager.loadFromDirectory("./configs");
```

**新代码**：

```typescript
import { parseWorkflow, ConfigFormat } from "@modular-agent/sdk/api/config";
import * as fs from "fs/promises";
import * as path from "path";

const files = await fs.readdir("./configs");

for (const file of files) {
  const content = await fs.readFile(path.join("./configs", file), "utf-8");
  const format = file.endsWith(".toml") ? ConfigFormat.TOML : ConfigFormat.JSON;
  const workflow = parseWorkflow(content, format);
  await sdk.workflows.register(workflow);
}
```

### 如果之前使用了 `loader.loadFromFile()`

**旧代码**：

```typescript
import { NodeTemplateLoader } from "@modular-agent/sdk/api/config";

const loader = new NodeTemplateLoader();
const template = await loader.loadFromFile("./template.json");
```

**新代码**：

```typescript
import { parseNodeTemplate, ConfigFormat } from "@modular-agent/sdk/api/config";
import * as fs from "fs/promises";

const content = await fs.readFile("./template.json", "utf-8");
const template = parseNodeTemplate(content, ConfigFormat.JSON);
```

### 如果之前使用了 `loader.loadAndRegister()`

**旧代码**：

```typescript
import { NodeTemplateLoader } from "@modular-agent/sdk/api/config";

const loader = new NodeTemplateLoader();
await loader.loadAndRegister("./template.json");
```

**新代码**：

```typescript
import { parseNodeTemplate, ConfigFormat } from "@modular-agent/sdk/api/config";
import { sdk } from "@modular-agent/sdk";
import * as fs from "fs/promises";

const content = await fs.readFile("./template.json", "utf-8");
const template = parseNodeTemplate(content, ConfigFormat.JSON);
sdk.nodeTemplates.register(template); // 应用层控制注册
```

## 优势

1. **职责清晰**：SDK 层专注配置解析，应用层负责文件 I/O 和配置管理
2. **平台无关**：SDK 可在任何环境使用，不依赖 Node.js 文件系统
3. **易于测试**：纯函数更容易进行单元测试，无需 mock 文件系统
4. **灵活控制**：应用层可以自定义文件加载策略（如从数据库、远程 API 加载）
5. **避免全局状态**：完全无状态，无副作用
6. **更好的可维护性**：明确的边界使代码更易理解和维护

## 向后兼容性

本次重构是**破坏性变更**，需要更新使用旧 API 的代码。建议：

1. 在应用层创建配置管理器（参考 `apps/web-app/src/config-manager.ts`）
2. 将文件 I/O 逻辑移到应用层
3. 使用 SDK 的纯解析接口
4. 更新相关测试代码

## 相关文件

- `sdk/api/config/parsers.ts` - 纯函数式配置解析接口
- `apps/web-app/src/config-manager.ts` - 应用层配置管理完整示例
