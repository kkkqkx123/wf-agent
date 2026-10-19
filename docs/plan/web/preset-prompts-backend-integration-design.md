# 预设提示词后端化与配置模块设计

## 1. 背景

首页 Composer 当前使用前端硬编码的 `SUGGESTIONS` 作为建议提示词。这套文本与后端已有的
`Template`、`SystemPromptFragment`、自定义提示词能力完全脱节，形成第二套提示词来源，
且无法自定义、无法与内置提示词/片段集成。

本阶段仅给出设计，不执行代码修改。

## 2. 设计目标

1. 提示词的唯一事实来源在后端：前端只选择、预览、提交，不持有业务提示词文本。
2. 预设提示词成为可管理资源，与内置提示词模板、系统提示词片段、自定义提示词统一。
3. 支持用户自定义提示词，并可与 agent 模板、workflow、模型 profile 关联。
4. 配置模块提供浏览、启用/禁用、编辑、导入导出等管理能力。

## 3. 后端资源模型

### 3.1 预设提示词

预设提示词建议作为 `Template` 的一种类别（`category = "preset"` 或新增 `kind = "prompt"`），
复用现有模板的变量、片段、描述、使用计数等字段。最小字段集：

- `id`、`name`、`description`
- `category`（preset / builtin / custom）
- `content`（支持 `{{variable}}` 与 `{{fragments}}`）
- `variables`、`fragments`
- `enabled`、`isDefault`、`tags`

### 3.2 与现有能力集成

- 内置提示词：通过 `wf-resource` 预置模板注册，作为只读 baseline。
- 提示词片段：通过 `SystemPromptFragment` 与 `FragmentCompositionConfig` 组合。
- 自定义提示词：用户创建的模板与内置模板同构，仅来源/权限不同。
- Agent 运行：继续使用 `system_prompt` 与 `system_prompt_template_id`，预设提示词可直接作为
  `system_prompt_template_id` 的候选。

## 4. API 设计（建议）

- `GET /api/v1/prompts/presets`：列出可用预设提示词（支持 category/enabled 过滤）。
- `GET /api/v1/prompts/presets/{id}`：详情。
- `POST /api/v1/prompts/presets`：创建自定义预设。
- `PUT /api/v1/prompts/presets/{id}`：更新。
- `DELETE /api/v1/prompts/presets/{id}`：删除自定义预设（内置只读）。
- `POST /api/v1/prompts/presets/{id}/enable|disable`：启停。
- `POST /api/v1/prompts/presets/{id}/usage`：使用计数。

前端 `ModelPicker` 式的 `PromptPresetPicker` 通过上述 API 获取列表。

## 5. 前端集成

### 5.1 Composer

- 移除硬编码 `SUGGESTIONS`。
- 在输入框上方按需显示“可用预设提示词” chips，点击后填入输入框（可追加而非覆盖）。
- 空状态不展示建议；由后端列表决定是否显示。

### 5.2 配置模块

- 资源页新增“提示词预设”面板：浏览、搜索、启用/禁用、编辑、导入导出。
- 设置页提供默认预设提示词选择与自动应用开关。
- Agent/Workflow 配置可引用预设提示词 id。

## 6. 迁移步骤

1. 后端新增预设提示词资源与 API。
2. 前端新增服务层与 picker，替换硬编码 suggestions。
3. 配置模块接入管理能力。
4. 移除前端常量与相关测试中的硬编码断言。

## 7. 风险与非目标

- 本阶段不实现；仅设计。
- 需要保证内置预设的只读语义与版本兼容。
- 自定义提示词的权限与多用户隔离留待后续设计。