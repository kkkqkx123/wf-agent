# StateGraph API 与 LangGraph 对比

## 1. LangGraph 原始示例

```python
from typing import TypedDict, List, Literal
from langgraph.graph import StateGraph, END
from operator import itemgetter

# 1. 定义我们自己的"世界状态"
class AgentWorldState(TypedDict):
    task: str
    messages: List[tuple[str, str]] 
    next_agent: Literal["Researcher", "Writer", "FINISH"]

# 2. 定义Agent节点，展示它如何与"世界状态"交互
def researcher_agent_node(state: AgentWorldState):
    task = state['task']
    print(f"--- [Agent: 研究员] 开始工作，任务: {task} ---")
    research_result = f"这是关于'{task}'的研究成果。"
    # 注意：这里返回的是一个包含元组的列表，以支持状态的累加
    return {"messages": [("Researcher", research_result)]}

def writer_agent_node(state: AgentWorldState):
    messages = state['messages']
    print(f"--- [Agent: 作家] 开始工作 ---")
    writing_result = f"基于以下研究成果：\n{messages[-1][1]}\n\n我完成了最终报告。"
    return {"messages": [("Writer", writing_result)]}

# 3. 定义我们的核心"调度器"节点
def dispatcher_node(state: AgentWorldState):
    last_message_sender = state['messages'][-1][0] if state['messages'] else "START"
    
    if last_message_sender == "Researcher":
        return {"next_agent": "Writer"}
    elif last_message_sender == "Writer":
        return {"next_agent": "FINISH"}
    else: # START
        return {"next_agent": "Researcher"}

# 4. 在LangGraph中组装
workflow = StateGraph(AgentWorldState)
workflow.add_node("researcher", researcher_agent_node)
workflow.add_node("writer", writer_agent_node)
workflow.add_node("dispatcher", dispatcher_node) 

workflow.set_entry_point("dispatcher")

workflow.add_conditional_edges(
    "dispatcher",
    itemgetter('next_agent'),
    {
        "Researcher": "researcher",
        "Writer": "writer",
        "FINISH": END
    }
)

workflow.add_edge("researcher", "dispatcher")
workflow.add_edge("writer", "dispatcher")

# 编译
app = workflow.compile()

# 运行
inputs = {"task": "AI在软件开发中的作用", "messages": []}
for s in app.stream(inputs, {"recursion_limit": 10}):
    print(s)
    print("----")
```

**代码行数：** 约40行

## 2. StateGraph API 实现

```typescript
import { SDK } from '../sdk/api';
import { StateGraph, END } from '../sdk/api/stategraph-api';

// 1. 定义我们自己的"世界状态"
interface AgentWorldState {
  task: string;
  messages: [string, string][];  // [sender, message]
  nextAgent: 'Researcher' | 'Writer' | 'FINISH';
}

// 2. 定义Agent节点，展示它如何与"世界状态"交互
function researcherAgentNode(state: AgentWorldState): Partial<AgentWorldState> {
  const { task } = state;
  console.log(`--- [Agent: 研究员] 开始工作，任务: ${task} ---`);
  const researchResult = `这是关于'${task}'的研究成果。`;
  return { 
    messages: [...state.messages, ['Researcher', researchResult]] as [string, string][],
    nextAgent: 'Writer' as const
  };
}

function writerAgentNode(state: AgentWorldState): Partial<AgentWorldState> {
  const { messages } = state;
  console.log(`--- [Agent: 作家] 开始工作 ---`);
  const lastMessage = messages[messages.length - 1][1];
  const writingResult = `基于以下研究成果：\n${lastMessage}\n\n我完成了最终报告。`;
  return { 
    messages: [...state.messages, ['Writer', writingResult]] as [string, string][],
    nextAgent: 'FINISH' as const
  };
}

// 3. 定义我们的核心"调度器"节点
function dispatcherNode(state: AgentWorldState): Partial<AgentWorldState> {
  const lastMessageSender = state.messages.length > 0 ? 
    state.messages[state.messages.length - 1][0] : 'START';
  
  if (lastMessageSender === 'Researcher') {
    return { nextAgent: 'Writer' };
  } else if (lastMessageSender === 'Writer') {
    return { nextAgent: 'FINISH' };
  } else { // START
    return { nextAgent: 'Researcher' };
  }
}

async function runExample() {
  // 创建SDK实例
  const sdk = new SDK();
  
  // 4. 使用StateGraph组装工作流
  const workflow = new StateGraph<AgentWorldState>({} as AgentWorldState);
  workflow.add_node("researcher", researcherAgentNode);
  workflow.add_node("writer", writerAgentNode);
  workflow.add_node("dispatcher", dispatcherNode); 
  
  workflow.set_entry_point("dispatcher");
  
  workflow.add_conditional_edges(
    "dispatcher",
    (state: AgentWorldState) => state.nextAgent,
    {
      "Researcher": "researcher",
      "Writer": "writer",
      "FINISH": END
    }
  );
  
  workflow.add_edge("researcher", "dispatcher");
  workflow.add_edge("writer", "dispatcher");
  
  // 编译
  const app = workflow.compile(sdk);
  
  // 运行
  const inputs = {
    task: "AI在软件开发中的作用",
    messages: [],
    nextAgent: "Researcher" as const
  };
  
  // 执行工作流
  const result = await app.invoke(inputs);
  
  // 或者使用流式执行
  for await (const chunk of app.stream(inputs)) {
    console.log(chunk);
  }
}
```

**代码行数：** 约70行（包含更多注释和类型定义）

## 3. 功能对比

| 特性 | LangGraph | StateGraph API | 评价 |
|------|-----------|----------------|------|
| **API简洁性** | 非常简洁 | 相对简洁 | LangGraph略优 |
| **类型安全** | Python类型提示 | TypeScript强类型 | StateGraph API更优 |
| **IDE支持** | 一般 | 优秀 | StateGraph API更优 |
| **错误检测** | 运行时 | 编译时 | StateGraph API更优 |
| **学习曲线** | 低 | 低到中等 | 相似 |
| **生态系统** | 成熟 | 正在建设 | LangGraph更优 |
| **调试能力** | 一般 | 优秀 | StateGraph API更优 |

## 4. 语法对比

### 4.1 状态定义
- **LangGraph**: `class AgentWorldState(TypedDict)`
- **StateGraph**: `interface AgentWorldState`

### 4.2 节点添加
- **LangGraph**: `workflow.add_node("name", function)`
- **StateGraph**: `workflow.add_node("name", function)`

### 4.3 条件边
- **LangGraph**: `add_conditional_edges(source, condition_func, mapping)`
- **StateGraph**: `add_conditional_edges(source, condition_func, mapping)`

### 4.4 执行
- **LangGraph**: `app.stream(inputs)` 或 `app.invoke(inputs)`
- **StateGraph**: `app.stream(inputs)` 或 `app.invoke(inputs)`

## 5. 优势分析

### 5.1 StateGraph API 优势
1. **类型安全**：完全的TypeScript类型检查
2. **编译时错误检测**：提前发现类型错误
3. **IDE支持**：更好的自动补全和重构
4. **调试能力**：更好的调试体验
5. **与JavaScript生态兼容**：可以用于前端和Node.js

### 5.2 LangGraph 优势
1. **成熟度**：更成熟的生态系统
2. **社区支持**：更大的社区和更多资源
3. **简洁性**：略微更简洁的语法
4. **文档**：更完善的文档

## 6. 总结

StateGraph API成功实现了与LangGraph相似的功能和API设计，同时提供了TypeScript的类型安全和其他优势。虽然在代码行数上略有增加（主要是类型定义），但在易用性方面几乎与LangGraph相当，而且提供了更强的类型安全和更好的开发体验。

这种实现达到了预期目标：提供类似LangGraph的易用性，同时保持TypeScript的类型安全优势。