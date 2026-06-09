# 特殊配置处理方式分析

本文档分析 MCP、Skill 和基础设施配置的特殊处理方式，这些配置类型由于使用场景的特殊性，需要不同于常规配置的加载和管理机制。

## 配置索引设计原则

### 核心原则：索引文件仅包含路径映射

**设计理念**：
- 索引文件**仅包含路径模式**（minimal and simple）
- 元数据在具体配置文件中定义（single source of truth）
- 支持通配符模式灵活匹配
- 索引只是"指针"，指向实际配置文件

**示例**：
```json
// configs/llm-profiles/index.json
{
  "version": "1.0",
  "type": "llm_profiles",
  "paths": [
    "./openai/*.toml",
    "./anthropic/*.toml",
    "./gemini-native/*.toml"
  ]
}
```

### 预设机制概述

对于需要预设支持的配置类型（MCP、Skill、基础设施），采用统一的预设模式：

- **索引文件** (`index.json`)：仅包含路径映射，指向各个预设的定义文件
- **预设定义文件** (`<name>.json`)：包含该预设的具体配置内容
- **选择机制**：通过 setting 指定使用的预设名称，匹配对应的预设定义文件

预设分为两类：
- **单文件预设**（如 MCP）：每个预设是一个独立的 JSON 文件，内容即为配置本身
- **多文件预设**（如 Skill）：预设定义文件中包含路径映射，指向多个实际配置文件

**优势**：
1. **元数据一致性**：元数据只在一处定义，避免索引与实际文件不一致
2. **索引精简**：索引文件极小，易于维护
3. **灵活扩展**：添加新配置只需创建文件，无需修改索引
4. **通配符支持**：`./*.toml`、`./**/*.toml` 等模式灵活匹配

### 通配符模式支持

| 模式 | 说明 | 示例 |
|------|------|------|
| `./*.toml` | 当前目录所有 TOML 文件 | `./gpt-5.5.profile.toml` |
| `./subdir/*.toml` | 子目录所有 TOML 文件 | `./openai/gpt-5.5.profile.toml` |
| `./**/*.toml` | 递归所有 TOML 文件 | `./openai/v1/gpt-5.5.profile.toml` |
| `./prefix-*.toml` | 前缀匹配 | `./gpt-*.toml` |

### 加载流程

```
1. 加载 index.json
2. 展开通配符路径 → 获取所有匹配的文件路径
3. 逐个加载配置文件
4. 从配置文件中提取元数据（id, name, description, tags 等）
5. 返回 ResolvedIndex（包含完整元数据）
```

---

## 1. MCP 配置的特殊处理

### 1.1 设计背景

MCP (Model Context Protocol) 配置具有以下特点：
- 配置量通常不大，但需要支持多个预设配置
- 需要在 workflow 和 agent 执行实例级别进行集成，而非仅全局/项目级
- 项目级配置优先级高于全局配置

### 1.2 当前实现

当前 MCP 配置采用两级加载模式：

```
全局配置: {settingsDir}/mcp-settings.json
项目配置: .wf/mcp.json (优先级高)
         .agent/mcp.json (备选)
```

加载逻辑：
1. 先加载全局配置
2. 再加载项目配置
3. 合并配置（项目配置覆盖全局配置）

### 1.3 预设机制设计（待实现）

为支持多个 MCP 预设，采用索引文件 + 预设定义文件的结构：

```json
// configs/mcp/index.json - MCP 预设索引（仅包含路径映射）
{
  "version": "1.0",
  "type": "mcp_presets",
  "paths": [
    "./presets/*.json"
  ]
}
```

```json
// configs/mcp/presets/default.json - 预设定义文件（名称即预设 ID）
{
  "id": "default",
  "name": "Default MCP Servers",
  "description": "Standard MCP server configuration",
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/allowed/dir"]
    }
  }
}
```

```json
// configs/mcp/presets/development.json - 另一个预设
{
  "id": "development",
  "name": "Development Preset",
  "description": "MCP servers for development environment",
  "tags": ["dev"],
  "mcpServers": {
    "filesystem": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/workspace"] },
    "database": { "command": "node", "args": ["mcp-db.js"] }
  }
}
```

**设计要点**：
- `index.json` 仅包含 `paths`，通过 glob 模式扫描所有预设定义文件
- 每个预设定义文件是一个独立 JSON，文件名（不含扩展名）即预设 ID
- 预设内容直接内联在预设定义文件中（单文件预设）
- 加载时扫描路径，根据文件名匹配预设名称

### 1.4 实例级 MCP 集成

Workflow 和 Agent 执行实例通过 setting 指定使用的 MCP 预设名称：

```toml
# workflow 配置示例
[mcp]
preset = "development"  # 匹配 presets/development.json
```

```toml
# agent loop 配置示例
[mcp]
preset = "production"
```

### 1.5 优先级规则

```
实例级配置（内联 MCP servers） > 实例级 preset 引用 > 项目级配置 > 全局配置
```

加载流程：
1. 加载 `configs/mcp/index.json`，获取预设路径模式
2. 扫描路径下的所有 `*.json` 文件，按文件名索引预设
3. 根据 setting 中的 `preset` 名称匹配对应的预设定义文件
4. 加载该预设定义文件中的 `mcpServers` 配置

---

## 2. Skill 配置的特殊处理

### 2.1 设计背景

Skill 配置的特点：
- 需要支持多个技能集合的索引
- Workflow 和 Agent 可能需要加载不同的技能子集
- 需要平衡便捷性（自动加载）和精确控制（手动指定）

### 2.2 当前实现

当前 Skill 配置采用与 MCP 相同的两级加载模式：

```
全局配置: {settingsDir}/skill-settings.json
项目配置: .wf/skills.json (优先级高)
         .agent/skills.json (备选)
```

配置内容：
```json
{
  "paths": ["./skills", "./custom-skills"],
  "autoScan": true
}
```

### 2.3 预设机制设计（待实现）

Skill 属于**多文件预设**，采用索引文件 + 集合定义文件（含路径映射）的结构：

```json
// configs/skills/index.json - 技能预设索引（仅包含路径映射）
{
  "version": "1.0",
  "type": "skill_presets",
  "paths": [
    "./collections/*.json"
  ]
}
```

```json
// configs/skills/collections/default.json - 集合定义文件（多文件预设，含路径映射）
{
  "id": "default",
  "name": "Default Skills",
  "description": "Standard skill collection",
  "paths": [
    "./definitions/*.md",
    "../built-in-skills/*.md"
  ]
}
```

```json
// configs/skills/collections/code-review.json - 另一个技能集合
{
  "id": "code-review",
  "name": "Code Review Skills",
  "description": "Skills for code review workflow",
  "tags": ["code", "review"],
  "paths": [
    "./definitions/code-review/*.md",
    "./definitions/troubleshoot.md"
  ]
}
```

**设计要点**：
- `index.json` 仅包含 `paths`，扫描所有集合定义文件
- 集合定义文件使用 `paths` 指向实际技能定义文件（`.md`），而非内联技能元数据
- 文件名（不含扩展名）即集合名称，用于 setting 匹配
- 每个技能定义文件（`.md`）包含完整的技能元数据和内容

### 2.4 Workflow/Agent 级别的技能指定

通过 setting 指定使用的技能集合：

```toml
[skills]
collection = "code-review"  # 匹配 collections/code-review.json
# 或指定多个技能 ID（从集合中筛选）
skillIds = ["troubleshoot", "code-review"]
```

### 2.5 加载流程**

```
1. 加载 configs/skills/index.json，获取集合路径模式
2. 扫描路径下的所有 *.json 文件，按文件名索引集合
3. 根据 setting 中的 collection 名称匹配对应的集合定义文件
4. 加载集合定义文件中的 paths，展开所有技能定义文件路径
5. 逐个加载技能定义文件，提取元数据和内容
6. 按 skillIds 筛选（如指定）
```

---

## 3. 基础设施配置的特殊处理

### 3.1 设计背景

基础设施配置包括：
- Metrics 配置
- Timeout 配置
- Storage 配置
- Output 配置
- File Checkpoint 配置
- Presets 配置

这些配置的特点：
- 通常只需要一个活跃配置
- 但可能需要多个预设用于不同环境（开发、测试、生产）
- 配置量相对固定

### 3.2 当前实现

当前每个基础设施配置类型都有独立的加载器：

```typescript
loadMetricsConfig(configPaths: string[])
loadTimeoutConfig(configPaths: string[])
loadStorageConfig(configPaths: string[])
// ...
```

采用多路径尝试模式，按优先级加载第一个存在的配置文件。

### 3.3 预设机制设计（待实现）

基础设施也属于**多文件预设**，采用索引文件 + 预设定义文件（含路径映射）的结构：

```json
// configs/infrastructure/index.json - 基础设施预设索引（仅包含路径映射）
{
  "version": "1.0",
  "type": "infrastructure_presets",
  "paths": [
    "./presets/*.json"
  ]
}
```

```json
// configs/infrastructure/presets/development.json - 预设定义文件（多文件预设，含路径映射）
{
  "id": "development",
  "name": "Development Environment",
  "description": "Infrastructure configs for development",
  "tags": ["dev"],
  "files": {
    "metrics": "./configs/dev/metrics.toml",
    "timeout": "./configs/dev/timeout.toml",
    "storage": "./configs/dev/storage.toml",
    "output": "./configs/dev/output.toml",
    "fileCheckpoint": "./configs/dev/file-checkpoint.toml"
  }
}
```

```json
// configs/infrastructure/presets/production.json
{
  "id": "production",
  "name": "Production Environment",
  "description": "Infrastructure configs for production",
  "tags": ["prod"],
  "files": {
    "metrics": "./configs/prod/metrics.toml",
    "timeout": "./configs/prod/timeout.toml",
    "storage": "./configs/prod/storage.toml",
    "output": "./configs/prod/output.toml",
    "fileCheckpoint": "./configs/prod/file-checkpoint.toml"
  }
}
```

**设计要点**：
- `index.json` 仅包含 `paths`，扫描所有预设定义文件
- 每个预设定义文件使用 `files` 字段指定各配置项的路径
- 文件名（不含扩展名）即预设名称，用于 setting 匹配
- 各配置项文件（如 `metrics.toml`）是独立的配置文件

### 3.4 预设切换机制

通过 setting 或环境变量指定活跃预设：

```toml
# 在项目配置中指定
[infrastructure]
preset = "production"
```

```bash
# 环境变量方式
export WF_INFRA_PRESET=production
```

### 3.5 加载流程

1. 加载 `configs/infrastructure/index.json`，获取预设路径模式
2. 扫描路径下的所有 `*.json` 文件，按文件名索引预设
3. 根据 setting 或环境变量中的 `preset` 名称匹配对应的预设定义文件
4. 加载预设定义文件中的 `files` 映射，获取各配置项路径
5. 逐个加载各配置项文件，应用默认值合并

---

## 4. 实现优先级建议

### Phase 1: 基础功能（已完成）
- ✅ 补充常规配置类型的加载器函数
- ✅ 设计并实现配置索引文件类型
- ✅ 实现索引文件加载逻辑

### Phase 2: MCP 预设机制（待实现）
- [ ] 设计 MCP 预设索引格式（`index.json` + 单文件预设）
- [ ] 实现预设加载器：扫描路径 → 按文件名索引 → 按名称匹配
- [ ] 支持实例级 MCP preset 引用
- [ ] 实现优先级合并逻辑

### Phase 3: Skill 预设机制（待实现）
- [ ] 设计 Skill 预设索引格式（`index.json` + 集合定义文件 + 路径映射）
- [ ] 实现 Skill 集合加载器：索引 → 集合文件 → 展开路径 → 加载技能
- [ ] 支持实例级 skill collection 指定
- [ ] 支持 skillIds 筛选

### Phase 4: 基础设施预设（待实现）
- [ ] 设计基础设施预设索引格式（`index.json` + 预设定义文件 + files 映射）
- [ ] 实现预设加载器：按名称匹配 → 加载 files 映射 → 逐项加载
- [ ] 支持 setting 和环境变量指定预设
- [ ] 更新现有加载器以支持预设模式

### Phase 5: CLI 工具支持（待实现）
- [ ] 索引文件生成命令（按路径模式扫描）
- [ ] 预设列表查看命令
- [ ] 预设切换命令
- [ ] 索引验证命令

---

## 5. 配置类型对比总结

| 配置类型 | 索引方式 | 预设类型 | 预设支持 | 实例级集成 | 优先级层次 |
|---------|---------|---------|---------|-----------|-----------|
| LLM Profiles | 路径索引 | — | 否 | 否 | 项目 > 全局 |
| Workflows | 路径索引 | — | 否 | 否 | 项目 > 全局 |
| Node Templates | 路径索引 | — | 否 | 否 | 项目 > 全局 |
| Scripts | 路径索引 | — | 否 | 否 | 项目 > 全局 |
| Prompt Templates | 路径索引 | — | 否 | 否 | 项目 > 全局 |
| Agent Loops | 路径索引 | — | 否 | 否 | 项目 > 全局 |
| **MCP** | 预设索引 | 单文件预设 | **是** | **是** | 实例 > 项目 > 全局 |
| **Skills** | 预设索引 | 多文件预设（集合+路径映射） | **是** | **是** | 实例 > 项目 > 全局 |
| **Infrastructure** | 预设索引 | 多文件预设（files 映射） | **是** | 否 | 项目 > 全局 |

---

## 6. 相关文件

- 类型定义：`packages/types/src/config/config-index.ts`
- 索引加载器：`apps/config-processor/src/config-index-loader.ts`
- 常规加载器：`apps/config-processor/src/loader-orchestrator.ts`
- MCP 加载器：`apps/config-processor/src/mcp-settings-loader.ts`
- Skill 加载器：`apps/config-processor/src/skill-settings-loader.ts`
