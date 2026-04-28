# CLI-App 文件 IO 设计 PRD

## 1. 概述

### 1.1 设计目标

- 区分**功能性文件 IO**（用于程序间数据交换）和**展示性文件 IO**（用于人类阅读）
- 功能性文件 IO 必须保证**所见即所得**，不包含任何格式化信息
- 支持 Graph 模式和 Agent 模式的不同文件管理需求
- 支持多 Agent、子工作流、Fork-Join 等复杂场景的输出隔离
- 每个执行实例提供聚合数据呈现文件，替代 TUI 完整输出

### 1.2 核心原则

1. **功能性文件零格式化** - 纯文本内容，便于复制粘贴和程序解析
2. **展示性文件 Markdown 格式** - 使用标准 Markdown，便于阅读和渲染
3. **实例隔离** - 不同运行实例的输出相互隔离
4. **聚合呈现** - 每个实例一个汇总文件，包含所有关键信息

---

## 2. 文件 IO 分类

### 2.1 功能性文件 IO（Functional File IO）

**定义**：用于程序间数据交换，内容会被读取并作为输入使用

**特点**：
- 纯文本内容，无格式化标记
- 可直接复制到外部使用
- Human Relay 输入文件直接作为 LLM 响应使用
- 支持手动编写 LLM 响应（用于调试或替代 API 调用）

| 文件 | 用途 | 写入方 | 读取方 | 格式 |
|------|------|--------|--------|------|
| `human-relay-input.txt` | Human Relay 用户输入（类 LLM 消息） | 用户/外部编辑器 | CLI-App | 纯文本 |
| `human-relay-output.txt` | Human Relay 提示词（复制到网页 LLM） | CLI-App | 用户 | 纯文本 |

### 2.2 展示性文件 IO（Presentation File IO）

**定义**：用于人类阅读，Markdown 格式

**特点**：
- 标准 Markdown 格式
- 使用 `======` 作为一级标题分隔
- 使用 `══════════════════════════════` 作为二级分隔
- 每个执行实例一个聚合呈现文件

| 文件 | 用途 | 写入方 | 读取方 |
|------|------|--------|--------|
| `output.md` | 执行实例聚合呈现文件 | CLI-App | 用户 |
| `execution-log.md` | 执行过程日志 | CLI-App | 用户 |

---

## 3. 功能性文件 IO 详细设计

### 3.1 Human Relay 输出文件（提示词）

**文件路径**：`.wf-agent/function/{session-name}/human-relay-output.txt`

**用途说明**：
- 包含发送给 LLM 的完整提示词
- 用户复制此内容到网页端 LLM
- 纯文本格式，便于复制粘贴

**格式示例**：
```
You are a helpful coding assistant. Please review the following code and suggest improvements.

## Context

The user is working on a sorting algorithm implementation.

## Code to Review

```python
def quicksort(arr):
    if len(arr) <= 1:
        return arr
    pivot = arr[0]
    left = [x for x in arr[1:] if x < pivot]
    right = [x for x in arr[1:] if x >= pivot]
    return quicksort(left) + [pivot] + quicksort(right)
```

## Task

Please analyze this implementation and provide specific suggestions for improvement.
```

### 3.2 Human Relay 输入文件（响应）

**文件路径**：`.wf-agent/function/{session-name}/human-relay-input.txt`

**用途说明**：
- 用户将网页端 LLM 的响应粘贴到此文件
- 支持标准 LLM 响应格式，包括工具调用
- 纯文本格式，直接作为 LLM 响应使用

**格式示例（普通响应）**：
```
I'll analyze this quicksort implementation for you.

## Issues Found

1. **Space Complexity**: The current implementation uses O(n) extra space
2. **Pivot Selection**: Fixed pivot can lead to O(n²) worst case
3. **Recursion Depth**: No protection against stack overflow

## Suggested Improvements

Here's an optimized in-place version:

```python
def quicksort_inplace(arr, low=0, high=None):
    if high is None:
        high = len(arr) - 1
    
    if low < high:
        # Use median-of-three for better pivot selection
        pivot = median_of_three(arr, low, high)
        pi = partition(arr, low, high, pivot)
        
        quicksort_inplace(arr, low, pi - 1)
        quicksort_inplace(arr, pi + 1, high)
    
    return arr
```
```

**格式示例（含工具调用 - XML 格式）**：
```
I'll help you read and analyze that file.

<tool_use>
<tool_name>file_read</tool_name>
<parameters>
<path>src/utils.py</path>
<encoding>utf-8</encoding>
</parameters>
</tool_use>

Let me also search for related code:

<tool_use>
<tool_name>code_search</tool_name>
<parameters>
<query>quicksort implementation</query>
<language>python</language>
</parameters>
</tool_use>
```

**格式示例（含工具调用 - JSON 格式）**：
```
I'll help you with that.

<<<TOOL_CALL>>>
{"tool": "file_read", "parameters": {"path": "src/config.json", "encoding": "utf-8"}}
<<<END_TOOL_CALL>>>

Now let me search for the relevant function:

<<<TOOL_CALL>>>
{"tool": "code_search", "parameters": {"query": "function processData", "language": "typescript"}}
<<<END_TOOL_CALL>>>
```

**设计说明**：
- 文件内容直接作为 LLM 响应使用
- 无请求 ID、时间戳等元数据
- 支持普通文本、代码块、工具调用等多种格式
- 工具调用格式与 `sdk/core/llm/formatters/tool-call-parser.ts` 兼容

---

## 4. 展示性文件 IO 详细设计

### 4.1 聚合呈现文件（output.md）

**文件路径**：`.wf-agent/display/{session-name}/output.md`

**用途**：替代 TUI 完整输出，聚合所有关键信息

**格式示例**：
```markdown
# Workflow Execution Output

======

## 基本信息

- **实例名称**: session-data-processing-001
- **实例 ID**: graph-1705312345678-abc123
- **工作流**: my-workflow
- **状态**: 运行中
- **开始时间**: 2024-01-15 10:30:00
- **当前节点**: node-456

══════════════════════════════

## 执行日志

### [10:30:05] Node: start
状态: ✓ 完成
耗时: 12ms

### [10:30:08] Node: llm-generate
状态: ✓ 完成
耗时: 2.3s
输出: 已生成计划...

### [10:30:12] Node: human-relay
状态: ⏳ 等待中
══════════════════════════════
Human Relay 进行中...

**操作步骤**：
1. 查看提示词：`.wf-agent/function/session-data-processing-001/human-relay-output.txt`
2. 复制提示词到网页端 LLM
3. 将 LLM 响应粘贴到：`.wf-agent/function/session-data-processing-001/human-relay-input.txt`
4. 保存文件，系统将自动继续

══════════════════════════════

## 变量状态

| 变量名 | 值 | 更新时间 |
|--------|-----|----------|
| status | "planning" | 10:30:08 |
| plan | {...} | 10:30:08 |

======

## 子实例

- [子工作流: data-processing](./sub-instances/session-sub-001/output.md)
- [子 Agent: code-reviewer](./sub-instances/session-agent-001/output.md)
```

---

## 5. 目录结构设计

### 5.1 基础目录结构

```
.wf-agent/
├── function/                    # 功能性文件 IO（程序间交换）
│   └── {session-name}/          # 按会话隔离
│       ├── human-relay-input.txt
│       └── human-relay-output.txt
│
└── display/                     # 展示性文件 IO（人类阅读）
    └── {session-name}/          # 按会话隔离
        ├── output.md            # 聚合呈现文件
        └── execution-log.md
```

### 5.2 会话命名规则

```typescript
// 格式: session-{name}-{timestamp}
// 示例: session-data-processing-001-1705312345678
//       session-quick-sort-1705312345679

interface SessionNamingOptions {
  // 用户指定或自动生成的名称
  name: string;
  // 时间戳
  timestamp: number;
  // 实例 ID（元数据，不用于目录名）
  instanceId: string;
}

function generateSessionName(options: SessionNamingOptions): string {
  const sanitized = options.name
    .toLowerCase()
    .replace(/[^a-z0-9-]/g, '-')
    .substring(0, 30);
  return `session-${sanitized}-${options.timestamp}`;
}
```

### 5.3 元数据存储

实例 ID 等元数据存储在 `output.md` 文件头部：

```markdown
---
instanceId: graph-1705312345678-abc123
type: graph
parentId: null
startedAt: 1705312345678
---

# Workflow Execution Output
```

---

## 6. Graph 模式 vs Agent 模式

### 6.1 Graph 模式文件管理

**文件组织**：
```
.wf-agent/function/session-{name}-{ts}/
├── human-relay-output.txt       # 提示词输出
└── human-relay-input.txt        # 用户响应输入

.wf-agent/display/session-{name}-{ts}/
├── output.md                    # 聚合呈现文件
├── execution-log.md
└── sub-instances/               # 子实例链接
    ├── session-sub-001 -> ../../session-sub-001/
    └── session-agent-001 -> ../../session-agent-001/
```

### 6.2 Agent 模式文件管理

**文件组织**：
```
.wf-agent/function/session-{name}-{ts}/
├── human-relay-output.txt
└── human-relay-input.txt

.wf-agent/display/session-{name}-{ts}/
├── output.md                    # 聚合呈现文件
├── execution-log.md
└── sub-instances/               # 子实例链接
    └── session-sub-agent-001 -> ../../session-sub-agent-001/
```

---

## 7. 多实例输出管理

### 7.1 实例隔离策略

| 场景 | 目录结构 | 说明 |
|------|----------|------|
| 单 Agent 执行 | `.wf-agent/function/session-{name}-{ts}/` | 独立目录 |
| 单 Graph 执行 | `.wf-agent/function/session-{name}-{ts}/` | 独立目录 |
| 多 Agent 并行 | 各自独立目录 + 父实例聚合文件链接 | 子实例链接 |
| 子工作流 | `.wf-agent/function/session-{name}-{ts}/` 独立目录 | 父实例聚合文件链接 |
| Fork-Join | 各分支独立目录 | Join 后汇总到父实例 |

### 7.2 子实例链接机制

在父实例的 `output.md` 中通过相对路径链接子实例：

```markdown
## 子实例

- [子工作流: data-processing](./sub-instances/session-data-processing-sub-001/output.md)
- [子 Agent: code-reviewer](./sub-instances/session-code-review-agent-001/output.md)
```

---

## 8. 文件监控机制

### 8.1 输入文件监控

```typescript
interface FileWatcher {
  /**
   * 开始监控输入文件
   */
  watch(inputFile: string, options: WatchOptions): Promise<string>;
  
  /**
   * 取消监控
   */
  unwatch(): void;
}

interface WatchOptions {
  /** 超时时间 */
  timeout: number;
  /** 轮询间隔 */
  pollInterval?: number;
}
```

### 8.2 监控流程

1. 创建功能性输入文件（空文件）
2. 启动文件系统监控（fs.watch）
3. 等待文件变化或超时
4. 读取完整内容作为 LLM 响应
5. 无需复杂解析，直接传递给 Agent

---

## 9. 配置选项

### 9.1 配置文件

```toml
# cli-config.toml

[io]
# 基础目录
base_dir = ".wf-agent"

[io.functional]
# 功能性文件目录
dir = ".wf-agent/function"
# 自动清理旧文件
auto_cleanup = true
# 保留天数
retention_days = 7

[io.display]
# 展示性文件目录
dir = ".wf-agent/display"
# 启用 Markdown 格式化
enable_markdown = true

[io.naming]
# 会话命名策略
# - auto: 自动生成（基于工作流/Agent名称）
# - manual: 用户指定
# - hybrid: 用户指定 + 时间戳
strategy = "hybrid"
# 名称最大长度
max_name_length = 30
```

---

## 10. 实现建议

### 10.1 核心模块

```
src/
├── io/
│   ├── functional/          # 功能性文件 IO
│   │   ├── human-relay.ts
│   │   └── index.ts
│   ├── display/             # 展示性文件 IO
│   │   ├── formatters/
│   │   ├── aggregation.ts   # 聚合文件生成
│   │   └── index.ts
│   ├── naming/              # 会话命名
│   │   └── session-namer.ts
│   └── watcher/             # 文件监控
│       └── file-watcher.ts
```

### 10.2 关键实现点

1. **功能性文件纯文本** - 直接读写，无需解析
2. **工具调用格式兼容** - 支持 XML 和 JSON 格式，与 SDK parser 兼容
3. **聚合文件增量更新** - 避免频繁全量写入
4. **子实例链接** - 使用相对路径，便于导航
5. **会话命名** - 支持自动命名和用户指定

---

## 11. Human Relay 工作流程

```
Agent Loop
    │
    ▼
触发 Human Relay
    │
    ├──► 生成提示词
    │
    ├──► 写入 .wf-agent/function/{session}/human-relay-output.txt
    │
    ├──► 更新 .wf-agent/display/{session}/output.md
    │         显示操作指南和文件路径
    │
    └──► 监控 .wf-agent/function/{session}/human-relay-input.txt
              │
              ▼ 用户粘贴响应
         读取文件内容
              │
              ▼ 作为 LLM 响应返回
         继续 Agent Loop
```

---

## 12. 工具调用格式参考

Human Relay 输入文件支持的工具调用格式与 `sdk/core/llm/formatters/tool-call-parser.ts` 兼容：

### XML 格式
```xml
<tool_use>
<tool_name>file_read</tool_name>
<parameters>
<path>src/utils.py</path>
<encoding>utf-8</encoding>
</parameters>
</tool_use>
```

### JSON 格式（带标记）
```
<<<TOOL_CALL>>>
{"tool": "file_read", "parameters": {"path": "src/utils.py", "encoding": "utf-8"}}
<<<END_TOOL_CALL>>>
```

### 原生格式
```json
{"name": "file_read", "arguments": "{\"path\": \"src/utils.py\"}"}
```
