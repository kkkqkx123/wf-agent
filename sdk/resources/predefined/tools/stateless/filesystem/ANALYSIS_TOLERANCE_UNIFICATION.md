# 工具容错机制统一分析报告

## 问题 1: apply-patch 是否应该复用通用 matcher？

### 当前状态对比

#### apply-patch/utils/matcher.ts (本地版本)
- ✅ 包含 `eof` 参数支持（用于匹配文件末尾）
- ✅ 功能完整，与通用版本基本一致
- ❌ 代码重复（159 行）

#### tools/utils/matcher.ts (通用版本)
- ✅ 已提取为共享模块
- ✅ 可跨工具复用
- ❌ 缺少 `eof` 参数支持

### 分析结论

**建议：应该复用，但需要扩展通用 matcher**

**理由：**
1. **避免代码重复**：两个版本的 `seekSequence` 实现几乎相同
2. **统一维护**：修改匹配逻辑只需改一处
3. **一致性保证**：所有工具使用相同的匹配算法

**实施方案：**

```typescript
// 在通用 matcher.ts 中添加 eof 支持
export function seekSequence(
  lines: string[],
  pattern: string[],
  start: number,
  options?: { eof?: boolean }  // 新增可选参数
): number | null {
  const eof = options?.eof ?? false;
  
  if (pattern.length === 0) {
    return start;
  }

  if (pattern.length > lines.length) {
    return null;
  }

  // 支持 EOF 匹配
  const searchStart = eof && lines.length >= pattern.length 
    ? lines.length - pattern.length 
    : start;

  const maxStart = lines.length - pattern.length;

  // Pass 1-4: 保持不变
  // ...
}
```

**迁移步骤：**
1. 扩展通用 `matcher.ts` 添加 `eof` 选项
2. 修改 `apply-patch/utils/apply.ts` 导入路径
3. 删除 `apply-patch/utils/matcher.ts`
4. 更新测试验证

---

## 问题 2: edit 工具是否需要容错机制？

### 当前 edit 工具的问题

**第 82 行：严格的字符串匹配**
```typescript
if (!content.includes(old_string)) {
  return {
    success: false,
    content: "",
    error: `String not found in file: "${old_string.substring(0, 100)}..."`,
  };
}
```

**潜在问题：**
1. ❌ **空白差异敏感**：尾部空格、换行符差异会导致失败
2. ❌ **Unicode 字符问题**：typographic quotes/dashes 无法匹配
3. ❌ **用户体验差**：LLM 提供的 old_string 稍有差异就失败

### 实际场景分析

#### 场景 1：尾部空白差异
```typescript
// 文件内容
const x = 1;   // 有尾部空格

// LLM 提供
old_string: "const x = 1;"  // 无尾部空格

// 当前行为：❌ 失败
// 期望行为：✅ 成功（trim-end 匹配）
```

#### 场景 2：Unicode 引号差异
```typescript
// 文件内容
console.log("hello")  // 普通引号

// LLM 提供（从某些文档复制）
old_string: 'console.log(\u201chello\u201d)'  // fancy quotes

// 当前行为：❌ 失败
// 期望行为：✅ 成功（Unicode 归一化）
```

#### 场景 3：缩进差异
```typescript
// 文件内容
    function test() {
        return 1;
    }

// LLM 提供（缩进不同）
old_string: "  function test() {\n    return 1;\n  }"

// 当前行为：❌ 失败
// 期望行为：⚠️ 谨慎处理（可能需要 trim 匹配）
```

### 建议方案

**✅ 应该扩展容错机制，但需谨慎设计**

#### 方案设计

**策略 1：多阶段匹配（推荐）**
```typescript
function findWithTolerance(content: string, target: string): {
  found: boolean;
  index: number;
  matchType: 'exact' | 'trim-end' | 'trim' | 'unicode';
} {
  // Pass 1: Exact match
  let index = content.indexOf(target);
  if (index !== -1) {
    return { found: true, index, matchType: 'exact' };
  }

  // Pass 2: Line-by-line with trim-end
  const contentLines = content.split(/\r?\n/);
  const targetLines = target.split(/\r?\n/);
  
  for (let i = 0; i <= contentLines.length - targetLines.length; i++) {
    if (linesMatchTrimEnd(contentLines.slice(i, i + targetLines.length), targetLines)) {
      const charIndex = getCharIndex(contentLines, i);
      return { found: true, index: charIndex, matchType: 'trim-end' };
    }
  }

  // Pass 3: Line-by-line with full trim
  // Pass 4: Unicode normalization
  
  return { found: false, index: -1, matchType: 'none' };
}
```

**策略 2：警告式容错（更安全）**
```typescript
if (matchType !== 'exact') {
  return {
    success: true,
    content: `Edited ${file_path} (matched with tolerance: ${matchType})\n` +
             `Warning: The matched content differs slightly from your input.\n` +
             `Please verify the changes are correct.`,
  };
}
```

#### 风险评估

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| **非预期匹配** | 高 | 返回匹配类型警告，让用户确认 |
| **多次出现歧义** | 中 | 结合 `require_unique` 参数 |
| **性能下降** | 低 | 先 exact match，失败才尝试容错 |

### 实施建议

**优先级：中**

1. **短期**：保持现状，通过 prompt 指导 LLM 提供精确的 old_string
2. **中期**：添加可选的容错模式（`tolerance?: 'none' | 'whitespace' | 'unicode'`）
3. **长期**：考虑弃用 edit 工具，推荐使用 apply-diff（更结构化）

**理由：**
- edit 工具是简单替换，缺乏上下文信息
- apply-diff 提供更好的结构化和定位能力
- 过度容错可能导致意外修改

---

## 问题 3: apply-patch/utils 添加 index.ts 导出

### 当前状态

**apply-patch/utils/** 缺少统一的导出入口

### 建议方案

**✅ 应该添加 index.ts**

**理由：**
1. **简化导入**：外部调用者不需要知道内部文件结构
2. **封装实现**：可以隐藏内部细节，只暴露公共 API
3. **一致性**：与 apply-diff/utils/index.ts 保持一致

### 实施方案

创建 `apply-patch/utils/index.ts`:

```typescript
/**
 * Apply-patch tool utilities
 */

// Type definitions
export type {
  PatchChunk,
  ChangeContext,
  ParsedPatch,
  ApplyPatchConfig,
  ChunkApplyResult,
} from "./types.js";

// Parser
export {
  parseUnifiedDiff,
  validatePatchStructure,
} from "./parser.js";

// Matcher
export { seekSequence } from "@wf-agent/sdk/resources/predefined/tools/utils/matcher.js";

// Apply logic
export { applyChunk } from "./apply.js";
```

### 好处

**使用前：**
```typescript
import { parseUnifiedDiff } from "./utils/parser.js";
import { applyChunk } from "./utils/apply.js";
import { seekSequence } from "./utils/matcher.js";
import type { PatchChunk } from "./utils/types.js";
```

**使用后：**
```typescript
import {
  parseUnifiedDiff,
  applyChunk,
  seekSequence,
  type PatchChunk,
} from "./utils/index.js";
```

---

## 总结与建议

### 优先级排序

1. **高优先级**：为 apply-patch/utils 添加 index.ts
   - 工作量小，收益明显
   - 提升代码组织性

2. **中优先级**：扩展通用 matcher 支持 eof 参数
   - 消除代码重复
   - 统一维护

3. **低优先级**：为 edit 工具添加容错机制
   - 需要谨慎评估风险
   - 可能引导用户使用更好的工具（apply-diff）

### 行动计划

#### Phase 1: 立即可做
- [ ] 创建 `apply-patch/utils/index.ts`
- [ ] 更新 apply-patch 内部导入使用 index.ts

#### Phase 2: 短期优化
- [ ] 扩展通用 `matcher.ts` 添加 `eof` 选项
- [ ] 修改 apply-patch 使用通用 matcher
- [ ] 删除 apply-patch/utils/matcher.ts
- [ ] 运行测试验证

#### Phase 3: 长期考虑
- [ ] 评估 edit 工具的容错需求
- [ ] 如需实现，采用警告式容错策略
- [ ] 更新文档说明各工具的适用场景

### 架构优势

完成重构后的架构：

```
sdk/resources/predefined/tools/
├── utils/
│   └── matcher.ts                    # ✅ 通用序列匹配（支持 eof）
└── stateless/filesystem/
    ├── apply-diff/utils/
    │   ├── index.ts                  # ✅ 统一导出
    │   ├── types.ts
    │   ├── parser.ts
    │   └── apply.ts (使用通用 matcher)
    ├── apply-patch/utils/
    │   ├── index.ts                  # ✅ 统一导出
    │   ├── types.ts
    │   ├── parser.ts
    │   └── apply.ts (使用通用 matcher)
    └── edit/
        └── handler.ts (可选：添加容错)
```

**核心优势：**
- ✅ DRY 原则：matcher 只实现一次
- ✅ 易维护：修改匹配逻辑一处生效
- ✅ 易理解：清晰的导出结构
- ✅ 易扩展：新工具可直接复用
