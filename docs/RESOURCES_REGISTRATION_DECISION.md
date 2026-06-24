# Resources 注册架构 - 决策要点总结

## 问题陈述

**核心问题**: Resources 目录中的注册逻辑分散在多个模块中，导致：
1. 注册流程难以理解和维护
2. 使用动态导入削弱类型安全
3. Predefined 和 Custom 的实现模式不一致
4. 新增资源类型时需要修改多个位置

## 推荐决策

**统一在 `registration/` 目录实现所有注册逻辑**

### 原因

| 问题 | 解决 | 收益 |
|------|------|------|
| 分散 | 集中管理 | 易于导航、维护 |
| 动态导入 | 静态导入 | 类型安全、IDE 支持 |
| 不一致 | 统一模式 | 学习成本低 |
| 关注点混淆 | 明确分层 | 代码职责清晰 |

## 目标架构

```
registration/
├── orchestrator.ts              # 统一入口 (registerAllResources)
├── predefined-registration.ts   # 预定义资源
├── custom-registration.ts       # 自定义资源  
├── application-registration.ts  # 应用层资源 (预留)
├── types.ts                     # 共用类型
└── index.ts                     # 导出接口
```

## 实施策略

### ✅ 采取的行动

1. **创建新的注册模块** - 在 `registration/` 目录下
2. **保留兼容层** - 原位置作为转发
3. **使用静态导入** - orchestrator.ts 改用 import 而非 await import()
4. **渐进式迁移** - 分阶段更新调用点

### ✅ 保留的内容

- ✅ `predefined/tools/registration.ts` - 内部实现细节，保留
- ✅ `predefined/trigger/registration.ts` - 内部实现细节，保留
- ✅ `predefined/workflow/registration.ts` - 内部实现细节，保留
- ✅ `predefined/` 和 `custom/` 下的资源创建逻辑 - 保留

### ❌ 不做的事

- ❌ 不修改资源创建逻辑（`registry.ts` 等）
- ❌ 不移动资源定义文件
- ❌ 不破坏现有 API

## 改进对比

### 导入方式

**迁移前**：
```typescript
// sdk-instance.ts
import { registerPredefinedContent } from "../resources/predefined/registration.js";

// registration/orchestrator.ts (使用动态导入)
const { registerPredefinedContent } = await import("../predefined/registration.js");
```

**迁移后**：
```typescript
// registration/orchestrator.ts (使用静态导入)
import { registerPredefinedContent } from "./predefined-registration.js";
import { registerCustomResources } from "./custom-registration.js";
```

**收益**：
- 编译时类型检查 ✅
- IDE 完全支持 ✅
- 打包工具优化 ✅
- 运行时确定加载 ✅

### 注册流程

**迁移前**（5 个位置）：
```
predefined/registration.ts (协调)
  ├─ predefined/tools/registration.ts (具体实现)
  ├─ predefined/trigger/registration.ts (具体实现)
  └─ predefined/workflow/registration.ts (具体实现)

custom/registration.ts (具体实现)

registration/orchestrator.ts (顶层协调)
```

**迁移后**（1 个位置协调）：
```
registration/
  ├─ predefined-registration.ts (调用下面的具体实现)
  │   └─ calls predefined/tools/registration.ts
  │   └─ calls predefined/trigger/registration.ts
  │   └─ calls predefined/workflow/registration.ts
  │
  ├─ custom-registration.ts (直接实现)
  │
  └─ orchestrator.ts (调用上面的协调模块)
```

## 关键特性

### 向后兼容

现有代码继续工作：

```typescript
// 旧导入路径仍然可用（通过兼容层）
import { registerPredefinedContent } from "@wf-agent/sdk/resources";
import { registerPredefinedContent } from "@wf-agent/sdk/resources/predefined/registration";

// 新推荐路径
import { registerPredefinedContent } from "@wf-agent/sdk/resources/registration/predefined-registration";
```

### 迁移成本

- 无需立即修改现有代码
- 可分阶段迁移
- 完整的测试覆盖
- 清晰的迁移指南

## 实施时机

### 建议

**优先级**: 🟡 **中等**

理由：
- 改进代码质量和可维护性
- 不影响现有功能
- 可与其他工作并行
- 影响范围可控

### 可纳入下一个优化周期

合适的时机：
- 代码冻结前（确保充分测试）
- 与其他重构工作结合
- 专属投入 1-2 天完成

## 验收标准

迁移完成后：

- [ ] 所有注册逻辑在 `registration/` 目录
- [ ] 使用静态导入而非动态导入
- [ ] 所有现有 API 保持工作
- [ ] 单元测试通过
- [ ] 集成测试通过
- [ ] 无类型检查错误
- [ ] 文档已更新

## 风险评估

| 风险 | 可能性 | 影响 | 缓解 |
|------|--------|------|------|
| 导入错误 | 低 | 中 | 充分测试 |
| 循环依赖 | 低 | 中 | 代码审查 |
| 遗漏调用点 | 低 | 高 | 完整搜索 |
| 功能回归 | 极低 | 高 | 端到端测试 |

## 后续行动

### 立即行动

1. 文档评审（本文档）
2. 团队讨论和决议
3. 任务分解和评估

### 执行阶段

1. 按照 [重构指南](./RESOURCES_REGISTRATION_REFACTORING_GUIDE.md) 实施
2. 逐个 Phase 完成并测试
3. 代码审查
4. 合并到主分支

### 验收阶段

1. 完整功能测试
2. 性能验证
3. 文档更新
4. 最终验收

---

## 附加信息

- **详细分析**: [Architecture Analysis](./RESOURCES_REGISTRATION_ARCHITECTURE_ANALYSIS.md)
- **重构指南**: [Refactoring Guide](./RESOURCES_REGISTRATION_REFACTORING_GUIDE.md)
- **当前代码**:
  - `packages/sdk/resources/predefined/registration.ts`
  - `packages/sdk/resources/custom/registration.ts`
  - `packages/sdk/resources/registration/orchestrator.ts`

---

**文档维护者**: AI Assistant  
**最后更新**: 2026-06-24  
**状态**: 待团队审核
