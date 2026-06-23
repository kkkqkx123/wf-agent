# CLI 应用依赖分析

## 依赖分类

### 必需依赖

#### CLI 框架
- `commander` - 功能强大的命令行框架，支持子命令、选项解析和帮助生成
- `@commander-js/extra-typings` - Commander.js 的额外类型定义

#### 项目内部依赖
- `@modular-agent/sdk` - 核心SDK包，提供所有工作流管理功能
- `@modular-agent/common-utils` - 公共工具函数

### 可选但推荐的依赖

#### 配置管理
- `cosmiconfig` - 支持JSON, TOML, JS/TS schema格式的配置加载器
- `zod` - 运行时验证库（已在SDK中使用）

#### 异步操作处理
- `p-map` - 支持并发的数组映射操作
- `p-limit` - 限制并发操作数量

#### 用户体验增强
- `ora` - 终端加载动画
- `chalk` - 终端字符串样式（颜色、粗体等）
- `cli-progress` - 进度条组件
- `inquirer` - 交互式命令行界面

#### 文件操作
- `fs-extra` - 增强版文件系统操作
- `yaml` - YAML 解析
- `@iarna/toml` - TOML 解析

#### 日志和错误处理
- `winston` - 功能全面的日志记录库

## 依赖选择理由

### Commander.js
- TypeScript 支持优秀
- 支持复杂的嵌套命令结构
- 自动帮助生成
- 社区活跃，文档完善
- 适合构建功能丰富的 CLI 应用

### Cosmiconfig
- 支持多种配置文件格式
- 与 SDK 中使用的配置格式一致
- 易于集成和使用

### Ora 和 Cli-Progress
- 提供良好的用户反馈
- 在长时间运行的操作中显示进度
- 提升 CLI 应用的专业感

### Winston
- 支持多种日志级别和输出目标
- 可配置性强
- 适合 CLI 应用的日志需求

## 安装命令

```bash
# 主要依赖
pnpm add commander @commander-js/extra-typings @modular-agent/sdk @modular-agent/common-utils

# 配置管理
pnpm add cosmiconfig zod

# 异步操作处理
pnpm add p-map p-limit

# 用户体验增强
pnpm add ora chalk cli-progress inquirer

# 文件操作
pnpm add fs-extra yaml @iarna/toml

# 日志和错误处理
pnpm add winston
```

## 开发依赖

```bash
# TypeScript 相关
pnpm add -D typescript @types/node ts-node

# 测试相关
pnpm add -D jest @types/jest ts-jest

# 代码质量
pnpm add -D eslint @typescript-eslint/parser @typescript-eslint/eslint-plugin prettier
```

## 依赖关系图

```
@modular-agent/cli-app
├── commander (@modular-agent/sdk)
├── @modular-agent/sdk (workspace dependency)
├── cosmiconfig
├── ora
├── chalk
├── fs-extra
├── yaml
├── @iarna/toml
├── winston
├── p-map
├── p-limit
├── cli-progress
├── inquirer
└── zod
```

## 版本策略

- 对于 Commander.js，使用最新稳定版本以获得最佳 TypeScript 支持
- 对于项目内部依赖，使用 workspace 协议确保与主项目同步
- 对于其他依赖，使用 LTS 或最新稳定版本
- 定期更新依赖以获得安全修复和新功能