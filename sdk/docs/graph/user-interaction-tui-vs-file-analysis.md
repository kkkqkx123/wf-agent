# User Interaction 方案分析：TUI vs 文件操作

## 1. 概述

本文档分析 CLI-App 中用户交互的实现方案，对比 **TUI 内直接操作** 和 **基于文件操作** 两种方案，结合 file-io-prd.md 的设计规范给出建议。

---

## 2. 当前实现分析

### 2.1 当前 Human Relay Handler 实现

当前 CLI-App 使用 `CLIHumanRelayHandler` 直接读取终端输入：

```typescript
// apps/cli-app/src/handlers/cli-human-relay-handler.ts
export class CLIHumanRelayHandler implements HumanRelayHandler {
  async handle(request: HumanRelayRequest, context: HumanRelayContext): Promise<HumanRelayResponse> {
    output.infoLog(request.prompt);
    
    // 直接使用 readline 读取用户输入
    const content = await this.promptUser();
    
    return { requestId: request.requestId, content, timestamp: Date.now() };
  }
  
  private promptUser(): Promise<string> {
    return new Promise((resolve, reject) => {
      const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
      // ...
    });
  }
}
```

### 2.2 当前 USER_INTERACTION 节点实现

`user-interaction-handler.ts` 通过 `UserInteractionHandler` 接口获取用户输入：

```typescript
interface UserInteractionHandler {
  handle(request: UserInteractionRequest, context: UserInteractionContext): Promise<unknown>;
}
```

---

## 3. 两种方案对比

### 3.1 方案 A：TUI 内直接操作

**实现方式**：使用 readline 或 TUI 库直接在终端读取用户输入

```
用户输入 ──► stdin ──► readline ──► Handler.handle() ──► 响应
```

**优点**：
| 特性 | 说明 |
|------|------|
| 简单直接 | 无需文件操作，实现代码量少 |
| 响应快速 | 无需轮询文件或等待文件系统事件 |
| 低延迟 | 毫秒级响应用户输入 |
| 无状态 | 不产生额外文件，简化清理工作 |

**缺点**：
| 特性 | 说明 |
|------|------|
| 无历史记录 | 会话结束后无法追溯交互内容 |
| 编辑体验差 | 不支持光标移动、文本补全 |
| 无法后台运行 | 必须前台保持连接 |
| 复杂输入困难 | 多行代码输入体验差 |
| 与设计不一致 | 与 file-io-prd.md 规范不兼容 |

### 3.2 方案 B：基于文件操作

**实现方式**：使用文件读写 + fs.watch 监控文件变化

```
触发交互 ──► 写入 human-relay-output.txt ──► 更新 output.md
                    │
                    ▼ 用户在外部编辑
              human-relay-input.txt
                    │
                    ▼ fs.watch 检测
              读取内容 ──► Handler.handle() ──► 响应
```

**优点**：
| 特性 | 说明 |
|------|------|
| 符合设计规范 | 与 file-io-prd.md 一致 |
| 保留历史 | 交互内容持久化，可追溯 |
| 编辑体验好 | 可用外部编辑器（VS Code 等） |
| 支持后台运行 | 进程可挂起，用户随时响应 |
| 复杂输入友好 | 支持多行、代码块、工具调用 |
| 并发支持 | 多会话可并行处理 |
| 便于调试 | 可直接查看/编辑输入文件 |

**缺点**：
| 特性 | 说明 |
|------|------|
| 响应延迟 | 文件监控 + 读取有额外开销 |
| 文件管理复杂 | 需要清理、命名、隔离 |
| 需要同步 | 并发写入需要处理冲突 |
| 实现复杂度 | 文件锁、错误处理更复杂 |

---

## 4. file-io-prd.md 规范分析

### 4.1 功能性文件 IO

```
.wf-agent/function/{session-name}/
├── human-relay-output.txt  # 提示词输出（用户复制到 LLM）
└── human-relay-input.txt   # 用户响应输入（系统读取）
```

**设计要点**：
- 纯文本格式，无格式化
- 支持 XML/JSON 工具调用格式
- 直接作为 LLM 响应使用

### 4.2 展示性文件 IO

```
.wf-agent/display/{session-name}/
├── output.md            # 聚合呈现文件
└── execution-log.md     # 执行过程日志
```

**设计要点**：
- Markdown 格式，便于阅读
- 显示操作指南和文件路径
- 替代 TUI 完整输出

### 4.3 文件监控机制

```typescript
interface FileWatcher {
  watch(inputFile: string, options: WatchOptions): Promise<string>;
  unwatch(): void;
}
```

**监控流程**：
1. 创建输入文件（空文件）
2. 启动 fs.watch 监控
3. 等待文件变化或超时
4. 读取完整内容

---

## 5. 方案选择建议

### 5.1 推荐方案：基于文件操作

**理由**：
1. **与现有设计一致**：file-io-prd.md 已定义了完整的文件 IO 方案
2. **支持复杂场景**：工具调用、代码块、多行输入
3. **便于调试**：用户可直接查看/修改输入文件
4. **支持后台**：适合长时间运行的 workflow
5. **历史追溯**：交互内容持久化

### 5.2 实现架构

```
┌─────────────────────────────────────────────────────────────┐
│                    CLI Application                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐     │
│  │ User       │    │ File       │    │ File       │     │
│  │ Interaction│───►│ Writer    │───►│ Watcher    │     │
│  │ Handler    │    │           │    │           │     │
│  └─────────────┘    └───────────┘    └─────────────┘     │
│         ▲                                     │           │
│         │                                     │           │
│  ┌─────┴─────────────┐    ┌─────────────┴─────────────┐   │
│  │ SDK             │    │ .wf-agent/         │   │
│  │ Handler.handle()│◄───│ function/session/   │   │
│  │                │    │ human-relay-input.txt│   │
│  └────────────────┘    └────────────────────────────┘   │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐  │
│  │ output.md (显示操作指南)                            │  │
│  │                                                     │  │
│  │ **操作步骤**：                                      │  │
│  │ 1. 查看提示词：./function/session/human-relay-output.txt│ │
│  │ 2. 复制到网页 LLM                                   │  │
│  │ 3. 将响应粘贴到：./function/session/human-relay-input.txt│ │
│  │ 4. 保存文件，系统自动继续                            │  │
│  └─────────────────────────────────────────────────────┘  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 5.3 关键实现点

#### 5.3.1 文件写入

```typescript
async writePromptFile(sessionId: string, prompt: string): Promise<string> {
  const dir = path.join('.wf-agent', 'function', sessionId);
  const file = path.join(dir, 'human-relay-output.txt');
  
  await fs.mkdir(dir, { recursive: true });
  await fs.writeFile(file, prompt, 'utf-8');
  
  return file;
}
```

#### 5.3.2 文件监控

```typescript
async watchInputFile(sessionId: string, timeout: number): Promise<string> {
  const file = path.join('.wf-agent', 'function', sessionId, 'human-relay-input.txt');
  
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      watcher.close();
      reject(new Error('Timeout waiting for user input'));
    }, timeout);
    
    const watcher = fs.watch(file, (eventType) => {
      if (eventType === 'change') {
        clearTimeout(timer);
        watcher.close();
        
        // 读取内容（带重试确保写入完成）
        setTimeout(async () => {
          const content = await fs.readFile(file, 'utf-8');
          resolve(content.trim());
        }, 100);
      }
    });
  });
}
```

#### 5.3.3 输出文件更新

```typescript
async updateOutputDisplay(sessionId: string, state: DisplayState): Promise<void> {
  const dir = path.join('.wf-agent', 'display', sessionId);
  const file = path.join(dir, 'output.md');
  
  const content = renderMarkdown({
    sessionId,
    state,
    instructions: [
      `查看提示词：.wf-agent/function/${sessionId}/human-relay-output.txt`,
      `将响应粘贴到：.wf-agent/function/${sessionId}/human-relay-input.txt`,
      '保存文件后系统自动继续',
    ],
  });
  
  await fs.writeFile(file, content, 'utf-8');
}
```

---

## 6. 混合方案（可选优化）

### 6.1 设计思路

针对不同场景提供不同的交互方式：

| 场景 | 交互方式 | 说明 |
|------|----------|------|
| Graph 工作流 | 文件操作 | 复杂场景，需要历史记录 |
| 快速调试 | TUI 直接输入 | 简单场景，快速响应 |
| Agent Loop | 可配置 | 默认文件，可切换 |

### 6.2 配置选项

```toml
[humanRelay]
# 交互方式
# - file: 基于文件操作（默认）
# - tui: TUI 直接输入
# - auto: 自动选择（简单场景用 tui，复杂场景用 file）
mode = "file"

[humanRelay.file]
# 功能性文件目录
dir = ".wf-agent/function"
# 超时时间（毫秒）
timeout = 300000
# 自动清理旧文件
autoCleanup = true
```

### 6.3 实现示例

```typescript
class HybridHumanRelayHandler implements HumanRelayHandler {
  async handle(request: HumanRelayRequest, context: HumanRelayContext): Promise<HumanRelayResponse> {
    const mode = config.humanRelay.mode;
    
    switch (mode) {
      case 'file':
        return this.handleFileMode(request, context);
      case 'tui':
        return this.handleTUIMode(request, context);
      case 'auto':
        // 简单提示用 TUI，复杂提示用文件
        return request.prompt.length < 200 
          ? this.handleTUIMode(request, context)
          : this.handleFileMode(request, context);
    }
  }
}
```

---

## 7. 迁移计划

### 7.1 第一阶段：文件操作 Handler

1. 创建 `FileHumanRelayHandler`
2. 实现文件读写和监控逻辑
3. 更新 `output.md` 显示
4. 添加配置选项

### 7.2 第二阶段：保持向后兼容

1. 添加配置项 `humanRelay.mode`
2. 默认使用新实现
3. 保留原有 `CLIHumanRelayHandler` 作为备选

### 7.3 第三阶段：完善功能

1. 添加文��锁机制
2. 添加输入验证
3. 支持编辑回退
4. 添加清理机制

---

## 8. 结论

**推荐基于文件操作的方案**，理由如下：

1. **设计一致性**：与 file-io-prd.md 规范完全兼容
2. **功能完整性**：支持工具调用、代码块等复杂输入
3. **用户体验**：可使用熟悉的编辑器编辑
4. **可维护性**：交互历史便于调试和追溯
5. **扩展性**：便于支持多端（Web、移动端）扩展

TUI 直接输入方案适合作为快速调试选项，不作为默认方案。