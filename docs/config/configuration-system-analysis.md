# 项目配置功能分析与集成指南

## 📋 目录

1. [配置系统架构概览](#配置系统架构概览)
2. [配置文件组织结构](#配置文件组织结构)
3. [配置加载机制](#配置加载机制)
4. [各模块配置详解](#各模块配置详解)
5. [配置集成方案](#配置集成方案)
6. [最佳实践与建议](#最佳实践与建议)

---

## 配置系统架构概览

### 核心设计原则

1. **分层配置架构**
   - 全局配置（`configs/global.toml`）
   - 模块级配置（`configs/llms/`, `configs/tools/`, `configs/prompts/`等）
   - 应用级配置（通过SDK Options传递）

2. **多格式支持**
   - TOML（主要格式，推荐用于复杂配置）
   - JSON（简单配置场景）
   - 环境变量注入（`${VAR:default}`语法）

3. **动态加载与热重载**
   - 支持运行时重新加载配置
   - 文件监听机制（watch_interval配置）
   - 配置变更自动生效

4. **验证与安全**
   - Zod Schema验证
   - 敏感信息模式匹配（secret_patterns）
   - 配置错误详细提示

### 技术栈

- **解析器**: `@iarna/toml` (TOML), 原生JSON.parse
- **验证器**: Zod Schema
- **管理器**: TomlParserManager（单例懒加载）
- **存储**: 内存缓存 + 可选持久化（StorageAdapter）

---

## 配置文件组织结构

```
configs/
├── global.toml                    # 全局基础配置
├── metrics.example.toml           # 监控指标配置示例
│
├── agents/                        # Agent Loop配置
│   └── code-assistant.toml        # 代码助手Agent示例
│
├── database/                      # 数据库配置
│   └── database.toml              # PostgreSQL/SQLite配置
│
├── environments/                  # 环境配置
│   └── development.toml           # 开发环境配置
│
├── llms/                          # LLM相关配置
│   ├── pools/                     # LLM池配置
│   │   ├── default-pool.toml
│   │   └── ...
│   ├── provider/                  # 提供商配置
│   │   ├── openai/
│   │   ├── anthropic/
│   │   ├── gemini/
│   │   └── mock/
│   ├── task_groups/               # 任务组配置
│   │   ├── planning.toml
│   │   ├── coding.toml
│   │   └── ...
│   └── retry.toml                 # 重试策略配置
│
├── prompts/                       # 提示词配置
│   ├── README.md                  # 提示词使用说明
│   ├── rules/                     # 规则提示词
│   │   ├── system-rules.toml
│   │   └── user-rules.toml
│   ├── system/                    # 系统提示词
│   │   ├── agent-system.toml
│   │   ├── workflow-system.toml
│   │   └── ...
│   ├── templates/                 # 提示词模板
│   │   ├── tool-description.toml
│   │   └── ...
│   └── user_commands/             # 用户命令提示词
│       ├── help.toml
│       └── ...
│
├── scripts/                       # 脚本配置
│   └── inline/                    # 内联脚本
│       ├── data-processor.toml
│       └── hello-world.toml
│
└── tools/                         # 工具配置
    ├── __registry__.toml          # 工具注册表（核心）
    ├── mcp/                       # MCP工具
    │   └── database.toml
    ├── rest/                      # REST API工具
    │   ├── fetch.toml
    │   ├── weather.toml
    │   └── duckduckgo_search.toml
    ├── stateful/                  # 有状态工具
    │   └── sequentialthinking.toml
    └── stateless/                 # 无状态工具
        ├── calculator.toml
        ├── hash_convert.toml
        └── time_tool.toml
```

---

## 配置加载机制

### 1. SDK初始化流程

```typescript
// apps/cli-app/src/index.ts 中的初始化顺序
const sdkInstance = await initializeSDK({
  // 1. 日志配置
  logging: {
    level: 'DEBUG',
    output: 'both',  // console | file | both
    format: 'json',
    filePath: 'logs/sdk.log'
  },
  
  // 2. 存储适配器
  workflowStorageAdapter: postgresAdapter,
  checkpointStorageAdapter: sqliteAdapter,
  taskStorageAdapter: memoryAdapter,
  
  // 3. Presets（预定义内容）
  presets: {
    predefinedTools: { enabled: true },
    predefinedWorkflows: { enabled: true },
    predefinedTriggers: { enabled: true },
    predefinedPrompts: { enabled: true }
  },
  
  // 4. LLM Profiles
  profiles: {
    profiles: [...],
    defaultProfileId: 'gpt-4o'
  },
  
  // 5. 其他配置
  mcp: { enabled: true },
  skills: { paths: ['./skills'] },
  humanRelay: { handler: customHandler }
});

await sdkInstance.waitForReady();
```

### 2. 配置文件加载API

#### 基础加载函数

```typescript
import { 
  loadConfigFile,      // 加载配置文件内容
  parseToml,           // 解析TOML
  parseJson,           // 解析JSON
  getConfigFormatFromPath  // 根据扩展名检测格式
} from "@wf-agent/sdk/api/shared/config";

// 使用示例
const { content, format } = await loadConfigFile('configs/agents/code-assistant.toml');
const config = parseToml(content);
```

#### 专用加载器

```typescript
// 工作流配置加载
import { parseWorkflow } from "@wf-agent/sdk/api/shared/config";
const workflow = await parseWorkflow('path/to/workflow.toml');

// Agent Loop配置加载
import { loadAgentLoopConfig } from "@wf-agent/sdk/api/shared/config";
const agentConfig = await loadAgentLoopConfig('agent-config.toml');

// Prompt模板加载
import { loadPromptTemplate } from "@wf-agent/sdk/api/shared/config";
const template = await loadPromptTemplate('template.toml');
```

### 3. 环境变量注入

配置文件中支持环境变量语法：

```toml
# configs/database/database.toml
host = "${DATABASE_HOST:localhost}"
port = "${DATABASE_PORT:5432}"
password = "${DATABASE_PASSWORD}"  # 无默认值，必须设置
```

**处理逻辑**：
- `${VAR:default}` - 如果环境变量不存在，使用默认值
- `${VAR}` - 如果环境变量不存在，保持原样或抛出错误（取决于配置）

---

## 各模块配置详解

### 1. 全局配置 (global.toml)

```toml
# 基础设置
log_level = "DEBUG"
env = "development"
debug = true
env_prefix = "AGENT_"

# 热重载
hot_reload = true
watch_interval = 5

# 日志输出
[[log_outputs]]
type = "console"
level = "DEBUG"
format = "text"

[[log_outputs]]
type = "file"
level = "DEBUG"
format = "json"
path = "logs/agent.log"
rotation = "daily"
max_size = "10MB"

# 敏感信息保护
secret_patterns = [
  "sk-[a-zA-Z0-9]{20,}",
  "ms-[a-zA-Z0-9]{15,}",
  "\\w+@\\w+\\.\\w+",
]
```

**集成要点**：
- 在CLI应用启动时首先加载
- 通过`loadConfigWithEnvOverride()`函数处理环境变量覆盖
- 影响整个应用的日志、调试行为

---

### 2. LLM配置 (llms/)

#### Provider配置示例

```toml
# configs/llms/provider/openai/gpt-4o.toml
[provider]
name = "openai"
model = "gpt-4o"
api_key = "${OPENAI_API_KEY}"
base_url = "https://api.openai.com/v1"

[parameters]
temperature = 0.7
max_tokens = 4096
top_p = 1.0

[retry]
max_retries = 3
backoff_multiplier = 2
initial_delay = 1000
```

#### Task Groups配置

```toml
# configs/llms/task_groups/planning.toml
[task_group]
id = "planning"
name = "规划任务"
description = "用于任务分解和规划的LLM配置"

[llm_profiles]
primary = "gpt-4o"
fallback = "claude-3-opus"

[parameters]
temperature = 0.3  # 较低温度以获得更确定的输出
```

**集成方式**：
- 通过SDK的`profiles`选项注册
- 或在应用中手动调用`profileAPI.create(profile)`
- Task Groups用于不同场景的LLM选择策略

---

### 3. 工具配置 (tools/)

#### 工具注册表 (__registry__.toml)

```toml
[metadata]
name = "tools_registry"
version = "1.0.0"

# 工具类型定义
[tool_types.builtin]
class_path = "src.domain.tools.types.builtin_tool:BuiltinTool"
description = "内置工具类型"
enabled = true
config_directory = "builtin"
config_files = ["calculator.toml", "hash_convert.toml"]

[tool_types.rest]
class_path = "src.domain.tools.types.rest_tool:RestTool"
description = "REST工具类型"
enabled = true
config_directory = "rest"
config_files = ["fetch.toml", "weather.toml"]

# 工具集
[tool_sets.basic_tools]
description = "基础工具集"
enabled = true
tools = ["calculator", "fetch", "weather"]

# 自动发现
[auto_discovery]
enabled = true
scan_directories = ["configs/tools/rest", "configs/tools/mcp"]
file_patterns = ["*.toml"]
exclude_patterns = ["__*", "_*"]
```

#### 单个工具配置示例

```toml
# configs/tools/stateless/calculator.toml
[tool]
id = "calculator"
name = "计算器"
description = "执行数学计算"
type = "STATELESS"
category = "utility"
version = "1.0.0"
enabled = true

[parameters]
precision = 10
allow_complex = false

[metadata]
author = "Modular Agent Team"
tags = ["math", "calculation"]
```

**集成方式**：
- SDK启动时自动扫描并注册（通过presets.predefinedTools）
- 可通过`allowList/blockList`控制启用哪些工具
- 支持运行时动态注册/注销

---

### 4. Agent配置 (agents/)

```toml
# configs/agents/code-assistant.toml
[agent]
id = "code-assistant"
name = "代码助手"
description = "一个用于代码编写和审查的 Agent"
version = "1.0.0"

# LLM配置
profileId = "gpt-4o"
systemPrompt = "你是一个专业的代码助手..."
maxIterations = 20
stream = true

# 工具配置
[agent.availableTools]
initial = ["read_file", "write_file", "edit_file", "shell_execute"]

# 检查点配置
[agent.checkpoint]
createOnEnd = true
createOnError = true
createOnIteration = false

# Hook配置
[[agent.hooks]]
hookType = "BEFORE_ITERATION"
eventName = "iteration_started"
enabled = true
weight = 10

[[agent.hooks]]
hookType = "AFTER_LLM_CALL"
eventName = "token_usage_record"
enabled = true
weight = 5

[agent.metadata]
author = "Modular Agent Team"
tags = ["code", "assistant"]
```

**集成方式**：
- 通过命令行参数加载：`modular-agent agent run -c agent-config.toml`
- 或在代码中通过`loadAgentLoopConfig()`加载
- Hook系统支持事件驱动的扩展点

---

### 5. 工作流配置

工作流配置通过专门的解析器处理，支持复杂的节点图和依赖关系：

```typescript
import { parseWorkflow } from "@wf-agent/sdk/api/shared/config";

const workflow = await parseWorkflow('workflow.toml');
workflowRegistry.register(workflow);
```

**特点**：
- 支持节点模板引用
- 支持触发器配置
- 支持子工作流嵌套
- 完整的验证和预处理

---

### 6. Prompt配置 (prompts/)

```toml
# configs/prompts/templates/tool-description.toml
[template]
id = "tool_description"
name = "工具描述模板"
version = "1.0.0"

[content]
system = """
You have access to the following tools:

{{#each tools}}
- {{name}}: {{description}}
  Parameters: {{parameters}}
{{/each}}
"""

[variables]
tools = []

[metadata]
category = "tool_integration"
language = "en"
```

**集成方式**：
- SDK启动时自动注册（presets.predefinedPrompts）
- 通过`promptTemplateRegistry`管理
- 支持变量替换和模板渲染

---

## 配置集成方案

### 方案1: 应用层完整集成（推荐用于CLI/Web应用）

```typescript
// apps/cli-app/src/config/integration.ts

import { createSDK } from "@wf-agent/sdk";
import { loadConfigWithEnvOverride } from "./cli/loader.js";
import { PostgresStorageAdapter } from "@wf-agent/storage";

export async function initializeAppConfig(configPath?: string) {
  // 1. 加载全局配置
  const globalConfig = await loadConfigWithEnvOverride(configPath);
  
  // 2. 创建存储适配器
  const dbConfig = await loadDatabaseConfig();
  const storageAdapter = new PostgresStorageAdapter({
    host: dbConfig.host,
    port: dbConfig.port,
    username: dbConfig.username,
    password: dbConfig.password,
    database: dbConfig.database,
  });
  
  // 3. 加载LLM Profiles
  const profiles = await loadLLMProfiles();
  
  // 4. 创建SDK实例
  const sdk = createSDK({
    // 日志配置
    logging: {
      level: globalConfig.log_level,
      output: 'both',
      format: globalConfig.debug ? 'text' : 'json',
    },
    
    // 存储配置
    workflowStorageAdapter: storageAdapter,
    checkpointStorageAdapter: storageAdapter,
    taskStorageAdapter: storageAdapter,
    
    // Presets配置
    presets: {
      predefinedTools: {
        enabled: true,
        allowList: globalConfig.enabled_tools,
      },
      predefinedWorkflows: { enabled: true },
      predefinedTriggers: { enabled: true },
      predefinedPrompts: { enabled: true },
    },
    
    // LLM配置
    profiles: {
      profiles: profiles,
      defaultProfileId: globalConfig.default_llm_profile,
    },
    
    // 其他配置
    debug: globalConfig.debug,
    mcp: { enabled: globalConfig.mcp_enabled },
  });
  
  // 5. 等待SDK就绪
  await sdk.waitForReady();
  
  return { sdk, globalConfig };
}
```

---

### 方案2: 最小化集成（适用于嵌入式场景）

```typescript
import { createSDK } from "@wf-agent/sdk";

// 仅启用必要功能
const sdk = createSDK({
  // 不使用持久化（纯内存）
  presets: {
    predefinedTools: { enabled: false },
    predefinedWorkflows: { enabled: false },
  },
  
  // 自定义LLM配置
  profiles: {
    profiles: [{
      id: 'custom',
      provider: 'openai',
      model: 'gpt-4',
      apiKey: process.env.OPENAI_API_KEY,
    }],
  },
});

await sdk.waitForReady();
```

---

### 方案3: 模块化按需加载

```typescript
import { 
  parseWorkflow,
  parseLLMProfile,
  loadAgentLoopConfig 
} from "@wf-agent/sdk/api/shared/config";

// 按需加载特定配置
async function loadSpecificConfigs() {
  // 加载工作流
  const workflow = await parseWorkflow('configs/workflows/my-workflow.toml');
  
  // 加载LLM Profile
  const profile = await parseLLMProfile('configs/llms/profiles/custom.toml');
  
  // 加载Agent配置
  const agentConfig = await loadAgentLoopConfig('configs/agents/assistant.toml');
  
  return { workflow, profile, agentConfig };
}
```

---

### 方案4: 动态配置热重载

```typescript
import { watch } from 'fs';
import { parseWorkflow } from "@wf-agent/sdk/api/shared/config";

class ConfigWatcher {
  private watchers: Map<string, any> = new Map();
  
  watchConfig(filePath: string, onChange: (config: any) => void) {
    const watcher = watch(filePath, async () => {
      try {
        const config = await parseWorkflow(filePath);
        onChange(config);
        console.log(`Config reloaded: ${filePath}`);
      } catch (error) {
        console.error(`Failed to reload config: ${error.message}`);
      }
    });
    
    this.watchers.set(filePath, watcher);
  }
  
  unwatchConfig(filePath: string) {
    const watcher = this.watchers.get(filePath);
    if (watcher) {
      watcher.close();
      this.watchers.delete(filePath);
    }
  }
}

// 使用
const watcher = new ConfigWatcher();
watcher.watchConfig('configs/workflows/dynamic.toml', (newConfig) => {
  workflowRegistry.update(newConfig);
});
```

---

## 最佳实践与建议

### ✅ 推荐做法

1. **配置分层管理**
   ```
   - 全局配置 → configs/global.toml
   - 环境配置 → configs/environments/{dev/prod}.toml
   - 业务配置 → configs/{module}/...
   ```

2. **敏感信息处理**
   ```toml
   # ❌ 不要硬编码
   api_key = "sk-1234567890"
   
   # ✅ 使用环境变量
   api_key = "${OPENAI_API_KEY}"
   ```

3. **配置验证**
   ```typescript
   // 始终验证配置
   const result = validateWorkflow(config);
   if (!result.valid) {
     throw new Error(`Invalid config: ${result.errors.join(', ')}`);
   }
   ```

4. **使用Presets控制预定义内容**
   ```typescript
   presets: {
     predefinedTools: { 
       enabled: true,
       allowList: ['read_file', 'write_file'],  // 白名单
     },
   }
   ```

5. **配置文档化**
   - 每个配置文件添加注释说明
   - 提供`.example.toml`作为模板
   - 维护README说明配置项含义

---

### ⚠️ 常见陷阱

1. **TOML解析器未初始化**
   ```typescript
   // ❌ 直接使用会报错
   parseToml(content);
   
   // ✅ 先初始化
   await TomlParserManager.initialize();
   parseToml(content);
   ```

2. **循环依赖问题**
   - 避免在工作流中创建循环引用
   - 使用`maxRecursionDepth`限制递归深度

3. **配置覆盖顺序**
   ```
   优先级从高到低：
   1. 命令行参数
   2. 环境变量
   3. 配置文件
   4. 默认值
   ```

4. **存储适配器未配置**
   ```typescript
   // ❌ 忘记配置会导致功能不可用
   createSDK({});
   
   // ✅ 明确配置
   createSDK({
     workflowStorageAdapter: adapter,
     checkpointStorageAdapter: adapter,
   });
   ```

---

### 🔧 调试技巧

1. **启用详细日志**
   ```typescript
   createSDK({
     logging: {
       level: 'debug',
       output: 'console',
     },
     debug: true,
   });
   ```

2. **检查配置加载状态**
   ```typescript
   const sdk = createSDK({...});
   await sdk.waitForReady();
   
   // 检查已注册的内容
   const workflows = sdk.api.workflows.list();
   const tools = sdk.api.tools.list();
   const profiles = sdk.api.profiles.list();
   ```

3. **验证配置文件语法**
   ```bash
   # 使用提供的验证工具
   node -e "
     import { validateTomlSyntax } from '@wf-agent/sdk/api/shared/config';
     const fs = require('fs');
     const content = fs.readFileSync('config.toml', 'utf-8');
     console.log(validateTomlSyntax(content));
   "
   ```

---

## 总结

### 配置系统优势

✅ **灵活性**: 支持TOML/JSON/环境变量多种格式  
✅ **可扩展**: 模块化设计，易于添加新配置类型  
✅ **安全性**: 敏感信息保护和配置验证  
✅ **动态性**: 支持热重载和运行时更新  
✅ **隔离性**: 多SDK实例完全隔离的配置  

### 集成建议

1. **新项目**: 使用方案1（完整集成），充分利用框架能力
2. **嵌入式场景**: 使用方案2（最小化集成），减少依赖
3. **微服务架构**: 使用方案3（模块化加载），按需引入
4. **开发环境**: 使用方案4（热重载），提升开发效率

### 后续优化方向

- [ ] 配置UI编辑器（Web界面）
- [ ] 配置版本管理和回滚
- [ ] 配置差异对比工具
- [ ] 自动化配置测试框架
- [ ] 配置性能分析和优化建议

---

**文档版本**: 1.0.0  
**最后更新**: 2026-05-18  
**维护者**: Modular Agent Framework Team
