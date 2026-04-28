# Tool Executors 重构方案总结

## 📋 重构决策

### ✅ 核心结论

**不建议删除** [`sdk/core/tools/base-tool-executor.ts`](../../../sdk/core/tools/base-tool-executor.ts)，而是采用**分层架构**：

1. **保留SDK层** - 提供抽象基类和适配器
2. **创建packages/tool-executors** - 提供具体实现
3. **依赖关系** - packages → sdk（单向依赖）

## 🎯 关键设计决策

### 1. 依赖方向

**正确的依赖关系**：
```
apps/ (应用层)
  ↓ 依赖
packages/tool-executors/ (实现层)
  ↓ 依赖
sdk/ (核心SDK)
```

**理由**：
- SDK是核心库，提供抽象和类型定义
- packages构建在SDK之上，提供具体实现
- 符合分层架构原则

### 2. 包命名

**最终选择**：`tool-executors`

**理由**：
- ✅ 清晰表达包的职责
- ✅ 与SDK的tools模块对应
- ✅ 符合monorepo命名惯例
- ✅ 简洁易理解

### 3. 目录结构

```
packages/tool-executors/
├── src/
│   ├── mcp/
│   │   ├── mcp-executor.ts
│   │   └── impl/
│   │       ├── stdio-transport.ts
│   │       ├── sse-transport.ts
│   │       └── mcp-session.ts
│   ├── rest/
│   │   ├── rest-executor.ts
│   │   └── impl/
│   │       └── http-client.ts
│   ├── stateful/
│   │   ├── stateful-executor.ts
│   │   └── impl/
│   │       └── instance-manager.ts
│   └── stateless/
│       ├── stateless-executor.ts
│       └── impl/
│           └── function-wrapper.ts
```

**设计特点**：
- ✅ 每个执行器独立模块
- ✅ impl目录存放具体实现
- ✅ 清晰的职责划分

### 4. MCP多模式支持

**支持的传输模式**：
1. **Stdio模式** - 通过标准输入输出通信
2. **SSE模式** - 通过Server-Sent Events通信

**实现方式**：
- 定义统一的 [`McpTransport`](./README.md#2-传输接口) 接口
- 分别实现 [`StdioTransport`](./README.md#2-stdio传输实现) 和 [`SseTransport`](./README.md#3-sse传输实现)
- [`McpExecutor`](./README.md#4-mcp执行器) 根据配置选择传输模式
- 复用SDK的 [`HttpTransport`](../../../sdk/core/http/transport.ts) 和 [`SseTransport`](../../../sdk/core/http/transport.ts)

### 5. 应用层使用方式

**推荐方案**：通过SDK统一导入

```typescript
// 应用层代码
import { 
  McpToolExecutor,
  RestToolExecutor,
  StatefulToolExecutor,
  StatelessToolExecutor 
} from '@modular-agent/sdk';

// SDK内部使用tool-executors的实现
const mcpExecutor = new McpToolExecutor();
```

**优点**：
- ✅ 统一的导入入口
- ✅ 应用层无需关心实现细节
- ✅ SDK可以控制版本兼容性
- ✅ 符合分层架构原则

### 6. 依赖配置

**packages/tool-executors/package.json**：
```json
{
  "dependencies": {
    "@modular-agent/sdk": "workspace:*"
  },
  "devDependencies": {
    "@types/node": "workspace:*"
  }
}
```

**关键点**：
- ✅ 只声明必要的依赖
- ✅ `@modular-agent/sdk` 包含 zod，无需重复添加
- ✅ TypeScript、Jest 等由根 package.json 统一管理

## 📊 架构优势

| 优势 | 说明 |
|------|------|
| **清晰的职责划分** | SDK提供抽象，packages提供实现 |
| **正确的依赖方向** | packages → sdk，符合架构层次 |
| **可复用性** | tool-executors可被多个app使用 |
| **可测试性** | 独立的测试环境，更好的隔离 |
| **向后兼容** | SDK层保持现有API不变 |
| **可扩展性** | 易于添加新的执行器和传输模式 |

## 🔄 迁移策略

### 阶段1：创建基础结构
- 创建 `packages/tool-executors` 目录
- 配置 `package.json` 和 `tsconfig.json`
- 创建目录结构

### 阶段2：实现MCP执行器
- 实现传输接口
- 实现Stdio传输
- 实现SSE传输
- 实现MCP执行器
- 编写测试

### 阶段3：实现其他执行器
- 实现REST执行器
- 实现Stateful执行器
- 实现Stateless执行器
- 编写测试

### 阶段4：SDK适配
- 修改SDK执行器为适配器
- 更新导出
- 验证兼容性

### 阶段5：文档和示例
- 编写使用文档
- 创建示例代码
- 更新架构文档

## 📁 文档结构

```
docs/packages/tool-executors/
├── README.md              # 设计文档（主文档）
├── architecture.md        # 架构设计
├── SUMMARY.md             # 本文档（总结）
├── migration-guide.md     # 迁移指南（待创建）
├── api.md                 # API文档（待创建）
└── best-practices.md      # 最佳实践（待创建）
```

## ✅ 完成的工作

1. ✅ 分析当前架构和依赖关系
2. ✅ 设计新的架构方案
3. ✅ 重新设计依赖关系和命名方案
4. ✅ 设计应用层使用方式
5. ✅ 设计MCP多模式支持（SSE/Stdio）
6. ✅ 创建文档目录结构
7. ✅ 编写设计文档（README.md）
8. ✅ 编写架构文档（architecture.md）
9. ✅ 编写总结文档（SUMMARY.md）

## 🎯 下一步行动

### 立即可执行
1. 创建 `packages/tool-executors` 目录结构
2. 配置 `package.json` 和 `tsconfig.json`
3. 开始实现MCP执行器

### 后续工作
1. 实现其他执行器
2. 编写测试用例
3. SDK适配器改造
4. 编写迁移指南
5. 创建使用示例

## 🔗 相关文档

- [设计文档](./README.md) - 详细的设计方案
- [架构文档](./architecture.md) - 架构设计和数据流
- [SDK架构](../../sdk/README.md) - SDK整体架构
- [工具类型定义](../../sdk/types/tool.ts) - Tool类型定义
- [HTTP传输层](../../sdk/core/http/transport.ts) - HTTP传输实现

## 💡 关键要点

1. **不删除BaseToolExecutor** - 保留SDK层的抽象基类
2. **正确的依赖方向** - packages → sdk
3. **清晰的命名** - tool-executors
4. **impl目录** - 存放具体实现
5. **MCP多模式** - 支持Stdio和SSE
6. **统一导入** - 应用层通过SDK导入
7. **最小依赖** - 只声明必要的依赖

## 🚀 开始实施

如果同意此方案，可以按照以下步骤开始实施：

```bash
# 1. 创建目录结构
mkdir -p packages/tool-executors/src/{mcp,rest,stateful,stateless}/impl
mkdir -p packages/tool-executors/__tests__/{mcp,rest,stateful,stateless}

# 2. 创建package.json
# （参考README.md中的配置）

# 3. 创建tsconfig.json
# （继承根配置）

# 4. 开始实现MCP执行器
# （按照README.md中的实现细节）
```

---

**文档版本**：v1.0  
**最后更新**：2024  
**维护者**：Modular Agent Framework Team