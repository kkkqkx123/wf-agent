# leaf² (leaf-flow) 功能文档

> 版本: 0.9.4  
> 最后更新: 2025年7月

---

## 目录

1. [蓝图管理 (Bud)](#1-蓝图管理-bud)
2. [模块参数编辑](#2-模块参数编辑)
3. [命令模板渲染](#3-命令模板渲染)
4. [动态绑定引擎](#4-动态绑定引擎)
5. [执行系统](#5-执行系统)
6. [实时日志与终端](#6-实时日志与终端)
7. [任务队列管理](#7-任务队列管理)
8. [MMAP 共享内存](#8-mmap-共享内存)
9. [Flow 流程编排](#9-flow-流程编排)
10. [文件系统浏览器](#10-文件系统浏览器)
11. [Meta 元数据管理](#11-meta-元数据管理)
12. [参数暂存与历史记录](#12-参数暂存与历史记录)
13. [导出与保存](#13-导出与保存)
14. [首页卡片与懒加载](#14-首页卡片与懒加载)
15. [右键菜单与快捷操作](#15-右键菜单与快捷操作)
16. [配置中心](#16-配置中心)
17. [Console 输入桥接](#17-console-输入桥接)
18. [路由与导航](#18-路由与导航)

---

## 1. 蓝图管理 (Bud)

### 1.1 蓝图文件系统

蓝图以 YAML 文件形式存储在 `bud/` 目录下，分为两类：

| 类型 | 目录 | 文件模式 | 用途 |
|------|------|----------|------|
| **Project** | `bud/leaf/` | `*.yaml` / `*.yml` | 定义独立项目的模块和参数 |
| **Flow** | `bud/sprig/` | `*.flow.yaml` / `*.flow.yml` | 定义多分支流程编排 |

### 1.2 Project 蓝图结构

一个 Project 对应一个可执行的 CLI 工具，包含以下字段：

```yaml
# bud/leaf/demo.yaml
name: Demo Tool          # 项目名称
desc: |                  # 项目描述（支持 Markdown）
  # Demo
  一个示例工具，演示叶子的功能
meta:                    # 项目级元参数（可选的全局变量）
  output_dir:
    name: 输出目录
    dtype: directory
    value: ./output
modules:                 # 模块列表
  - key: download        # 模块标识
    name: 下载           # 模块显示名称
    desc: 下载模块       # 模块描述
    template:            # 命令模板（支持字符串或数组）
      - 'yt-dlp #{url} -o #{output}'
    shell: auto          # 执行器选择
    disabled: false      # 是否禁用
    arguments:           # 参数列表
      - key: url
        name: 视频链接
        desc: 要下载的视频URL
        dtype: string    # 数据类型
        required: true
      - key: output
        name: 输出路径
        dtype: file
        method: mmap     # 内存映射文件模式
        template: '--output #{}'  # 参数自身模板
```

### 1.3 Flow 蓝图结构

Flow 用于编排多个 Project 的模块，支持分支和模块引用：

```yaml
# bud/sprig/demo.flow.yaml
name: Demo Workflow
desc: 多步骤工作流示例
branches:
  - key: step1
    name: 第一步
    desc: 准备数据
    modules:
      - key: "demo.download"    # 引用 project.module
        template: 'python download.py #{url}'
        arguments:
          - key: url
            value: "https://example.com/data"
  - key: step2
    name: 第二步
    desc: 处理数据
    modules:
      - key: "demo.process"
        disabled: false
```

### 1.4 蓝图获取流程

```
后端                              前端
┌──────────┐                    ┌──────────┐
│ config   │                    │  Home    │
│ .yaml    │                    │  View    │
│  budDir  │                    │          │
└────┬─────┘                    └────┬─────┘
     │                               │
     ▼                               ▼
┌──────────┐    GET /api/projects ┌──────────┐
│ Scan     │◄──────────────────── │ fetch    │
│ bud/leaf/│   names[]            │ Base     │
│ & sprig/ │                      │ Project  │
└────┬─────┘ ──────────────────► └────┬─────┘
     │    list of Project/Flow        │
     │    (仅 name/desc/meta)         │
     │                               │
     ▼                               ▼
┌──────────┐  GET /api/project/  ┌──────────┐
│ Read &   │◄────  key ───────── │ parse    │
│ Parse    │   yaml content      │ Flows    │
│ YAML     │ ──────────────────► │ /Config  │
└──────────┘   Full YAML         └──────────┘
                                    │
                                    ▼
                               ┌──────────┐
                               │ Store    │
                               │ (响应式)  │
                               └──────────┘
```

---

## 2. 模块参数编辑

### 2.1 参数数据类型 (`dtype`)

| 类型 | 描述 | 默认 UI 组件 |
|------|------|-------------|
| `string` | 字符串 | 文本输入框 |
| `number` | 数值 | 数字输入框 / 滑块 (slide) |
| `boolean` | 布尔值 | 复选框 / 开关 (switch) |
| `file` | 文件路径 | 路径选择 + 文件浏览器 |
| `directory` | 目录路径 | 路径选择 + 文件浏览器 |

### 2.2 参数交互方式 (`method`)

| 方式 | 适用类型 | UI 组件 | 说明 |
|------|---------|---------|------|
| `input` (默认) | 所有 | 输入框 | 自由文本输入 |
| `slide` | number | 滑块 | 在 `min`/`max` 范围内滑动选择 |
| `radio` | 所有 | 单选框 | 从 `options` 列表中单选 |
| `select` | 所有 | 下拉菜单 | 从 `options` 列表中单选 |
| `switch` | boolean | 开关按钮 | 视觉化的开关 |
| `mmap` | file | 路径选择 | 共享内存文件模式 |

### 2.3 参数属性

每个参数可配置以下属性：

| 属性 | 类型 | 说明 |
|------|------|------|
| `key` | string | 参数唯一标识符 |
| `name` | string | 显示名称 |
| `desc` | string | 参数描述（鼠标悬停 Popover 显示） |
| `dtype` | enum | 数据类型 |
| `method` | enum | 交互方式 |
| `value` | any | 默认值（支持数组表示多值） |
| `required` | boolean | 是否必填 |
| `template` | string | 值模板（`#{}` 简写为参数自身） |
| `multiple` | boolean | 支持多值（ArrayWrapper） |
| `options` | array | 单选/多选的选项列表 |
| `min/max/step` | number | 数值约束 |
| `dir` | string | 文件浏览器的初始目录 |
| `dynamicBind` | array | 动态绑定规则 |

### 2.4 参数输入组件体系

参数输入使用 `getComponent(dtype, method)` 工厂模式根据数据类型和交互方式选择组件：

```
ArgumentRow
  ├─ dtype=file|directory ─── PathInput
  │                            └─ FileBrowser (按钮打开)
  ├─ method=select ─────────── SelectInput
  ├─ method=radio ──────────── RadioInput
  ├─ dtype=string ──────────── StringInput
  ├─ dtype=number + slide ──── SliderInput
  ├─ dtype=number ──────────── NumberInput
  ├─ dtype=boolean + switch ── SwitchInput
  ├─ dtype=boolean ─────────── BooleanInput
  └─ multiple=true (非 radio/file) ─── ArrayWrapper
                                          └─ innerComponent × N
```

---

## 3. 命令模板渲染

### 3.1 模板语法

命令模板支持 `#{paramKey}` 占位符替换：

```
# 命令模板
yt-dlp #{url} -o #{output}

# 参数模板（参数独立处理）
template: '--output #{output}'
# 简写：template: '--output #{}'   (= --output #{key}=--output #{url})
```

### 3.2 渲染流程

```
模板: 'yt-dlp #{url} -o #{output}'
        │
        ▼
正则匹配所有 #{paramKey} 占位符
        │
        ▼
对每个匹配的参数:
  1. 获取 rawValue (用户输入值)
  2. 应用参数 template (预处理):
     例: output.template = '--output #{}'
         rawValue = './output.mp4'
         → preRendered = '--output ./output.mp4'
  3. 合并参数值:
     value = preRendered || rawValue
        │
        ▼
替换模板中的占位符:
  #{url}    → https://example.com/video
  #{output} → --output ./output.mp4
        │
        ▼
最终命令: 'yt-dlp https://example.com/video -o --output ./output.mp4'
```

### 3.3 特殊值处理

| 场景 | 处理方式 |
|------|---------|
| **空值** | 替换为 `''` 空字符串 |
| **数组值** | JSON.stringify 序列化 + 智能引号处理 |
| **引号嵌套** | 自动转义内部引号 |
| **布尔值** | `true` / `false` 字符串化 |

### 3.4 多模板命令

一个模块可以包含多个命令模板（数组），每个模板独立渲染后依次执行：

```yaml
template:
  - 'cd #{work_dir}'
  - 'python download.py #{url}'
  - 'python process.py #{input_dir}'
```

### 3.5 预览功能

用户在"执行"前可点击"预览"按钮，查看渲染后的完整命令字符串，支持复制到剪贴板。

---

## 4. 动态绑定引擎

### 4.1 概述

动态绑定 (DynamicBind) 是 leaf² 的特色功能，允许参数的某个属性变化时自动触发另一个参数的更新，实现类似 Excel 公式的联动效果。

### 4.2 配置语法

在模块的 `dynamicBind` 字段中定义绑定规则：

```yaml
dynamicBind:
  - from: url.value           # 数据源: 本模块另一参数的某属性
    to: playlist.value        # 目标: 本模块目标参数的某属性
  - from: url.value           # 间接引用: 值作为 url/path
    to: playlist.options      # 目标属性可以是 options (下拉选项)
    fromRule: "keys(playlists)"   # 提取规则
  - from: "#{url.value}"      # 直接绑定 (同 from: url.value)
    to: playlist.value
  - from: "#{{url.value}}"    # 间接绑定: url 的值作为远程地址获取 JSON
    to: playlist.options
    fromRule: "items[].name"  # lodash _.get 路径提取
```

### 4.3 绑定模式

| 模式 | 语法 | 说明 |
|------|------|------|
| **直接绑定** | `#{param.attr}` | 直接引用参数值，同步更新目标 |
| **间接绑定** | `#{{param.attr}}` | 参数值作为 URL 或文件路径，fetch 解析后更新 |
| **恒等绑定** | 无 `#` | 直接使用 string 值作为数据源 |

### 4.4 数据源类型

| 数据源 | 处理方式 |
|--------|---------|
| HTTP URL | `fetch(url) → await response.json()` |
| 本地 JSON 文件 | `readFile(path) → JSON.parse(content)` |
| 本地 YAML 文件 | `readFile(path) → yaml.parse(content)` |
| 本地目录 | `listEntries(path) → [filename, ...]` |

### 4.5 提取规则 (`fromRule`)

对获取的数据应用 `lodash _.get` 路径提取，支持三种内置函数：

| 函数 | 说明 | 示例 |
|------|------|------|
| `keys(obj)` | 提取对象的所有键 | `keys(playlists)` → `["work","study"]` |
| `values(obj)` | 提取对象的所有值 | `values(data)` → `[...]` |
| `len(obj)` | 取对象/数组长度 | `len(items)` → `42` |

### 4.6 绑定生命周期

```
模块加载到 Store
    │
    ▼
dBind.init(cacheKey, setFunc, mobj)
    │ 解析所有 dynamicBind 规则
    │ 建立来源参数 → [目标绑定列表] 的映射
    │ 缓存到 variedBindCache
    ▼
用户修改参数
    │
    ▼
handleSetArgument(index, updates)
    │ 更新 Store 中的参数值
    │ 触发 UI 重新渲染
    │
    ▼
dBind.update(cacheKey, setFunc, mobj, aobj, changedKeys)
    │ 查找以 changedKeys 中属性为 srcAttr 的绑定
    │ 获取来源值 → (直接/间接) 获取数据 → 应用 fromRule
    │ 调用 updateBindTarget → setFunc 更新目标参数
    │ 触发 UI 重新渲染
    ▼
目标参数值更新，用户可见
```

---

## 5. 执行系统

### 5.1 三种执行器

| 执行器 | 触发条件 | 工作方式 | 适用场景 |
|--------|---------|---------|---------|
| **Executor** | `shell: no` | 每条命令独立创建子进程 (`exec.CommandContext`) | 简单快速命令，无需交互 |
| **SharedExecutor** | 默认 / 其他 | 在一个共享 Shell 进程中逐命令写入 stdin | 需要环境共享、顺序执行 |
| **PtyExecutor** | `shell: pty:bash` | 创建 PTY 伪终端会话，全双工 I/O | 交互式程序、需要终端模拟 |

### 5.2 执行器选择逻辑

```go
func InitExecutor(ctx context.Context, cancel context.CancelFunc, shell string) (IExecutor, error) {
    if strings.HasPrefix(shell, "pty:") {
        return newPtyExecutor(...)      // PTY 模式
    }
    if shell == "no" {
        return &Executor{...}           // 直接执行
    }
    return newSharedExecutor(...)        // 共享 Shell
}
```

### 5.3 SharedExecutor 工作流程

```
SharedExecutor
  │
  ├── shellMeta(shell) → cmd.exe / powershell / bash
  │   确定 Shell 路径和启动参数
  │
  ├── exec.CommandContext(ctx, name, args...)
  │   启动 Shell 进程
  │
  ├── 逐命令写入 stdin (Add command)
  │   Windows: cmd /c command
  │   Unix:    command; echo __CMD_DONE__
  │
  └── Run() → cmd.Wait()
      等待进程结束
```

### 5.4 PtyExecutor 工作流程

```
PtyExecutor
  │
  ├── openSession(cols, rows) → go-pty 创建 PTY
  │
  ├── 启动 shell 进程，绑定到 PTY
  │
  ├── 逐命令写入 PTY (Add command):
  │   Windows: cmd /c command\n
  │   Unix:    command\n
  │
  ├── readPtyChunks: 异步读取 PTY 输出
  │   通过 channel 发送给 flush 协程
  │
  ├── logPtyStream: 输出流处理
  │   清理终端控制字符
  │   解析状态码
  │   过滤 EOF 序列
  │
  ├── Resize(cols, rows): 调整 PTY 尺寸
  │
  ├── Input(input): 向 PTY 写入输入
  │
  └── Run():
       ├── shell=no → runDirectCommands
       │    直接 exec 非交互命令
       └── 其他 → runShellCommands
            在 PTY shell 中执行命令
```

### 5.5 任务提交 API

```
POST /api/execution
{
  "commands": ["yt-dlp #{url}", "ffmpeg -i #{input} #{output}"],
  "shell": "auto"
}
→ 返回 taskId, status, commands, shell
```

### 5.6 执行流程（完整链路）

```
用户点击"执行"
    │
    ▼
前端: validateRequiredArguments(arguments) → 检查必填参数
    │
    ▼
前端: gatherArgumentStatus → 收集所有参数值 + 合并 meta
    │
    ▼
前端: renderCommand(template, rmap) → 渲染命令字符串
    │
    ▼
前端: POST /api/execution { commands, shell }
    │
    ▼
后端: TaskQueue.AddTask → 创建 Task，加入队列
    │
    ▼
后端: 队列串行处理 → InitExecutor → Executor.Run()
    │
    ▼
后端: 日志/状态通过 SSE 实时推送
    │
    ▼
前端: 收到 onTaskStatusCallback → Store 更新
    │
    ▼
前端: GlobalLogs 终端显示实时输出
```

---

## 6. 实时日志与终端

### 6.1 双通道日志架构

```
后端 LogManager
  │
  ├── SubscribeStream()
  │   字节流 channel
  │   └─→ GET /api/execution/logs-stream (application/octet-stream)
  │       前端: fetch + ReadableStream.getReader() 逐块读取
  │       显示在: GlobalLogs 的 xterm 终端
  │
  └── SubscribeStatus()
      JSON 事件 channel
      └─→ GET /api/execution/logs-event (text/event-stream SSE)
          前端: EventSource 接收
          触发: TaskStatusEvent → executionLogStore → UI 更新
```

### 6.2 SSE 状态事件

```
事件格式 (SSE):
data: {"event":"task_status","taskId":42,"status":"running","timestamp":"..."}

支持的状态:
  pending    → 任务已加入队列
  running    → 任务正在执行
  completed  → 任务已完成
  failed     → 任务失败
  cancelled  → 任务被取消
```

### 6.3 xterm 终端集成

GlobalLogs 组件集成 `@xterm/xterm` 和 `@xterm/addon-fit`：

| 功能 | 实现 |
|------|------|
| **终端输出** | `terminal.write(log.bytes)` 逐块写入 |
| **自动大小** | ResizeObserver 监听容器变化 → `fitAddon.fit()` |
| **终端尺寸同步** | 浏览器端 → `POST /api/execution/resize { cols, rows }` → PTY 端 |
| **焦点管理** | 点击终端区域自动聚焦 |
| **滚动回看** | 6000 行回看缓冲区 |
| **输入转发** | `terminal.onData` → `POST /api/execution/input { input }` |
| **深色主题** | 自定义 Theme (深蓝背景 + 绿色文字) |
| **ConPTY 支持** | Windows 平台使用 ConPTY 后端 |
| **字体回退** | Maple Mono NF CN → Consolas → monospace |

### 6.4 重连机制

```
日志流 (fetch + reader):
  断开 → 最多重试 3 次 → 间隔 3 秒
  心跳: 服务端每 3 秒 flush 一次

状态流 (EventSource):
  onerror → 最多重试 3 次 → 间隔 3 秒
  心跳: SSE 注释行 ": heartbeat\n\n"
```

### 6.5 连接状态指示

| 状态 | 指示器 |
|------|--------|
| 已连接 | 绿色圆点 + "活跃" 标签 |
| 已断开 | 红色圆点 + "断开" 标签 + 脉冲动画 |

---

## 7. 任务队列管理

### 7.1 队列数据结构

```go
type TaskQueue struct {
    mu        sync.Mutex
    tasks     []*Task              // FIFO 队列
    active    bool                 // 是否正在处理
}

type Task struct {
    ID        int                  // 自增 ID
    Commands  []string             // 命令列表
    Shell     string               // Shell 选择
    Status    TaskStatus           // pending/running/completed/failed/cancelled
    CreatedAt, StartedAt, EndedAt  time.Time
    Error     string               // 错误信息
}
```

### 7.2 队列调度逻辑

```
AddTask(commands, shell)
  → 创建 Task (ID递增, status=pending)
  → 追加到队列末尾
  → 如果 active==false, 启动 process()
       │
       ▼
process()
  → active = true
  → 循环:
    1. 取第一个 pending 任务
       ├── 没有 → active = false, return
       └── 有 → executeTask(task)
                 │
                 ▼
    2. executeTask:
       a. ResetMmap() 清空 mmap
       b. ReplaceMmapMarker → ?MMAP_FILE? 替换为实际路径
       c. 逐命令 executor.Add(cmd)
       d. executor.Run()
          ├── 成功 → status = completed
          ├── 失败 → status = failed
          ├── 取消 → status = cancelled
          └── Broadcast 状态事件
       e. 回到 1
```

### 7.3 任务列表查询

分页查询，支持按 `taskId` 游标和 `limit` 控制：

```
GET /api/execution?taskId=5&limit=10
→ 返回 taskId > 5 的最多 10 条任务

GET /api/execution?taskId=10&limit=-5
→ 返回 taskId < 10 的最多 5 条任务（反向）
```

### 7.4 任务取消

```
DELETE /api/execution?taskId=42
→ 取消任务 #42
  ├── pending → 直接从队列移除
  └── running → executor.Exit() 终止进程
```

### 7.5 前端任务队列页面

前端 `/tasks` 页面展示完整任务列表：

| 功能 | 说明 |
|------|------|
| **任务列表** | 按创建时间倒序排列，分页加载（每页 20 条） |
| **状态颜色** | completed=绿色, failed=红色, running=绿色+脉冲, cancelled=橙色, pending=灰色 |
| **展开详情** | 点击任务行展开显示 Shell、时间、Commands、Error 详情 |
| **取消操作** | 对 pending/running 任务可点击取消（需确认） |
| **滚动分页** | 滚动到底部自动加载更多 |
| **实时状态** | 通过 SSE 事件实时更新任务状态 |

---

## 8. MMAP 共享内存

### 8.1 概述

MMAP (Memory-Mapped File) 机制用于 Flow 编排中，前一个模块的输出文件内容通过共享内存在进程间传递，供后一个模块获取。

### 8.2 工作流程

```
模块A (写端)                     模块B (读端)
  │                               │
  │ 1. 参数 method: mmap          │ 1. 参数 method: mmap
  │    dtype: file                │    dtype: file
  │    标记文件类型为 mmap        │    标记文件类型为 mmap
  │                               │
  │ 2. 命令中的 ?MMAP_FILE?       │ 2. 命令中的 ?MMAP_FILE?
  │    被替换为 mmap 文件路径     │    被替换为 mmap 文件路径
  │                               │
  │ 3. 运行命令产生输出文件       │ 3. 读取 mmap 文件内容
  │    如: ffmpeg -i input.mp4    │    获取模块A的输出路径
  │          output.mp4           │
  │                               │
  │ 4. 文件路径自动写入 mmap      │ 4. 将路径作为参数值传递
  │    (temp.leaf.mmap)           │    给模块B的命令
  └──────────────┬───────────────┘
                 │
         ┌───────▼────────┐
         │ temp.leaf.mmap  │
         │ (内存映射文件)   │
         └────────────────┘
```

### 8.3 mmap 文件管理

```go
type Mmap struct {
    file    *os.File         // 临时文件
    mmap    mmap.MMap        // 内存映射
    size    int64            // 当前大小
}

ResetMmap():   // 清空/初始化共享内存
  如果文件不存在 → 创建
  截断为 0
  映射到内存

CloseAndClean():  // 清理
  解除映射
  关闭文件
  删除临时文件
```

---

## 9. Flow 流程编排

### 9.1 分支结构

Flow 包含多个 Branch，每个 Branch 包含一组模块引用：

```
Flow "视频处理工作流"
  ├── Branch "下载"
  │   ├── Module "demo.download"
  │   ├── Module "demo.convert"
  │   └── Module "demo.upload"
  ├── Branch "处理"
  │   ├── Module "demo.extract_audio"
  │   └── Module "demo.transcode"
  └── Branch "发布"
      └── Module "demo.publish"
```

### 9.2 模块引用与合并

Flow 的 Branch 引用 Project 的 Module，不是直接复制而是通过深拷贝 + 覆盖：

1. **解析引用**: `"demo.boolean-input"` → project: `demo`, module: `boolean-input`
2. **查找源模块**: 从已加载的 Project 中找到对应 Module
3. **深拷贝**: `_.cloneDeep(sMod)` 复制一份
4. **字段覆盖**: 如果 Flow 的 YAML 中定义了同名字段，覆盖拷贝的版本
5. **参数合并**: 逐 key 合并参数属性
6. **元数据合并**: 合并 Project.meta + Flow.meta → Branch.meta

### 9.3 键名去重

当同一 Branch 中多次引用同一模块时，自动为后续实例添加 `#N` 后缀：

```
modules:
  - key: "demo.download"    → 模块 key: "download"
  - key: "demo.download"    → 模块 key: "download#2"
  - key: "demo.download"    → 模块 key: "download#3"
```

### 9.4 Branch 切换

通过 BranchTabs 组件在多个分支间切换，每个分支独立编辑和执行。

---

## 10. 文件系统浏览器

### 10.1 功能概述

FileBrowser 是一个完整的文件系统浏览和选择器，集成在文件/目录类型的参数输入中。

### 10.2 功能列表

| 功能 | 说明 |
|------|------|
| **目录树浏览** | 显示当前目录内容，支持目录图标和文件图标 |
| **路径导航** | 手动输入路径、面包屑导航 |
| **文件选择** | 点击文件/目录填入参数 |
| **搜索过滤** | 按名称实时过滤 |
| **目录切换** | 双击目录进入、面包屑回退 |
| **CD 命令** | 手动输入 cd 命令切换目录 |
| **创建文件/目录** | 在界面上直接创建新文件或目录 |
| **跨平台路径** | Windows (`C:\`) 和 Unix (`/`) 路径支持 |
| **当前目录感知** | 通过 `dir` 参数指定初始浏览目录 |

### 10.3 文件类型判断

根据文件扩展名自动判断类型并显示相应的图标：

| 分类 | 匹配 |
|------|------|
| 图片 | `.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.bmp`, `.svg` |
| 视频 | `.mp4`, `.avi`, `.mov`, `.mkv`, `.flv`, `.wmv` |
| 音频 | `.mp3`, `.wav`, `.flac`, `.aac`, `.ogg`, `.wma` |
| 代码 | `.py`, `.js`, `.ts`, `.go`, `.rs`, `.java`, `.c`, `.cpp`, `.html`, `.css` |
| 文本 | `.txt`, `.md`, `.json`, `.yaml`, `.yml`, `.xml`, `.csv`, `.toml`, `.ini` |
| 压缩 | `.zip`, `.rar`, `.7z`, `.tar`, `.gz` |
| 可执行 | `.exe`, `.msi`, `.sh`, `.bat` |

### 10.4 路径选择组件 (PathInput)

文件/目录类型参数默认使用 PathInput 组件：

```
PathInput
  ├── 显示当前值
  ├── 点击"浏览"按钮打开 FileBrowser 弹窗
  ├── 重置按钮清除当前值
  └── 支持手动输入路径
```

---

## 11. Meta 元数据管理

### 11.1 概述

Meta 系统允许在项目/流/分支层级定义额外的参数变量，这些变量可以被子模块引用。

### 11.2 Meta 层级

```
Project.meta           (项目级，所有模块可见)
  └── Flow.meta        (流级，所有分支可见)
       ├── Branch.meta (分支级，该分支所有模块可见)
       └── Module.meta (模块级，仅在单个命令渲染中使用)
```

### 11.3 Meta 合并规则

```go
resolveModuleMeta(rMod, p.meta) → 合并后的 meta
Object.assign(branchMeta, moduleMeta) → 加上分支级 meta
```

优先级：Module.meta > Branch.meta > Flow.meta > Project.meta

### 11.4 Meta 编辑界面

在 `ProjectConfig` / `FlowConfig` 页面中，`MetaSection` 组件提供：

| 功能 | 说明 |
|------|------|
| **查看** | 展示当前所有 meta 键值对 |
| **编辑值** | 直接修改 meta 参数的值 |
| **删除** | 删除不需要的 meta 键值对 |
| **新建** | 指定键名、值、数据类型、交互方式，添加新的 meta |
| **类型推断** | 自动根据值推断 dtype（boolean/number/string） |

---

## 12. 参数暂存与历史记录

### 12.1 临时参数暂存

用户在编辑参数时，可以随时暂存当前参数值，避免意外丢失：

```
前端行为:
  saveTemporaryArgument(type, key, subKey, rawMap)
    → 构建对象 { type, key, subKey, map: {...} }
    → localStorage.setItem('leaf-arg-temp', JSON.stringify(store))

  loadTemporaryArgument(type, key, subKey)
    → 从 localStorage 读取并匹配

  clearTemporaryArgument(type, key, subKey)
    → 从 localStorage 移除
```

暂存数据按 `(type, projectKey, moduleKey)` 三元组存储和恢复。

### 12.2 执行历史记录

每次执行成功后，参数值自动保存到历史记录：

```
前端行为:
  saveExecutedArgument(type, key, subKey, rawMap)
    → 追加到 localStorage 列表

  loadExecutedArguments(type, key, subKey)
    → 读取并过滤匹配的记录
    → 按时间倒序返回
```

### 12.3 历史下拉菜单

ExecutionHistoryDropdown 组件功能：

| 功能 | 说明 |
|------|------|
| **历史列表** | 显示该模块/分支的过往执行参数 |
| **参数摘要** | 显示 key: value 格式的参数摘要 |
| **一键恢复** | 点击历史项，自动填充所有参数值 |
| **逐项删除** | 带 2 秒防误触的删除按钮 |
| **清空所有** | 一键清除该模块的所有历史记录 |
| **Flow 支持** | 显示多模块参数分布（每个模块一行） |
| **横向滚动** | 使用鼠标滚轮水平滚动内容区域 |

---

## 13. 导出与保存

### 13.1 导出功能

用户可以将当前编辑的参数配置导出为独立的 YAML 配置文件：

```
导出流程:
  1. 用户点击"导出" → ExportDialog 弹出
  2. 选择文件名
  3. 选择保存位置（bud/leaf 或 bud/sprig 目录）
  4. 选项:
     ├── "写入" → POST 到后端保存为文件
     └── "导出" → Blob URL 浏览器下载
```

### 13.2 YAML 序列化

```typescript
toYaml(project): string
  → yaml.stringify(project, { indent: 2, simpleKeys: true })
  → 生成格式化的 YAML 配置文件
```

---

## 14. 首页卡片与懒加载

### 14.1 卡片概览

首页展示所有 Project 和 Flow 的卡片式概览：

```
├── 项目 (Project Section)
│   ├── 卡片 1: Demo Tool
│   ├── 卡片 2: MSST
│   ├── 卡片 3: N_m3u8DL-RE
│   └── ...
│
└── 流 (Flow Section)
    └── 卡片: Demo Workflow
```

### 14.2 卡片内容

LeafCard 组件渲染每个卡片：

| 区域 | 内容 |
|------|------|
| **标题** | 项目/流名称 |
| **描述** | Markdown 渲染的描述（最多 2 行，超长省略） |
| **标签** | 所有模块名 → 绿色圆角标签，可点击直接进入 |
| **描述悬停** | 鼠标悬停 → Popover 显示完整 Markdown 描述 |

### 14.3 卡片交互

| 操作 | 行为 |
|------|------|
| **点击卡片** | 进入项目/流，默认选中第一个模块/分支 |
| **点击标签** | 进入项目/流，自动切换到该模块/分支 |
| **右键菜单** | 显示操作菜单 |
| **错误状态** | 加载失败的卡片背景变为红色 |

### 14.4 懒加载机制

使用 `IntersectionObserver` 实现按需加载：

```
useLazyLoad({
  getNames,     // 获取所有名称列表
  getItem,      // 检查是否已加载
  fetchBatch,   // 批量获取（batchSize=6）
  loadItem,     // 加载到 Store
})
  │
  ├── 卡片进入可视区域 → loadBatch 触发
  ├── 批量加载 6 个 → 串行执行（避免并发）
  ├── 加载失败的异步标记 → 不再重试
  └── 组件卸载 → observer.disconnect()
```

### 14.5 文件变更检测

首页加载时，扫描配置目录计算文件的修改时间戳，缓存到 localStorage：

```
initConfigMTime()
  → 扫描 bud/leaf/ + bud/sprig/
  → 提取文件名 stem + 修改时间
  → 与 localStorage 中的缓存时间对比
  → 取最大值保证新文件优先
  → 按修改时间倒序排列
```

---

## 15. 右键菜单与快捷操作

### 15.1 右键菜单

在首页卡片上右键弹出 ContextMenu：

| 菜单项 | 行为 |
|--------|------|
| **打开** | 进入项目，默认选中第一个模块 |
| **配置** | 进入项目配置页面 |
| **外部编辑** | 用系统编辑器打开 YAML 文件 |
| **置顶/取消置顶** | 在 localStorage 中置顶，排序优先 |
| **导出** | 打开导出对话框 |

### 15.2 外部编辑

```typescript
openExternalEditor(name, type)
  → POST /api/directory { path, content? }  // 打开文件
  → 后端调用系统默认编辑器打开 YAML 文件
```

---

## 16. 配置中心

### 16.1 服务器配置

在 `scheduler/config.yaml` 中配置：

```yaml
port: 8892                            # 服务器端口
email: 233@233.com                    # 联系邮箱
budDir: ../bud                        # 蓝图文件目录（入口）
rootDir: .                            # 根目录
openBrowser: true                     # 启动时自动打开浏览器
logFile: logs/leaf.log                # 日志文件
```

### 16.2 配置 API

```
GET /api/config
→ 返回 { budDir, rootDir, ... } 配置信息
```

### 16.3 前端配置 Store

`configStore` 负责：

| 方法 | 说明 |
|------|------|
| `fetch(force)` | 获取服务器配置（带缓存，支持强制刷新） |
| `rootDir(force)` | 获取运行根目录的绝对路径（缓存 + 防重入） |
| `data()` | 响应式配置信号 |

---

## 17. Console 输入桥接

### 17.1 概述

后端在启动时可选启动 Console 输入桥接，允许用户在服务器终端直接输入并转发给当前运行的执行器。

### 17.2 实现

```go
// main.go
go consoleInputBridge()
    → bufio.Scanner(os.Stdin)
    → 读取用户输入
    → 调用 taskQueue.Input(userInput)
    → 转发给当前 Task 的 PtyExecutor
```

适用于 PTY 模式下需要交互式输入的场景。

---

## 18. 路由与导航

### 18.1 路由表

| 路径 | 视图 | 守卫 | 说明 |
|------|------|------|------|
| `/` | Home | — | 首页，展示所有项目/流卡片 |
| `/projects/:project` | ProjectLayout | withProjectGuard | 项目详情页 |
| `/projects/:project/:module` | ProjectLayout | withProjectGuard | 项目 + 选中模块 |
| `/projects/:project/config` | ProjectLayout | withProjectGuard | 项目配置页 |
| `/flows/:flow` | FlowLayout | withFlowGuard | 流详情页 |
| `/flows/:flow/:branch` | FlowLayout | withFlowGuard | 流 + 选中分支 |
| `/flows/:flow/config` | FlowLayout | withFlowGuard | 流配置页 |
| `/tasks` | TaskQueue | — | 任务队列详情 |
| `*` | NotFound | — | 404 页面 |

### 18.2 路由守卫

| 守卫 | 逻辑 |
|------|------|
| `withProjectGuard` | 检查 params.project 是否有效 → 尝试 fetch → 失败则 404 |
| `withFlowGuard` | 检查 params.flow 是否有效 → 尝试 fetch → 失败则 404 |

### 18.3 导航辅助

`useAppNavigate` 封装导航逻辑，支持：

```typescript
navigate('project', projectKey, moduleKey?)  // 导航到项目
navigate('flow', flowKey, branchKey?)        // 导航到流
navigate('projectConfig', projectKey)        // 导航到项目配置
navigate('flowConfig', flowKey)              // 导航到流配置
```

### 18.4 URL 参数编码

`useDecodedParams` Hook 自动对路由参数进行 URL 编解码：

```
示例: /projects/Demo%20Tool/download
  → params.project = "Demo Tool"
  → params.module  = "download"
```

---

## 功能清单速览

| # | 功能 | 前端 | 后端 | 依赖 |
|---|------|------|------|------|
| 1 | 蓝图文件系统 & YAML 解析 | config.ts | routes/project.go | yaml.js, yaml.v3 |
| 2 | 模块参数编辑（8 种组件） | argumentInput/* | — | — |
| 3 | 命令模板渲染 | execution.ts | — | lodash |
| 4 | 动态绑定引擎 | dynamicBind.ts | — | lodash, yaml.js |
| 5 | 三种执行器 | — | exec*.go | go-pty, creack/pty |
| 6 | 实时日志 & xterm 终端 | GlobalLogs.tsx | log_manager.go | @xterm/xterm |
| 7 | 任务队列管理 | TaskQueue.tsx | task_queue.go | — |
| 8 | MMAP 共享内存 | — | mmap.go | mmap-go |
| 9 | Flow 流程编排 | config.ts, Flow.tsx | project.go | lodash |
| 10 | 文件系统浏览器 | FileBrowser.tsx | routes/files.go | — |
| 11 | Meta 元数据管理 | ModuleMeta.tsx | — | — |
| 12 | 参数暂存与历史 | execution.ts, HistoryDropdown | — | localStorage |
| 13 | 导出与保存 | ExportDialog.tsx | routes/files.go | yaml.js |
| 14 | 首页卡片 & 懒加载 | Home.tsx, LeafCard.tsx | — | IntersectionObserver |
| 15 | 右键菜单 | ContextMenu.tsx | — | — |
| 16 | 配置中心 | stores/config.ts | config.go | — |
| 17 | Console 输入桥接 | — | main.go | — |
| 18 | 路由与导航 | routes/*, App.tsx | — | @solidjs/router |
