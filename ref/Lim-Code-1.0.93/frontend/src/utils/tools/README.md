# 工具注册框架

这是一个统一的工具注册和显示框架，用于在消息列表中展示各种工具的调用和执行结果。

## 核心概念

### 1. 工具配置 (ToolConfig)

每个工具需要提供以下配置：

```typescript
interface ToolConfig {
  /** 工具名称 */
  name: string
  
  /** 工具显示标签（可选，默认使用name） */
  label?: string
  
  /** 图标 (codicon) */
  icon?: string
  
  /** 描述生成器 - 根据参数生成描述文本 */
  descriptionFormatter: (args: Record<string, unknown>) => string
  
  /** 内容面板组件 - 用于展开后显示详细信息 */
  contentComponent?: Component
  
  /** 默认内容渲染器 - 如果没有自定义组件，使用此函数渲染 */
  contentFormatter?: (args: Record<string, unknown>, result?: Record<string, unknown>) => string
}
```

### 2. 三部分显示

工具消息在界面上分为三部分：

1. **name（工具名称）**：显示在 "Tool" 角色标签后面
2. **description（描述）**：显示工具的关键参数摘要，方便快速查看
3. **content（内容）**：详细的调用和结果信息，需要展开查看

## 使用方法

### 1. 基础注册（使用内容格式化器）

最简单的方式是提供一个 `contentFormatter` 函数：

```typescript
import { registerTool } from '../toolRegistry'

registerTool('read_file', {
  name: 'read_file',
  label: '读取文件',
  icon: 'codicon-file-text',
  
  // 描述生成器 - 显示在消息列表中
  descriptionFormatter: (args) => {
    const path = args.path as string
    return `读取: ${path}`
  },
  
  // 内容格式化器 - 展开后显示
  contentFormatter: (args, result) => {
    if (result?.content) {
      const content = result.content as string
      const lines = content.split('\n')
      return `文件内容 (共 ${lines.length} 行):\n\n${content}`
    }
    return '无内容'
  }
})
```

### 2. 高级注册（使用自定义组件）

对于复杂的显示需求，可以创建自定义 Vue 组件：

```typescript
// readFilePanel.vue
<script setup lang="ts">
const props = defineProps<{
  args: Record<string, unknown>
  result?: Record<string, unknown>
  error?: string
}>()
</script>

<template>
  <div class="read-file-panel">
    <div class="file-path">{{ args.path }}</div>
    <div v-if="result?.content" class="file-content">
      <pre>{{ result.content }}</pre>
    </div>
    <div v-if="error" class="error">{{ error }}</div>
  </div>
</template>
```

然后注册时使用这个组件：

```typescript
import { registerTool } from '../toolRegistry'
import ReadFilePanel from './readFilePanel.vue'

registerTool('read_file', {
  name: 'read_file',
  label: '读取文件',
  icon: 'codicon-file-text',
  
  descriptionFormatter: (args) => {
    return `读取: ${args.path}`
  },
  
  // 使用自定义组件
  contentComponent: ReadFilePanel
})
```

### 3. 在消息中使用

创建工具消息时，需要提供 `ToolUsage` 数据：

```typescript
const message: Message = {
  id: 'msg-1',
  role: 'tool',
  content: '',
  timestamp: Date.now(),
  tools: [
    {
      id: 'tool-1',
      name: 'read_file',
      args: {
        path: '/path/to/file.ts'
      },
      result: {
        content: 'file content here...'
      },
      status: 'success',
      duration: 150
    }
  ]
}
```

## 完整示例

### 示例 1：write_file 工具

```typescript
// writeFile.ts
import { registerTool } from '../toolRegistry'

registerTool('write_file', {
  name: 'write_file',
  label: '写入文件',
  icon: 'codicon-save',
  
  descriptionFormatter: (args) => {
    const path = args.path as string
    const lines = (args.content as string).split('\n').length
    return `写入 ${lines} 行到: ${path}`
  },
  
  contentFormatter: (args, result) => {
    const path = args.path as string
    const content = args.content as string
    const preview = content.split('\n').slice(0, 5).join('\n')
    
    return `文件: ${path}\n\n写入内容预览:\n${preview}\n...`
  }
})
```

### 示例 2：execute_command 工具

```typescript
// executeCommand.ts
import { registerTool } from '../toolRegistry'

registerTool('execute_command', {
  name: 'execute_command',
  label: '执行命令',
  icon: 'codicon-terminal',
  
  descriptionFormatter: (args) => {
    const command = args.command as string
    return `执行: ${command}`
  },
  
  contentFormatter: (args, result) => {
    const command = args.command as string
    const output = result?.output as string || ''
    const exitCode = result?.exitCode as number
    
    let content = `命令: ${command}\n\n`
    content += `退出码: ${exitCode}\n\n`
    content += `输出:\n${output}`
    
    return content
  }
})
```

### 示例 3：search_files 工具（自定义组件）

```typescript
// SearchFilesPanel.vue
<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  args: Record<string, unknown>
  result?: Record<string, unknown>
}>()

const matches = computed(() => {
  return (props.result?.matches as any[]) || []
})
</script>

<template>
  <div class="search-files-panel">
    <div class="search-query">
      搜索: {{ args.pattern }}
      <span v-if="args.path"> 在 {{ args.path }}</span>
    </div>
    
    <div class="matches">
      <div v-for="match in matches" :key="match.file" class="match-item">
        <div class="match-file">{{ match.file }}</div>
        <div class="match-line">第 {{ match.line }} 行</div>
        <pre class="match-context">{{ match.context }}</pre>
      </div>
    </div>
    
    <div class="summary">
      共找到 {{ matches.length }} 个匹配
    </div>
  </div>
</template>

<style scoped>
.search-files-panel {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.match-item {
  padding: 8px;
  background: var(--vscode-editor-background);
  border: 1px solid var(--vscode-panel-border);
  border-radius: 2px;
}

.match-file {
  font-weight: 600;
  color: var(--vscode-textLink-foreground);
}

.match-context {
  margin-top: 4px;
  font-size: 11px;
  white-space: pre-wrap;
}
</style>
```

```typescript
// searchFiles.ts
import { registerTool } from '../toolRegistry'
import SearchFilesPanel from './SearchFilesPanel.vue'

registerTool('search_files', {
  name: 'search_files',
  label: '搜索文件',
  icon: 'codicon-search',
  
  descriptionFormatter: (args) => {
    const pattern = args.pattern as string
    const path = args.path as string
    return `搜索 "${pattern}" 在 ${path}`
  },
  
  contentComponent: SearchFilesPanel
})
```

## 注册管理

### 注册所有工具

在 `utils/tools/index.ts` 中统一注册：

```typescript
// 导入所有工具注册
import './readFile'
import './writeFile'
import './executeCommand'
import './searchFiles'
// ... 更多工具

export { toolRegistry, registerTool, getToolConfig } from '../toolRegistry'
```

### 在 main.ts 中初始化

```typescript
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'

// 导入工具注册（会自动执行所有注册）
import './utils/tools'

const app = createApp(App)
const pinia = createPinia()

app.use(pinia)
app.mount('#app')
```

## 界面展示

### 收起状态

```
┌─────────────────────────────────────┐
│ > 🔧 read_file                  ✓   │
│   读取: src/app.ts                  │
└─────────────────────────────────────┘
```

### 展开状态

```
┌─────────────────────────────────────┐
│ ˅ 🔧 read_file                  ✓   │
│   读取: src/app.ts                  │
├─────────────────────────────────────┤
│ 文件内容 (共 150 行):               │
│                                     │
│ import { Component } from 'vue'     │
│ import { defineComponent } from... │
│ ...                                 │
└─────────────────────────────────────┘
```

## 最佳实践

1. **描述简洁明了**：只显示关键参数，便于快速浏览
2. **图标统一**：使用 VSCode Codicons，保持视觉一致
3. **内容结构化**：使用清晰的标签和分组
4. **错误突出**：错误信息使用醒目颜色
5. **性能优化**：对于大量数据，考虑分页或虚拟滚动

## 扩展性

### 添加新工具

1. 在 `utils/tools/` 创建新文件 `yourTool.ts`
2. 使用 `registerTool` 注册配置
3. 在 `utils/tools/index.ts` 导入

### 自定义样式

可以在自定义组件中使用 VSCode 主题变量：

- `--vscode-foreground`
- `--vscode-editor-background`
- `--vscode-panel-border`
- 等等...

完整变量列表请参考 VSCode 主题文档。