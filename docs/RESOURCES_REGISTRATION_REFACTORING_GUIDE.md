# Resources 注册重构指南

## 快速概览

### 现状问题

| 问题 | 表现 | 严重程度 |
|------|------|--------|
| **分散** | 注册逻辑分布在 5 个位置 | 🔴 高 |
| **动态导入** | 使用 `await import()` | 🔴 高 |
| **不一致** | predefined/custom 设计模式不同 | 🟡 中 |
| **关注点混淆** | 资源模块混合了创建和注册 | 🟡 中 |

### 推荐方案

**方案 A：完全集中化** ✅ 推荐

将所有注册逻辑移到 `registration/` 目录。

---

## 实施路线

### Phase 1：创建新的注册模块

**1. 创建 `registration/predefined-registration.ts`**

将 `predefined/registration.ts` 的逻辑复制过来：

```typescript
/**
 * Unified Registration Entry for Predefined Content
 */

import type { TriggerTemplateRegistry } from "@sdk/shared/registry/trigger-template-registry.js";
import type { WorkflowRegistry } from "@sdk/workflow/stores/workflow-registry.js";
import type { ToolRegistry } from "@sdk/shared/registry/tool-registry.js";
import type { PresetsConfig } from "@wf-agent/sdk/resources";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";

// Import from submodules
import { 
  registerPredefinedTriggers, 
  unregisterPredefinedTriggers 
} from "../predefined/trigger/index.js";

import { 
  registerPredefinedWorkflows, 
  unregisterPredefinedWorkflows 
} from "../predefined/workflow/index.js";

import { 
  registerPredefinedTools, 
  unregisterPredefinedTools 
} from "../predefined/tools/registration.js";

const logger = createContextualLogger({ component: "PredefinedRegistration" });

export interface PredefinedRegistrationResult {
  triggers: {
    success: string[];
    failures: Array<{ triggerName: string; error: string }>;
  };
  workflows: {
    success: string[];
    failures: Array<{ workflowId: string; error: string }>;
  };
  tools: {
    success: string[];
    failures: Array<{ toolId: string; error: string }>;
  };
}

function mapLlmSummaryConfig(cc?: {
  prompt?: string;
  timeout?: number;
  maxTriggers?: number;
}): { compressionPrompt?: string; timeout?: number; maxTriggers?: number } | undefined {
  if (!cc) return undefined;
  return {
    compressionPrompt: cc.prompt,
    timeout: cc.timeout,
    maxTriggers: cc.maxTriggers,
  };
}

export async function registerPredefinedContent(
  triggerRegistry: TriggerTemplateRegistry,
  workflowRegistry: WorkflowRegistry,
  toolService: ToolRegistry,
  presets?: PresetsConfig,
  skipIfExists: boolean = true,
): Promise<PredefinedRegistrationResult> {
  // ... 复制 predefined/registration.ts 中的逻辑
}

export async function unregisterPredefinedContent(
  // ... 复制unregister相关逻辑
) {
  // ...
}
```

**2. 创建 `registration/custom-registration.ts`**

将 `custom/registration.ts` 的逻辑复制过来：

```typescript
/**
 * Custom Resources Registration
 */

import type { ToolRegistry } from "@sdk/shared/registry/tool-registry.js";
import type { TriggerTemplateRegistry } from "@sdk/shared/registry/trigger-template-registry.js";
import { createContextualLogger } from "@sdk/utils/contextual-logger.js";
import { toSdkTool } from "@sdk/services/tools/utils.js";
import type {
  CustomToolDefinition,
  CustomTriggerDefinition,
  CustomPromptDefinition,
  CustomResources,
} from "../custom/types.js";

const logger = createContextualLogger({ component: "CustomResourcesRegistration" });

export interface CustomResourcesRegistrationResult {
  tools: {
    success: string[];
    failures: Array<{ toolId: string; error: string }>;
  };
  triggers: {
    success: string[];
    failures: Array<{ triggerName: string; error: string }>;
  };
  prompts: {
    success: string[];
    failures: Array<{ promptId: string; error: string }>;
  };
}

export function registerCustomTools(
  // ... 复制自定义工具注册逻辑
) { }

export function registerCustomTriggers(
  // ... 复制自定义触发器注册逻辑
) { }

export function registerCustomPrompts(
  // ... 复制自定义提示注册逻辑
) { }

export function registerCustomResources(
  // ... 复制统一的自定义资源注册逻辑
) { }
```

**3. 创建 `registration/application-registration.ts` (预留)**

```typescript
/**
 * Application Resources Registration (Reserved for Future Use)
 */

import type { ToolRegistry } from "@sdk/shared/registry/tool-registry.js";
import type { TriggerTemplateRegistry } from "@sdk/shared/registry/trigger-template-registry.js";
import type { WorkflowRegistry } from "@sdk/workflow/stores/workflow-registry.js";

export interface ApplicationResourcesRegistrationResult {
  tools: {
    success: string[];
    failures: Array<{ toolId: string; error: string }>;
  };
}

export async function registerApplicationResources(
  triggerRegistry: TriggerTemplateRegistry,
  workflowRegistry: WorkflowRegistry,
  toolRegistry: ToolRegistry,
  applicationResources?: unknown,
): Promise<ApplicationResourcesRegistrationResult> {
  // TODO: Implement application resource registration
  return {
    tools: { success: [], failures: [] },
  };
}
```

### Phase 2：更新 Orchestrator

**更新 `registration/orchestrator.ts`**

```typescript
/**
 * Unified Resources Registration Orchestrator
 * 
 * Coordinates registration of resources from all three pipelines using static imports.
 */

import { createContextualLogger } from "@sdk/utils/contextual-logger.js";
import type { TriggerTemplateRegistry } from "@sdk/shared/registry/trigger-template-registry.js";
import type { WorkflowRegistry } from "@sdk/workflow/stores/workflow-registry.js";
import type { ToolRegistry } from "@sdk/shared/registry/tool-registry.js";
import type { PresetsConfig, CustomResources } from "@wf-agent/sdk/resources";

// ✅ 改为静态导入
import { 
  registerPredefinedContent,
  type PredefinedRegistrationResult
} from "./predefined-registration.js";

import { 
  registerCustomResources,
  type CustomResourcesRegistrationResult
} from "./custom-registration.js";

import { 
  registerApplicationResources,
  type ApplicationResourcesRegistrationResult
} from "./application-registration.js";

import type { UnifiedRegistrationResult } from "./types.js";

const logger = createContextualLogger({ component: "UnifiedResourcesOrchestrator" });

export async function registerAllResources(
  triggerRegistry: TriggerTemplateRegistry,
  workflowRegistry: WorkflowRegistry,
  toolRegistry: ToolRegistry,
  presets?: PresetsConfig,
  customResources?: CustomResources,
  applicationResources?: unknown,
  skipIfExists: boolean = true,
): Promise<UnifiedRegistrationResult> {
  logger.info("Starting unified resources registration");

  const result: UnifiedRegistrationResult = {
    predefined: {
      tools: { success: [], failures: [] },
      triggers: { success: [], failures: [] },
      workflows: { success: [], failures: [] },
    },
    custom: {
      tools: { success: [], failures: [] },
      triggers: { success: [], failures: [] },
      prompts: { success: [], failures: [] },
    },
  };

  // =====================================================================
  // Pipeline 1: Register Predefined Resources
  // =====================================================================
  try {
    logger.debug("Starting predefined resources registration pipeline");
    const predefinedResult = await registerPredefinedContent(
      triggerRegistry,
      workflowRegistry,
      toolRegistry,
      presets,
      skipIfExists,
    );
    result.predefined = predefinedResult;
    logger.info("Predefined resources registration completed", {
      tools: predefinedResult.tools.success.length,
      triggers: predefinedResult.triggers.success.length,
      workflows: predefinedResult.workflows.success.length,
    });
  } catch (error) {
    logger.error("Failed during predefined resources registration", { error });
  }

  // =====================================================================
  // Pipeline 2: Register Custom Resources
  // =====================================================================
  if (customResources) {
    try {
      logger.debug("Starting custom resources registration pipeline");
      const customResult = registerCustomResources(
        {
          toolRegistry,
          triggerRegistry,
        },
        customResources,
      );
      result.custom = customResult;
      logger.info("Custom resources registration completed", {
        tools: customResult.tools.success.length,
        triggers: customResult.triggers.success.length,
        prompts: customResult.prompts.success.length,
      });
    } catch (error) {
      logger.error("Failed during custom resources registration", { error });
    }
  }

  // =====================================================================
  // Pipeline 3: Register Application Resources
  // =====================================================================
  if (applicationResources) {
    try {
      logger.debug("Starting application resources registration pipeline");
      const appResult = await registerApplicationResources(
        triggerRegistry,
        workflowRegistry,
        toolRegistry,
        applicationResources,
      );
      result.application = appResult;
      logger.info("Application resources registration completed");
    } catch (error) {
      logger.error("Failed during application resources registration", { error });
    }
  }

  logger.info("Unified resources registration completed");
  return result;
}
```

**关键改变**：
- ❌ 移除所有 `await import()`
- ✅ 使用静态导入
- ✅ 直接调用函数（不需要动态加载）
- ✅ 改进的错误处理

### Phase 3：创建兼容层（保留向后兼容性）

**更新 `predefined/registration.ts`**

```typescript
/**
 * Compatibility Layer for Predefined Content Registration
 * 
 * @deprecated Use @wf-agent/sdk/resources/registration/predefined-registration instead
 * This file maintains backward compatibility. All logic has been moved to registration/ directory.
 */

export { 
  registerPredefinedContent,
  unregisterPredefinedContent,
  type PredefinedRegistrationResult,
} from "../registration/predefined-registration.js";
```

**更新 `custom/registration.ts`**

```typescript
/**
 * Compatibility Layer for Custom Resources Registration
 * 
 * @deprecated Use @wf-agent/sdk/resources/registration/custom-registration instead
 * This file maintains backward compatibility. All logic has been moved to registration/ directory.
 */

export {
  registerCustomTools,
  registerCustomTriggers,
  registerCustomPrompts,
  registerCustomResources,
  type CustomResourcesRegistrationResult,
} from "../registration/custom-registration.js";
```

### Phase 4：更新 Index 文件

**更新 `registration/index.ts`**

```typescript
/**
 * Unified Resources Registration Module
 * 
 * Exports registration functions and types for:
 * - Predefined resources (SDK built-in)
 * - Custom resources (user-provided)
 * - Application resources (runtime-defined)
 * - Unified orchestrator
 */

export type {
  PredefinedRegistrationResult,
  CustomResourcesRegistrationResult,
  ApplicationResourcesRegistrationResult,
  UnifiedRegistrationResult,
} from "./types.js";

export { registerAllResources } from "./orchestrator.js";

// Direct access to individual registration functions (if needed)
export { 
  registerPredefinedContent,
  unregisterPredefinedContent,
  type PredefinedRegistrationResult as PredefinedResult,
} from "./predefined-registration.js";

export {
  registerCustomResources,
  type CustomResourcesRegistrationResult as CustomResult,
} from "./custom-registration.js";

export {
  registerApplicationResources,
  type ApplicationResourcesRegistrationResult as ApplicationResult,
} from "./application-registration.js";
```

### Phase 5：渐进式迁移调用点

逐步更新所有导入位置：

**更新 `sdk-instance.ts` - 可选（目前仍可用）**

从：
```typescript
import { registerPredefinedContent } from "../../../resources/predefined/registration.js";
```

改为：
```typescript
import { registerPredefinedContent } from "../../../resources/registration/predefined-registration.js";
```

或保持原样使用兼容层：
```typescript
import { registerPredefinedContent } from "../../../resources/predefined/registration.js"; // 兼容层会重定向
```

---

## 迁移检查清单

- [ ] 创建 `registration/predefined-registration.ts`
- [ ] 创建 `registration/custom-registration.ts`
- [ ] 创建 `registration/application-registration.ts`
- [ ] 更新 `registration/orchestrator.ts`（使用静态导入）
- [ ] 更新 `registration/index.ts`
- [ ] 创建兼容层：`predefined/registration.ts`
- [ ] 创建兼容层：`custom/registration.ts`
- [ ] 运行单元测试
- [ ] 运行集成测试
- [ ] 验证所有导入路径
- [ ] 测试向后兼容性
- [ ] 更新相关文档
- [ ] 代码审查
- [ ] 合并到主分支

---

## 测试策略

### 1. 单元测试

确保注册函数工作正常：

```bash
cd packages/sdk
pnpm test packages/sdk/resources/__tests__/registration
```

### 2. 集成测试

确保 orchestrator 正确协调三个管道：

```bash
cd packages/sdk
pnpm test packages/sdk/resources/__tests__/orchestration.int.test.ts
```

### 3. 向后兼容性测试

验证兼容层：

```bash
cd packages/sdk
pnpm test packages/sdk/resources/__tests__/compatibility
```

### 4. 端到端测试

验证 sdk-instance 正确使用新的注册模块。

---

## 预期收益

| 方面 | 改进 |
|------|------|
| **代码聚集** | 从 5 个位置 → 1 个目录 |
| **导入安全** | 从动态导入 → 静态导入 |
| **类型检查** | 显著提升 |
| **可维护性** | 大幅简化 |
| **新增资源** | 更清晰的过程 |
| **IDE 支持** | 完全支持查找引用等功能 |

---

## 常见问题

### Q: 为什么要移动 predefined/tools 等的注册？

**A**: 不要移动！`predefined/tools/registration.ts` 等文件应该保留，因为它们是：
- 具体的资源类型实现
- 由 `predefined-registration.ts` 调用
- 职责明确且稳定

我们只是将 `predefined/registration.ts`（协调层）移动到 `registration/predefined-registration.ts`。

### Q: 是否需要立即迁移所有调用点？

**A**: 不需要！兼容层会自动重定向，所以现有代码继续工作。可以逐步迁移。

### Q: 如果出现问题如何回滚？

**A**: 
- 未提交：`git restore` 恢复文件
- 已提交：`git revert` 反向提交

---

## 相关文档

- [Resources 注册架构分析](./RESOURCES_REGISTRATION_ARCHITECTURE_ANALYSIS.md) - 详细分析
- [Predefined Registration](../packages/sdk/resources/predefined/registration.ts) - 当前实现
- [Custom Registration](../packages/sdk/resources/custom/registration.ts) - 当前实现
