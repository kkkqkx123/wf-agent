# Workflow 集成测试

本目录包含基于 `docs/tests/workflows` 文档编写的 Workflow 集成测试文件。

## 测试文件结构

```
workflows/
├── 01-registration.test.ts          # 工作流注册测试
├── 02-batch-operations.test.ts      # 批量操作测试
├── 03-query.test.ts                 # 查询测试
├── 04-deletion.test.ts              # 删除测试
├── 05-api.test.ts                   # API测试
├── 06-preprocessing.test.ts         # 预处理测试
├── 07-reference-check.test.ts       # 引用检查测试
├── 08-relationship-management.test.ts # 关系管理测试
├── 09-error-handling.test.ts        # 错误处理测试
└── README.md                        # 本文件
```

## 测试覆盖范围

### 01-registration.test.ts - 工作流注册测试

- 注册 STANDALONE 工作流
- 注册 TRIGGERED_SUBWORKFLOW 工作流
- 注册 DEPENDENT 工作流
- 注册带触发器的工作流
- 参数替换功能
- 验证失败场景（缺少字段、无效节点类型、重复ID等）

### 02-batch-operations.test.ts - 批量操作测试

- 批量注册全部成功
- 批量注册部分失败
- 递归加载子目录
- 文件模式过滤
- 空目录处理
- 不存在的目录处理

### 03-query.test.ts - 查询测试

- 查询存在的工作流
- 查询不存在的工作流
- 列出所有工作流
- 详细模式列出工作流
- 空注册表处理
- 类型过滤
- 状态过滤
- 查询带触发器的工作流
- JSON格式输出

### 04-deletion.test.ts - 删除测试

- 删除存在的工作流
- 删除不存在的工作流
- 删除有依赖的工作流
- 级联删除工作流
- 强制删除选项
- 删除带触发器的工作流

### 05-api.test.ts - API测试

- API创建工作流
- API查询所有工作流
- API版本化更新工作流
- API获取工作流详情
- API删除工作流
- API验证功能

### 06-preprocessing.test.ts - 预处理测试

- SUBGRAPH节点展开
- 多个SUBGRAPH节点处理
- 触发器引用解析
- 循环引用检测
- 无效引用处理
- 参数解析
- 依赖分析
- 结构验证

### 07-reference-check.test.ts - 引用检查测试

- 有效引用检查
- 无效引用检测
- 循环引用检测
- 多种引用类型检查
- 无引用处理

### 08-relationship-management.test.ts - 关系管理测试

- 父子关系建立
- 查询子工作流
- 查询父工作流
- 依赖关系管理
- 多父关系处理
- 关系类型识别
- 关系清理
- 无关系处理

### 09-error-handling.test.ts - 错误处理测试

- ConfigurationValidationError处理
- WorkflowNotFoundError处理
- ExecutionError处理
- 文件系统错误处理
- 权限错误处理
- 网络错误处理
- 错误恢复
- 错误日志记录

## 测试辅助工具

### WorkflowTestHelper

位于 `apps/cli-app/__tests__/helpers/workflow-test-helpers.ts`

提供以下辅助方法：

- 创建各种类型的工作流配置
- 工作流文件读写
- ID提取
- 输出验证
- 错误消息提取
- 工作流列表解析

## 测试数据模板

测试数据模板位于 `apps/cli-app/__tests__/fixtures/workflows/` 目录：

### 有效工作流模板

- `minimal-wf.toml` - 最小工作流
- `complete-wf.toml` - 完整工作流
- `trigger-wf.toml` - 带触发器的工作流
- `triggered-subwf.toml` - 触发子工作流
- `dependent-wf.toml` - 依赖工作流
- `param-wf.toml` - 参数化工作流
- `parent-wf.toml` - 父工作流
- `child-wf.toml` - 子工作流

### 无效工作流模板

- `invalid-missing-fields.toml` - 缺少必需字段
- `invalid-node-type.toml` - 无效节点类型
- `duplicate-node.toml` - 重复节点ID
- `invalid-edge.toml` - 无效边引用
- `multiple-start.toml` - 多个START节点
- `multiple-end.toml` - 多个END节点

### 循环引用模板

- `workflow-a.toml` - 循环引用工作流A
- `workflow-b.toml` - 循环引用工作流B

## 运行测试

### 运行所有workflow集成测试

```bash
cd apps/cli-app
pnpm test integration/workflows
```

### 运行特定测试文件

```bash
cd apps/cli-app
pnpm test integration/workflows/01-registration.test.ts
```

### 运行特定测试用例

```bash
cd apps/cli-app
pnpm test integration/workflows/01-registration.test.ts -t "should register a STANDALONE workflow successfully"
```

## 测试输出

测试输出保存在 `apps/cli-app/__tests__/outputs/workflows/` 目录：

- 每个测试类别有独立的输出目录
- 包含测试日志和HTML报告
- 使用TestLogger记录所有测试执行详情

## 测试框架特性

- 使用 Vitest 作为测试框架
- 集成 CLIRunner 执行CLI命令
- 使用 TestHelper 管理临时文件
- 使用 TestLogger 记录测试执行
- 生成HTML测试报告
- 支持测试隔离和清理

## 注意事项

1. 所有测试都使用独立的临时目录，确保测试隔离
2. 每个测试后都会清理临时文件
3. 测试覆盖了正常场景和错误场景
4. 测试输出包含详细的执行日志
5. 某些测试可能需要特定的CLI命令支持，如果命令不存在，测试会相应调整

## 扩展测试

如需添加新的测试用例：

1. 在相应的测试文件中添加新的测试用例
2. 使用 WorkflowTestHelper 创建测试数据
3. 使用 CLIRunner 执行CLI命令
4. 使用 TestLogger 记录测试执行
5. 验证预期结果

## 相关文档

- 测试需求文档：`docs/tests/workflows/`
- 测试框架文档：`apps/cli-app/__tests__/integration/test-framework.test.ts`
- CLI实现：`apps/cli-app/src/`
