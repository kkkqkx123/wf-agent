# 提示词构建系统设计分析报告

## 概述

本文档对 SDK 提示词构建系统的设计进行分析，重点关注两个问题：

1. **片段验证时机** — 是否应该在注册时（初始化阶段）完成片段验证，而非在运行时发现丢失
2. **级联删除机制** — 当前如何管理片段/模板之间的依赖关系，以及设计是否需要改进

---

## 一、片段验证时机分析

### 1.1 当前设计

当前系统中存在两个注册表：

| 注册表 | 类 | 注册时机 | 验证方式 |
|--------|-----|----------|----------|
| 片段注册表 | [`FragmentRegistry`](sdk/resources/predefined/prompt-templates/fragment-registry.ts:4) | 调用 `initializeFragmentRegistry()` 时批量注册 | **无验证** — 直接 `Map.set()` |
| 模板注册表 | [`PromptTemplateRegistry`](sdk/resources/predefined/template-registry.ts:22) | 调用 `register()` 时逐个注册 | 仅检查 ID 重复（日志警告） |

**运行时缺失检测**：

在 [`composeSystemPrompt()`](sdk/resources/predefined/prompts/fragments/composer.ts:36) 中，当遍历 `fragmentIds` 时，如果某个片段 ID 未注册，仅输出一条日志警告：

```typescript
// composer.ts:53-55
} else {
  logger.warn(`Fragment '${fragmentId}' not found`);
}
```

这意味着：
- 片段丢失**不会抛出错误**，仅静默跳过
- 日志可能被淹没在生产环境的其他日志中
- 调用方无法感知片段丢失

### 1.2 问题分析

#### 问题 1：注册时无结构验证

[`FragmentRegistry.register()`](sdk/resources/predefined/prompt-templates/fragment-registry.ts:7) 仅执行 `this.fragments.set(fragment.id, fragment)`，没有任何验证：

- 不验证 `fragment.id` 格式是否符合命名规范（如 `fragments.role.xxx`）
- 不验证 `fragment.content` 是否为空
- 不验证 `fragment.variables` 中声明的变量是否在 `content` 中实际使用
- 不验证 `fragment.variables` 中标记为 `required` 的变量是否有 `defaultValue`

#### 问题 2：运行时丢失不可恢复

当 `composeSystemPrompt()` 发现片段丢失时，已经处于运行时阶段：

- 无法自动恢复或加载缺失的片段
- 生成的提示词不完整，但调用方无法得知
- 对于关键片段（如角色定义），丢失会导致生成的提示词完全不可用

#### 问题 3：模板与片段之间的交叉引用无验证

[`PromptTemplate`](packages/types/src/prompt-template.ts:9) 类型定义了 `fragments?: string[]` 字段，表示一个模板可以引用多个片段。但：

- 注册模板时**不验证**引用的片段 ID 是否存在于 `FragmentRegistry` 中
- 注册片段时**不验证**是否有模板引用了该片段
- 这种交叉引用断裂只有在运行时才会暴露

### 1.3 改进建议

#### 建议 1：在 FragmentRegistry.register() 中添加基础验证

```typescript
register(fragment: SystemPromptFragment): void {
  // 验证 ID 格式
  if (!fragment.id || typeof fragment.id !== "string") {
    throw new Error(`Fragment ID is required`);
  }
  
  // 验证内容非空
  if (!fragment.content || typeof fragment.content !== "string") {
    throw new Error(`Fragment '${fragment.id}' content is required`);
  }
  
  // 验证变量声明与实际使用的一致性
  if (fragment.variables && fragment.variables.length > 0) {
    for (const variable of fragment.variables) {
      const placeholder = `{{${variable.name}}}`;
      const isUsed = fragment.content.includes(placeholder);
      if (!isUsed && variable.required) {
        logger.warn(
          `Fragment '${fragment.id}' declares required variable '${variable.name}' ` +
          `but it is not used in the content`
        );
      }
    }
  }
  
  // 检查重复
  if (this.fragments.has(fragment.id)) {
    logger.warn(`Fragment '${fragment.id}' already exists, will be overwritten`);
  }
  
  this.fragments.set(fragment.id, fragment);
}
```

#### 建议 2：在初始化时进行完整性检查

```typescript
function initializeFragmentRegistry(): void {
  fragmentRegistry.registerAll(ALL_PREDEFINED_FRAGMENTS);
  
  // 验证所有预定义片段都已注册
  const missingIds = ALL_PREDEFINED_FRAGMENTS
    .filter(f => !fragmentRegistry.has(f.id))
    .map(f => f.id);
  
  if (missingIds.length > 0) {
    logger.error(`Failed to register fragments: ${missingIds.join(", ")}`);
  }
}
```

#### 建议 3：添加跨注册表引用验证

在 `PromptTemplateRegistry.register()` 中，如果模板的 `fragments` 字段非空，验证这些片段 ID 是否已注册：

```typescript
register(template: PromptTemplate): void {
  // 验证引用的片段是否存在
  if (template.fragments && template.fragments.length > 0) {
    for (const fragmentId of template.fragments) {
      if (!fragmentRegistry.has(fragmentId)) {
        logger.warn(
          `Template '${template.id}' references fragment '${fragmentId}' ` +
          `which is not registered`
        );
      }
    }
  }
  
  // ... 原有逻辑
}
```

#### 建议 4：提供严格模式选项

为 `composeSystemPrompt()` 添加一个 `strict` 选项，在严格模式下，片段丢失时抛出错误而非仅日志警告：

```typescript
export function composeSystemPrompt(
  config: FragmentCompositionConfig,
  fragmentVariables?: Map<string, Record<string, unknown>>,
  strict: boolean = false,
): string {
  // ...
  for (const fragmentId of config.fragmentIds) {
    const fragment = fragmentRegistry.get(fragmentId);
    if (fragment) {
      // ... 正常处理
    } else {
      if (strict) {
        throw new Error(`Fragment '${fragmentId}' not found in registry`);
      }
      logger.warn(`Fragment '${fragmentId}' not found`);
    }
  }
  // ...
}
```

### 1.4 改进优先级

| 改进项 | 优先级 | 影响范围 | 破坏性 |
|--------|--------|----------|--------|
| `register()` 基础字段验证 | 高 | 仅 FragmentRegistry | 低（新增验证，不改变接口） |
| 初始化完整性检查 | 高 | 仅初始化流程 | 低（新增日志，不改变行为） |
| 跨注册表引用验证 | 中 | PromptTemplateRegistry | 低（新增日志警告） |
| 严格模式 | 中 | composeSystemPrompt 接口 | 中（新增参数，向后兼容） |
| 变量-内容一致性验证 | 低 | FragmentRegistry | 低（新增警告） |

---

## 二、级联删除机制分析

### 2.1 当前设计

当前系统中与"级联删除"相关的机制非常有限：

#### 2.1.1 FragmentRegistry 的删除能力

[`FragmentRegistry`](sdk/resources/predefined/prompt-templates/fragment-registry.ts:4) 提供了 `clear()` 方法（清空所有片段），但**没有提供单个删除方法**（如 `unregister(id)`）：

```typescript
clear(): void {
  this.fragments.clear();
}
```

#### 2.1.2 PromptTemplateRegistry 的删除能力

[`PromptTemplateRegistry`](sdk/resources/predefined/template-registry.ts:22) 提供了 `unregister()` 和 `clear()` 方法：

```typescript
unregister(id: string): boolean {
  return this.templates.delete(id);
}

clear(): void {
  this.templates.clear();
  this.initialized = false;
}
```

但 `unregister()` **仅删除自身**，不处理任何依赖关系。

#### 2.1.3 依赖关系现状

系统中存在以下依赖关系：

1. **PromptTemplate.fragments → SystemPromptFragment** — 模板可以引用片段（通过 `fragments?: string[]` 字段）
2. **PromptTemplate 之间的继承** — 通过 `TemplateComposition` 类型定义（`baseTemplateId + overrides`），但当前**没有任何代码使用此机制**
3. **预定义片段列表 → 具体片段** — `ALL_PREDEFINED_FRAGMENTS` 数组硬编码了所有片段
4. **预定义片段组合 → 片段 ID** — `ASSISTANT_SYSTEM_PROMPT_FRAGMENTS` 和 `CODER_SYSTEM_PROMPT_FRAGMENTS` 硬编码了片段 ID 列表

### 2.2 问题分析

#### 问题 1：无依赖追踪

当前没有任何机制追踪注册表项之间的依赖关系：

- 删除一个模板时，不知道有哪些其他模板或片段引用了它
- 删除一个片段时，不知道有哪些模板引用了它
- 无法判断删除操作是否会导致系统状态不一致

#### 问题 2：硬编码的依赖关系

预定义片段组合（`ASSISTANT_SYSTEM_PROMPT_FRAGMENTS`、`CODER_SYSTEM_PROMPT_FRAGMENTS`）是硬编码的字符串数组：

```typescript
// composer.ts:94-100
export const ASSISTANT_SYSTEM_PROMPT_FRAGMENTS = [
  "fragments.role.assistant",
  "fragments.capability.general",
  "fragments.capability.general-principles",
  "fragments.constraint.general-interaction",
  "fragments.constraint.general",
];
```

这意味着：
- 如果某个片段被重命名或删除，这些硬编码的 ID 不会自动更新
- 没有编译时检查来验证这些 ID 是否有效
- 运行时才会发现片段丢失

#### 问题 3：TemplateComposition 机制未使用

[`TemplateComposition`](packages/types/src/fragment.ts:19) 类型定义了模板继承机制：

```typescript
export interface TemplateComposition {
  baseTemplateId: string;
  overrides: Partial<PromptTemplate>;
  fragmentReplacements?: Record<string, string>;
}
```

但搜索整个代码库，**没有任何地方使用 `TemplateComposition`**。这是一个已定义但未实现的设计，属于死代码。

#### 问题 4：无引用计数

两个注册表都使用简单的 `Map<string, T>` 存储，没有引用计数：

- 无法知道某个片段被多少个模板引用
- 无法知道某个模板被多少个组合引用
- 删除操作无法评估影响范围

### 2.3 改进建议

#### 建议 1：添加依赖图追踪

在注册表中添加依赖图，追踪注册项之间的引用关系：

```typescript
class FragmentRegistry {
  private fragments = new Map<string, SystemPromptFragment>();
  private dependents = new Map<string, Set<string>>(); // fragmentId → Set<templateId>
  
  register(fragment: SystemPromptFragment): void {
    // ... 验证逻辑 ...
    this.fragments.set(fragment.id, fragment);
  }
  
  /** 记录某个模板引用了此片段 */
  addDependent(fragmentId: string, templateId: string): void {
    if (!this.dependents.has(fragmentId)) {
      this.dependents.set(fragmentId, new Set());
    }
    this.dependents.get(fragmentId)!.add(templateId);
  }
  
  /** 获取引用此片段的所有模板 ID */
  getDependents(fragmentId: string): string[] {
    return Array.from(this.dependents.get(fragmentId) ?? []);
  }
  
  /** 安全删除：返回受影响的模板列表 */
  unregister(id: string): { removed: boolean; affectedTemplates: string[] } {
    const affected = this.getDependents(id);
    const removed = this.fragments.delete(id);
    this.dependents.delete(id);
    return { removed, affectedTemplates: affected };
  }
}
```

#### 建议 2：将硬编码的片段组合改为注册表引用

将 `ASSISTANT_SYSTEM_PROMPT_FRAGMENTS` 和 `CODER_SYSTEM_PROMPT_FRAGMENTS` 从硬编码数组改为注册表管理的组合定义：

```typescript
// 在 FragmentRegistry 中添加组合管理
interface FragmentComposition {
  id: string;
  fragmentIds: string[];
  description?: string;
}

class FragmentRegistry {
  private compositions = new Map<string, FragmentComposition>();
  
  registerComposition(composition: FragmentComposition): void {
    // 验证所有引用的片段 ID 都已注册
    const missingIds = composition.fragmentIds.filter(id => !this.fragments.has(id));
    if (missingIds.length > 0) {
      throw new Error(
        `Composition '${composition.id}' references unregistered fragments: ${missingIds.join(", ")}`
      );
    }
    this.compositions.set(composition.id, composition);
  }
  
  getComposition(id: string): FragmentComposition | undefined {
    return this.compositions.get(id);
  }
}
```

这样，在注册组合时就能验证所有引用的片段是否存在，将运行时错误提前到初始化阶段。

#### 建议 3：实现级联删除

当删除一个片段时，自动处理受影响的模板和组合：

```typescript
unregister(id: string): UnregisterResult {
  const affectedTemplates = this.getDependents(id);
  const affectedCompositions = this.getCompositionsUsingFragment(id);
  
  const removed = this.fragments.delete(id);
  this.dependents.delete(id);
  
  return {
    removed,
    affectedTemplates,
    affectedCompositions,
    // 是否执行级联删除由调用方决定
  };
}

// 级联删除（谨慎使用）
cascadeUnregister(id: string): CascadeResult {
  const result = this.unregister(id);
  
  // 从所有组合中移除该片段 ID
  for (const compId of result.affectedCompositions) {
    const comp = this.compositions.get(compId)!;
    comp.fragmentIds = comp.fragmentIds.filter(fid => fid !== id);
  }
  
  return {
    removedFragment: id,
    affectedTemplates: result.affectedTemplates,
    affectedCompositions: result.affectedCompositions,
  };
}
```

#### 建议 4：移除未使用的 TemplateComposition 类型

[`TemplateComposition`](packages/types/src/fragment.ts:19) 类型已定义但未使用，建议：

- 如果未来有计划实现模板继承，保留定义并添加 TODO 注释
- 如果无此计划，移除该类型以减少认知负担

### 2.4 改进优先级

| 改进项 | 优先级 | 影响范围 | 破坏性 |
|--------|--------|----------|--------|
| 移除未使用的 TemplateComposition | 高 | packages/types | 低（移除死代码） |
| 添加依赖图追踪 | 中 | FragmentRegistry + PromptTemplateRegistry | 低（新增功能，不改变接口） |
| 组合注册时验证片段存在 | 中 | FragmentRegistry | 低（新增验证） |
| 级联删除 | 低 | FragmentRegistry | 中（新增方法） |

---

## 三、总结

### 3.1 核心发现

| 问题 | 严重程度 | 建议方案 |
|------|----------|----------|
| 注册时无验证 | 高 | 在 `register()` 中添加基础字段验证 |
| 运行时片段丢失仅日志警告 | 高 | 初始化完整性检查 + 严格模式 |
| 无依赖追踪 | 中 | 添加依赖图 |
| 硬编码片段 ID 列表 | 中 | 改为注册表管理的组合 |
| TemplateComposition 未使用 | 中 | 移除或添加 TODO |
| 变量-内容一致性无验证 | 低 | 注册时可选验证 |

### 3.2 推荐实施路线

1. **立即实施**：在 `FragmentRegistry.register()` 中添加基础字段验证（ID 非空、内容非空）
2. **立即实施**：在 `initializeFragmentRegistry()` 中添加初始化完整性检查
3. **短期实施**：在 `PromptTemplateRegistry.register()` 中添加跨注册表引用验证
4. **短期实施**：移除未使用的 `TemplateComposition` 类型
5. **中期实施**：添加依赖图追踪，为级联删除奠定基础
6. **长期实施**：将硬编码片段组合改为注册表管理的组合定义

### 3.3 设计原则建议

**Fail Fast（快速失败）原则**：

当前设计倾向于"容忍错误"（片段丢失仅日志警告），这违反了 Fail Fast 原则。建议：

- 在初始化阶段（注册时）尽可能多地验证
- 在运行时（组合时）对关键片段使用严格模式
- 将可恢复的错误（如非关键片段丢失）和不可恢复的错误（如角色定义片段丢失）区分对待

**防御性编程 vs 契约式编程**：

当前设计偏向防御性编程（容忍错误、静默恢复），但对于提示词构建这种关键路径，契约式编程更为合适：

- 明确前置条件（注册时验证）
- 明确后置条件（组合结果完整性检查）
- 违反契约时快速失败，而非静默降级
