# 配置加载 - 快速参考卡

## 🎯 核心数据流（3步）

```
CLI 启动
├─> apps/cli-app/src/index.ts (preAction hook)
│   └─> loadConfigWithEnvOverride()
│
SDK 初始化
├─> createSDK({ presets, storageAdapters, defaultTimeout, ... })
│
索引注册
└─> registerAllIndexResolvers()
    └─> 9 个索引类型解析器已就位
```

---

## 📍 关键文件及其职责

### 应用层
| 文件 | 职责 | 关键函数 |
|------|------|---------|
| `apps/cli-app/src/index.ts` | 启动序列入口 | preAction hook 触发配置加载 |
| `apps/cli-app/src/config/index.ts` | 配置聚合 | `loadConfigWithEnvOverride()` |

### 处理层（config-processor）
| 文件 | 职责 | 关键函数 |
|------|------|---------|
| `loader-orchestrator.ts` | 编排所有加载器 | `loadMetricsConfig()`, `loadTimeoutConfig()`, 等 |
| `config-index-loader.ts` | 加载文件索引 | `resolveLLMProfileIndex()`, `resolveWorkflowIndex()`, 等 |
| `preset-index-loaders.ts` | 加载预设索引 | `resolveMcpPresetsIndex()`, `resolveSkillPresetsIndex()`, 等 |
| `config-index-resolver.ts` | 索引注册工厂 | `registerAllIndexResolvers()` |
| `config-file-loader.ts` | 文件 I/O | `readConfigFile()`, `loadConfigFile()` |
| `mcp-settings-loader.ts` | MCP 配置 | `loadAndMergeMcpSettings()` |
| `skill-settings-loader.ts` | Skill 配置 | `loadAndMergeSkillConfig()` |

### SDK 层
| 文件 | 职责 | 关键函数 |
|------|------|---------|
| `sdk/api/shared/config/config-index.ts` | 注册模式实现 | `registerResolver()`, `loadConfigIndex()` |
| `sdk/api/shared/config/parsers/*` | 格式解析 | JSON/TOML 解析 |
| `sdk/api/shared/config/processors/*` | 域处理 | 20+ 个专用处理器 |

### 类型层
| 文件 | 职责 |
|------|------|
| `packages/types/src/config/*` | 所有配置类型定义 |

---

## 🔄 初始化序列（preAction hook 中的顺序）

```
1. initializeTomlParser()              ← SDK 初始化 TOML 支持
2. loadConfigWithEnvOverride()         ← 加载所有配置
3. output.reconfigure()                ← 应用输出设置
4. initLogger()                        ← CLI 日志初始化
5. initSDKLogger()                     ← SDK 日志初始化
6. initializeStorageManager()          ← 初始化存储
7. createSDK({ config props })         ← 创建 SDK 实例
8. sdkInstance.waitForReady()          ← 等待 bootstrap 完成
9. registerAllIndexResolvers()         ← 注册 9 个索引解析器
10. CLIUserInteractionManager.init()   ← 初始化交互处理
11. initializeContainer()              ← 初始化 DI 容器
```

---

## 📦 9 个已注册的索引类型

### 文件索引（6个）
1. ✅ `llm_profiles` → `resolveLLMProfileIndex()`
2. ✅ `workflows` → `resolveWorkflowIndex()`
3. ✅ `node_templates` → `resolveNodeTemplateIndex()`
4. ✅ `scripts` → `resolveScriptIndex()`
5. ✅ `prompt_templates` → `resolvePromptTemplateIndex()`
6. ✅ `agent_loops` → `resolveAgentLoopIndex()`

### 预设索引（3个）
7. ✅ `mcp_presets` → `resolveMcpPresetsIndex()`
8. ✅ `skill_presets` → `resolveSkillPresetsIndex()`
9. ✅ `infrastructure_presets` → `resolveInfrastructurePresetsIndex()`

---

## 🔌 配置使用示例

### 在应用中获取配置
```typescript
import { loadConfigIndex } from "@wf-agent/sdk/api";

// 注意：resolvers 已在 registerAllIndexResolvers() 中注册
const workflows = await loadConfigIndex("workflows", "./path");
workflows.entries.forEach(entry => {
  console.log(`- ${entry.id}: ${entry.name}`);
});
```

### 环境变量覆盖
```bash
# 配置文件中定义的值会被环境变量覆盖
export WF_AGENT_TIMEOUT=60
export WF_AGENT_DEBUG=true
modular-agent run
```

---

## 🗂️ 配置文件搜索路径

### 主配置文件（按优先级）
1. `--config <path>` (命令行指定)
2. `$WF_AGENT_CONFIG_PATH` (环境变量)
3. `./.wf-agent/config.toml` (项目)
4. `./.wf-agent/config.json` (项目)
5. `~/.wf-agent/config.toml` (用户)
6. `~/.wf-agent/config.json` (用户)

### MCP 配置文件
- `./.wf-agent/mcp.json` (项目)
- `~/.claude/mcp.json` (用户)

### Skill 配置文件
- `./.wf-agent/skill.json` (项目)
- `~/.claude/skills.json` (用户)

---

## ⚡ 性能特征

| 操作 | 模式 | 备注 |
|------|------|------|
| 主配置加载 | 同步 + 异步合并 | 多个配置类型并行加载 |
| 索引加载 | 异步 | 按需加载，延迟注册 |
| 环境变量合并 | 同步 | 加载后覆盖 |
| 缓存 | 基础实现 | 可进一步优化 |

---

## ✅ 集成完整性

| 环节 | 状态 | 说明 |
|------|------|------|
| 应用入口 | ✅ | 正确的初始化序列 |
| 配置加载 | ✅ | 完整的文件 I/O 和解析 |
| SDK 集成 | ✅ | 配置正确传递给 SDK |
| 索引注册 | ✅ | 9 个解析器已注册 |
| 存储初始化 | ✅ | 存储适配器已传入 |
| 环境变量 | ✅ | 覆盖机制完整 |

---

## 🔗 跨模块依赖

```
apps/cli-app
  ├─ 依赖: @wf-agent/config-processor (配置加载)
  ├─ 依赖: @wf-agent/sdk/api (SDK 创建)
  └─ 依赖: @wf-agent/types (类型定义)

packages/config-processor
  ├─ 依赖: @wf-agent/sdk/api (使用解析器/处理器)
  ├─ 依赖: @wf-agent/types (配置类型)
  └─ 依赖: @wf-agent/common-utils (文件系统工具)

sdk/api/shared/config
  ├─ 依赖: @wf-agent/types (配置类型)
  └─ 依赖: @wf-agent/common-utils (工具函数)
```

---

## 🚀 启动检查清单

在应用启动时，应该看到：
- [x] ✅ TOML 解析器已初始化
- [x] ✅ 全局配置已加载
- [x] ✅ 存储管理器已初始化
- [x] ✅ SDK 已创建并启动
- [x] ✅ 9 个索引解析器已注册
- [x] ✅ 交互处理器已初始化
- [x] ✅ DI 容器已初始化

---

## 📝 常见任务

### 添加新的配置类型
1. 在 `packages/types/src/config/` 中定义类型
2. 在 `sdk/api/shared/config/processors/` 中创建处理器
3. 在 `packages/config-processor/loader-orchestrator.ts` 中添加加载函数
4. 导出到 `packages/config-processor/src/index.ts`
5. 在应用中调用 `loadNewConfigType()`

### 添加新的配置索引类型
1. 在 `packages/config-processor/config-index-loader.ts` 中创建解析函数
2. 在 `packages/config-processor/config-index-resolver.ts` 中注册
3. 在 `packages/config-processor/src/index.ts` 中导出
4. 新的索引类型会在 `registerAllIndexResolvers()` 中自动注册

### 调试配置加载
```bash
# 启用详细日志
modular-agent --verbose --debug command

# 指定配置文件
modular-agent --config ./my-config.toml command

# 使用环境变量
WF_AGENT_DEBUG=true modular-agent command
```

---

## 🔍 主要导出/注册点

### config-processor 导出
```typescript
// 通用 I/O
export { readConfigFile, loadConfigFile, tryLoadConfigFile }

// 域特定加载器
export { loadMetricsConfig, loadTimeoutConfig, ... (15个) }

// 索引加载器
export { loadIndexFile, resolveWorkflowIndex, ... (9个) }

// MCP/Skill 加载
export { loadMcpSettings, loadSkillConfig, ... }

// 索引注册
export { registerAllIndexResolvers }
```

### SDK 注册点
```typescript
// 在 sdk/api/shared/config/config-index.ts 中
registerResolver("workflows", resolveWorkflowIndex)
registerResolver("llm_profiles", resolveLLMProfileIndex)
// ... 7个其他
```

---

## 📊 配置类型汇总

- 🔸 基础设施配置: 9个 (metrics, timeout, storage, output, 等)
- 🔸 模板配置: 4个 (prompt, node, hook, trigger)
- 🔸 LLM 配置: 3个 (llm-profile, checkpoint, script)
- 🔸 工具配置: 3个 (glob, list-files, read-file)
- 🔸 脚本配置: 5个 (script, executor, flow, interactive, validator)
- **总计**: 24+ 个配置类型已支持

---

## 🎓 理解关键概念

### 索引 vs 配置
- **索引** (Index): 目录文件，列表结构 (`.wf-agent/workflows/index.json`)
- **配置** (Config): 功能配置文件 (`.wf-agent/config.toml`)

### 文件索引 vs 预设索引
- **文件索引**: 从磁盘上的 index.json 文件加载
- **预设索引**: 从内存中的预设配置动态生成

### 注册模式
SDK 提供抽象接口，config-processor 在启动时注册实现，从而实现松耦合。

---

## 🔐 安全考虑

- ✅ 环境变量覆盖提供灵活性（适合 CI/CD）
- ✅ 默认值提供安全退路
- ✅ 验证器检查配置有效性
- ⚠️ 确保敏感信息不被记录到日志中

---

> 此文档基于 2026-06-20 的代码状态。配置系统已完全集成，所有 9 个索引类型已注册，可在生产环境中使用。
