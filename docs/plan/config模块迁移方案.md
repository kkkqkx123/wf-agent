# wf-config 模块迁移方案

## 一、迁移范围

### 1.1 来源 TS 模块

| TS 模块 | 职责 | 迁移目标 |
|---------|------|---------|
| `packages/sdk/api/shared/config/parsers/` | TOML/JSON 解析 | `wf-config::parser` |
| `packages/sdk/api/shared/config/processors/` | 领域 parse/validate/merge/transform | `wf-config::processor` |
| `packages/sdk/api/shared/config/config-utils.ts` | 参数替换 (`{{parameters.xxx}}`) | `wf-config::processor::substitute` |
| `packages/sdk/api/shared/config/env-mapping.ts` | 环境变量覆盖 | `wf-config::env` |
| `packages/sdk/api/shared/config/validator.ts` | 通用验证工具 | `wf-config::validator` |
| `packages/sdk/api/shared/config/config-index.ts` | 索引解析器注册 | `wf-config::index` |
| `packages/config-processor/src/` | 文件 I/O 编排 | `wf-config::loader` |

### 1.2 不迁移的内容

| 内容 | 原因 |
|------|------|
| `packages/types/src/config/` 的 Zod schema | 类型定义已在 `wf-types` 中存在 |
| `packages/runtime/src/config/` 的 app 特定逻辑 | 属于 `wf-runtime`，不在 `wf-config` 范围 |
| `apps/cli-app/src/config/` / `apps/server/src/config/` | 应用层，属于 Phase 6 |

### 1.3 已存在于 `wf-types` 的类型（直接复用）

以下类型已在 `wf-types` 中定义，`wf-config` 直接引用，不重复定义：

- `config/schemas.rs` → `StorageConfig`, `CompressionConfig`
- `config/metrics.rs` → `MetricsConfig`
- `config/output.rs` → `OutputConfig`
- `config/timeout.rs` → `TimeoutConfig`
- `llm/profile.rs` → `LlmProfile`
- `workflow/config.rs` → `WorkflowConfig`
- `workflow/definition.rs` → `WorkflowTemplate`, `WorkflowDefinition`
- `node/configs/` → 所有节点配置类型
- `script/` → `ScriptExecutorConfig`, `SandboxConfig` 等
- `prompt_template.rs` → `PromptTemplate`
- `trigger/template.rs` → `TriggerTemplate`
- `workflow/hook_template.rs` → `HookTemplate`
- `agent/` → `AgentConfig`, `AgentDefinition`

---

## 二、Crate 结构设计

### 2.1 依赖关系

```
wf-types ← wf-common
    ↓
wf-config ← wf-storage (仅 loader 模块需要文件 I/O，实际依赖 wf-common 的 IoError)
```

实际依赖：`wf-types` + `wf-common`。不依赖 `wf-storage`（文件 I/O 用 `std::fs` + `tokio::fs`）。

### 2.2 模块布局

```
crates/wf-config/
├── Cargo.toml
├── src/
│   ├── lib.rs              # include!("wf_config.rs") 或 pub mod
│   ├── wf_config.rs        # 根模块，汇总 re-export
│   ├── error.rs            # ConfigError + ConfigResult
│   ├── parser.rs           # TOML/JSON 解析
│   ├── processor/          # 领域处理（parse/validate/merge/transform）
│   │   ├── mod.rs
│   │   ├── llm_profile.rs
│   │   ├── node_template.rs
│   │   ├── trigger.rs
│   │   ├── hook.rs
│   │   ├── script.rs
│   │   ├── prompt.rs
│   │   ├── agent_loop.rs
│   │   ├── workflow.rs
│   │   ├── infrastructure.rs  # metrics/timeout/storage/output/sandbox/presets
│   │   └── substitute.rs      # 参数替换
│   ├── validator.rs        # 通用验证工具
│   ├── env.rs              # 环境变量覆盖
│   ├── index.rs            # 索引解析器注册
│   └── loader.rs           # 文件 I/O 编排（可选，也可归入 wf-runtime）
```

### 2.3 新增依赖

| 依赖 | 用途 | 版本 |
|------|------|------|
| `toml` | TOML 解析（替代 `@iarna/toml`） | 0.8 |
| `regex` | 参数替换正则 | 1 |
| `glob` | glob 模式匹配（替代 `matchGlobPattern`） | 0.3 |

`serde_json` 已在 workspace 中，无需新增。

---

## 三、TS → Rust 设计调整

### 3.1 解析层：从 Zod 到 serde

**TS 模式**：Zod schema 运行时验证
```typescript
const LLMProfileSchema = z.object({...});
const result = LLMProfileSchema.safeParse(data);
```

**Rust 模式**：serde 反序列化即验证
```rust
// wf-types 中已定义
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmProfile { ... }

// parser 中直接反序列化
let profile: LlmProfile = toml::from_str(content)?;
```

**调整要点**：
- Zod 的 `refine`/`superfine` 业务规则需手动实现为 `validate_*` 函数
- Zod 的 `transform` 直接由 serde 的 `Deserialize` 实现替代
- 可选字段用 `Option<T>` + `#[serde(default)]` 替代 Zod 的 `.optional().default()`

### 3.2 验证层：从 Zod 到手动实现

**TS 模式**：声明式 schema + 自动错误收集
```typescript
function validateLLMProfile(config: unknown): ValidationResult<LlmProfile> {
  return validateConfig(config, LLMProfileSchema);
}
```

**Rust 模式**：显式验证函数返回 `ConfigResult<T>`
```rust
pub fn validate_llm_profile(profile: &LlmProfile) -> ConfigResult<()> {
    if profile.model.is_empty() {
        return Err(ConfigError::ValidationError("model cannot be empty".into()));
    }
    Ok(())
}
```

**调整要点**：
- 简单字段校验（非空、范围、枚举）→ 手动 `if` + `ConfigError`
- 跨字段校验（如 allowList ∩ blockList = ∅）→ 独立验证函数
- 批量验证 → 返回 `Vec<ConfigError>` 而非抛首个错误

### 3.3 Merge/Defaults：从 spread 到 `Default` trait

**TS 模式**：
```typescript
function mergeTimeoutWithDefaults(user: Partial<TimeoutConfig>): Required<TimeoutConfig> {
  return { ...DEFAULT_TIMEOUT_CONFIG, ...user };
}
```

**Rust 模式**：
```rust
impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            workflow_execution_completion: 30000,
            node_completion: 30000,
            max_allowed: 300000,
        }
    }
}

// 使用 serde(default) 自动填充缺失字段
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeoutConfig {
    #[serde(default = "default_workflow_timeout")]
    pub workflow_execution_completion: i64,
    // ...
}
```

**调整要点**：
- 简单 merge → `#[serde(default)]` + `Default` trait
- 需要路径解析的 merge（如 `FileCheckpointConfig` 的 `dbPath`）→ 手动 `merge_*` 函数
- 深度 merge（如 `MetricsConfig` 的 collector 级别）→ 手动函数或 `serde(flatten)`

### 3.4 参数替换：从 `structuredClone` + 递归到 Visitor 模式

**TS 模式**：
```typescript
function substituteParameters<T>(obj: T, parameters?: Record<string, unknown>): T {
  const cloned = structuredClone(obj);
  // 递归替换 {{parameters.xxx}}
}
```

**Rust 模式**：
```rust
pub fn substitute_parameters(
    config: &mut WorkflowConfig,
    parameters: &HashMap<String, String>,
) -> ConfigResult<()> {
    // 直接修改，无需 clone（所有权已转移）
    for node in &mut config.nodes {
        if let Some(ref mut name) = node.name {
            *name = replace_parameters(name, parameters)?;
        }
    }
    Ok(())
}
```

**调整要点**：
- 无需 `structuredClone` — Rust 所有权系统天然支持 in-place 修改
- 正则替换用 `regex` crate：`/\{\{parameters\.([a-zA-Z0-9_.-]+)\}\}/`
- 返回 `ConfigResult<()>` 而非新对象（调用方决定 clone 时机）

### 3.5 环境变量覆盖：从 `process.env` 到 `std::env::var`

**TS 模式**：
```typescript
function applyEnvOverrides<T>(config: T, mapping: EnvMapping<T>): T {
  const result = { ...config };
  for (const [key, entry] of Object.entries(mapping)) {
    const value = process.env[entry.env];
    if (value !== undefined) result[key] = entry.parser(value);
  }
  return result;
}
```

**Rust 模式**：
```rust
pub fn apply_env_overrides(
    config: &mut AppConfig,
    mapping: &[EnvMapping],
) -> ConfigResult<()> {
    for entry in mapping {
        match std::env::var(entry.env_var) {
            Ok(value) => {
                let parsed = (entry.parser)(&value)?;
                (entry.apply)(config, parsed);
            }
            Err(std::env::VarError::NotPresent) => {
                if entry.required {
                    return Err(ConfigError::MissingEnvVar(entry.env_var.into()));
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
```

**调整要点**：
- `EnvMapping` 用 struct + function pointer 替代 TS 的 `Record<string, EnvMappingEntry>`
- 支持 `string`/`int`/`float`/`boolean`/`json`/`list` 标准解析器
- `to_env_name` 函数保留（camelCase → SCREAMING_SNAKE_CASE）

### 3.6 索引解析器：从 `Map<IndexType, Function>` 到 `HashMap` + `Box<dyn Fn>`

**TS 模式**：
```typescript
const RESOLVE_FUNCTIONS = new Map<IndexType, IndexResolver>();
function registerResolver(type: IndexType, resolver: IndexResolver): void {
  RESOLVE_FUNCTIONS.set(type, resolver);
}
```

**Rust 模式**：
```rust
type IndexResolver = Box<dyn Fn(&Path) -> ConfigResult<ResolvedIndex> + Send + Sync>;

struct IndexRegistry {
    resolvers: HashMap<IndexType, IndexResolver>,
}

impl IndexRegistry {
    pub fn register(&mut self, ty: IndexType, resolver: IndexResolver) -> ConfigResult<()> {
        self.resolvers.insert(ty, resolver).map(|_| ()).ok_or_else(|| /* already exists */)
    }

    pub async fn resolve(&self, ty: &IndexType, path: &Path) -> ConfigResult<ResolvedIndex> {
        self.resolvers.get(ty).ok_or(ConfigError::UnregisteredIndex)?(path).await
    }
}
```

**调整要点**：
- 使用 `DashMap` 支持并发注册（对标 TS 的单线程 Map）
- resolver 返回 `Pin<Box<dyn Future>>` 或 `async-trait`
- `createIndexResolver` 工厂函数保留，封装 index → glob → load → metadata 流程

### 3.7 文件 I/O 编排：从 `Promise.all` 到 `join!`

**TS 模式**：
```typescript
async function loadInfrastructureConfigs(projectRoot: string): Promise<InfrastructureConfigBundle> {
  const [metrics, timeout, storage, output, presets, sandbox] = await Promise.all([
    loadMetricsConfig(paths),
    loadTimeoutConfig(paths),
    // ...
  ]);
  return { metrics, timeout, storage, output, presets, sandbox };
}
```

**Rust 模式**：
```rust
pub async fn load_infrastructure_configs(
    project_root: &Path,
) -> ConfigResult<InfrastructureConfigBundle> {
    let (metrics, timeout, storage, output, presets, sandbox) = tokio::join!(
        load_metrics_config(project_root),
        load_timeout_config(project_root),
        load_storage_config(project_root),
        load_output_config(project_root),
        load_presets_config(project_root),
        load_sandbox_config(project_root),
    )?;
    Ok(InfrastructureConfigBundle { metrics, timeout, storage, output, presets, sandbox })
}
```

**调整要点**：
- `tokio::join!` 替代 `Promise.all`
- 文件读取用 `tokio::fs::read_to_string`
- glob 展开用 `glob` crate 替代自定义 `matchGlobPattern`
- 路径操作使用 `std::path::PathBuf`

---

## 四、分步实施计划

### Phase A: 解析层 + 验证层（2 天）

| 任务 | 产出 | 测试 |
|------|------|------|
| `parser.rs` — TOML/JSON 解析 + `getConfigFormatFromPath` | `parse_toml`, `parse_json`, `parse_config_file` | 单元测试：格式检测、解析正确性、错误处理 |
| `validator.rs` — 通用验证工具 | `validate_required`, `validate_range`, `validate_enum` | 单元测试：各验证器通过/失败场景 |
| `error.rs` — 错误类型 | `ConfigError` enum | — |

### Phase B: 领域 Processor（3 天）

| 任务 | 产出 | 测试 |
|------|------|------|
| `processor/llm_profile.rs` | `parse_llm_profile`, `validate_llm_profile` | 解析 + 验证 |
| `processor/node_template.rs` | `parse_node_template`, `validate_node_template` | 解析 + 验证 |
| `processor/trigger.rs` | `parse_trigger_template`, `validate_trigger_template` | 解析 + 验证 |
| `processor/hook.rs` | `parse_hook_template`, `validate_hook_template` | 解析 + 验证 |
| `processor/script.rs` | `parse_script`, `validate_script` | 解析 + 验证 |
| `processor/prompt.rs` | `parse_prompt_template`, `validate_prompt_template` | 解析 + 验证 |
| `processor/agent_loop.rs` | `parse_agent_loop`, `validate_agent_loop` | 解析 + 验证 |
| `processor/workflow.rs` | `parse_workflow`, `transform_workflow` | 解析 + 参数替换 + 节点/边转换 |
| `processor/infrastructure.rs` | 所有 `merge_*_with_defaults` | 默认值填充正确性 |
| `processor/substitute.rs` | `substitute_parameters` | 参数替换、缺失参数保留原占位符 |

### Phase C: 环境变量 + 索引（1.5 天）

| 任务 | 产出 | 测试 |
|------|------|------|
| `env.rs` | `apply_env_overrides`, `EnvMappingBuilder`, 标准解析器 | 各类型解析、缺失值、默认值 |
| `index.rs` | `IndexRegistry`, `create_index_resolver` | 注册、解析、未注册错误 |

### Phase D: 文件 I/O 编排（1.5 天）

| 任务 | 产出 | 测试 |
|------|------|------|
| `loader.rs` | `load_config_file`, `load_infrastructure_configs`, `load_mcp_settings`, `load_skill_config` | 端到端：从文件到完整配置 |

### Phase E: 集成测试（1 天）

| 任务 | 产出 |
|------|------|
| 临时 config 文件 fixture | `tests/fixtures/` |
| 端到端测试 | 覆盖所有 loader + processor 路径 |

---

## 五、工作量估算

| 阶段 | 工作量 | 累计 |
|------|--------|------|
| Phase A: 解析 + 验证基础 | 2 天 | 2 天 |
| Phase B: 领域 Processor | 3 天 | 5 天 |
| Phase C: 环境变量 + 索引 | 1.5 天 | 6.5 天 |
| Phase D: 文件 I/O 编排 | 1.5 天 | 8 天 |
| Phase E: 集成测试 | 1 天 | 9 天 |
| **总计** | **~9 天（~1.8 周）** | — |

原方案估算 0.5 周偏乐观，实际需要约 2 周（含测试）。

---

## 六、风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| TOML 解析行为差异 | 配置兼容性问题 | 使用 `toml` crate 的 `Value` 中间表示，对比 TS `@iarna/toml` 输出 |
| Zod refine 规则遗漏 | 验证不完整 | 逐函数对照 TS 实现，编写 property-based test |
| glob 语义差异 | 文件发现不一致 | 使用 `glob` crate，测试覆盖 `**`, `*`, `*.ext` 模式 |
| 异步文件 I/O 性能 | 配置加载慢 | `tokio::fs` + `join!` 并行加载，无需额外缓存 |

---

## 七、与原方案（2.3 wf-config）的差异

| 维度 | 原方案 | 本方案 |
|------|--------|--------|
| 范围 | 仅 `ConfigProcessor` + `Orchestrator` | 完整覆盖 SDK 纯逻辑 + config-processor I/O |
| 类型定义 | 未明确 | 复用 `wf-types` 已有类型，不重复定义 |
| 验证策略 | "继承 TS 验证逻辑" | serde 反序列化 + 手动 `validate_*` 函数 |
| 依赖 | 未规划 | 新增 `toml`, `regex`, `glob` |
| 工作量 | 0.5 周 | ~2 周 |
