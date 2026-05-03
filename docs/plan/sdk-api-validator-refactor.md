# SDK API 配置校验层重构方案

## 1. 背景与目标

### 1.1 现状分析
目前 `sdk/api/shared/config/validators` 目录中存在多份针对特定配置类型（如 LLM Profile、Prompt Template）的校验逻辑。这些逻辑与 `sdk/core`、`sdk/workflow` 以及 `packages/types` 中的定义存在冗余，且缺乏统一的 Workflow 配置文件校验入口。

### 1.2 设计原则
*   **单一真理来源 (Single Source of Truth)**：所有校验逻辑应基于 `packages/types` 中定义的 Zod Schema。
*   **职责下沉**：深度业务校验（如图结构完整性）应由核心模块（`sdk/workflow`）负责，API 层仅负责轻量级格式校验和结果聚合。
*   **架构一致性**：`sdk/api` 作为顶层门面，不应包含独立的业务校验实现，而应作为核心校验能力的编排者。

## 2. 重构方案

### 2.1 目录结构调整
建议将 `sdk/api/shared/config/validators` 简化为“适配器层”，或直接移除该目录，将校验逻辑统一迁移至以下位置：

| 配置类型 | 校验逻辑归属 | 说明 |
| :--- | :--- | :--- |
| **Workflow** | `sdk/workflow/validation/workflow-validator.ts` | 保留现有的深度图校验，增加前置 Schema 校验。 |
| **LLM Profile** | `packages/types/src/llm/profile-schema.ts` | 使用 Zod Schema 进行静态校验。 |
| **Prompt Template** | `packages/prompt-templates/src/schema.ts` | 在模板包内完成校验。 |
| **通用工具** | `sdk/utils/validation-helpers.ts` | 提取通用的字段检查函数。 |

### 2.2 Workflow Validator 实现方案

为了保持 API 层的简洁性，Workflow 的校验将分为两个阶段：

#### 阶段一：轻量级 Schema 校验 (在 `api` 层)
在解析 TOML/JSON 后，立即使用 `packages/types` 中的 `WorkflowDefinitionSchema` 进行基础字段校验。

```typescript
// sdk/api/shared/config/processors/workflow.ts
import { WorkflowDefinitionSchema } from '@wf-agent/types';

export function validateWorkflowConfigFile(config: unknown) {
  const result = WorkflowDefinitionSchema.safeParse(config);
  if (!result.success) {
    return err(result.error.errors.map(e => new SchemaValidationError(e.message)));
  }
  return ok(result.data);
}
```

#### 阶段二：深度业务校验 (在 `workflow` 层)
调用现有的 `WorkflowValidator` 进行节点引用、循环检测等业务逻辑校验。

```typescript
// sdk/api/shared/config/processors/workflow.ts
import { WorkflowValidator } from '../../../../workflow/validation/workflow-validator.js';

export function validateWorkflowDeep(workflow: WorkflowDefinition) {
  const validator = new WorkflowValidator();
  return validator.validate(workflow);
}
```

### 2.3 其他配置类型的处理
*   **LLM Profile**: 删除 `llm-profile-validator.ts`。在 `processors/llm-profile.ts` 中直接调用 `LLMProfileSchema.parse()`。
*   **Prompt Template**: 删除 `prompt-template-validator.ts`。在 `processors/prompt-template.ts` 中调用模板包的校验函数。

## 3. 实施步骤

1.  **完善 Schema**：确保 `packages/types` 中所有配置类型都有对应的 Zod Schema。
2.  **迁移逻辑**：将 `validators` 目录下的复杂逻辑迁移至核心模块或替换为 Schema 调用。
3.  **更新导出**：修改 `sdk/api/shared/config/index.ts`，确保导出的校验函数指向新的实现。
4.  **清理冗余**：删除 `sdk/api/shared/config/validators` 目录下不再需要的文件。

## 4. 预期收益

*   **代码量减少**：消除约 60% 的重复校验代码。
*   **维护性提升**：修改校验规则只需在 `types` 或核心模块中进行。
*   **性能优化**：通过分层校验，快速拦截格式错误，避免进入耗时的深度校验流程。
