# Mini-Agent 基本功能实现文档

## 1. Agent 基本功能概述

Mini-Agent 是一个基于 MiniMax M2 模型的智能代理系统，实现了完整的代理执行循环。其核心功能包括：
- 与用户的多轮对话
- 工具调用和结果处理
- 上下文管理和长期记忆
- 智能决策和任务执行

## 2. Agent 核心执行流程

### 2.1 初始化阶段
```python
class Agent:
    def __init__(
        self,
        llm_client: LLMClient,
        system_prompt: str,
        tools: list[Tool],
        max_steps: int = 50,
        workspace_dir: str = "./workspace",
        token_limit: int = 80000,
    ):
```

在初始化阶段，Agent 设置了：
- LLM 客户端用于与模型通信
- 可用工具字典
- 最大执行步数限制
- 工作空间目录
- 令牌数量限制（用于上下文管理）
- 初始消息历史（包含系统提示）

### 2.2 执行循环
Agent 的核心执行流程在 `run()` 方法中实现：

```python
async def run(self) -> str:
    step = 0
    while step < self.max_steps:
        # 1. 检查并摘要消息历史
        await self._summarize_messages()
        
        # 2. 调用 LLM 获取响应
        response = await self.llm.generate(messages=self.messages, tools=tool_schemas)
        
        # 3. 添加助手消息到历史
        self.messages.append(assistant_msg)
        
        # 4. 如果有工具调用，则执行
        if response.tool_calls:
            for tool_call in response.tool_calls:
                # 执行工具并获取结果
                result = await tool.execute(**arguments)
                
                # 添加工具结果到消息历史
                tool_msg = Message(...)
                self.messages.append(tool_msg)
        
        step += 1
    
    return response.content
```

## 3. 上下文处理逻辑

### 3.1 令牌估算
Agent 使用 tiktoken 库准确估算消息历史的令牌数量：

```python
def _estimate_tokens(self) -> int:
    try:
        encoding = tiktoken.get_encoding("cl100k_base")
    except Exception:
        return self._estimate_tokens_fallback()

    total_tokens = 0
    for msg in self.messages:
        # 计算内容令牌数
        if isinstance(msg.content, str):
            total_tokens += len(encoding.encode(msg.content))
        # ... 其他内容类型的处理
        
        # 计算思维链令牌数
        if msg.thinking:
            total_tokens += len(encoding.encode(msg.thinking))
            
        # 计算工具调用令牌数
        if msg.tool_calls:
            total_tokens += len(encoding.encode(str(msg.tool_calls)))
            
        # 每条消息的元数据开销
        total_tokens += 4

    return total_tokens
```

### 3.2 智能摘要机制
当令牌数量超过预设限制时，Agent 会触发智能摘要机制：

```python
async def _summarize_messages(self):
    estimated_tokens = self._estimate_tokens()
    
    if estimated_tokens <= self.token_limit:
        return  # 不需要摘要
    
    # 查找所有用户消息的索引
    user_indices = [i for i, msg in enumerate(self.messages) 
                   if msg.role == "user" and i > 0]
    
    # 构建新消息列表：系统提示 + 用户消息 + 执行摘要
    new_messages = [self.messages[0]]  # 保留系统提示
    
    for i, user_idx in enumerate(user_indices):
        # 添加当前用户消息
        new_messages.append(self.messages[user_idx])
        
        # 确定要摘要的消息范围
        if i < len(user_indices) - 1:
            next_user_idx = user_indices[i + 1]
        else:
            next_user_idx = len(self.messages)
        
        # 提取执行消息
        execution_messages = self.messages[user_idx + 1 : next_user_idx]
        
        if execution_messages:
            # 创建摘要
            summary_text = await self._create_summary(execution_messages, i + 1)
            summary_message = Message(
                role="user",
                content=f"[Assistant Execution Summary]\n\n{summary_text}",
            )
            new_messages.append(summary_message)
    
    # 替换消息列表
    self.messages = new_messages
```

### 3.3 执行摘要创建
系统使用 LLM 本身来创建高质量的摘要：

```python
async def _create_summary(self, messages: list[Message], round_num: int) -> str:
    summary_content = f"Round {round_num} execution process:\n\n"
    for msg in messages:
        if msg.role == "assistant":
            content_text = msg.content if isinstance(msg.content, str) else str(msg.content)
            summary_content += f"Assistant: {content_text}\n"
            if msg.tool_calls:
                tool_names = [tc.function.name for tc in msg.tool_calls]
                summary_content += f"  → Called tools: {', '.join(tool_names)}\n"
        elif msg.role == "tool":
            result_preview = msg.content if isinstance(msg.content, str) else str(msg.content)
            summary_content += f"  ← Tool returned: {result_preview}...\n"

    # 调用 LLM 生成摘要
    summary_prompt = f"""请提供以下代理执行过程的简洁摘要：
    {summary_content}
    ...
    """
    
    response = await self.llm.generate(
        messages=[Message(role="system", content="..."), 
                 Message(role="user", content=summary_prompt)],
    )
    
    return response.content
```

## 4. 上下文更新方式

### 4.1 消息历史更新
Agent 通过以下方式持续更新上下文：

1. **添加用户消息**：
```python
def add_user_message(self, content: str):
    self.messages.append(Message(role="user", content=content))
```

2. **添加助手回复**：
```python
# 在每次 LLM 响应后
assistant_msg = Message(
    role="assistant",
    content=response.content,
    thinking=response.thinking,
    tool_calls=response.tool_calls,
)
self.messages.append(assistant_msg)
```

3. **添加工具结果**：
```python
# 在工具执行完成后
tool_msg = Message(
    role="tool",
    content=result.content if result.success else f"Error: {result.error}",
    tool_call_id=tool_call_id,
    name=function_name,
)
self.messages.append(tool_msg)
```

### 4.2 会话记忆机制
Agent 通过 SessionNoteTool 实现长期记忆：

```python
class SessionNoteTool(Tool):
    async def execute(self, content: str, category: str = "general") -> ToolResult:
        # 加载现有笔记
        notes = self._load_from_file()
        
        # 添加新笔记
        note = {
            "timestamp": datetime.now().isoformat(),
            "category": category,
            "content": content,
        }
        notes.append(note)
        
        # 保存回文件
        self._save_to_file(notes)
        
        return ToolResult(
            success=True,
            content=f"Recorded note: {content} (category: {category})",
        )
```

### 4.3 工作空间集成
Agent 将工作空间信息注入到系统提示中：

```python
# 在初始化时
if "Current Workspace" not in system_prompt:
    workspace_info = f"\n\n## Current Workspace\nYou are currently working in: `{self.workspace_dir.absolute()}`\nAll relative paths will be resolved relative to this directory."
    system_prompt = system_prompt + workspace_info
```

## 5. 工具集成与上下文交互

### 5.1 工具调用流程
Agent 通过以下流程处理工具调用：

1. LLM 返回工具调用请求
2. Agent 解析工具调用参数
3. 执行相应的工具
4. 将工具结果作为新消息添加到上下文中
5. 继续执行循环

### 5.2 工具模式定义
Agent 向 LLM 提供工具模式信息：

```python
# 获取工具模式
tool_schemas = [tool.to_schema() for tool in self.tools.values()]

# 发送给 LLM
response = await self.llm.generate(messages=self.messages, tools=tool_schemas)
```

## 6. 总结

Mini-Agent 通过以下方式实现了一个功能完整的代理系统：

1. **智能上下文管理**：通过令牌估算和智能摘要机制，有效管理长对话的上下文
2. **持续上下文更新**：在每次交互后更新消息历史，保持上下文连贯性
3. **工具集成**：无缝集成多种工具，扩展代理的能力
4. **长期记忆**：通过笔记工具实现跨会话的记忆功能
5. **工作空间感知**：代理了解当前工作环境，能够执行相关操作

这种设计使得 Mini-Agent 能够处理复杂的多步骤任务，同时保持良好的性能和上下文管理。