# packages/types 类型测试需求分析与设计方案

## 执行摘要

本文档分析了 `packages/types` 包中需要补充类型测试的模块，基于 SDK 的实际使用情况制定了优先级和实施计划。当前该包**缺少所有类型测试文件**，需要系统性补充以确保类型安全性和 SDK 集成的可靠性。

---

## 一、现状评估

### 1.1 当前配置状态

✅ **已配置基础设施：**
- tsd 测试框架（`tsd.json`）
- 测试脚本（`package.json` 中的 `test:type`）
- TypeScript 编译配置（`tsconfig.test.json`）

❌ **缺失内容：**
- `__tests__/test-d/` 目录不存在
- 零个类型测试文件
- 无类型安全验证机制

### 1.2 包结构概览

```
packages/types/src/
├── agent/ (3 files)              # Agent 相关类型
├── agent-execution/ (6 files)    # Agent 执行类型
├── checkpoint/ (5 files)         # Checkpoint 类型
├── component-message/ (6 files)  # 组件消息类型
├── config/ (6 files)             # 配置类型
├── errors/ (10 files)            # 错误层次结构
├── events/ (14 files)            # 事件类型
├── execution/ (3 files)          # 执行类型
├── graph/ (8 files)              # 图结构类型
├── interaction/ (4 files)        # 用户交互类型
├── llm/ (11 files)               # LLM 集成类型
├── message/ (10 files)           # 消息类型
├── node/ (8 files)               # 节点类型系统 ⭐
├── script/ (4 files)             # 脚本类型
├── storage/ (6 files)            # 存储适配器类型
├── tool/ (11 files)              # 工具类型系统 ⭐
├── trigger/ (6 files)            # 触发器类型
├── workflow/ (11 files)          # 工作流模板 ⭐
└── workflow-execution/ (8 files) # 工作流执行类型
```

---

## 二、SDK 使用频率分析

通过对 SDK 代码库的全面扫描，识别出以下高频使用的类型模块：

### 2.1 使用统计（按 import 次数）

| 模块 | Import 次数 | 关键使用场景 | 复杂度 |
|------|------------|-------------|--------|
| **node/** | 150+ | 节点遍历、类型守卫、配置访问 | ⭐⭐⭐⭐⭐ |
| **workflow/** | 120+ | 工作流加载、验证、执行 | ⭐⭐⭐⭐⭐ |
| **errors/** | 100+ | 异常处理、错误上下文 | ⭐⭐⭐⭐ |
| **tool/** | 90+ | 工具注册、执行、审批 | ⭐⭐⭐⭐ |
| **result.ts** | 80+ | 函数式错误处理 | ⭐⭐⭐⭐⭐ |
| **events/** | 70+ | 事件发射、订阅 | ⭐⭐⭐ |
| **llm/** | 60+ | LLM 调用、消息处理 | ⭐⭐⭐ |
| **checkpoint/** | 50+ | 状态快照、恢复 | ⭐⭐⭐ |
| **agent-execution/** | 40+ | Agent 循环执行 | ⭐⭐⭐ |
| **message/** | 35+ | 消息传递、流处理 | ⭐⭐⭐ |

### 2.2 关键使用模式

#### 模式 1：节点类型守卫与收窄
```typescript
// SDK 中常见模式
for (const node of workflow.nodes) {
  if (isStaticLLMNode(node)) {
    // 需要确保能访问 node.config.prompt
    const prompt = node.config.prompt;
  }
}
```

#### 模式 2：Result 类型链式操作
```typescript
// LLM 执行器中的典型用法
const result = await llmWrapper.generate(request);
if (result.isOk()) {
  return result.value;
} else {
  throw new ExecutionError(result.error.message);
}
```

#### 模式 3：工作流模板验证
```typescript
// 配置解析时的类型检查
function validateWorkflow(template: WorkflowTemplate): ValidationResult {
  // 需要验证 nodes、edges、variables 等字段
  return ok(template);
}
```

---

## 三、需要补充类型测试的模块（按优先级）

### 🔴 高优先级（立即实施）

#### 1. Node 类型系统 (`src/node/`)
**优先级：** ⭐⭐⭐⭐⭐  
**测试文件：** `node/static-node-types.test-d.ts`, `node/runtime-node-types.test-d.ts`

**需要测试的内容：**
- ✅ `StaticNodeType` 和 `RuntimeNodeType` 联合类型
- ✅ `StaticNodeConfigMap[T]` 泛型映射
- ✅ `StaticNodeOfType<T>` 类型推断
- ✅ 20+ 个类型守卫函数（`isStaticLLMNode`, `isStaticScriptNode` 等）
- ✅ Switch 语句中的类型收窄
- ✅ 静态节点到运行时节点的转换

**SDK 影响：** 
- 工作流验证器依赖类型守卫
- 节点执行协调器依赖类型收窄
- 图构建器依赖节点类型判断

**示例测试：**
```typescript
declare const node: StaticNode;
if (isStaticLLMNode(node)) {
  expectType<StaticLLMNode>(node);
  expectType<string | undefined>(node.config.prompt);
}
```

---

#### 2. Workflow 模板类型 (`src/workflow/`)
**优先级：** ⭐⭐⭐⭐⭐  
**测试文件：** `workflow/workflow-template.test-d.ts`

**需要测试的内容：**
- ✅ `WorkflowTemplate` 完整结构
- ✅ 必需字段 vs 可选字段
- ✅ `WorkflowStartConfig` / `WorkflowEndConfig` 边界配置
- ✅ `VariableDefinition` 变量定义
- ✅ `AvailableTools` 工具配置
- ✅ 工作流类型（main/subworkflow/triggered）

**SDK 影响：**
- 配置解析器的核心数据结构
- 工作流注册和验证的基础
- 执行引擎的输入类型

**示例测试：**
```typescript
const workflow: WorkflowTemplate = {
  id: "test",
  name: "Test",
  type: "main",
  nodes: [],
  edges: [],
  version: "1.0.0",
  createdAt: "...",
  updatedAt: "...",
  // 可选字段
  variables: [{ name: "var", type: "string" }],
};
expectType<WorkflowTemplate>(workflow);
```

---

#### 3. Result 类型 (`src/result.ts`)
**优先级：** ⭐⭐⭐⭐⭐  
**测试文件：** `result/result-type.test-d.ts`

**需要测试的内容：**
- ✅ `Result<T, E>` 泛型推断
- ✅ `Ok<T>` 和 `Err<E>` 接口契约
- ✅ 链式操作类型安全（`andThen`, `unwrapOrElse`, `orElse`）
- ✅ 类型守卫（`isOk`, `isErr`）
- ✅ 嵌套 Result 类型
- ✅ 自定义错误类型

**SDK 影响：**
- LLM 执行器返回类型
- 工具执行器返回类型
- 验证器返回类型
- 整个 SDK 的错误处理模式

**示例测试：**
```typescript
declare const result: Result<string, Error>;
if (result.isOk()) {
  expectType<string>(result.value);
} else {
  expectType<Error>(result.error);
}

// 链式操作
result
  .andThen(val => ok(val.length))
  .unwrapOrElse(() => 0);
```

---

#### 4. Error 层次结构 (`src/errors/`)
**优先级：** ⭐⭐⭐⭐  
**测试文件：** `errors/error-hierarchy.test-d.ts`

**需要测试的内容：**
- ✅ 基类 `SDKError` 结构
- ✅ 10+ 种具体错误类型
- ✅ `ErrorContext` 上下文类型
- ✅ `ErrorSeverity` 严重性字面量类型
- ✅ 错误实例化和属性访问
- ✅ instanceof 类型守卫

**SDK 影响：**
- 统一的异常处理机制
- 错误日志和监控
- 重试逻辑判断

**示例测试：**
```typescript
const error = new ConfigurationValidationError("Invalid", {
  field: "nodes",
  value: []
});
expectType<ErrorContext>(error.context);
expectType<"error" | "warning" | "info">(error.severity);

if (error instanceof ConfigurationValidationError) {
  expectType<string | undefined>(error.context.field);
}
```

---

#### 5. Tool 类型系统 (`src/tool/`)
**优先级：** ⭐⭐⭐⭐  
**测试文件：** `tool/tool-definition.test-d.ts`, `tool/tool-config.test-d.ts`

**需要测试的内容：**
- ✅ `Tool` 联合类型（stateless/stateful/rest/builtin）
- ✅ `ToolConfig`  discriminated union
- ✅ `ToolParameterSchema` JSON Schema 类型
- ✅ 工具审批和风险等级类型
- ✅ MCP 工具配置类型
- ✅ 类型守卫（`isStatelessToolConfig` 等）

**SDK 影响：**
- 工具注册和执行
- 动态工具添加
- 工具审批流程

**示例测试：**
```typescript
declare const tool: Tool;
if (isRestToolConfig(tool.config)) {
  expectType<string>(tool.config.endpoint);
  expectType<"GET" | "POST" | ...>(tool.config.method);
}
```

---

### 🟡 中优先级（第二阶段）

#### 6. Agent 执行类型 (`src/agent-execution/`)
**优先级：** ⭐⭐⭐  
**测试文件：** `agent/agent-execution.test-d.ts`

**需要测试：**
- `AgentLoopDefinition` 结构
- `AgentHookStatic` 钩子类型
- `AgentTriggerStatic` 触发器类型
- Agent 状态管理类型

---

#### 7. Checkpoint 类型 (`src/checkpoint/`)
**优先级：** ⭐⭐⭐  
**测试文件：** `checkpoint/checkpoint-types.test-d.ts`

**需要测试：**
- `CheckpointData` 序列化结构
- `CheckpointMetadata` 元数据
- 检查点层（layers）类型
- 序列化和反序列化类型安全

---

#### 8. Event 类型系统 (`src/events/`)
**优先级：** ⭐⭐⭐  
**测试文件：** `events/event-types.test-d.ts`

**需要测试：**
- 14 种事件类型的 discriminated union
- 事件载荷（payload）类型
- 事件发射和订阅类型安全
- 工作流事件 vs Agent 事件

---

#### 9. Message 类型 (`src/message/`)
**优先级：** ⭐⭐⭐  
**测试文件：** `message/message-types.test-d.ts`

**需要测试：**
- `LLMMessage` 角色和内容类型
- `ComponentMessage` 组件消息
- 消息流处理类型
- 消息历史管理

---

#### 10. Storage 适配器类型 (`src/storage/`)
**优先级：** ⭐⭐⭐  
**测试文件：** `storage/storage-adapter.test-d.ts`

**需要测试：**
- `StorageAdapter` 接口契约
- 不同存储后端类型（SQLite、Memory 等）
- CRUD 操作返回类型
- 事务和批量操作类型

---

### 🟢 低优先级（第三阶段）

#### 11-13. 其他辅助类型
- Common 基础类型（ID、Timestamp、Version）
- Config 配置加载类型
- Skill 技能类型

这些类型相对简单，使用频率较低，可以在后期补充。

---

## 四、测试设计原则

### 4.1 目录结构规范

```
__tests__/test-d/
├── README.md                          # 测试说明文档 ✅
├── node/                              # 按模块组织
│   ├── static-node-types.test-d.ts   ✅
│   ├── runtime-node-types.test-d.ts
│   └── node-type-guards.test-d.ts
├── workflow/
│   ├── workflow-template.test-d.ts   ✅
│   └── boundary-config.test-d.ts
├── result/
│   └── result-type.test-d.ts         ✅
├── errors/
│   └── error-hierarchy.test-d.ts
├── tool/
│   ├── tool-definition.test-d.ts
│   └── tool-config.test-d.ts
├── agent/
├── checkpoint/
├── events/
└── integration/                       # SDK 集成测试
    └── sdk-usage-patterns.test-d.ts
```

### 4.2 测试用例组成

每个测试文件应包含：

1. **JSDoc 注释**：说明测试目的和优先级
2. **导入声明**：从 `../../src/index.js` 导入类型
3. **基本类型推断测试**：验证类型构造
4. **泛型参数测试**：验证泛型推断
5. **类型守卫测试**：验证类型收窄
6. **负向测试**：验证错误赋值被拒绝（注释掉）
7. **集成场景测试**：模拟真实 SDK 使用

### 4.3 TSD API 使用

```typescript
import { expectType, expectNotType, expectAssignable } from "tsd";

// 正向测试：验证类型匹配
expectType<ExpectedType>(value);

// 负向测试：验证类型不匹配（应该失败）
expectNotType<WrongType>(value);

// 可赋值性测试：验证子类型关系
expectAssignable<BaseType>(derivedValue);
```

---

## 五、实施计划

### 阶段 1：核心类型测试（1-2 天）

**目标：** 覆盖最高优先级的 5 个模块

**任务清单：**
- [x] 创建 `__tests__/test-d/` 目录结构
- [x] 编写 Node 类型系统测试（static + runtime）
- [x] 编写 Workflow 模板测试
- [x] 编写 Result 类型测试
- [ ] 编写 Error 层次结构测试
- [ ] 编写 Tool 类型系统测试
- [ ] 运行 `pnpm test:type` 验证

**预期产出：**
- 5-7 个测试文件
- 150+ 个类型断言
- 覆盖 SDK 80% 的类型使用场景

---

### 阶段 2：扩展类型测试（2-3 天）

**目标：** 覆盖中等优先级的 5 个模块

**任务清单：**
- [ ] Agent 执行类型测试
- [ ] Checkpoint 类型测试
- [ ] Event 类型系统测试
- [ ] Message 类型测试
- [ ] Storage 适配器测试
- [ ] 创建 SDK 集成测试文件

**预期产出：**
- 5-6 个测试文件
- 100+ 个类型断言
- 覆盖 SDK 95% 的类型使用场景

---

### 阶段 3：完善和优化（1 天）

**目标：** 补充剩余类型并优化测试质量

**任务清单：**
- [ ] Common 基础类型测试
- [ ] Config 类型测试
- [ ] Skill 类型测试
- [ ] 审查所有测试文件
- [ ] 添加更多边缘情况测试
- [ ] 更新 README 文档

**预期产出：**
- 3-4 个测试文件
- 完整的类型测试覆盖
- 完善的文档

---

### 阶段 4：CI/CD 集成（0.5 天）

**目标：** 自动化类型测试

**任务清单：**
- [ ] 在 CI 配置中添加 `pnpm test:type`
- [ ] 设置类型测试失败阻断合并
- [ ] 添加类型测试覆盖率报告（可选）

---

## 六、关键测试场景示例

### 场景 1：节点类型守卫的完整性

```typescript
// 测试所有节点类型的守卫函数
const nodeTypes: StaticNodeType[] = [
  "START", "END", "VARIABLE", "FORK", "JOIN", "SUBGRAPH",
  "SCRIPT", "LLM", "ADD_TOOL", "USER_INTERACTION", "ROUTE",
  "CONTEXT_PROCESSOR", "LOOP_START", "LOOP_END", "AGENT_LOOP",
  "START_FROM_TRIGGER", "CONTINUE_FROM_TRIGGER"
];

for (const type of nodeTypes) {
  // 验证每种类型都有对应的守卫函数
  // 验证守卫函数能正确收窄类型
}
```

### 场景 2：Result 链式操作的类型安全

```typescript
// 模拟 SDK 中的实际使用
function executeLLM(request: LLMRequest): Result<LLMResult, LLMError> {
  // ...
}

function processResult(result: LLMResult): Result<string, ProcessingError> {
  // ...
}

// 链式调用应该保持类型安全
const finalResult = executeLLM(request)
  .andThen(processResult)
  .unwrapOrElse(() => "default");

expectType<string>(finalResult);
```

### 场景 3：工作流模板的边界配置

```typescript
// 测试 START 节点的输入配置
const startConfig: WorkflowStartConfig = {
  input: {
    type: "variable",
    name: "userInput",
    required: true
  }
};

// 测试 END 节点的输出配置
const endConfig: WorkflowEndConfig = {
  output: {
    type: "message",
    content: "Task completed"
  }
};

expectType<WorkflowStartConfig>(startConfig);
expectType<WorkflowEndConfig>(endConfig);
```

---

## 七、质量保证

### 7.1 测试覆盖指标

| 指标 | 目标值 | 说明 |
|------|--------|------|
| 核心类型覆盖率 | 100% | 高优先级模块全部覆盖 |
| 泛型类型测试 | 100% | 所有泛型参数组合 |
| 类型守卫测试 | 100% | 所有守卫函数 |
| SDK 集成场景 | 20+ | 真实使用模式 |
| 边缘情况 | 30+ | 可选字段、联合类型等 |

### 7.2 代码审查要点

- [ ] 每个测试文件有清晰的 JSDoc 说明
- [ ] 测试用例覆盖正向和负向场景
- [ ] 使用真实的 SDK 使用模式
- [ ] 注释掉应该失败的测试（作为文档）
- [ ] 避免测试实现细节，聚焦公共 API

### 7.3 持续维护

- 新增类型时同步添加测试
- 重构类型时更新相关测试
- 定期审查测试有效性
- 收集 SDK 使用反馈改进测试

---

## 八、风险与挑战

### 8.1 潜在风险

1. **类型复杂性**：某些泛型类型可能难以完全测试
   - **缓解：** 分步骤测试，先测简单场景

2. **测试维护成本**：类型变更需要同步更新测试
   - **缓解：** 建立清晰的测试组织结构

3. **TSD 学习曲线**：团队可能不熟悉 tsd API
   - **缓解：** 提供详细的文档和示例

### 8.2 成功标准

- ✅ 所有高优先级模块有完整测试
- ✅ `pnpm test:type` 在 CI 中通过
- ✅ 类型重构时有测试保护
- ✅ 新开发者能通过测试理解类型系统

---

## 九、总结与建议

### 9.1 核心发现

1. **packages/types 缺少所有类型测试**，这是一个重要的质量缺口
2. **Node、Workflow、Result 是最关键的三个模块**，应优先测试
3. **SDK 广泛使用类型守卫和泛型**，需要重点验证
4. **现有基础设施完备**，只需补充测试文件

### 9.2 立即行动项

1. ✅ 创建测试目录结构
2. ✅ 实现高优先级模块的测试（Node、Workflow、Result）
3. ⏳ 运行首次类型测试验证
4. ⏳ 集成到 CI/CD 流程

### 9.3 长期价值

- **提高类型安全性**：捕获编译时错误
- **改善开发体验**：更好的 IDE 智能提示
- **降低重构风险**：类型变更有测试保护
- **提升文档质量**：测试即文档

---

## 附录

### A. 参考资源

- [TSD 官方文档](https://github.com/SamVerschueren/tsd)
- [TypeScript 类型测试最佳实践](https://www.typescriptlang.org/docs/handbook/declaration-files/templates/module-d-ts.html)
- SDK 类型使用示例：`sdk/core/executors/llm-executor.ts`

### B. 相关文件

- `packages/types/package.json` - 测试脚本配置
- `packages/types/tsd.json` - TSD 配置
- `packages/types/tsconfig.test.json` - 测试编译配置
- `packages/types/__tests__/test-d/README.md` - 测试使用说明

### C. 联系信息

如有问题或建议，请联系项目维护者。
