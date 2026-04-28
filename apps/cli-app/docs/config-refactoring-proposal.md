# CLI App 配置系统重构方案

## 1. 执行摘要

### 1.1 核心结论

**cosmiconfig 依赖必须移除**。当前配置系统存在根本性问题，需要彻底重构。

### 1.2 关键问题

1. **TOML 支持名存实亡**：cosmiconfig 的 TOML loader 实现存在缺陷，实际无法可靠工作
2. **SDK 能力未利用**：SDK 已提供完整的配置加载和解析功能，cli-app 却重复实现
3. **配置发现过度复杂**：自动向上搜索配置文件导致不可控的行为
4. **架构分层混乱**：CLI 配置与 SDK 配置混合，职责不清

---

## 2. 当前问题深度分析

### 2.1 cosmiconfig 的 TOML 支持问题

#### 2.1.1 代码层面的问题

当前实现 ([loader.ts#L23-L37](file:///d:/项目/agent/wf-agent/apps/cli-app/src/config/cli/loader.ts))：

```typescript
this.explorer = cosmiconfig("modular-agent", {
  searchPlaces: [
    "package.json",
    ".modular-agentrc",
    ".modular-agentrc.json",
    ".modular-agentrc.toml",  // TOML 支持声明
    ".modular-agentrc.ts",
    ".modular-agentrc.js",
    "modular-agent.config.js",
    "modular-agent.config.ts",
    "modular-agent.config.toml",
  ],
  loaders: {
    ".toml": (filepath, content) => {
      return TOML.parse(content);  // 同步解析
    },
  },
});
```

**问题分析**：

| 问题 | 说明 | 影响 |
|------|------|------|
| Loader 返回类型不匹配 | cosmiconfig 期望 loader 返回 `Promise<{ config: any }>` 或 `{ config: any }` | 可能导致解析结果被错误处理 |
| 同步/异步不一致 | TOML.parse 是同步的，但 cosmiconfig 内部可能按异步处理 | 行为不可预测 |
| 错误处理缺失 | loader 没有 try-catch，TOML 解析错误会抛出到 cosmiconfig 内部 | 错误信息丢失或被包装 |
| 缓存问题 | cosmiconfig 对自定义 loader 的缓存策略不明确 | 配置更新后可能读取旧值 |

#### 2.1.2 实际使用问题

- 项目主要使用 TOML 格式（SDK 配置、工作流定义）
- cosmiconfig 对 TOML 的支持是"附加功能"而非核心能力
- 自定义 loader 的可靠性未经充分测试

### 2.2 SDK 能力重复问题

#### 2.2.1 SDK 已提供的功能

SDK 在 [config-utils.ts](file:///d:/项目/agent/wf-agent/sdk/api/shared/config/config-utils.ts) 中提供：

```typescript
// 检测配置格式
export function detectConfigFormat(filePath: string): ConfigFormat

// 读取配置文件
export async function readConfigFile(filePath: string): Promise<string>

// 加载配置内容（结合上述两个功能）
export async function loadConfigContent(filePath: string): Promise<{
  content: string;
  format: ConfigFormat;
}>
```

SDK 在 [parsers.ts](file:///d:/项目/agent/wf-agent/sdk/api/shared/config/parsers.ts) 中提供：

```typescript
export function parseWorkflow(content: string, format: ConfigFormat, parameters?: Record<string, any>)
export function parseNodeTemplate(content: string, format: ConfigFormat)
export function parseTriggerTemplate(content: string, format: ConfigFormat)
export function parseScript(content: string, format: ConfigFormat)
export function parseLLMProfile(content: string, format: ConfigFormat)
```

#### 2.2.2 cli-app 的重复实现

cli-app 的 [config-manager.ts](file:///d:/项目/agent/wf-agent/apps/cli-app/src/config/sdk/config-manager.ts) 实际上是对 SDK 功能的包装，但：

1. **没有复用 SDK 的 `loadConfigContent`**：虽然导入并使用了，但架构上显得多余
2. **增加了不必要的抽象层**：ConfigManager 类只是批量操作的包装
3. **与 CLI 配置系统分离**：cli-app 有两套配置系统（CLI 配置 + SDK 配置）

### 2.3 配置发现机制问题

#### 2.3.1 过度复杂的搜索路径

当前支持 10 种配置文件位置：

```
package.json
.modular-agentrc
.modular-agentrc.json
.modular-agentrc.toml
.modular-agentrc.ts
.modular-agentrc.js
modular-agent.config.js
modular-agent.config.ts
modular-agent.config.toml
```

**问题**：
- 维护成本高
- 用户困惑（不知道用哪个）
- 潜在冲突（多个配置文件同时存在时行为不可预测）

#### 2.3.2 向上搜索的不可控性

```typescript
result = await this.explorer.search();  // 自动向上搜索到用户主目录
```

**风险**：
- 用户可能在不知情的情况下加载了上级目录的配置
- 不同工作目录下执行相同命令可能产生不同结果
- 调试困难（用户不知道实际加载了哪个配置）

### 2.4 架构分层问题

#### 2.4.1 当前架构

```
┌─────────────────────────────────────┐
│           CLI Application            │
├─────────────────────────────────────┤
│  CLI Config (cosmiconfig)            │  ← 问题：过度复杂
│  - loader.ts                         │
│  - accessor.ts                       │
│  - schema.ts (zod)                   │
├─────────────────────────────────────┤
│  SDK Config Manager                  │  ← 问题：重复包装
│  - config-manager.ts                 │
├─────────────────────────────────────┤
│  SDK (actual parsing)                │  ← 实际能力在这里
│  - loadConfigContent                 │
│  - parseWorkflow, etc.               │
└─────────────────────────────────────┘
```

#### 2.4.2 问题总结

1. **三层配置系统**：CLI 配置、SDK Config Manager、SDK 实际解析
2. **依赖冗余**：cosmiconfig + @iarna/toml 重复
3. **职责不清**：CLI 配置应该简单直接，不应包含复杂的发现逻辑

---

## 3. 重构方案

### 3.1 设计原则

1. **显式优于隐式**：配置文件路径必须显式指定，禁止自动搜索
2. **单一职责**：CLI 配置只负责 CLI 行为，SDK 配置只负责 SDK 行为
3. **依赖最小化**：移除 cosmiconfig，复用 SDK 能力
4. **简单可预测**：配置加载行为必须清晰、可预测、可调试

### 3.2 新架构设计

```
┌─────────────────────────────────────┐
│           CLI Application            │
├─────────────────────────────────────┤
│  CLI Config (simplified)             │
│  - 显式文件路径或默认路径             │
│  - 使用 SDK 的 loadConfigContent     │
│  - Zod 验证                          │
├─────────────────────────────────────┤
│  SDK Config (direct use)             │
│  - 直接使用 SDK 的解析功能           │
│  - 无需额外包装层                    │
└─────────────────────────────────────┘
```

### 3.3 具体实施步骤

#### 步骤 1：移除 cosmiconfig 依赖

**文件变更**：
- `apps/cli-app/package.json`：移除 `cosmiconfig`
- `apps/cli-app/src/config/cli/loader.ts`：重写

**新 loader.ts 实现**：

```typescript
/**
 * CLI Configuration Loader (Refactored)
 * Simplified configuration loading without cosmiconfig.
 */

import { loadConfigContent } from "@wf-agent/sdk";
import TOML from "@iarna/toml";
import type { CLIConfig } from "./types.js";
import { CLIConfigSchema } from "./schema.js";
import { DEFAULT_CONFIG } from "./defaults.js";
import { getOutput } from "../../utils/output.js";

const output = getOutput();

/**
 * Parse configuration content based on format
 */
function parseConfigContent(content: string, format: "json" | "toml"): unknown {
  switch (format) {
    case "json":
      return JSON.parse(content);
    case "toml":
      return TOML.parse(content);
    default:
      throw new Error(`Unsupported config format: ${format}`);
  }
}

/**
 * Load CLI configuration from explicit path or default location
 * @param configPath Explicit config file path (optional)
 * @returns Validated configuration object
 */
export async function loadConfig(configPath?: string): Promise<CLIConfig> {
  // If no path specified, use default location
  const targetPath = configPath || "./.modular-agent.toml";
  
  try {
    // Use SDK's loadConfigContent
    const { content, format } = await loadConfigContent(targetPath);
    
    // Parse the content
    const rawConfig = parseConfigContent(content, format);
    
    // Validate with Zod
    const validatedConfig = CLIConfigSchema.parse(rawConfig);
    
    // Merge with defaults
    return { ...DEFAULT_CONFIG, ...validatedConfig };
  } catch (error) {
    // If explicit path was specified and failed, throw error
    if (configPath) {
      throw new Error(
        `Failed to load config from ${configPath}: ${
          error instanceof Error ? error.message : String(error)
        }`
      );
    }
    
    // If default path failed, use defaults with warning
    output.warnLog(
      "Config file not found or invalid, using default configuration:",
      { error: String(error) }
    );
    return CLIConfigSchema.parse(DEFAULT_CONFIG);
  }
}

/**
 * Load configuration with environment variable overrides
 */
export async function loadConfigWithEnvOverride(
  configPath?: string
): Promise<CLIConfig> {
  const config = await loadConfig(configPath);
  
  // Apply environment variable overrides
  if (process.env["CLI_VERBOSE"] === "true") {
    config.verbose = true;
  }
  if (process.env["CLI_DEBUG"] === "true") {
    config.debug = true;
  }
  if (process.env["CLI_LOG_LEVEL"]) {
    config.logLevel = process.env["CLI_LOG_LEVEL"] as CLIConfig["logLevel"];
  }
  if (process.env["LOG_DIR"]) {
    config.output = { ...config.output, dir: process.env["LOG_DIR"] };
  }
  
  return config;
}
```

#### 步骤 2：简化配置文件支持

**支持的配置文件**：
- `.modular-agent.toml`（推荐，默认）
- `.modular-agent.json`（可选）

**移除的支持**：
- `package.json` 中的配置字段
- `.modular-agentrc`（无扩展名）
- `.ts` / `.js` 配置文件（复杂且不必要）
- 向上搜索功能

#### 步骤 3：合并配置系统

**当前问题**：
- `cli/` 目录：CLI 自身配置
- `sdk/` 目录：SDK 配置管理器

**重构后**：

```
src/config/
├── index.ts           # 统一导出
├── types.ts           # 配置类型定义（合并 CLI 和通用类型）
├── schema.ts          # Zod 验证模式
├── defaults.ts        # 默认值
├── loader.ts          # 配置加载（简化版）
└── accessor.ts        # 配置访问器（可选保留）
```

**移除**：
- `sdk/config-manager.ts`：直接使用 SDK 的功能，无需包装

#### 步骤 4：更新入口文件

**当前** ([index.ts#L59](file:///d:/项目/agent/wf-agent/apps/cli-app/src/index.ts))：

```typescript
const config = await loadConfig(options.config);
```

**行为变更**：
- 如果 `--config` 指定了路径，必须存在且有效，否则报错
- 如果未指定，尝试加载 `./.modular-agent.toml`，不存在则使用默认值

### 3.4 迁移指南

#### 对用户的影响

| 场景 | 迁移前 | 迁移后 |
|------|--------|--------|
| 使用默认配置 | 自动搜索 | 需创建 `.modular-agent.toml` 或使用默认值 |
| 使用自定义路径 | `--config <path>` | 保持不变 |
| 使用 `package.json` | 配置在 `package.json` 中 | 需迁移到 `.modular-agent.toml` |
| 使用 `.js` 配置 | `.modular-agentrc.js` | 需转换为 TOML 或 JSON |

#### 迁移脚本示例

```bash
# 检查现有配置
ls -la .modular-agent* 2>/dev/null || echo "No config found"

# 创建新的 TOML 配置
cat > .modular-agent.toml << 'EOF'
[storage]
type = "json"

[storage.json]
baseDir = "./storage"

[output]
dir = "./outputs"
enableLogTerminal = true
EOF
```

---

## 4. 收益分析

### 4.1 移除的依赖

- `cosmiconfig`: ~50KB + 子依赖
- 简化依赖树，减少潜在安全漏洞

### 4.2 代码简化

| 文件 | 当前行数 | 重构后估计 | 减少 |
|------|----------|------------|------|
| loader.ts | ~190 | ~80 | 58% |
| config-manager.ts | ~350 | 0（移除） | 100% |
| 总计 | ~540 | ~80 | 85% |

### 4.3 行为可预测性

- 配置加载路径明确
- 错误信息清晰
- 调试简单

### 4.4 维护成本

- 配置系统与 SDK 保持一致
- 单一 TOML 解析实现（@iarna/toml）
- 减少测试场景（无需测试自动搜索）

---

## 5. 风险评估

### 5.1 破坏性变更

**高风险**：
- 使用 `package.json` 配置的用户需要迁移
- 使用 `.js` / `.ts` 配置文件的用户需要迁移
- 依赖向上搜索功能的用户需要显式指定配置路径

**缓解措施**：
- 提供清晰的迁移文档
- 在首次运行时检测旧配置并给出警告
- 提供迁移工具脚本

### 5.2 回滚方案

如需回滚：
1. 恢复 `package.json` 中的 cosmiconfig 依赖
2. 恢复 `loader.ts` 的 cosmiconfig 实现
3. 恢复 `config-manager.ts`

---

## 6. 实施计划

### 6.1 阶段划分

| 阶段 | 任务 | 预计时间 |
|------|------|----------|
| 1 | 编写新 loader.ts | 1 天 |
| 2 | 移除 cosmiconfig 依赖 | 0.5 天 |
| 3 | 移除 config-manager.ts，更新引用 | 1 天 |
| 4 | 更新文档和测试 | 1 天 |
| 5 | 验证和回归测试 | 1 天 |

### 6.2 验证清单

- [ ] 新 loader 能正确加载 TOML 配置
- [ ] 新 loader 能正确加载 JSON 配置
- [ ] 显式路径不存在时报错
- [ ] 默认路径不存在时使用默认值
- [ ] 环境变量覆盖正常工作
- [ ] Zod 验证正常工作
- [ ] 所有现有测试通过
- [ ] 文档已更新

---

## 7. 结论

**建议立即执行此重构**。

cosmiconfig 在当前项目中：
1. 提供的 TOML 支持不可靠
2. 功能与 SDK 重复
3. 引入不可控的配置发现行为
4. 增加不必要的依赖和复杂性

重构后的配置系统将：
1. 更简单（~85% 代码减少）
2. 更可预测（显式路径）
3. 更可靠（复用 SDK 能力）
4. 更易维护（单一实现）
