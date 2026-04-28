# Human Relay 文件 IO 设计方案

## 一、需求分析

### 1.1 核心问题

在 TUI 环境下实现 Human Relay 面临以下挑战：

1. **渲染开销** - 富文本展示和实时更新导致 TUI 性能下降
2. **输入复杂性** - 多行文本编辑在 TUI 中实现复杂
3. **用户体验** - 用户可能更习惯使用外部编辑器
4. **状态管理** - 需要处理超时、取消等复杂状态

### 1.2 设计目标

- **降低复杂度** - 将 Human Relay 从 TUI 渲染中剥离
- **提升体验** - 允许用户使用熟悉的编辑器
- **保持简洁** - CLI 应用保持轻量级
- **可追溯性** - 所有交互记录到文件，便于审计

---

## 二、整体架构

### 2.1 架构图

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Agent Loop                                  │
│                         (运行中)                                     │
└───────────────────────────────┬─────────────────────────────────────┘
                                │
                                ▼ 触发 Human Relay
┌─────────────────────────────────────────────────────────────────────┐
│                     HumanRelayManager                               │
├─────────────────────────────────────────────────────────────────────┤
│  1. 收集上下文信息                                                   │
│  2. 格式化输出内容                                                   │
│  3. 写入 human-relay-output.txt                                      │
│  4. 启动文件监控                                                     │
│  5. 在 TUI 显示简洁提示                                              │
└───────────────────────────────┬─────────────────────────────────────┘
                                │
              ┌─────────────────┴─────────────────┐
              │                                   │
              ▼                                   ▼
┌─────────────────────────┐         ┌─────────────────────────────┐
│   human-relay-output.txt │         │   human-relay-input.txt     │
│   (只读，供用户查看)      │         │   (可写，用户输入响应)       │
├─────────────────────────┤         ├─────────────────────────────┤
│ - 请求 ID                │         │ - 响应内容                   │
│ - 提示内容               │         │ - 时间戳                     │
│ - 对话历史               │         │ - 关联请求 ID                │
│ - 元数据                 │         │                             │
└─────────────────────────┘         └─────────────────────────────┘
                                              │
                                              │ 文件变化
                                              ▼
                                    ┌─────────────────────┐
                                    │   FileWatcher       │
                                    │   (监控输入文件)     │
└───────────────────────────────────┴─────────────────────┘
                                          │
                                          ▼ 解析响应
                                    ┌─────────────────────┐
                                    │   ResponseParser    │
                                    │   (提取有效内容)     │
└───────────────────────────────────┴─────────────────────┘
                                          │
                                          ▼ 返回 Agent Loop
                                    ┌─────────────────────┐
                                    │   Agent 继续执行    │
                                    └─────────────────────┘
```

### 2.2 交互流程

```
┌─────────┐     ┌─────────────┐     ┌──────────────┐     ┌──────────┐
│ Agent   │────►│ HumanRelay  │────►│ Write Output │────►│ File     │
│ Loop    │     │ Manager     │     │ File         │     │ System   │
└─────────┘     └─────────────┘     └──────────────┘     └──────────┘
                                              │
                                              │ 创建/更新
                                              ▼
                                       ┌─────────────┐
                                       │ human-relay-│
                                       │ output.txt  │
                                       └─────────────┘
                                              │
                                              │ 显示提示
                                              ▼
                                       ┌─────────────┐
                                       │ TUI 提示    │
                                       │ (简洁)      │
                                       └─────────────┘
                                              │
                                              │ 用户编辑
                                              ▼
                                       ┌─────────────┐
                                       │ human-relay-│
                                       │ input.txt   │
                                       └─────────────┘
                                              │
                                              │ 监控变化
                                              ▼
┌─────────┐     ┌─────────────┐     ┌──────────────┐     ┌──────────┐
│ Agent   │◄────│ Parse       │◄────│ FileWatcher  │◄────│ File     │
│ Loop    │     │ Response    │     │ Detect       │     │ Change   │
└─────────┘     └─────────────┘     └──────────────┘     └──────────┘
```

---

## 三、文件格式规范

### 3.1 输出文件 (human-relay-output.txt)

#### 文件位置
```
./wf-agent/output/human-relay-output.txt
```

#### 文件格式

```
═══════════════════════════════════════════════════════════════════════
Human Relay Request
═══════════════════════════════════════════════════════════════════════
Request ID: req-abc123-def456
Timestamp: 2024-01-15T10:30:00.000Z
Timeout: 300000ms (5 minutes)
Session: session-xyz789
═══════════════════════════════════════════════════════════════════════

【提示内容】
请检查以下代码实现，指出潜在的性能问题：

【对话历史】
┌─────────────────────────────────────────────────────────────────────┐
[USER] 2024-01-15T10:25:00Z
───────────────────────────────────────────────────────────────────────
帮我优化这个排序算法

[ASSISTANT] 2024-01-15T10:26:30Z
───────────────────────────────────────────────────────────────────────
我已经实现了快速排序：

```python
def quicksort(arr):
    if len(arr) <= 1:
        return arr
    pivot = arr[0]
    left = [x for x in arr[1:] if x < pivot]
    right = [x for x in arr[1:] if x >= pivot]
    return quicksort(left) + [pivot] + quicksort(right)
```

[USER] 2024-01-15T10:28:00Z
───────────────────────────────────────────────────────────────────────
看起来不错，但我不确定性能如何

[ASSISTANT] 2024-01-15T10:29:30Z
───────────────────────────────────────────────────────────────────────
我需要您的专业意见来评估这个实现。
└─────────────────────────────────────────────────────────────────────┘

【上下文信息】
• 迭代次数: 3 / 10
• 工具调用: 2 次
  - file_read: src/algorithms/sort.py
  - code_analysis: complexity_check
• 相关文件:
  - src/algorithms/sort.py
  - tests/test_sort.py

【元数据】
Language: python
Topic: algorithm-optimization
Priority: normal

═══════════════════════════════════════════════════════════════════════
操作指南
═══════════════════════════════════════════════════════════════════════
1. 查看上方的完整上下文
2. 编辑 human-relay-input.txt 文件输入您的响应
3. 保存文件后，系统将自动读取您的输入
4. 如需取消，请在输入文件中写入 "CANCEL"

输入文件位置: ./wf-agent/output/human-relay-input.txt
═══════════════════════════════════════════════════════════════════════
```

#### 格式说明

| 部分 | 说明 | 必需 |
|------|------|------|
| 请求头 | 请求 ID、时间戳、超时时间 | 是 |
| 提示内容 | 当前需要用户处理的问题 | 是 |
| 对话历史 | 完整的对话上下文 | 是 |
| 上下文信息 | 迭代次数、工具调用等 | 否 |
| 元数据 | 语言、主题等附加信息 | 否 |
| 操作指南 | 用户操作说明 | 是 |

### 3.2 输入文件 (human-relay-input.txt)

#### 文件位置
```
./wf-agent/output/human-relay-input.txt
```

#### 文件格式

```
═══════════════════════════════════════════════════════════════════════
Human Relay Response
═══════════════════════════════════════════════════════════════════════
Request ID: req-abc123-def456
Response Time: 2024-01-15T10:32:15.000Z
Status: PENDING
═══════════════════════════════════════════════════════════════════════

【您的响应】
(请在此下方输入您的响应内容)

代码存在以下性能问题：

1. **空间复杂度高**: 使用了额外的数组存储 left 和 right，空间复杂度为 O(n)
   建议改用原地分区（in-place）实现

2. **递归深度问题**: 对于已排序数组，递归深度为 O(n)，可能导致栈溢出
   建议：
   - 使用尾递归优化
   - 或改用迭代实现
   - 或随机选择 pivot

3. **pivot 选择**: 固定选择第一个元素作为 pivot，在特定数据分布下性能退化到 O(n²)
   建议随机选择 pivot 或使用三数取中法

4. **缺少优化**: 对于小数组，可以切换到插入排序
   建议设置阈值（如 10），小数组使用插入排序

【操作】
- 保存文件以提交响应
- 写入 "CANCEL" 取消请求
- 写入 "SKIP" 跳过此问题

═══════════════════════════════════════════════════════════════════════
```

#### 状态字段

| 状态值 | 说明 |
|--------|------|
| PENDING | 等待用户输入 |
| SUBMITTED | 已提交，等待处理 |
| CANCELLED | 用户取消 |
| SKIPPED | 用户跳过 |
| TIMEOUT | 超时 |

---

## 四、核心组件设计

### 4.1 HumanRelayFileManager

```typescript
/**
 * Human Relay 文件管理器
 */
export class HumanRelayFileManager {
  private outputDir: string;
  private outputFile: string;
  private inputFile: string;
  private encoding: BufferEncoding;

  constructor(config?: HumanRelayFileConfig) {
    this.outputDir = config?.outputDir ?? './wf-agent/output';
    this.outputFile = path.join(this.outputDir, 'human-relay-output.txt');
    this.inputFile = path.join(this.outputDir, 'human-relay-input.txt');
    this.encoding = config?.encoding ?? 'utf-8';
    
    // 确保输出目录存在
    this.ensureOutputDir();
  }

  /**
   * 写入输出文件
   */
  async writeOutput(request: HumanRelayRequest): Promise<void> {
    const content = this.formatOutput(request);
    await fs.promises.writeFile(this.outputFile, content, this.encoding);
  }

  /**
   * 初始化输入文件模板
   */
  async initInputFile(requestId: string): Promise<void> {
    const template = this.formatInputTemplate(requestId);
    await fs.promises.writeFile(this.inputFile, template, this.encoding);
  }

  /**
   * 读取输入文件
   */
  async readInput(): Promise<string | null> {
    try {
      const content = await fs.promises.readFile(this.inputFile, this.encoding);
      return this.parseResponse(content);
    } catch (error) {
      return null;
    }
  }

  /**
   * 清空输入文件
   */
  async clearInput(): Promise<void> {
    await fs.promises.writeFile(this.inputFile, '', this.encoding);
  }

  /**
   * 获取文件路径
   */
  getPaths(): { output: string; input: string } {
    return {
      output: this.outputFile,
      input: this.inputFile,
    };
  }

  private ensureOutputDir(): void {
    if (!fs.existsSync(this.outputDir)) {
      fs.mkdirSync(this.outputDir, { recursive: true });
    }
  }

  private formatOutput(request: HumanRelayRequest): string {
    // 格式化输出内容
    // ...
  }

  private formatInputTemplate(requestId: string): string {
    // 格式化输入模板
    // ...
  }

  private parseResponse(content: string): string | null {
    // 解析响应内容
    // ...
  }
}
```

### 4.2 HumanRelayFileWatcher

```typescript
/**
 * Human Relay 文件监控器
 */
export class HumanRelayFileWatcher extends EventEmitter {
  private watcher?: fs.FSWatcher;
  private inputFile: string;
  private isWatching: boolean = false;

  constructor(inputFile: string) {
    super();
    this.inputFile = inputFile;
  }

  /**
   * 开始监控
   */
  start(): void {
    if (this.isWatching) return;

    this.watcher = fs.watch(this.inputFile, (eventType) => {
      if (eventType === 'change') {
        this.emit('change');
      }
    });

    this.isWatching = true;
  }

  /**
   * 停止监控
   */
  stop(): void {
    this.watcher?.close();
    this.watcher = undefined;
    this.isWatching = false;
  }

  /**
   * 等待文件变化
   */
  async waitForChange(timeout: number): Promise<boolean> {
    return new Promise((resolve) => {
      const timeoutId = setTimeout(() => {
        this.off('change', onChange);
        resolve(false);
      }, timeout);

      const onChange = () => {
        clearTimeout(timeoutId);
        resolve(true);
      };

      this.once('change', onChange);
    });
  }
}
```

### 4.3 HumanRelayFileHandler

```typescript
/**
 * Human Relay 文件处理器
 * 实现 HumanRelayHandler 接口
 */
export class HumanRelayFileHandler implements HumanRelayHandler {
  private fileManager: HumanRelayFileManager;
  private fileWatcher: HumanRelayFileWatcher;
  private pollingInterval: number;

  constructor(config?: HumanRelayFileConfig) {
    this.fileManager = new HumanRelayFileManager(config);
    this.fileWatcher = new HumanRelayFileWatcher(
      this.fileManager.getPaths().input
    );
    this.pollingInterval = config?.pollingInterval ?? 1000;
  }

  /**
   * 处理 Human Relay 请求
   */
  async handle(
    request: HumanRelayRequest,
    context: HumanRelayContext
  ): Promise<HumanRelayResponse> {
    const startTime = Date.now();

    try {
      // 1. 写入输出文件
      await this.fileManager.writeOutput(request);

      // 2. 初始化输入文件
      await this.fileManager.initInputFile(request.requestId);

      // 3. 启动文件监控
      this.fileWatcher.start();

      // 4. 等待用户输入或超时
      const response = await this.waitForResponse(
        request.requestId,
        request.timeout
      );

      return {
        requestId: request.requestId,
        content: response,
        timestamp: Date.now(),
      };
    } finally {
      // 清理
      this.fileWatcher.stop();
    }
  }

  /**
   * 等待用户响应
   */
  private async waitForResponse(
    requestId: string,
    timeout: number
  ): Promise<string> {
    const deadline = Date.now() + timeout;

    while (Date.now() < deadline) {
      const remaining = deadline - Date.now();
      
      // 等待文件变化或超时
      const changed = await this.fileWatcher.waitForChange(
        Math.min(this.pollingInterval, remaining)
      );

      if (changed) {
        // 读取并解析响应
        const content = await this.fileManager.readInput();
        
        if (content) {
          // 验证响应是否对应当前请求
          if (this.validateResponse(content, requestId)) {
            // 检查特殊命令
            const command = this.parseCommand(content);
            if (command === 'CANCEL') {
              throw new Error('User cancelled');
            }
            if (command === 'SKIP') {
              return '[SKIPPED]';
            }
            return content;
          }
        }
      }
    }

    throw new Error('Human Relay timeout');
  }

  /**
   * 验证响应
   */
  private validateResponse(content: string, requestId: string): boolean {
    const match = content.match(/Request ID:\s*(\S+)/);
    return match !== null && match[1] === requestId;
  }

  /**
   * 解析特殊命令
   */
  private parseCommand(content: string): string | null {
    const trimmed = content.trim().toUpperCase();
    if (trimmed === 'CANCEL' || trimmed === 'SKIP') {
      return trimmed;
    }
    return null;
  }
}
```

---

## 五、TUI 集成

### 5.1 TUI 提示组件

```typescript
/**
 * Human Relay TUI 提示组件
 */
export class HumanRelayTUIIndicator implements Component {
  private requestId: string;
  private startTime: number;
  private timeout: number;

  constructor(requestId: string, timeout: number) {
    this.requestId = requestId;
    this.startTime = Date.now();
    this.timeout = timeout;
  }

  render(width: number): string[] {
    const elapsed = Date.now() - this.startTime;
    const remaining = Math.max(0, this.timeout - elapsed);
    const remainingSec = Math.ceil(remaining / 1000);

    const lines: string[] = [];
    lines.push('┌' + '─'.repeat(width - 2) + '┐');
    lines.push('│' + ' '.repeat(width - 2) + '│');
    lines.push(
      '│' + this.center('🤖 Human Relay Request', width - 2) + '│'
    );
    lines.push('│' + ' '.repeat(width - 2) + '│');
    lines.push(
      '│' + this.pad(`Request ID: ${this.requestId.substring(0, 8)}...`, width - 2) + '│'
    );
    lines.push('│' + ' '.repeat(width - 2) + '│');
    lines.push(
      '│' + this.pad(`⏱️  Timeout: ${remainingSec}s`, width - 2) + '│'
    );
    lines.push('│' + ' '.repeat(width - 2) + '│');
    lines.push(
      '│' + this.pad('📄 See: human-relay-output.txt', width - 2) + '│'
    );
    lines.push(
      '│' + this.pad('✏️  Edit: human-relay-input.txt', width - 2) + '│'
    );
    lines.push('│' + ' '.repeat(width - 2) + '│');
    lines.push('└' + '─'.repeat(width - 2) + '┘');

    return lines;
  }

  private center(text: string, width: number): string {
    const padding = Math.max(0, width - text.length);
    const left = Math.floor(padding / 2);
    const right = padding - left;
    return ' '.repeat(left) + text + ' '.repeat(right);
  }

  private pad(text: string, width: number): string {
    const padding = Math.max(0, width - text.length);
    return ' ' + text + ' '.repeat(padding - 1);
  }

  invalidate(): void {
    // 无需缓存
  }
}
```

### 5.2 集成到主 TUI

```typescript
/**
 * 在 TUI 中显示 Human Relay 提示
 */
export class TUIHumanRelayIntegration {
  private tui: TUI;
  private overlayHandle?: OverlayHandle;

  constructor(tui: TUI) {
    this.tui = tui;
  }

  /**
   * 显示 Human Relay 提示
   */
  showIndicator(requestId: string, timeout: number): void {
    const indicator = new HumanRelayTUIIndicator(requestId, timeout);
    
    this.overlayHandle = this.tui.showOverlay(indicator, {
      anchor: 'bottom-right',
      offsetX: -2,
      offsetY: -1,
      width: 50,
      nonCapturing: true,
    });
  }

  /**
   * 隐藏提示
   */
  hideIndicator(): void {
    this.overlayHandle?.hide();
    this.overlayHandle = undefined;
  }
}
```

---

## 六、使用示例

### 6.1 基础使用

```typescript
import { HumanRelayFileHandler } from './human-relay/file-handler';

// 创建处理器
const handler = new HumanRelayFileHandler({
  outputDir: './wf-agent/output',
  pollingInterval: 1000,
});

// 注册到 SDK
sdk.humanRelay.registerHandler(handler);

// 当 Agent Loop 触发 Human Relay 时：
// 1. 自动写入 human-relay-output.txt
// 2. 在 TUI 显示简洁提示
// 3. 监控 human-relay-input.txt
// 4. 读取用户响应并返回
```

### 6.2 与 TUI 集成

```typescript
import { TUIHumanRelayIntegration } from './tui/human-relay-integration';

const tui = new TUI(terminal);
const humanRelayUI = new TUIHumanRelayIntegration(tui);

// 创建带 UI 的处理器
class HumanRelayUIHandler extends HumanRelayFileHandler {
  async handle(request, context) {
    // 显示 TUI 提示
    humanRelayUI.showIndicator(request.requestId, request.timeout);
    
    try {
      // 调用父类处理
      const response = await super.handle(request, context);
      return response;
    } finally {
      // 隐藏提示
      humanRelayUI.hideIndicator();
    }
  }
}
```

---

## 七、错误处理

### 7.1 错误类型

| 错误类型 | 说明 | 处理方式 |
|----------|------|----------|
| FileWriteError | 文件写入失败 | 重试 3 次，然后报错 |
| FileReadError | 文件读取失败 | 等待后重试 |
| TimeoutError | 超时 | 返回超时错误 |
| CancelError | 用户取消 | 返回取消状态 |
| ParseError | 解析失败 | 忽略并继续等待 |

### 7.2 错误处理示例

```typescript
private async handleError(error: Error): Promise<never> {
  if (error.message === 'User cancelled') {
    throw new HumanRelayCancelError();
  }
  if (error.message === 'Human Relay timeout') {
    throw new HumanRelayTimeoutError();
  }
  throw new HumanRelayError(`Human Relay failed: ${error.message}`);
}
```

---

## 八、配置选项

### 8.1 配置接口

```typescript
interface HumanRelayFileConfig {
  /** 输出目录 */
  outputDir?: string;
  
  /** 文件编码 */
  encoding?: BufferEncoding;
  
  /** 轮询间隔（毫秒） */
  pollingInterval?: number;
  
  /** 是否启用文件轮转 */
  enableRotation?: boolean;
  
  /** 最大文件数 */
  maxFiles?: number;
  
  /** 是否在输出中包含完整对话历史 */
  includeFullHistory?: boolean;
  
  /** 是否在输出中包含工具调用详情 */
  includeToolCalls?: boolean;
  
  /** 自定义模板 */
  templates?: {
    output?: string;
    input?: string;
  };
}
```

### 8.2 默认配置

```typescript
const DEFAULT_CONFIG: HumanRelayFileConfig = {
  outputDir: './wf-agent/output',
  encoding: 'utf-8',
  pollingInterval: 1000,
  enableRotation: true,
  maxFiles: 10,
  includeFullHistory: true,
  includeToolCalls: false,
};
```

---

## 九、迁移指南

### 9.1 从 Readline 迁移

```typescript
// 旧实现
import { CLIHumanRelayHandler } from './handlers/cli-human-relay-handler';
sdk.humanRelay.registerHandler(new CLIHumanRelayHandler());

// 新实现
import { HumanRelayFileHandler } from './human-relay/file-handler';
sdk.humanRelay.registerHandler(new HumanRelayFileHandler());
```

### 9.2 渐进式迁移

1. **阶段一** - 保留 readline 作为 fallback
2. **阶段二** - 默认使用文件 IO，可选 readline
3. **阶段三** - 完全切换到文件 IO

---

## 十、注意事项

1. **文件权限** - 确保输出目录可读写
2. **并发处理** - 同一时间只能有一个 Human Relay 请求
3. **编码一致** - 统一使用 UTF-8 编码
4. **清理机制** - 定期清理旧的输出文件
5. **跨平台** - 路径处理使用 path 模块
