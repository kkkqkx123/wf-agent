# CLI 应用分阶段执行方案

## 第一阶段：项目初始化和基础设置

### 目标
建立CLI应用的基础结构和开发环境(根目录已配置的依赖通过workspace指定版本)

### 任务
1. 创建 `apps/cli-app` 目录结构
2. 初始化 package.json 文件
3. 配置 TypeScript 编译选项
4. 设置基本的构建和开发脚本
5. 安装核心依赖（commander、@modular-agent/sdk等）
6. 创建入口文件和基本命令结构

### 交付物
- 可运行的空CLI应用
- 基础开发环境配置
- 基本的命令框架

---

## 第二阶段：核心功能实现

### 目标
实现CLI应用的核心功能，包括工作流、线程、检查点和模板管理

### 任务
1. 实现工作流管理命令（注册、列表、详情、删除）
   - `workflow register <file>` - 从文件注册工作流
   - `workflow register-batch <directory>` - 批量注册工作流
   - `workflow list` - 列出所有工作流
   - `workflow show <id>` - 查看工作流详情
   - `workflow delete <id>` - 删除工作流
2. 实现线程管理命令（执行、暂停、恢复、停止）
   - `thread run <workflow-id>` - 执行工作流线程
   - `thread pause <thread-id>` - 暂停线程
   - `thread resume <thread-id>` - 恢复线程
   - `thread stop <thread-id>` - 停止线程
   - `thread list` - 列出所有线程
   - `thread show <thread-id>` - 查看线程详情
   - `thread delete <thread-id>` - 删除线程
3. 实现检查点管理命令（创建、载入、列表）
   - `checkpoint create <thread-id>` - 创建检查点
   - `checkpoint load <checkpoint-id>` - 载入检查点
   - `checkpoint list` - 列出所有检查点
   - `checkpoint show <checkpoint-id>` - 查看检查点详情
   - `checkpoint delete <checkpoint-id>` - 删除检查点
4. 实现模板管理命令（注册、列表、详情）
   - `template register-node <file>` - 注册节点模板
   - `template register-nodes-batch <directory>` - 批量注册节点模板
   - `template register-trigger <file>` - 注册触发器模板
   - `template register-triggers-batch <directory>` - 批量注册触发器模板
   - `template list-nodes` - 列出所有节点模板
   - `template list-triggers` - 列出所有触发器模板
   - `template show-node <id>` - 查看节点模板详情
   - `template show-trigger <id>` - 查看触发器模板详情
   - `template delete-node <id>` - 删除节点模板
   - `template delete-trigger <id>` - 删除触发器模板
5. 开发适配器层，连接CLI参数与SDK API
   - 实现 BaseAdapter 基类，提供通用功能
   - 实现 WorkflowAdapter 继承 BaseAdapter
   - 实现 ThreadAdapter 继承 BaseAdapter
   - 实现 CheckpointAdapter 继承 BaseAdapter
   - 实现 TemplateAdapter 继承 BaseAdapter
6. 实现基础的错误处理和日志记录
   - 使用 @modular-agent/common-utils 的日志系统
   - 在 BaseAdapter 中统一错误处理逻辑

### 交付物
- 完整的功能命令集
- 适配器层实现（统一继承 BaseAdapter）
- 基础错误处理机制

---

## 第三阶段：用户体验优化

### 目标
提升CLI应用的用户体验和可用性

### 任务
1. 添加进度指示器和加载动画
   - 使用 `ora` 实现长时间操作的加载动画
   - 在批量操作和文件解析时显示进度
2. 实现彩色输出和格式化显示
   - 使用 `chalk` 美化输出
   - 支持 `--table` 选项以表格形式展示数据
   - 状态信息使用颜色区分（绿色=成功/运行中，黄色=暂停，红色=失败/停止）
3. 添加交互式命令（使用inquirer）
   - 删除操作前的确认提示（当未使用 `--force` 时）
   - 交互式配置向导
4. 实现配置文件支持
   - 使用 `cosmiconfig` 加载配置文件
   - 支持 `.modular-agentrc`、`.modular-agentrc.json` 等格式
   - 配置项：apiUrl、apiKey、defaultTimeout、verbose、debug、logLevel、outputFormat、maxConcurrentThreads
5. 添加详细的帮助信息和使用示例
   - 每个命令都有清晰的描述
   - 提供使用示例
6. 实现命令自动补全功能
   - 添加 shell 补全脚本生成
   - 支持 bash/zsh/fish

### 交付物
- 用户友好的界面（彩色输出、加载动画）
- 配置管理功能（多格式配置文件支持）
- 命令补全支持
- 交互式确认功能

---

## 第四阶段：高级功能和扩展

### 目标
添加高级功能和扩展性支持

### 任务
1. 实现批量操作功能
   - ✅ 已在第二阶段完成（register-batch、register-nodes-batch、register-triggers-batch）
   - 添加并行处理支持（使用 `p-map`、`p-limit`）
   - 批量导入导出功能
2. 添加导入导出功能
   - 工作流配置的导入导出
   - 支持 JSON/TOML/YAML 格式
3. 实现状态监控和实时反馈
   - 线程执行状态的实时监控
   - 日志流式输出
4. 添加插件系统支持
   - 预留插件接口
   - 允许第三方扩展命令
5. 实现国际化支持
   - 多语言输出支持
   - 可切换的语言包
6. 添加性能监控和诊断工具
   - 命令执行时间统计
   - 内存使用情况监控
   - 调试模式下的详细诊断信息

### 交付物
- 高级操作功能（批量处理、导入导出）
- 插件系统框架
- 国际化支持
- 性能监控工具

---

## 当前实施状态

| 阶段 | 状态 | 完成度 |
|------|------|--------|
| 第一阶段：项目初始化和基础设置 | ✅ 已完成 | 100% |
| 第二阶段：核心功能实现 | ✅ 已完成 | 100% |
| 第三阶段：用户体验优化 | 🔄 部分完成 | 70% |
| 第四阶段：高级功能和扩展 | ⏳ 待开始 | 0% |

### 第二阶段已完成的关键实现

1. **适配器层架构**
   - ✅ BaseAdapter 基类提供统一的 SDK 访问和错误处理
   - ✅ 所有适配器统一继承 BaseAdapter
   - ✅ 通过构造函数注入 SDK 实例

2. **配置管理**
   - ✅ ConfigManager 统一管理配置加载
   - ✅ 支持单文件和批量加载
   - ✅ 支持递归目录扫描
   - ✅ 支持文件模式过滤

3. **日志和格式化**
   - ✅ CLILogger 封装基础日志功能
   - ✅ 彩色输出支持
   - ✅ 表格格式输出
   - ✅ 状态颜色编码

### 第三阶段待完成的任务

- [ ] 添加 `ora` 加载动画到长时间操作
- [ ] 实现删除操作的交互式确认（使用 `inquirer`）
- [ ] 添加命令自动补全功能
- [ ] 完善配置文件验证

### 第四阶段计划任务

- [ ] 设计插件系统接口
- [ ] 实现国际化框架
- [ ] 添加性能监控工具
- [ ] 实现导入导出功能
