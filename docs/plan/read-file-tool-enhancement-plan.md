# read_file 工具增强与 misc 工具函数集成计划

## 背景

`sdk/utils/misc` 提供了丰富的文件处理工具函数，但大部分未被实际集成到 read_file 工具中。同时，`sdk/resources/predefined/tools/stateless/filesystem/read-file/handler.ts` 之前仅实现了 slice 模式，但 schema 和 description 已经声明了 `mode`、`indentation` 等参数，存在脱节。

## 已完成工作

### 1. handler.ts 重构 — mode 分发集成

**改动文件**: `sdk/resources/predefined/tools/stateless/filesystem/read-file/handler.ts`

- 从 `@wf-agent/sdk/utils` 导入 `readWithSlice`、`readWithIndentation` 及类型
- 解析 `mode` 参数，按 `"slice"` / `"indentation"` 分发
- **Slice 模式**: 替换手动 `lines.slice()` + `formatLineNumbers()`，改用 `readWithSlice`（内部已集成字符截断和行号格式化）
- **Indentation 模式**: 接入 `readWithIndentation`，支持 `anchor_line`、`max_levels`、`include_siblings`、`include_header`、`max_lines` 等参数
- 使用 `config.maxChars` 替代硬编码 `DEFAULT_CHAR_LIMIT`，使用 `config.maxLines` 替代硬编码行数默认值
- 简化 truncation message 逻辑，统一由底层函数返回的 metadata 驱动

### 2. ReadFileConfig 扩展

**改动文件**: `sdk/resources/predefined/tools/types.ts`

- 新增 `maxChars?: number` — 字符硬上限（默认 50000），替代 handler 中的 `DEFAULT_CHAR_LIMIT`
- 新增 `maxLines?: number` — 行数上限（默认 2000），替代 handler 中的默认 limit
- 修正 `maxFileSize` 注释（单位应为 bytes 而非 characters）

### 3. 创建 tools 配置处理器

**新增文件**:
- `sdk/api/shared/config/processors/tools/index.ts` — 导出入口
- `sdk/api/shared/config/processors/tools/read-file.ts` — ReadFileConfig 验证/转换/导出

**修改文件**: `sdk/api/shared/config/processors/index.ts` — 添加 tools 模块导出

该处理器遵循与其他配置处理器一致的 `validate` / `transform` / `export` 模式，可直接用于全局配置文件加载。

---

## 剩余工作项

### 高优先级

#### 1. 为 handler.ts 添加单元测试

**路径**: `sdk/resources/predefined/tools/stateless/filesystem/read-file/__tests__/handler.test.ts`

**测试覆盖**:
- Slice mode: offset/limit/max_chars 的边界条件
- Indentation mode: anchor_line 定位、语义块提取、字符截断
- 空文件、超大文件、各种 edge case
- 参数验证（offset < 1, anchor_line < 1 等）

#### 2. 为 tools 配置处理器添加单元测试

**路径**: `sdk/api/shared/config/processors/tools/__tests__/read-file.test.ts`

**测试覆盖**:
- 默认值填充
- 非法输入的验证错误
- 序列化/反序列化的一致性

#### 3. 为 misc 工具函数添加单元测试

**路径**: `sdk/utils/misc/__tests__/`（新建）

**测试覆盖**:
- `file-reader.ts`: `readWithSlice` 的字符截断、`readWithIndentation` 的语义块提取
- `stream-reader.ts`: 大文件流式读取
- `line-number-utils.ts`: 行号剥离、智能截断
- `terminal-output-utils.ts`: 控制符处理

---

### 中优先级

#### 4. read_lines_with_stream 集成 — 超大文件支持

**描述**: 当前 handler 使用 `readFile(absolutePath)` 将整个文件读入内存，对于超大文件（100MB+）可能导致 OOM。`readLinesWithStream` 使用 Node.js 流式读取，可以在不加载全文件的情况下读取指定行范围。

**方案**:
- 在 `ReadFileConfig` 中新增 `maxMemoryFileSize?: number`（默认 10MB）
- handler 中在文件大小超过该阈值时，切换为流式读取
- Slice mode 使用 `readLinesWithStream`，Indentation mode 仍需要全文件（因为需要分析缩进结构）

**改动文件**:
- `sdk/resources/predefined/tools/types.ts` — 新增配置字段
- `sdk/resources/predefined/tools/stateless/filesystem/read-file/handler.ts` — 添加文件大小判断和流式路径
- `sdk/api/shared/config/processors/tools/read-file.ts` — 更新处理器

#### 5. Truncation message 差异化展示

**描述**: 当前 truncation message 统一为 "Showing lines X-Y of Z"。对于 indentation mode，应额外说明语义块范围；对于字符截断，应提示触发原因和字符数。

**方案**: 在 handler 中根据 `wasCharTruncated` 标志位生成更精确的消息。

---

### 低优先级

#### 6. 全局配置文件集成 tools 配置

**描述**: 当前工具配置通过 `PredefinedToolsOptions.config.readFile` 在代码中注入。后续应支持通过全局配置文件（如 `wf-agent.toml`）加载工具配置。

**前置条件**:
- 配置系统已支持 tools 维度的配置段
- 设计配置文件的 tools 段落 schema

#### 7. 为 indentation mode 添加性能基准测试

**描述**: `readWithIndentation` 的缩进分析是 O(n) 的，对于超大文件（10 万行以上）可能成为瓶颈。应补充基准测试以量化性能，并在必要时优化。

#### 8. terminal-output-utils 的集成

**描述**: `processCarriageReturns`、`processBackspaces`、`applyRunLengthEncoding` 三个函数目前与 read_file 无关。它们是为 shell 命令输出处理设计的，应在 shell 执行工具（如 `run_shell` 或 `backend_shell`）中集成。

**方案**: 在 shell 相关工具的文档中引用这些工具函数，或在 shell 输出处理器中默认启用。

#### 9. 专用格式读取工具

**描述**: 当前 handler 对 PDF/DOCX/XLSX/IPYNB 等格式返回错误提示。后续可以为这些格式创建专用的读取工具（如 `read_pdf`、`read_ipynb`），复用 misc 中的工具函数。

---

### 已取消

- ~~Token 预算硬上限~~ — 已取消，转为字符硬上限（`maxChars`）。字符限制更轻量、更可预测，无需依赖 token 估算的延迟开销。

---

## 架构示意

```
用户请求
    │
    ▼
read_file handler
    │
    ├─ 前置检查 (file exists, size, ignore, protect, binary)
    │
    ├─ mode === "indentation" ?
    │       │
    │       ▼
    │   readWithIndentation(content, { anchorLine, maxLevels, ... })
    │       │
    │       ▼
    │   IndentationReadResult { content, includedRanges, wasTruncated }
    │
    └─ mode === "slice" (default) ?
            │
            ▼
        readWithSlice(content, { offset, limit, maxChars })
            │
            ▼
        SliceReadResult { content, returnedLines, wasTruncated }
    │
    ▼
  后处理 (truncation message, protection notice)
    │
    ▼
  ToolOutput { success, content }
```

## 依赖关系

```
handler.ts
    ├── @wf-agent/sdk/utils
    │       ├── readWithSlice        (file-reader.ts)
    │       ├── readWithIndentation  (file-reader.ts)
    │       ├── resolveFilePath      (file-utils.ts)
    │       ├── formatFileSize       (file-utils.ts)
    │       └── isLikelyTextFile     (file-utils.ts)
    │
    ├── @wf-agent/sdk/services
    │       ├── IgnoreController
    │       └── ProtectController
    │
    └── ReadFileConfig (types.ts)
            ├── maxFileSize
            ├── maxChars         ← 新增
            └── maxLines         ← 新增
```