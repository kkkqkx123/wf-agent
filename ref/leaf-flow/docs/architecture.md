# leaf² (leaf-flow) 架构设计文档

> 版本: 0.9.4  
> 最后更新: 2025年7月

---

## 1. 项目概述

leaf² (leaf-flow) 是一个轻量级的声明式执行框架（**l**ightweight **ea**sy **f**ast and **f**lexible execution framework），核心目标是为 CLI/TUI 工具通过 YAML 配置快速生成结构化的 Web 交互界面。

### 设计理念

- **声明式配置**：通过 YAML 蓝图（Bud）定义工具的命令行参数和流程，而非编写代码
- **轻量快速**：1秒启动，无需 Python 等重型运行时
- **界面即配置**：YAML 配置直接驱动 UI 渲染，实现"配置即界面"
- **流程编排**：支持多模块串行执行和数据传递（mmap 机制）

---

## 2. 技术栈

| 层次 | 技术 | 用途 |
|------|------|------|
| **后端** | Go 1.25+ / Fiber v3 | HTTP API 服务器、任务调度、命令执行 |
| **前端** | SolidJS 1.9+ / TypeScript 5.9 | 响应式 Web UI |
| **构建** | Vite (Rolldown) + Bun | 前端构建和运行 |
| **样式** | Tailwind CSS 4.x | UI 样式 |
| **路由** | @solidjs/router | 前端路由 |
| **配置解析** | yaml.v3 (Go) / yaml.js (JS) | YAML 蓝图解析 |
| **伪终端** | go-pty / creack/pty | PTY 终端模拟 |
| **共享内存** | mmap-go | 进程间 mmap 数据传递 |
| **命令执行** | Go os/exec | 子进程管理 |

---

## 3. 项目结构

```
leaf-flow/
├── bud/                          # 蓝图目录 - YAML 配置存放处
│   ├── leaf/                     #   项目蓝图 (Project)
│   │   ├── demo.yaml             #     示例项目
│   │   ├── MSST.yaml
│   │   ├── N_m3u8DL-RE.yaml
│   │   ├── rift-svc.yaml
│   │   └── so-vits-svc.yaml
│   └── sprig/                    #   流蓝图 (Flow)
│       └── demo.flow.yaml
│
├── scheduler/                    # 调度器 - Go 后端
│   ├── main.go                   #   入口：Fiber 服务器启动、路由注册
│   ├── config.yaml               #   配置文件
│   ├── go.mod / go.sum           #   Go 模块依赖
│   ├── routes/                   #   HTTP API 路由层
│   │   ├── api.go                #     统一路由注册、响应格式
│   │   ├── execution.go          #     任务执行 API
│   │   ├── project.go            #     蓝图项目/流查询 API
│   │   └── files.go              #     文件系统浏览 API
│   ├── execution/                #   执行引擎核心
│   │   ├── task_queue.go         #     任务队列管理
│   │   ├── exec.go               #     基础执行器 (shell=no)
│   │   ├── exec_shared.go        #     共享 Shell 执行器
│   │   ├── exec_pty.go           #     PTY 伪终端执行器
│   │   ├── exec_windows.go       #     Windows 平台特化
│   │   ├── exec_nonwindows.go    #     非 Windows 平台特化
│   │   ├── log_manager.go        #     日志广播与持久化
│   │   ├── mmap.go               #     内存映射文件管理
│   │   └── helpers.go            #     工具函数
│   └── utils/                    #   工具库
│       ├── config.go             #     配置加载与解析
│       └── color.go              #     终端彩色输出
│
├── ui/                           # 前端 - SolidJS 应用
│   ├── package.json              #   依赖与脚本
│   ├── vite.config.ts            #   Vite 构建配置
│   ├── tailwind.config.js        #   Tailwind 样式配置
│   ├── src/
│   │   ├── main.tsx              #   应用入口
│   │   ├── App.tsx               #   根组件：导航栏、布局
│   │   ├── style.css             #   全局样式
│   │   ├── types/                #   TypeScript 类型定义
│   │   │   ├── api.ts            #     API 响应类型
│   │   │   ├── project.ts        #     项目/模块/参数类型
│   │   │   ├── execution.ts      #     任务/执行状态类型
│   │   │   └── file.ts           #     文件系统类型
│   │   ├── apis/                 #   API 调用层
│   │   │   ├── execution.ts      #     执行相关 API
│   │   │   ├── project.ts        #     蓝图项目/流 API
│   │   │   └── file.ts           #     文件系统 API
│   │   ├── stores/               #   响应式状态管理
│   │   │   ├── project.ts        #     项目数据 Store
│   │   │   ├── flow.ts           #     流数据 Store
│   │   │   ├── config.ts         #     配置 Store
│   │   │   └── executionLog.ts   #     执行日志 Store
│   │   ├── utils/                #   工具函数
│   │   │   ├── config.ts         #     YAML 解析与合并
│   │   │   ├── dynamicBind.ts    #     动态绑定引擎
│   │   │   ├── execution.ts      #     命令渲染/参数管理
│   │   │   ├── metaHelper.ts     #     元数据处理
│   │   │   ├── path.ts           #     路径工具
│   │   │   ├── constants.ts      #     常量
│   │   │   ├── routes/           #     路由定义
│   │   │   │   ├── index.tsx     #       路由表
│   │   │   │   └── guard.tsx     #       路由守卫
│   │   │   └── hooks/            #     自定义 Hooks
│   │   │       ├── useCommandExecution.ts  # 命令执行 Hook
│   │   │       ├── useDecodedParams.ts     # 解码路由参数
│   │   │       ├── useLazyLoad.ts          # 延迟加载
│   │   │       ├── useMessage.tsx          # 消息提示
│   │   │       ├── useScroll.ts            # 横向滚动
│   │   │       └── useStickyBottom.ts      # 底部粘性定位
│   │   ├── views/                #   页面组件
│   │   │   ├── Home.tsx          #     首页：项目/流卡片展示
│   │   │   ├── TaskQueue.tsx     #     任务队列详情页
│   │   │   ├── NotFound.tsx      #     404 页面
│   │   │   ├── project/          #     项目页面
│   │   │   │   ├── Project.tsx         # 模块参数编辑页
│   │   │   │   ├── ProjectConfig.tsx   # 项目配置页
│   │   │   │   └── ProjectLayout.tsx   # 项目布局容器
│   │   │   └── flow/             #     流页面
│   │   │       ├── Flow.tsx           # 分支模块参数页
│   │   │       ├── FlowConfig.tsx     # 流配置页
│   │   │       └── FlowLayout.tsx     # 流布局容器
│   │   └── components/           #   可复用组件
│   │       ├── CardSection.tsx   #     卡片区域容器
│   │       ├── ContextMenu.tsx   #     右键菜单
│   │       ├── ExecuteActionBar.tsx   # 执行操作栏
│   │       ├── ExecutionHistoryDropdown.tsx # 执行历史下拉
│   │       ├── ExportDialog.tsx  #     导出对话框
│   │       ├── GlobalLogs.tsx    #     全局日志
│   │       ├── argumentInput/    #     参数输入组件簇
│   │       │   ├── index.tsx     #       统一入口
│   │       │   ├── types.ts      #       类型定义
│   │       │   ├── StringInput.tsx, NumberInput.tsx, ...
│   │       │   └── FileBrowser.tsx  #   文件浏览器
│   │       ├── view/             #     视图相关组件
│   │       │   ├── LeafCard.tsx  #       项目/流卡片
│   │       │   ├── FlowHeader.tsx, ProjectHeader.tsx  # 头部
│   │       │   ├── ModuleTabs.tsx, BranchTabs.tsx     # 标签导航
│   │       │   └── ModuleMeta.tsx    # 元数据编辑
│   │       └── common/           #     通用基础组件
│   │           ├── NButton.tsx, NCheckbox.tsx
│   │           ├── NPopover.tsx, NMarkdown.tsx
│   │           └── Icons.tsx
│   │
│   ├── public/                   #   静态资源
│   └── index.html                #   HTML 入口
│
├── assets/                       # 项目资源文件
│   ├── banner.png
│   ├── bud-manual_zh-CN.md       #   蓝图编写手册
│   └── screenshots/              #   截图
│
└── README.md                     # 项目介绍
```

---

## 4. 分层架构

### 4.1 架构总览

```
┌──────────────────────────────────────────────────────────────────┐
│                        浏览器 (Browser)                          │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │              SolidJS 前端应用 (Solid 1.9)                  │  │
│  │  ┌──────┐ ┌──────────┐ ┌──────────┐ ┌────────────────┐   │  │
│  │  │Views │ │Components│ │  Stores  │ │  Utils/APIs    │   │  │
│  │  │ 页面  │ │  UI组件   │ │  状态管理  │ │  API调用/工具   │   │  │
│  │  └──────┘ └──────────┘ └──────────┘ └────────────────┘   │  │
│  └──────────────────────────┬───────────────────────────────┘  │
│                             │ HTTP / SSE                        │
└─────────────────────────────┼──────────────────────────────────┘
                              │
┌─────────────────────────────┼──────────────────────────────────┐
│                 Go 调度器 (Scheduler)                           │
│  ┌──────────────────────────┴───────────────────────────────┐  │
│  │              Fiber HTTP 服务器 (v3)                      │  │
│  │  ┌──────────────┐ ┌────────────┐ ┌────────────────┐     │  │
│  │  │ /api/execution│ │/api/projects│ │ /api/file/...  │     │  │
│  │  │  执行路由     │ │ 项目路由    │ │ 文件系统路由    │     │  │
│  │  └──────┬───────┘ └────────────┘ └───────┬────────┘     │  │
│  │         │                                 │              │  │
│  └─────────┼─────────────────────────────────┼──────────────┘  │
│            │                                 │                 │
│  ┌─────────▼─────────────────────────────────▼──────────────┐  │
│  │              执行引擎 (Execution Engine)                  │  │
│  │  ┌─────────────┐ ┌────────────┐ ┌──────────────────┐    │  │
│  │  │  TaskQueue   │ │ Executors  │ │  LogManager      │    │  │
│  │  │  任务队列     │ │ 执行器     │ │  日志管理/广播    │    │  │
│  │  └──────┬──────┘ └──┬────┬───┘ └──────────────────┘    │  │
│  │         │          │    │                               │  │
│  │         │   ┌──────┘    └──────┐                       │  │
│  │         │   │                  │                       │  │
│  │  ┌──────┴───▼──┐  ┌───────────▼────┐                  │  │
│  │  │ SharedExec  │  │  PtyExecutor   │                  │  │
│  │  │ 共享Shell   │  │  伪终端执行器   │                  │  │
│  │  └──────┬──────┘  └───────┬───────┘                  │  │
│  └─────────┼─────────────────┼──────────────────────────┘  │
│            │                 │                              │
│  ┌─────────▼─────────────────▼──────────────────────────┐  │
│  │              操作系统子进程 (OS Subprocesses)          │  │
│  │     cmd.exe / powershell / bash / sh / ...           │  │
│  └──────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────┘
```

### 4.2 前端架构 (SolidJS)

#### 路由系统

```
/                          → HomeView (首页，显示所有蓝图卡片)
/tasks                    → TaskQueueView (任务队列详情)
/flows/:flow              → FlowLayout + FlowView (流页面)
  /:branch                →   选定分支的模块参数页
/projects/:project        → ProjectLayout + ProjectView (项目页面)
  /:module                →   选定模块的参数编辑页
*                         → NotFoundView (404)
```

路由守卫机制：
- `withProjectGuard`：在进入项目页面前验证项目是否存在并可加载
- `withFlowGuard`：在进入流页面前验证流是否存在并可加载

#### 状态管理

采用 SolidJS 的 `createSignal` + `createStore` 模式，没有使用外部状态管理库：

| Store | 文件 | 职责 |
|-------|------|------|
| `projectStore` | `stores/project.ts` | 缓存已加载的 Project 数据，提供 `fetch/load/get` 方法 |
| `flowStore` | `stores/flow.ts` | 缓存已加载的 Flow 数据 |
| `configStore` | `stores/config.ts` | 后端配置（地址、目录等） |
| `executionLogStore` | `stores/executionLog.ts` | 实时日志流与任务状态事件 |

#### 组件层级

```
App (根组件：导航栏 + 底部信息)
├── HomeView
│   ├── CardSection (项目卡片区域)
│   │   └── LeafCard × N (每个蓝图卡片)
│   ├── CardSection (流卡片区域)
│   │   └── LeafCard × N
│   ├── ContextMenu (右键菜单)
│   └── ExportDialog (导出对话框)
│
├── ProjectLayout
│   └── ProjectView
│       ├── ProjectHeader (项目信息头部)
│       ├── ModuleTabs (模块切换标签)
│       ├── ArgumentRow × N (参数输入行)
│       │   └── (StringInput / NumberInput / BooleanInput /
│       │       SelectInput / RadioInput / SwitchInput /
│       │       SliderInput / PathInput / FileBrowser)
│       ├── ExecutionHistoryDropdown (历史参数下拉)
│       └── ExecuteActionBar (执行操作栏：预览/执行/保存)
│
├── FlowLayout
│   └── FlowView
│       ├── FlowHeader (流信息头部)
│       ├── BranchTabs (分支切换标签)
│       ├── ModuleMeta (流级元数据编辑)
│       ├── ... (同 ProjectView 的参数编辑区)
│       └── ExecuteActionBar
│
└── TaskQueueView
    └── GlobalLogs (实时日志 + 任务列表)
```

### 4.3 后端架构 (Go/Fiber)

#### 路由分组

| 前缀 | 文件 | 方法 | 路由 | 功能 |
|------|------|------|------|------|
| `/api` | routes/api.go | GET | `/ping` | 健康检查 |
| | routes/execution.go | POST | `/execution` | 提交执行任务 |
| | | GET | `/execution` | 查询任务队列 |
| | | DELETE | `/execution?taskId=` | 取消任务 |
| | | POST | `/execution/input` | 向运行中任务输入 |
| | | POST | `/execution/resize` | 调整终端尺寸 |
| | | GET | `/execution/logs-event` | SSE 任务状态事件流 |
| | | GET | `/execution/logs-stream` | 日志字节流 |
| | routes/project.go | GET | `/projects` | 查询蓝图 |
| | | POST | `/projects` | 保存新蓝图 |
| | | GET | `/config` | 获取服务器配置 |
| | routes/files.go | GET | `/file` | 读取文件内容 |
| | | GET | `/directory` | 列出目录 |
| | | POST | `/directory` | 创建文件/目录 |
| | | GET | `/path-absolute` | 转绝对路径 |
| | | GET | `/path-type` | 判断路径类型 |
| 静态 | main.go | — | `/` | 静态文件服务 (UI 构建产物) |

#### API 响应格式

所有 API 统一使用 `JSONResponse` 封装：

```json
{
  "success": true,
  "message": "操作提示信息",
  "data": { ... }
}
```

#### 执行器体系

三种执行器模式，通过 `shell` 字段选择：

```
                          IExecutor (接口)
                              │
          ┌───────────────────┼───────────────────┐
          │                   │                   │
    SharedExecutor       Executor            PtyExecutor
    (共享 Shell)       (直接执行, shell=no)   (伪终端)
          │                                   │
    cmd.exe / bash/sh                    go-pty 包装
    标准管道 I/O                         PTY 全双工 I/O
    支持输入/输出                       支持终端 resize
    逐命令写入 stdin                    全终端模拟
```

执行器选择逻辑 (`InitExecutor`)：
1. `shell` 以 `pty:` 开头 → `PtyExecutor`（使用指定 shell）
2. `shell` 为 `no` → `Executor`（直接 `exec.CommandContext`）
3. 其他 → `SharedExecutor`（在共享 shell 进程中执行）

---

## 5. 核心概念与数据流

### 5.1 蓝图体系 (Bud)

leaf² 的核心抽象是 **Bud（蓝图）**，以 YAML 文件组织：

```
bud/
├── leaf/           # Project 蓝图 — 单一任务的模块集合
│   └── demo.yaml
│       ├── name: 项目名称
│       ├── desc: 项目描述
│       ├── meta: 项目级元参数
│       └── modules: 模块列表
│           ├── key: 模块标识
│           ├── name: 模块名称
│           ├── template: 命令模板 (支持 #{param} 替换)
│           ├── shell: 执行器选择
│           ├── arguments: 参数列表
│           │   ├── key/name/desc: 标识
│           │   ├── dtype: 类型 (string/number/file/directory/boolean)
│           │   ├── method: 交互方式 (input/slide/radio/select/mmap/switch)
│           │   ├── value: 默认值
│           │   ├── required/template/multiple: 辅助属性
│           │   └── options/min/max/step: 约束
│           └── dynamicBind: 动态绑定规则
│
└── sprig/          # Flow 蓝图 — 多分支流程编排
    └── demo.flow.yaml
        ├── name/desc/meta
        └── branches: 分支列表
            ├── key/name/desc
            └── modules: 模块引用
                ├── key: "project.module" 引用语法
                ├── template/arguments: 参数覆盖
                └── dynamicBind: 新增动态绑定
```

### 5.2 数据流：从 YAML 到命令执行

```
┌──────────┐    ┌──────────────┐    ┌───────────┐    ┌────────────┐
│ bud/*.yaml│───▶│  config.ts   │───▶│   Stores  │───▶│   Views    │
│ YAML 蓝图  │    │ YAML 解析合并  │    │ 响应式缓存  │    │ 参数编辑页面 │
└──────────┘    └──────────────┘    └──────┬────┘    └──────┬─────┘
                                           │               │ 用户输入
                                           │               ▼
                                           │       ┌──────────────┐
                                           │       │  execution.ts │
                                           │       │ 模板替换渲染    │
                                           │       │ = 生成命令字符串 │
                                           │       └──────┬───────┘
                                           │               │
                                           │               ▼
                                           │       ┌──────────────┐
                                           │       │  POST /exec   │
                                           │       │  → 后端调度器  │
                                           │       └──────┬───────┘
                                           │               │
                                           ▼               ▼
                                    ┌──────────────────────────┐
                                    │     TaskQueue            │
                                    │  1. 添加任务到队列        │
                                    │  2. 串行取出执行           │
                                    │  3. 选择 Executor 执行     │
                                    │  4. 广播日志/状态          │
                                    └──────────────────────────┘
```

### 5.3 模板渲染流程 (前端)

```
用户输入参数值
       │
       ▼
gatherArgumentStatus(params, meta)
       │ 逐参数计算: rawValue → 应用 template → renderValue
       │ 合并 meta 级参数
       ▼
ArgumentRenderMap (参数名 → {rawValue, value, preRendered})
       │
       ▼
renderCommand(template, amap)
       │ 正则匹配 #{paramKey} 占位符
       │ └─ 单值: 直接替换
       │ └─ 数组: JSON.stringify + 引号处理
       │ 空值处理 / 引号转义
       ▼
完整命令字符串 → POST /api/execution
```

### 5.4 用户输入参数模板处理

参数支持 `template` 字段，在传入主命令模板之前预处理值：

```
原始值 "tachibana"
    │
    ▼
参数 template: '--save #{}'
    │  #{} 简写 = #{key} = #{pkg}
    │  → '--save tachibana'
    ▼
模块 template: 'bun a #{pkg}'
    │  → 'bun a --save tachibana'
    ▼
最终命令
```

### 5.5 动态绑定机制 (DynamicBind)

参数间可建立动态绑定关系，当一个参数属性变化时自动更新另一个参数：

```
from (数据源)
  ├── URL (http/https) → fetch JSON
  ├── 本地文件 → 读取 json/yaml
  ├── 本地目录 → 列出文件名
  └── #{param.attr} → 直接引用本模块另一参数的属性
       └── #{{param.attr}} → 间接引用 (值作为 url/path 再次解析)
           │
           ▼
fromRule (可选, lodash _.get 语法)
  → 从数据源提取值路径
  → 支持 keys() / values() / len() 函数
      │
      ▼
to (目标)
  → argumentKey.targetAttr
       │
       ▼
setFunc(aIndex, propKey, value)
  → 更新 Store 中的参数属性
  → 触发 UI 重新渲染
```

### 5.6 执行流程 (后端)

```
POST /api/execution { commands, shell }
       │
       ▼
TaskQueue.AddTask()
  → 创建 Task (status=pending)
  → 加入队列末尾
  → 如果队列空闲则立即 process()
       │
       ▼
TaskQueue.process()
  → 取出下一个 pending 任务
  → 标记 running
  → 调用 executeTask()
       │
       ▼
TaskQueue.executeTask()
  → ResetMmap() (清空共享内存)
  → ReplaceMmapMarker (替换 ?MMAP_FILE? 为实际路径)
  → 逐条命令调用 executor.Add(cmd)
  → 调用 executor.Run()
       │
       ├──→ Executor.Run() (shell=no)
       │    exec.CommandContext(ctx, name, args...)
       │    逐命令创建新进程
       │
       ├──→ SharedExecutor (默认)
       │    在共享 shell (cmd/powershell/bash) 中
       │    逐命令写入 stdin → 进程执行 → 读取 stdout/stderr
       │
       └──→ PtyExecutor (pty:)
            创建 PTY 会话 → 写入命令 → 读取全双工输出
            支持 resize / 交互式输入
       │
       ▼
   完成 → 广播任务状态事件 (SSE)
       → Broadcast stdout/stderr (日志流)
       → 标记 completed / failed / cancelled
       → 处理下一个队列任务
```

### 5.7 MMAP 共享内存机制

Flow 多模块串行执行时，通过 mmap 实现模块间数据传递：

```
模块A (写)                         模块B (读)
  │                                 │
  │ 写入文件内容到                    │ 读取文件内容
  │ mmap 共享内存                    │ 从 mmap 共享内存
  │                                 │
  └─────────► temp.leaf.mmap ◄──────┘
              (内存映射文件)
                 │
            mmapMarker:
            ?MMAP_FILE?
                 │
         执行时替换为实际路径
```

工作流程：
1. 参数 `method: mmap` 的 `file` 类型参数触发 mmap 模式
2. 执行前 `ResetMmap()` 清空共享内存
3. 命令中的 `?MMAP_FILE?` 占位符被替换为 `temp.leaf.mmap` 的绝对路径
4. 前一模块的输出文件路径会通过 mmap 传递到后一模块的输入

---

## 6. 关键设计决策

### 6.1 为什么前后端分离？

- **前端 (SolidJS)**：负责交互界面、YAML解析、模板渲染、动态绑定，运行在浏览器中
- **后端 (Go)**：负责任务排队、进程管理、日志广播、文件服务
- 前端通过 HTTP API 与后端通信，后端无状态（除了任务队列）
- 静态文件由 Go/Fiber 直接 serve，单二进制部署

### 6.2 为什么选择 Fiber v3？

- 轻量级 HTTP 框架，性能接近原生 net/http
- 内置 CORS、静态文件服务、中间件支持
- 单一二进制部署，无需外部依赖

### 6.3 任务队列的设计

- **单 Goroutine 消费**：保证命令串行执行，避免竞态
- **FIFO 队列**：`tasks []*Task` 切片实现
- **可取消**：支持正在运行和等待中的任务取消
- **SSE 事件通知**：通过 LogManager 广播任务状态变更

### 6.4 实时日志的设计

- **双通道**：SSE 事件流 (`logs-event`) + 字节流 (`logs-stream`)
- **SSE**：推送结构化的任务状态事件（JSON）
- **字节流**：推送原始 stdout/stderr 输出（chunked transfer）
- **重连机制**：前端自动重连（最多 3 次）
- **本地持久化**：可选日志文件记录

### 6.5 Flow 的模块引用与合并

Flow 的 Branch 引用 Project 中的 Module，通过深拷贝 + 覆盖实现：

```
引用: "demo.boolean-input"
       │
       ▼
parseFlows → 从已加载的 Project 中找到对应 Module
       │
       ▼
_.cloneDeep(sMod) → 深拷贝原始模块
       │
       ▼
覆盖 fields (name/desc/template/shell/disabled/dynamicBind)
       │
       ▼
mergeArgument (逐 key 覆盖 argument 属性)
       │
       ▼
resolveModuleMeta (合并 Project.meta + Flow.meta)
```

---

## 7. 部署与运行

### 开发模式

```bash
# 后端 (Go)
cd scheduler && go run main.go

# 前端 (Bun + Vite)
cd ui && bun install && bun dev
```

### 生产构建

```bash
# 构建前端
cd ui && bun run build

# 构建后端
cd scheduler && go build -o leaf.exe
```

### 启动流程

1. `main.go` 读取 `config.yaml`
2. 启动 Console 输入桥接（支持向运行中进程输入）
3. 初始化 Fiber 服务器，注册路由
4. 注册静态文件服务 → UI 构建产物
5. 自动打开浏览器（`openBrowser: true`）
6. 监听退出信号，优雅关闭

---

## 8. 扩展点

| 扩展点 | 位置 | 方式 |
|--------|------|------|
| 新输入组件 | `ui/src/components/argumentInput/` | 实现新的 dtype/method 组件 |
| 新执行器 | `scheduler/execution/` | 实现 `IExecutor` 接口 |
| 新蓝图类型 | `bud/*/` | 编写 YAML 配置文件 |
| 新 API 路由 | `scheduler/routes/` | 添加路由处理函数并注册 |
| 动态绑定数据源 | `ui/src/utils/dynamicBind.ts` | 扩展 `fetchDataSource` 支持新协议 |

---

## 9. 依赖关系图

```
Go 后端依赖:
  fiber v3 ──► fasthttp ──► 高性能 HTTP
  go-pty     ──► creack/pty ──► PTY 终端
  mmap-go    ──► 内存映射文件
  yaml.v3    ──► YAML 解析

前端依赖:
  solid-js        ──► 响应式 UI
  @solidjs/router ──► 前端路由
  tailwindcss     ──► 实用优先 CSS
  yaml.js         ──► YAML 解析 (浏览器端)
  lodash          ──► 工具函数 (_.get, _.cloneDeep)
  @xterm/xterm    ──► 终端模拟 (任务详情页)
```

---

> 本文档覆盖了 leaf-flow v0.9.4 的完整架构。如需了解蓝图编写细节，请参阅 [蓝图手册](../assets/bud-manual_zh-CN.md)。
