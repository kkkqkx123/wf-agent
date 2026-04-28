# 执行器包架构决策文档

## 文档信息

- **创建日期**: 2025-01-XX
- **文档版本**: 1.0
- **状态**: 已批准
- **相关包**: `@modular-agent/script-executors`, `@modular-agent/tool-executors`

## 一、执行摘要

### 决策结论

**✅ 保持 `script-executors` 和 `tool-executors` 作为独立包**

### 核心理由

1. **App 层扩展需求**: App 需要访问执行器基类进行自定义扩展
2. **SDK 封装原则**: SDK 只导出 API 层，不暴露 core 实现
3. **架构清晰性**: 执行器作为独立层，职责明确
4. **符合设计原则**: 开闭原则、单一职责原则

### 架构优势

- App 层可以访问script-executors / tool-executors (独立包)
- SDK 内部使用SDK Core Layer，对外暴露SDK API Layer给App层使用

## 二、当前架构分析

### 2.1 依赖关系

```
sdk
  ├── @modular-agent/script-executors
  └── @modular-agent/tool-executors
        ├── @modular-agent/types
        └── @modular-agent/common-utils
```

### 2.2 使用情况

**SDK 内部使用**:
- [`ScriptService`](sdk/core/services/script-service.ts:11): 使用 IScriptExecutor 接口
- [`ToolService`](sdk/core/services/tool-service.ts:12-17): 直接实例化具体执行器

**外部使用**:
- 无任何应用或其他包直接使用
- 只有 SDK 的 package.json 引用这两个包

### 2.3 代码重复分析

| 组件 | script-executors | tool-executors | 重复度 |
|------|------------------|----------------|--------|
| **ParameterValidator** | 简单字段验证 | Zod schema 验证 | 类名相同，实现不同 |
| **RetryStrategy** | 基于错误名称判断 | 基于错误类型判断 | 逻辑相似，实现不同 |
| **TimeoutController** | 简单超时控制 | 完善超时控制 + 资源清理 | 逻辑相似，细节不同 |
| **BaseExecutor** | BaseScriptExecutor | BaseExecutor | 执行流程高度相似 |

## 三、保持独立的必要性分析

### 3.1 App 层扩展需求

**场景描述**:
App 层需要自定义执行器逻辑，例如：
- 添加审计日志
- 实现缓存机制
- 增强安全检查
- 性能监控

**实现示例**:
```typescript
// apps/web-app/src/custom-executors.ts
import { BaseScriptExecutor } from '@modular-agent/script-executors';

class AuditedScriptExecutor extends BaseScriptExecutor {
  async doExecute(script: Script, context?: ExecutionContext) {
    const startTime = Date.now();
    
    // 执行前审计
    await this.auditService.logExecutionStart(script);
    
    try {
      const result = await super.doExecute(script, context);
      
      // 执行后审计
      await this.auditService.logExecutionSuccess(script, result);
      
      return result;
    } catch (error) {
      // 错误审计
      await this.auditService.logExecutionError(script, error);
      throw error;
    }
  }
}
```

### 3.2 SDK 封装原则

**设计原则**:
- SDK 只导出 API 层
- 隐藏 core 层实现细节
- 保证内部实现的稳定性

**当前实现**:
```typescript
// sdk/index.ts - 保持封装
export * from './api/index.js'; // ✅ 只导出 API 层
// ❌ 不导出 core 层，包括 ScriptService、ToolService 等
```

**如果合并到 SDK 的问题**:
```typescript
// 如果执行器在 sdk/core/executors/
// sdk/index.ts 不导出 core 层
export * from './api/index.js'; // ❌ App 无法访问 BaseScriptExecutor

// App 层尝试扩展
import { BaseScriptExecutor } from '@modular-agent/sdk'; // ❌ 编译错误
// TypeScript 错误：Module '"@modular-agent/sdk"' has no exported member 'BaseScriptExecutor'
```

### 3.3 架构清晰性

**分层架构**:
```
┌─────────────────────────────────────┐
│         App Layer                   │
│  (可以访问 script-executors 包)      │
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│         SDK API Layer               │
│  (只导出 API，隐藏 core 实现)        │
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│         SDK Core Layer              │
│  (内部实现，不对外暴露)              │
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│  script-executors / tool-executors  │
│  (独立包，App 可以直接访问)          │
└─────────────────────────────────────┘
```

## 四、未来可能需要独立的场景

### 4.1 多应用场景

**触发条件**:
- 有 2 个以上应用独立使用执行器
- 应用对执行器的功能需求差异大
- 应用需要独立版本控制

**具体例子**:
```typescript
// apps/web-app - 只需要脚本执行
import { ShellExecutor, PythonExecutor } from '@modular-agent/script-executors';

// apps/cli-tool - 只需要工具执行
import { StatelessExecutor, RestExecutor } from '@modular-agent/tool-executors';

// apps/worker-service - 需要两者
import { ScriptExecutor, ToolExecutor } from '@modular-agent/sdk';
```

### 4.2 插件化架构

**触发条件**:
- 需要支持用户自定义执行器
- 需要动态加载执行器
- 需要执行器独立分发

**插件系统示例**:
```typescript
// 用户自定义脚本执行器
import { BaseScriptExecutor } from '@modular-agent/script-executors';

class CustomScriptExecutor extends BaseScriptExecutor {
  async doExecute(script: Script, context?: ExecutionContext) {
    // 自定义实现
  }
}

// 动态注册
executorRegistry.register('custom-script', new CustomScriptExecutor());
```

### 4.3 独立版本管理

**触发条件**:
- 执行器更新频率远高于 SDK
- 需要执行器独立版本号
- 需要执行器向后兼容性

**版本策略示例**:
```json
// SDK 版本稳定
"@modular-agent/sdk": "^1.0.0"

// 执行器频繁更新
"@modular-agent/script-executors": "^2.5.0"
"@modular-agent/tool-executors": "^3.2.0"
```

### 4.4 性能优化

**触发条件**:
- 应用只需要部分执行器
- 包体积是关键指标
- 启动速度是关键指标

**按需加载示例**:
```typescript
// 只加载需要的执行器
import { ShellExecutor } from '@modular-agent/script-executors/shell';
import { PythonExecutor } from '@modular-agent/script-executors/python';

// 而不是加载整个 SDK
import { SDK } from '@modular-agent/sdk'; // 包含所有执行器
```

### 4.5 安全隔离

**触发条件**:
- 需要严格沙箱隔离
- 需要不同安全级别
- 需要独立安全策略

**沙箱隔离示例**:
```typescript
// 脚本执行器沙箱
class ScriptSandbox {
  private isolatedContext: VM.Context;
  
  async execute(script: Script): Promise<Result> {
    // 严格沙箱执行
  }
}

// 工具执行器沙箱
class ToolSandbox {
  private permissionManager: PermissionManager;
  
  async execute(tool: Tool): Promise<Result> {
    // 权限控制执行
  }
}
```

## 五、优化建议

### 5.1 统一公共组件

**目标**: 减少代码重复，提高一致性

**实施方案**:
1. 将 RetryStrategy、TimeoutController 提取到 `common-utils`
2. 两个包依赖统一的公共组件
3. 保持各自的 ParameterValidator（因为验证逻辑不同）

**目录结构**:
```
packages/common-utils/
└── executors/
    ├── RetryStrategy.ts
    └── TimeoutController.ts

packages/script-executors/
└── src/
    ├── core/
    │   └── base/
    │       ├── BaseScriptExecutor.ts
    │       └── ParameterValidator.ts
    └── scripts/
        └── ...

packages/tool-executors/
└── src/
    ├── core/
    │   └── base/
    │       ├── BaseExecutor.ts
    │       └── ParameterValidator.ts
    └── tools/
        └── ...
```

### 5.2 明确文档

**目标**: 清晰说明执行器包的用途和使用场景

**文档内容**:
1. 包的定位和职责
2. 使用场景和示例
3. 扩展指南
4. 最佳实践

### 5.3 版本管理

**目标**: 保持执行器包与 SDK 版本同步

**策略**:
1. 执行器包版本与 SDK 版本保持一致
2. 使用 workspace 协议管理依赖
3. 统一发布流程

## 六、未来演进路径

### 6.1 第一阶段：当前设计（推荐）

**特点**:
- 保持独立包设计
- App 可以直接访问执行器基类
- SDK 保持封装，只导出 API 层

**使用示例**:
```typescript
// apps/web-app/src/custom-executors.ts
import { BaseScriptExecutor } from '@modular-agent/script-executors';

class CustomScriptExecutor extends BaseScriptExecutor {
  // 自定义实现
}

// apps/web-app/src/app.ts
import { getSDK } from '@modular-agent/sdk';
import { CustomScriptExecutor } from './custom-executors';

// 使用 SDK API
const sdk = getSDK();
await sdk.scripts.execute(scriptName, options);

// 或者使用自定义执行器
const customExecutor = new CustomScriptExecutor();
const result = await customExecutor.execute(script, options);
```

### 6.2 第二阶段：插件化架构

**触发条件**:
- 有多个 App 需要自定义执行器
- 需要动态加载执行器
- 需要执行器独立分发

**架构设计**:
```
┌─────────────────────────────────────┐
│         App Layer                   │
│  ┌───────────────────────────────┐  │
│  │  CustomScriptExecutor        │  │
│  │  (插件)                       │  │
│  └───────────────────────────────┘  │
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│         SDK API Layer               │
│  ┌───────────────────────────────┐  │
│  │  ExecutorRegistry            │  │
│  │  (支持插件注册)               │  │
│  └───────────────────────────────┘  │
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│         SDK Core Layer              │
│  (内部实现，不对外暴露)              │
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│  script-executors / tool-executors  │
│  (独立包，作为基础实现)              │
└─────────────────────────────────────┘
```

**实现示例**:
```typescript
// sdk/api/core/executor-registry.ts
export class ExecutorRegistry {
  private scriptExecutors = new Map<string, IScriptExecutor>();
  private toolExecutors = new Map<string, IToolExecutor>();
  
  // 注册自定义执行器
  registerScriptExecutor(type: string, executor: IScriptExecutor): void {
    this.scriptExecutors.set(type, executor);
  }
  
  registerToolExecutor(type: string, executor: IToolExecutor): void {
    this.toolExecutors.set(type, executor);
  }
  
  // 获取执行器
  getScriptExecutor(type: string): IScriptExecutor | undefined {
    return this.scriptExecutors.get(type);
  }
  
  getToolExecutor(type: string): IToolExecutor | undefined {
    return this.toolExecutors.get(type);
  }
}

// apps/web-app/src/app.ts
import { getSDK } from '@modular-agent/sdk';
import { CustomScriptExecutor } from './custom-executors';

const sdk = getSDK();

// 注册自定义执行器
sdk.executorRegistry.registerScriptExecutor('custom-shell', new CustomScriptExecutor());

// 使用自定义执行器
await sdk.scripts.execute(scriptName, options);
```

### 6.3 第三阶段：完全独立分发

**触发条件**:
- 第三方开发者参与
- 需要执行器独立发布
- 需要社区共享机制

**生态示例**:
```json
// 第三方执行器包
{
  "name": "@my-company/custom-executors",
  "dependencies": {
    "@modular-agent/script-executors": "workspace:*"
  }
}
```

## 七、决策矩阵

| 场景 | 权重 | 当前状态 | 建议 |
|------|------|----------|------|
| App 层扩展需求 | 高 | ✅ 有 | 保持独立 |
| SDK 封装原则 | 高 | ✅ 有 | 保持独立 |
| 架构清晰性 | 高 | ✅ 有 | 保持独立 |
| 多应用场景 | 中 | ❌ 无 | 保持独立 |
| 插件化架构 | 中 | ❌ 无 | 保持独立 |
| 独立版本管理 | 中 | ❌ 无 | 保持独立 |
| 性能优化 | 低 | ❌ 无 | 保持独立 |
| 安全隔离 | 低 | ❌ 无 | 保持独立 |
| 独立测试 | 低 | ✅ 有 | 保持独立 |

## 八、风险评估

### 8.1 保持独立的风险

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|----------|
| 代码重复 | 中 | 高 | 提取公共组件到 common-utils |
| 版本管理复杂 | 低 | 中 | 统一版本发布流程 |
| 依赖复杂度 | 低 | 低 | 使用 workspace 协议 |
| 维护成本 | 中 | 中 | 明确职责边界 |

### 8.2 合并到 SDK 的风险

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|----------|
| App 无法扩展 | 高 | 高 | 无有效缓解措施 |
| 破坏封装性 | 高 | 高 | 无有效缓解措施 |
| 违反开闭原则 | 高 | 高 | 无有效缓解措施 |
| 增加维护负担 | 中 | 中 | 无有效缓解措施 |

## 九、最终结论

### ✅ 推荐保持当前设计

**核心原因**:
1. **App 层扩展需求**: App 需要访问执行器基类进行自定义
2. **SDK 封装原则**: SDK 只导出 API 层，不暴露 core 实现
3. **架构清晰性**: 执行器作为独立层，职责明确
4. **符合设计原则**: 开闭原则、单一职责原则

**架构优势**:
```
App 层
  ↓ 可以访问
script-executors / tool-executors (独立包)
  ↓ SDK 内部使用
SDK Core Layer
  ↓ 对外暴露
SDK API Layer
  ↓ App 使用
App 层
```

**预期收益**:
- ✅ 支持 App 层自定义扩展
- ✅ 保持 SDK 封装性
- ✅ 清晰的分层架构
- ✅ 符合设计原则

**优化方向**:
1. 统一公共组件（RetryStrategy、TimeoutController）
2. 明确文档和使用指南
3. 统一版本管理
4. 为未来插件化架构预留接口

## 十、实施计划

### 10.1 短期优化（1-2 周）

1. **统一公共组件**
   - 将 RetryStrategy 提取到 common-utils
   - 将 TimeoutController 提取到 common-utils
   - 更新两个包的依赖

2. **完善文档**
   - 编写执行器包使用指南
   - 添加扩展示例
   - 更新 README

3. **版本管理**
   - 统一版本号
   - 更新发布流程

### 10.2 中期演进（1-3 个月）

1. **插件化架构设计**
   - 设计 ExecutorRegistry 接口
   - 实现动态注册机制
   - 编写插件开发文档

2. **性能优化**
   - 支持按需加载
   - 优化包体积
   - 提升启动速度

### 10.3 长期规划（3-6 个月）

1. **生态建设**
   - 支持第三方执行器
   - 建立执行器市场
   - 社区共享机制

2. **安全增强**
   - 沙箱隔离
   - 权限管理
   - 审计日志

## 十一、参考文档

- [SDK 架构文档](../sdk/architecture/)
- [API 层设计](../sdk/api-layer-modules-analysis.md)
- [工具执行器架构](../packages/tool-executors/architecture.md)
- [设计原则](../../AGENTS.md)

## 十二、变更历史

| 版本 | 日期 | 作者 | 变更内容 |
|------|------|------|----------|
| 1.0 | 2025-01-XX | AI Agent | 初始版本，完成架构分析和决策 |

## 十三、审批记录

| 角色 | 姓名 | 日期 | 状态 |
|------|------|------|------|
| 架构师 | - | - | 待审批 |
| 技术负责人 | - | - | 待审批 |
| 项目经理 | - | - | 待审批 |