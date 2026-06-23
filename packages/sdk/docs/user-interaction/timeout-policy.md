### 1. 核心设计理念：策略模式 (Policy Pattern)

我们将引入 `InteractionPolicy` 概念，用于描述一个交互请求在“非正常路径”下的行为准则。

- **解耦**：将“如何展示”与“如何处理异常/超时”分离。
- **可配置**：允许不同的工具或节点根据自身重要性选择不同的兜底策略。
- **可扩展**：为未来接入 LLM 自动决策预留了字段入口。

---

### 2. 类型定义方案 (`packages/types包`)

我们需要在现有的 `types.ts` 中增加以下核心定义：

```typescript
/**
 * 交互超时后的处理策略
 */
export type TimeoutAction =
  | "fail" // 立即标记为失败，中断流程
  | "default" // 使用预设的默认值继续流程
  | "llm-decide" // 触发临时 LLM 调用进行智能判断（待实现）
  | "wait"; // 无限期等待，不触发超时逻辑

/**
 * 交互失败/兜底策略配置
 */
export interface InteractionPolicy {
  /**
   * 超时时间（毫秒）。设置为 0 或 undefined 表示根据全局配置或无限等待
   */
  timeoutMs?: number;

  /**
   * 超时发生时的动作
   * @default 'fail'
   */
  onTimeout: TimeoutAction;

  /**
   * 当 onTimeout 为 'default' 时使用的返回值
   * 例如：工具审批场景下可以是 { approved: false }
   */
  defaultValue?: any;

  /**
   * 当 onTimeout 为 'llm-decide' 时，提供给 LLM 的上下文提示词模板
   * 支持占位符，如 {eventSummary}, {executionContext}
   */
  llmDecisionPrompt?: string;

  /**
   * 是否允许重试。如果用户输入无效，是否重新提问
   * @default true
   */
  allowRetry?: boolean;
}

/**
 * 增强的交互处理器接口
 * 增加了策略获取能力，以便协调器能提前知晓如何处理异常情况
 */
export interface IInteractionHandler<T extends BaseEvent = BaseEvent> {
  readonly eventType: string;

  /**
   * 获取该处理器对应的交互策略
   * 如果未提供，协调器将使用全局默认策略
   */
  getPolicy?(): InteractionPolicy;

  /**
   * 处理交互事件并返回结果
   */
  handle(event: T): Promise<InteractionResponse>;
}
```

---

### 3. 协调器接口调整预览

1.  **策略优先级**：`Handler.getPolicy()` > `registerHandler` 时传入的配置 > 全局默认配置。
2.  **超时监控器**：协调器内部将维护一个 `Map<string, NodeJS.Timeout>`，根据每个事件的 `timeoutMs` 动态设置定时器。
3.  **执行分支**：
    - 若触发超时且动作为 `'fail'`：直接调用 `emitFailure`。
    - 若动作为 `'default'`：直接将 `defaultValue` 包装成响应并发射。
    - 若动作为 `'llm-decide'`：调用（未来的）`LLMDecisionService`。

---

### 4. 典型场景配置示例

| 场景               | `onTimeout`    | `defaultValue`        | 说明                                            |
| :----------------- | :------------- | :-------------------- | :---------------------------------------------- |
| **关键信息追问**   | `'fail'`       | -                     | 必须拿到用户答案，否则无法继续。                |
| **文件删除审批**   | `'default'`    | `{ approved: false }` | 安全原则：未确认即视为拒绝。                    |
| **代码格式化建议** | `'default'`    | `{ approved: true }`  | 低风险操作：未确认则默认执行以提高效率。        |
| **复杂方案选择**   | `'llm-decide'` | -                     | 让 LLM 根据之前的对话历史选择一个最可能的选项。 |

### 5. 下一步行动建议

1.  **更新类型文件**：将上述定义写入 `sdk/interaction/types.ts`。
2.  **导出新类型**：确保这些类型能通过 `sdk/index.ts` 暴露给外部应用。
3.  **文档化**：在 `docs/sdk/interaction-design.md`（如果存在）中记录这些策略的含义，方便后续开发 LLM 决策组件时参考。

你觉得这个类型定义是否覆盖了你目前考虑到的所有边界情况？如果有特定的业务场景需要特殊的字段，我们可以现在补充进去。
