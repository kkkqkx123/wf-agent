# Web App 功能清单

## 概述

本文档基于对 Modular Agent Framework 的 CLI 应用、Lim-Code 参考实现以及 SDK 核心功能的分析,详细列出 Web 应用应实现的功能清单。

## 一、核心功能模块

### 1. 工作流管理模块 (Workflow Management)

#### 1.1 工作流注册与配置
- **从文件注册工作流**
  - 支持 JSON/TOML 配置文件解析
  - 支持单个文件注册
  - 支持批量目录注册
  - 支持递归加载子目录
  - 支持文件模式匹配过滤
  - 支持运行时参数注入

- **工作流配置管理**
  - 工作流基本信息编辑(名称、描述)
  - 节点配置管理
  - 边配置管理
  - 变量定义管理
  - 触发器配置

#### 1.2 工作流展示与查询
- **工作流列表展示**
  - 卡片视图/表格视图切换
  - 搜索和过滤功能
  - 分页展示
  - 状态筛选(启用/禁用)

- **工作流详情查看**
  - 基本信息展示
  - 节点拓扑图可视化
  - 节点详细信息
  - 边连接关系
  - 变量列表
  - 触发器列表
  - 执行历史

#### 1.3 工作流可视化编辑
- **图形化编辑器**
  - 节点拖拽添加
  - 节点位置调整
  - 节点连线操作
  - 节点删除操作
  - 缩放和平移
  - 自动布局

- **节点编辑**
  - 节点类型选择
  - 节点参数配置
  - 节点条件配置
  - 节点模板应用

#### 1.4 工作流执行控制
- **执行操作**
  - 启动工作流执行
  - 暂停执行
  - 恢复执行
  - 取消执行
  - 输入参数配置

- **执行监控**
  - 实时执行状态
  - 执行进度展示
  - 执行日志查看
  - 执行结果展示

### 2. 线程监控模块 (Thread Monitoring)

#### 2.1 线程列表管理
- **线程列表展示**
  - 所有线程列表
  - 状态筛选(运行中、暂停、完成、失败、取消)
  - 工作流关联筛选
  - 时间范围筛选
  - 搜索功能

- **线程基本信息**
  - 线程 ID
  - 关联工作流
  - 当前状态
  - 创建时间
  - 执行时长
  - 当前节点

#### 2.2 线程实时监控
- **执行流程可视化**
  - 节点执行路径高亮
  - 当前执行节点标识
  - 已完成节点标记
  - 失败节点标识
  - 执行时间统计

- **实时日志流**
  - 节点执行日志
  - 工具调用日志
  - LLM 交互日志
  - 错误日志
  - 日志级别过滤
  - 日志搜索

- **执行进度展示**
  - 总体进度条
  - 节点级进度
  - 预计剩余时间
  - 资源使用情况

#### 2.3 线程控制操作
- **生命周期控制**
  - 暂停线程
  - 恢复线程
  - 取消线程
  - 重试失败节点

- **检查点管理**
  - 创建检查点
  - 查看检查点列表
  - 从检查点恢复
  - 检查点对比

#### 2.4 线程消息查看
- **消息历史**
  - 消息列表展示
  - 消息类型筛选
  - 消息内容搜索
  - 消息详情查看
  - 消息导出

### 3. Agent Loop 交互模块 (Agent Loop Interaction)

#### 3.1 Agent Loop 管理
- **Agent Loop 列表**
  - 所有实例列表
  - 状态筛选(运行中、暂停、完成)
  - Profile 关联筛选
  - 搜索功能

- **Agent Loop 基本信息**
  - 实例 ID
  - 关联 Profile
  - 当前状态
  - 迭代次数
  - 创建时间
  - 运行时长

#### 3.2 实时对话界面
- **对话交互**
  - 消息输入框
  - 消息发送
  - 流式响应显示
  - 消息历史展示
  - 多轮对话支持

- **消息展示**
  - 用户消息
  - AI 响应消息
  - 工具调用消息
  - 系统消息
  - 消息时间戳
  - 消息状态标识

#### 3.3 工具调用查看
- **工具调用记录**
  - 工具名称
  - 调用参数
  - 执行结果
  - 执行时长
  - 成功/失败状态

- **工具调用详情**
  - 参数详情查看
  - 结果详情查看
  - 错误信息展示
  - 重试操作

#### 3.4 消息流展示
- **实时消息流**
  - 文本流式输出
  - 工具调用开始事件
  - 工具调用结束事件
  - 迭代完成事件
  - 错误事件

- **消息流控制**
  - 停止生成
  - 重新生成
  - 消息编辑
  - 消息删除

#### 3.5 Agent Loop 控制
- **生命周期控制**
  - 启动 Agent Loop
  - 暂停 Agent Loop
  - 恢复 Agent Loop
  - 停止 Agent Loop
  - 克隆 Agent Loop
  - 清理已完成实例

- **检查点管理**
  - 创建检查点
  - 从检查点恢复
  - 查看检查点列表

- **变量管理**
  - 查看变量列表
  - 设置变量值
  - 变量类型展示

### 4. 资源管理模块 (Resource Management)

#### 4.1 工具管理 (Tool Management)
- **工具注册表**
  - 工具列表展示
  - 工具分类展示
  - 工具搜索
  - 工具详情查看

- **工具配置**
  - 启用/禁用工具
  - 工具参数配置
  - 工具权限配置
  - 自动执行配置

- **工具测试**
  - 工具调用测试
  - 参数验证
  - 结果预览

#### 4.2 脚本管理 (Script Management)
- **脚本注册表**
  - 脚本列表展示
  - 脚本分类展示
  - 脚本搜索
  - 脚本详情查看

- **脚本编辑**
  - 脚本内容编辑
  - 语法高亮
  - 参数定义编辑
  - 元数据编辑

- **脚本测试**
  - 脚本执行测试
  - 参数输入
  - 执行结果展示
  - 执行日志查看

#### 4.3 LLM Profile 管理 (LLM Profile Management)
- **Profile 列表**
  - Profile 列表展示
  - 类型筛选(OpenAI、Anthropic、Gemini、Mock)
  - 搜索功能

- **Profile 配置**
  - 基本信息(名称、类型)
  - API 配置(URL、Key)
  - 模型配置(模型列表、默认模型)
  - 参数配置(温度、最大 Token 等)
  - 超时和重试配置
  - 代理配置

- **Profile 测试**
  - 连接测试
  - 模型列表获取
  - 简单对话测试

#### 4.4 Skill 管理 (Skill Management)
- **Skill 列表**
  - Skill 列表展示
  - 分类展示
  - 搜索功能
  - 启用/禁用状态

- **Skill 配置**
  - Skill 加载
  - Skill 启用/禁用
  - Skill 参数配置
  - Skill 内容发送配置

- **Skill 详情**
  - Skill 描述
  - Skill 内容
  - Skill 元数据
  - 关联工具

#### 4.5 触发器管理 (Trigger Management)
- **触发器列表**
  - 触发器列表展示
  - 类型筛选
  - 状态筛选
  - 搜索功能

- **触发器配置**
  - 触发器类型选择
  - 触发条件配置
  - 触发动作配置
  - 启用/禁用

- **触发器模板**
  - 模板列表
  - 从模板创建
  - 模板编辑

#### 4.6 变量管理 (Variable Management)
- **变量列表**
  - 变量列表展示
  - 类型筛选
  - 搜索功能

- **变量配置**
  - 变量定义
  - 变量类型
  - 默认值
  - 验证规则

### 5. 事件监控模块 (Event Monitoring)

#### 5.1 事件流展示
- **实时事件流**
  - 事件列表实时更新
  - 事件类型标识
  - 事件来源标识
  - 事件时间戳
  - 事件详情

- **事件类型**
  - 执行事件(开始、完成、错误、取消、进度)
  - 节点执行事件
  - 工具调用事件
  - LLM 交互事件
  - 自定义事件

#### 5.2 事件过滤
- **过滤条件**
  - 事件类型过滤
  - 时间范围过滤
  - 来源过滤
  - 关联 ID 过滤
  - 自定义条件过滤

#### 5.3 事件统计
- **统计信息**
  - 事件频率统计
  - 事件类型分布
  - 时间分布图
  - 错误率统计

### 6. 检查点管理模块 (Checkpoint Management)

#### 6.1 检查点列表
- **检查点展示**
  - 检查点列表
  - 关联线程/Agent Loop
  - 创建时间
  - 检查点大小
  - 检查点类型

#### 6.2 检查点操作
- **检查点管理**
  - 创建检查点
  - 从检查点恢复
  - 删除检查点
  - 检查点对比

- **检查点详情**
  - 状态快照
  - 消息历史
  - 变量状态
  - 执行上下文

### 7. Human Relay 模块 (Human Relay)

#### 7.1 Human Relay 管理
- **Relay 列表**
  - Relay 实例列表
  - 状态筛选
  - 关联线程

- **Relay 操作**
  - 创建 Relay
  - 提交响应
  - 取消 Relay
  - 超时管理

### 8. 消息管理模块 (Message Management)

#### 8.1 消息列表
- **消息展示**
  - 消息列表
  - 消息类型筛选
  - 时间范围筛选
  - 关联线程筛选

- **消息操作**
  - 消息详情查看
  - 消息删除
  - 消息导出

## 二、前端功能模块

### 1. 用户界面组件

#### 1.1 布局组件
- **主布局**
  - 顶部导航栏
  - 侧边栏菜单
  - 内容区域
  - 状态栏

- **侧边栏**
  - 可折叠菜单
  - 功能模块导航
  - 快捷操作

#### 1.2 通用组件
- **基础组件**
  - Button(按钮)
  - Input(输入框)
  - Select(选择器)
  - Modal(模态框)
  - Table(表格)
  - Form(表单)
  - Card(卡片)
  - Tooltip(提示)
  - Loading(加载)
  - Empty(空状态)

- **高级组件**
  - Markdown 渲染器
  - 代码编辑器
  - JSON 查看器
  - 文件选择器
  - 日期选择器
  - 颜色选择器

#### 1.3 业务组件
- **工作流组件**
  - 工作流卡片
  - 工作流编辑器
  - 节点配置面板
  - 边编辑面板

- **线程组件**
  - 线程卡片
  - 线程监控面板
  - 执行流程图
  - 日志查看器

- **Agent Loop 组件**
  - 对话界面
  - 消息列表
  - 消息输入框
  - 工具调用展示

### 2. 状态管理

#### 2.1 全局状态
- **应用状态**
  - 用户偏好设置
  - UI 主题设置
  - 语言设置
  - 侧边栏状态

- **数据状态**
  - 工作流列表
  - 线程列表
  - Agent Loop 列表
  - 资源列表

#### 2.2 模块状态
- **工作流状态**
  - 当前工作流
  - 编辑状态
  - 执行状态

- **线程状态**
  - 当前线程
  - 监控数据
  - 日志数据

- **Agent Loop 状态**
  - 当前 Agent Loop
  - 对话历史
  - 流式状态

### 3. 实时通信

#### 3.1 WebSocket 连接
- **连接管理**
  - 连接建立
  - 心跳保持
  - 断线重连
  - 连接状态展示

- **消息处理**
  - 消息发送
  - 消息接收
  - 消息路由
  - 错误处理

#### 3.2 事件订阅
- **事件订阅管理**
  - 订阅事件
  - 取消订阅
  - 事件过滤
  - 事件处理

### 4. 可视化功能

#### 4.1 工作流可视化
- **图形渲染**
  - 节点渲染
  - 边渲染
  - 布局计算
  - 缩放平移

- **交互功能**
  - 节点选择
  - 节点拖拽
  - 边连线
  - 区域选择

#### 4.2 执行流程可视化
- **执行路径展示**
  - 节点执行状态
  - 执行路径高亮
  - 当前节点标识
  - 时间线展示

#### 4.3 数据可视化
- **图表组件**
  - 折线图(趋势)
  - 柱状图(统计)
  - 饼图(分布)
  - 甘特图(时间线)

## 三、后端功能模块

### 1. REST API 层

#### 1.1 工作流 API
- **工作流资源**
  - GET /api/workflows - 列出工作流
  - GET /api/workflows/:id - 获取工作流详情
  - POST /api/workflows - 注册工作流
  - PUT /api/workflows/:id - 更新工作流
  - DELETE /api/workflows/:id - 删除工作流
  - POST /api/workflows/register-file - 从文件注册
  - POST /api/workflows/register-batch - 批量注册

#### 1.2 线程 API
- **线程资源**
  - GET /api/threads - 列出线程
  - GET /api/threads/:id - 获取线程详情
  - POST /api/threads - 创建线程
  - DELETE /api/threads/:id - 删除线程

- **线程操作**
  - POST /api/threads/:id/execute - 执行线程
  - POST /api/threads/:id/pause - 暂停线程
  - POST /api/threads/:id/resume - 恢复线程
  - POST /api/threads/:id/cancel - 取消线程

#### 1.3 Agent Loop API
- **Agent Loop 资源**
  - GET /api/agent-loops - 列出 Agent Loop
  - GET /api/agent-loops/:id - 获取详情
  - POST /api/agent-loops - 创建 Agent Loop
  - DELETE /api/agent-loops/:id - 删除 Agent Loop

- **Agent Loop 操作**
  - POST /api/agent-loops/:id/run - 运行
  - POST /api/agent-loops/:id/pause - 暂停
  - POST /api/agent-loops/:id/resume - 恢复
  - POST /api/agent-loops/:id/cancel - 取消

#### 1.4 资源管理 API
- **工具 API**
  - GET /api/tools - 列出工具
  - GET /api/tools/:id - 获取工具详情
  - PUT /api/tools/:id/config - 更新工具配置

- **脚本 API**
  - GET /api/scripts - 列出脚本
  - GET /api/scripts/:id - 获取脚本详情
  - POST /api/scripts - 创建脚本
  - PUT /api/scripts/:id - 更新脚本
  - DELETE /api/scripts/:id - 删除脚本
  - POST /api/scripts/:id/execute - 执行脚本

- **LLM Profile API**
  - GET /api/profiles - 列出 Profile
  - GET /api/profiles/:id - 获取 Profile 详情
  - POST /api/profiles - 创建 Profile
  - PUT /api/profiles/:id - 更新 Profile
  - DELETE /api/profiles/:id - 删除 Profile
  - POST /api/profiles/:id/test - 测试 Profile

- **Skill API**
  - GET /api/skills - 列出 Skill
  - GET /api/skills/:id - 获取 Skill 详情
  - POST /api/skills - 加载 Skill
  - PUT /api/skills/:id/config - 更新 Skill 配置

- **触发器 API**
  - GET /api/triggers - 列出触发器
  - GET /api/triggers/:id - 获取触发器详情
  - POST /api/triggers - 创建触发器
  - PUT /api/triggers/:id - 更新触发器
  - DELETE /api/triggers/:id - 删除触发器

#### 1.5 检查点 API
- **检查点资源**
  - GET /api/checkpoints - 列出检查点
  - GET /api/checkpoints/:id - 获取检查点详情
  - POST /api/checkpoints - 创建检查点
  - DELETE /api/checkpoints/:id - 删除检查点
  - POST /api/checkpoints/:id/restore - 从检查点恢复

#### 1.6 事件 API
- **事件资源**
  - GET /api/events - 列出事件
  - GET /api/events/stats - 获取事件统计

### 2. WebSocket 层

#### 2.1 实时事件推送
- **线程事件**
  - thread:started - 线程启动
  - thread:completed - 线程完成
  - thread:error - 线程错误
  - thread:cancelled - 线程取消
  - thread:progress - 线程进度
  - node:executed - 节点执行完成

- **Agent Loop 事件**
  - agent-loop:started - Agent Loop 启动
  - agent-loop:completed - Agent Loop 完成
  - agent-loop:error - Agent Loop 错误
  - agent-loop:text - 文本流
  - agent-loop:tool-call - 工具调用
  - agent-loop:iteration - 迭代完成

- **工具事件**
  - tool:started - 工具开始执行
  - tool:completed - 工具执行完成
  - tool:error - 工具执行错误

#### 2.2 双向通信
- **客户端请求**
  - subscribe - 订阅事件
  - unsubscribe - 取消订阅
  - execute - 执行操作
  - cancel - 取消操作

- **服务端响应**
  - event - 事件推送
  - result - 执行结果
  - error - 错误消息

### 3. 适配器层

#### 3.1 SDK 适配器
- **命令适配器**
  - 将 HTTP 请求转换为 SDK Command
  - 处理命令执行
  - 转换结果为 HTTP 响应

- **查询适配器**
  - 将 HTTP 请求转换为 SDK Query
  - 处理查询执行
  - 转换结果为 HTTP 响应

#### 3.2 数据适配器
- **数据转转**
  - SDK 数据模型转前端数据格式
  - 前端数据格式转 SDK 数据模型
  - 数据验证和清理

### 4. 中间件层

#### 4.1 认证授权中间件
- **认证**
  - Token 验证
  - Session 管理
  - 权限检查

#### 4.2 错误处理中间件
- **错误处理**
  - 错误捕获
  - 错误转转
  - 错误日志
  - 错误响应

#### 4.3 日志记录中间件
- **请求日志**
  - 请求信息记录
  - 响应信息记录
  - 性能指标记录

#### 4.4 CORS 中间件
- **跨域处理**
  - CORS 头设置
  - 预检请求处理

## 四、对比分析

### CLI 应用 vs Web 应用

| 功能维度 | CLI 应用 | Web 应用 |
|---------|---------|---------|
| **用户界面** | 命令行界面 | Web 图形界面 |
| **交互方式** | 命令输入 | 图形化操作、表单、拖拽 |
| **实时反馈** | 文本输出、日志流 | 实时更新、可视化图表、动画 |
| **可视化** | 无 | 工作流可视化、执行流程图、统计图表 |
| **多用户** | 单用户 | 多用户支持(可扩展) |
| **部署方式** | 本地安装 | 服务器部署、云端访问 |
| **适配器层** | 直接调用 SDK | 通过 HTTP/WebSocket 与后端通信 |
| **状态管理** | 进程内存 | 前端 Store + 后端状态 |

### Lim-Code 参考实现借鉴

| 功能模块 | Lim-Code 实现 | Web App 借鉴点 |
|---------|--------------|---------------|
| **聊天界面** | Vue 组件化聊天界面 | Agent Loop 对话界面设计 |
| **消息展示** | 消息列表、Markdown 渲染 | 消息流展示、富文本渲染 |
| **工具调用** | 工具调用可视化 | 工具调用记录展示 |
| **设置面板** | 分类设置面板 | 资源配置界面设计 |
| **实时通信** | VS Code Webview 通信 | WebSocket 通信模式 |
| **状态管理** | Vue Store | Svelte Store 状态管理 |
| **组件化** | Vue 组件库 | Svelte 组件库设计 |

## 五、技术实现要点

### 1. 前端技术栈
- **框架**: Svelte 5 (Runes API)
- **构建工具**: Vite
- **路由**: SvelteKit
- **状态管理**: Svelte Stores
- **样式**: TailwindCSS
- **可视化**: D3.js 或 ECharts
- **实时通信**: WebSocket Client

### 2. 后端技术栈
- **运行时**: Node.js
- **Web 框架**: 原生 fetch 或 Express
- **WebSocket**: ws 或 Socket.io
- **SDK 集成**: @modular-agent/sdk

### 3. 关键设计模式
- **适配器模式**: 封装 SDK 调用
- **观察者模式**: 事件订阅和推送
- **命令模式**: SDK Command/Query
- **工厂模式**: API 工厂
- **单例模式**: 全局状态管理

### 4. 性能优化
- **前端**: 代码分割、懒加载、虚拟滚动
- **后端**: 连接池、缓存、流式响应
- **通信**: 消息压缩、增量更新

## 六、开发优先级

### P0 - 核心功能(第一阶段)
1. 基础框架搭建(前后端项目结构)
2. 工作流管理(列表、详情、注册)
3. 线程监控(列表、详情、实时监控)
4. Agent Loop 交互(对话界面、消息流)
5. REST API 框架
6. WebSocket 通信框架

### P1 - 重要功能(第二阶段)
1. 工作流可视化编辑器
2. 线程执行流程可视化
3. 资源管理(工具、脚本、Profile、Skill)
4. 检查点管理
5. 事件监控

### P2 - 增强功能(第三阶段)
1. 触发器管理
2. 变量管理
3. Human Relay
4. 高级可视化(统计图表)
5. 性能优化

### P3 - 扩展功能(第四阶段)
1. 多用户支持
2. 权限管理
3. 国际化
4. 插件系统
5. 移动端适配

## 七、总结

Web 应用作为 Modular Agent Framework 的 Web 前端,需要实现完整的工作流管理、线程监控、Agent Loop 交互和资源管理功能。通过前后端分离架构,提供直观的可视化界面和实时交互能力。

核心实现要点:
1. **前后端分离**: 前端负责 UI 和交互,后端提供 API 和实时通信
2. **实时通信优先**: 线程执行和 Agent Loop 交互依赖 WebSocket 实时推送
3. **可视化能力**: 工作流编辑器和执行流程可视化是核心卖点
4. **组件化设计**: 可复用的组件库,支持独立开发和测试
5. **SDK 集成**: 后端通过适配器层调用 SDK,保持架构清晰

通过借鉴 CLI 应用的命令设计和 Lim-Code 的 UI 实现,可以快速构建功能完善的 Web 应用。
