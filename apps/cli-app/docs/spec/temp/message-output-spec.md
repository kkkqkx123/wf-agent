# CLI-App 消息输出规范

## 一、需求背景

### 1.1 问题描述

在 TUI 化改造过程中，面临以下挑战：

1. **渲染开销过大** - Human Relay 和工具调用详情在 TUI 中实时渲染会导致性能问题
2. **防抖和缓存复杂性** - 频繁更新需要复杂的防抖和缓存机制
3. **CLI 简洁性要求** - 需要保持 CLI 应用的简单性和可维护性

### 1.2 设计目标

- 降低 TUI 渲染负担，仅显示摘要信息
- 完整信息通过文件 IO 输出，便于查看和追溯
- 提供统一的消息类型定义，支持可配置的输出模式
- 保持 CLI 应用的核心简洁性

---

## 二、输出模式设计

### 2.1 三种输出模式

| 模式 | 描述 | 适用场景 |
|------|------|----------|
| **TUI 摘要模式** | 仅在 TUI 显示摘要信息 | 实时监控、快速浏览 |
| **文件输出模式** | 完整信息写入文件 | 详细查看、日志留存 |
| **混合模式** | 摘要显示 + 文件记录 | 默认推荐模式 |

### 2.2 文件输出规范

```
./wf-agent/
├── output/
│   ├── agent-output.txt          # Agent Loop 完整输出
│   ├── tool-calls.log            # 工具调用详细日志
│   ├── human-relay-output.txt    # Human Relay 前的上下文输出
│   └── human-relay-input.txt     # Human Relay 用户输入
└── logs/
    └── cli-app.log               # 应用运行日志
```

---

## 三、组件消息类型定义

### 3.1 消息类型层级

```
Message
├── SystemMessage          # 系统级消息
│   ├── StartupMessage     # 启动消息
│   ├── ShutdownMessage    # 关闭消息
│   └── ErrorMessage       # 错误消息
│
├── AgentMessage           # Agent 相关消息
│   ├── AgentStartMessage      # Agent 开始执行
│   ├── AgentEndMessage        # Agent 执行结束
│   ├── IterationMessage       # 迭代完成消息
│   └── StreamMessage          # 流式输出片段
│
├── ToolMessage            # 工具调用消息
│   ├── ToolCallStartMessage   # 工具调用开始
│   ├── ToolCallEndMessage     # 工具调用结束
│   ├── ToolResultMessage      # 工具执行结果
│   └── ToolErrorMessage       # 工具执行错误
│
├── HumanRelayMessage      # Human Relay 消息
│   ├── HumanRelayRequestMessage   # 请求用户输入
│   ├── HumanRelayResponseMessage  # 用户响应
│   └── HumanRelayTimeoutMessage   # 超时消息
│
└── WorkflowMessage        # 工作流消息
    ├── WorkflowStartMessage     # 工作流开始
    ├── WorkflowEndMessage       # 工作流结束
    ├── NodeExecuteMessage       # 节点执行
    └── CheckpointMessage        # 检查点消息
```

### 3.2 基础消息接口

```typescript
/**
 * 基础消息接口
 */
interface BaseMessage {
  /** 消息唯一标识 */
  id: string;
  /** 消息类型 */
  type: MessageType;
  /** 时间戳 */
  timestamp: number;
  /** 消息级别 */
  level: 'debug' | 'info' | 'warn' | 'error';
  /** 所属会话/线程 ID */
  sessionId?: string;
}

/**
 * 消息类型枚举
 */
enum MessageType {
  // 系统消息
  STARTUP = 'system.startup',
  SHUTDOWN = 'system.shutdown',
  ERROR = 'system.error',
  
  // Agent 消息
  AGENT_START = 'agent.start',
  AGENT_END = 'agent.end',
  AGENT_ITERATION = 'agent.iteration',
  AGENT_STREAM = 'agent.stream',
  
  // 工具消息
  TOOL_CALL_START = 'tool.call_start',
  TOOL_CALL_END = 'tool.call_end',
  TOOL_RESULT = 'tool.result',
  TOOL_ERROR = 'tool.error',
  
  // Human Relay 消息
  HUMAN_RELAY_REQUEST = 'human_relay.request',
  HUMAN_RELAY_RESPONSE = 'human_relay.response',
  HUMAN_RELAY_TIMEOUT = 'human_relay.timeout',
  
  // 工作流消息
  WORKFLOW_START = 'workflow.start',
  WORKFLOW_END = 'workflow.end',
  WORKFLOW_NODE = 'workflow.node',
  WORKFLOW_CHECKPOINT = 'workflow.checkpoint',
}
```

### 3.3 详细消息定义

#### 3.3.1 Agent 消息

```typescript
/**
 * Agent 开始消息
 */
interface AgentStartMessage extends BaseMessage {
  type: MessageType.AGENT_START;
  data: {
    agentId: string;
    profileId: string;
    maxIterations: number;
    tools: string[];
    systemPrompt?: string;
  };
}

/**
 * Agent 迭代消息
 */
interface AgentIterationMessage extends BaseMessage {
  type: MessageType.AGENT_ITERATION;
  data: {
    iteration: number;
    maxIterations: number;
    toolCallCount: number;
    messageCount: number;
  };
}

/**
 * Agent 流式消息
 */
interface AgentStreamMessage extends BaseMessage {
  type: MessageType.AGENT_STREAM;
  data: {
    chunk: string;
    isComplete: boolean;
  };
}
```

#### 3.3.2 工具消息

```typescript
/**
 * 工具调用开始消息
 */
interface ToolCallStartMessage extends BaseMessage {
  type: MessageType.TOOL_CALL_START;
  data: {
    toolCallId: string;
    toolName: string;
    arguments: Record<string, unknown>;
  };
}

/**
 * 工具调用结束消息
 */
interface ToolCallEndMessage extends BaseMessage {
  type: MessageType.TOOL_CALL_END;
  data: {
    toolCallId: string;
    toolName: string;
    success: boolean;
    duration: number;
  };
}

/**
 * 工具执行结果消息（详细）
 */
interface ToolResultMessage extends BaseMessage {
  type: MessageType.TOOL_RESULT;
  data: {
    toolCallId: string;
    toolName: string;
    result: unknown;
    output: string;  // 格式化后的输出
  };
}
```

#### 3.3.3 Human Relay 消息

```typescript
/**
 * Human Relay 请求消息
 */
interface HumanRelayRequestMessage extends BaseMessage {
  type: MessageType.HUMAN_RELAY_REQUEST;
  data: {
    requestId: string;
    prompt: string;
    context: {
      messages: Array<{
        role: string;
        content: string;
      }>;
      metadata?: Record<string, unknown>;
    };
    timeout: number;
  };
}

/**
 * Human Relay 响应消息
 */
interface HumanRelayResponseMessage extends BaseMessage {
  type: MessageType.HUMAN_RELAY_RESPONSE;
  data: {
    requestId: string;
    content: string;
    responseTime: number;
  };
}
```

---

## 四、消息输出控制器

### 4.1 控制器接口

```typescript
/**
 * 消息输出控制器
 */
interface MessageOutputController {
  /**
   * 处理消息
   */
  handleMessage(message: BaseMessage): void;
  
  /**
   * 配置输出模式
   */
  configure(config: OutputConfig): void;
  
  /**
   * 刷新输出
   */
  flush(): Promise<void>;
  
  /**
   * 关闭控制器
   */
  close(): Promise<void>;
}

/**
 * 输出配置
 */
interface OutputConfig {
  /** TUI 输出配置 */
  tui?: {
    enabled: boolean;
    messageTypes: MessageType[];  // 允许在 TUI 显示的消息类型
    maxHistory: number;           // TUI 历史记录上限
  };
  
  /** 文件输出配置 */
  file?: {
    enabled: boolean;
    outputDir: string;
    files: {
      agentOutput: string;
      toolCalls: string;
      humanRelayOutput: string;
      humanRelayInput: string;
    };
    rotation?: {
      enabled: boolean;
      maxSize: number;      // 最大文件大小（字节）
      maxFiles: number;     // 保留文件数量
    };
  };
  
  /** 摘要配置 */
  summary?: {
    enabled: boolean;
    toolCallSummary: boolean;     // 显示工具调用摘要
    iterationSummary: boolean;    // 显示迭代摘要
    maxToolNameLength: number;    // 工具名称最大长度
  };
}
```

### 4.2 默认配置

```typescript
const DEFAULT_OUTPUT_CONFIG: OutputConfig = {
  tui: {
    enabled: true,
    messageTypes: [
      MessageType.AGENT_START,
      MessageType.AGENT_END,
      MessageType.AGENT_ITERATION,
      MessageType.TOOL_CALL_START,
      MessageType.TOOL_CALL_END,
      MessageType.HUMAN_RELAY_REQUEST,
      MessageType.HUMAN_RELAY_RESPONSE,
    ],
    maxHistory: 100,
  },
  file: {
    enabled: true,
    outputDir: './wf-agent/output',
    files: {
      agentOutput: 'agent-output.txt',
      toolCalls: 'tool-calls.log',
      humanRelayOutput: 'human-relay-output.txt',
      humanRelayInput: 'human-relay-input.txt',
    },
    rotation: {
      enabled: true,
      maxSize: 10 * 1024 * 1024,  // 10MB
      maxFiles: 5,
    },
  },
  summary: {
    enabled: true,
    toolCallSummary: true,
    iterationSummary: true,
    maxToolNameLength: 30,
  },
};
```

---

## 五、Human Relay 文件 IO 实现

### 5.1 工作流程

```
Agent Loop
    │
    ▼
触发 Human Relay
    │
    ├──► 写入 human-relay-output.txt (完整上下文)
    │
    ├──► TUI 显示摘要提示
    │         "Human Relay 请求已写入 human-relay-output.txt"
    │         "请在 human-relay-input.txt 中输入响应"
    │
    └──► 监控 human-relay-input.txt 变化
              │
              ▼
        检测到新内容
              │
              └──► 读取并解析响应
                        │
                        └──► 继续 Agent Loop
```

### 5.2 文件格式

#### human-relay-output.txt

```
═══════════════════════════════════════════════════════════════
Human Relay Request - 2024-01-15T10:30:00.000Z
Request ID: req-abc123
Timeout: 300000ms (5 minutes)
═══════════════════════════════════════════════════════════════

【当前提示】
请检查以下代码并指出潜在问题：

【对话历史】
[USER]: 帮我写一个快速排序算法
[ASSISTANT]: ```python
def quicksort(arr):
    if len(arr) <= 1:
        return arr
    pivot = arr[0]
    left = [x for x in arr[1:] if x < pivot]
    right = [x for x in arr[1:] if x >= pivot]
    return quicksort(left) + [pivot] + quicksort(right)
```

【元数据】
- 迭代次数: 3/10
- 工具调用次数: 2
- 相关文件: src/sorting.py

═══════════════════════════════════════════════════════════════
请在 human-relay-input.txt 中输入您的响应
格式: 直接输入文本内容，以空行结束
═══════════════════════════════════════════════════════════════
```

#### human-relay-input.txt

```
═══════════════════════════════════════════════════════════════
Response for Request: req-abc123
Timestamp: 2024-01-15T10:32:15.000Z
═══════════════════════════════════════════════════════════════

代码存在以下问题：
1. 空间复杂度为 O(n)，不是原地排序
2. 对于已排序数组，递归深度为 O(n)，可能导致栈溢出
3. 缺少类型注解和文档字符串

建议改用原地分区实现。

═══════════════════════════════════════════════════════════════
```

### 5.3 监控机制

```typescript
/**
 * Human Relay 文件监控器
 */
class HumanRelayFileWatcher {
  private watcher?: fs.FSWatcher;
  private resolvePromise?: (value: string) => void;
  
  /**
   * 开始监控输入文件
   */
  async watchForResponse(requestId: string, timeout: number): Promise<string> {
    return new Promise((resolve, reject) => {
      const inputFile = './wf-agent/output/human-relay-input.txt';
      
      // 设置超时
      const timeoutId = setTimeout(() => {
        this.cleanup();
        reject(new Error('Human Relay timeout'));
      }, timeout);
      
      // 监控文件变化
      this.watcher = fs.watch(inputFile, (eventType) => {
        if (eventType === 'change') {
          const content = fs.readFileSync(inputFile, 'utf-8');
          
          // 解析响应
          const response = this.parseResponse(content, requestId);
          if (response) {
            clearTimeout(timeoutId);
            this.cleanup();
            resolve(response);
          }
        }
      });
      
      this.resolvePromise = resolve;
    });
  }
  
  /**
   * 解析响应内容
   */
  private parseResponse(content: string, expectedRequestId: string): string | null {
    // 提取 Request ID
    const requestIdMatch = content.match(/Request:\s*(\S+)/);
    if (!requestIdMatch || requestIdMatch[1] !== expectedRequestId) {
      return null;
    }
    
    // 提取响应内容（分隔符之后的内容）
    const parts = content.split(/═+\n/);
    if (parts.length >= 3) {
      return parts[2].trim();
    }
    
    return null;
  }
  
  /**
   * 清理资源
   */
  private cleanup(): void {
    this.watcher?.close();
    this.watcher = undefined;
    this.resolvePromise = undefined;
  }
}
```

---

## 六、工具调用输出规范

### 6.1 TUI 摘要显示

```
┌─────────────────────────────────────────────────────────────┐
│ Agent Loop - Running                                         │
├─────────────────────────────────────────────────────────────┤
│ Iteration: 3/10  │  Tools: 5  │  Time: 00:02:34             │
├─────────────────────────────────────────────────────────────┤
│ [✓] file_read (0.2s) - src/utils.ts                         │
│ [✓] code_search (0.5s) - "quicksort implementation"         │
│ [→] llm_generate (2.1s) - Optimizing...                     │
│                                                              │
│ 详细日志: ./wf-agent/output/tool-calls.log               │
└─────────────────────────────────────────────────────────────┘
```

### 6.2 文件输出格式

```
═══════════════════════════════════════════════════════════════
Tool Call Log - 2024-01-15T10:30:00.000Z
═══════════════════════════════════════════════════════════════

[CALL #1] file_read
Timestamp: 2024-01-15T10:30:01.234Z
Duration: 234ms
Status: SUCCESS

Arguments:
{
  "path": "src/utils.ts",
  "encoding": "utf-8"
}

Result:
{
  "content": "export function helper() {...}",
  "size": 1024,
  "lines": 45
}

───────────────────────────────────────────────────────────────

[CALL #2] code_search
Timestamp: 2024-01-15T10:30:02.567Z
Duration: 456ms
Status: SUCCESS

Arguments:
{
  "query": "quicksort implementation",
  "language": "python"
}

Result:
[
  {
    "file": "src/algorithms/sort.py",
    "line": 23,
    "snippet": "def quicksort(arr):..."
  }
]

═══════════════════════════════════════════════════════════════
```

---

## 七、用户配置

### 7.1 配置文件

```toml
# cli-config.toml

[output]
# 输出目录
output_dir = "./wf-agent/output"

[output.tui]
# 启用 TUI 输出
enabled = true
# 在 TUI 显示的消息类型
message_types = [
  "agent.start",
  "agent.end",
  "agent.iteration",
  "tool.call_start",
  "tool.call_end",
  "human_relay.request",
  "human_relay.response"
]
# TUI 历史记录上限
max_history = 100

[output.file]
# 启用文件输出
enabled = true
# 文件轮转
rotation_enabled = true
max_file_size = "10MB"
max_files = 5

[output.summary]
# 显示摘要
enabled = true
# 工具调用摘要
tool_call_summary = true
# 迭代摘要
iteration_summary = true
```

### 7.2 环境变量

```bash
# 输出模式
CLI_OUTPUT_MODE=tui-summary  # tui-summary | file-only | hybrid

# 输出目录
CLI_OUTPUT_DIR=./wf-agent/output

# TUI 历史记录上限
CLI_TUI_MAX_HISTORY=100

# 文件轮转
CLI_FILE_ROTATION=true
CLI_MAX_FILE_SIZE=10MB
```

---

## 八、实现建议

### 8.1 目录结构

```
src/
├── message/
│   ├── types.ts              # 消息类型定义
│   ├── controller.ts         # 消息输出控制器
│   ├── handlers/
│   │   ├── tui-handler.ts    # TUI 输出处理器
│   │   ├── file-handler.ts   # 文件输出处理器
│   │   └── summary-handler.ts # 摘要生成处理器
│   └── config.ts             # 配置管理
├── human-relay/
│   ├── file-watcher.ts       # 文件监控实现
│   └── file-formatter.ts     # 文件格式化
└── ...
```

### 8.2 关键实现点

1. **消息队列** - 使用队列缓冲消息，避免频繁 IO
2. **批量写入** - 文件输出采用批量写入策略
3. **防抖渲染** - TUI 渲染采用防抖机制
4. **文件锁** - Human Relay 文件操作需要简单的锁机制
5. **编码处理** - 统一使用 UTF-8 编码

---

## 九、迁移路径

### 9.1 阶段一：文件输出（立即实现）

- 实现基础消息类型定义
- 实现文件输出处理器
- Human Relay 改用文件 IO

### 9.2 阶段二：TUI 摘要（后续实现）

- 实现 TUI 输出处理器
- 实现摘要生成器
- 集成到 TUI 界面

### 9.3 阶段三：配置系统（最后实现）

- 实现配置管理
- 支持用户自定义
- 文档完善
