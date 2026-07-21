# 命令权限控制分阶段改进方案

> 基于 destructive_command_guard 参考分析，对 wf-agent 命令安全模块的渐进式增强计划。

## 阶段总览

```
Phase 1 ──→ Phase 2 ──→ Phase 3 ──→ Phase 4
(基础加固)  (性能+分级)  (深度防御)  (体验增强)
 2 项 P0      3 项 P1      2 项 P2+P3   1 项 P3
```

---

## Phase 1: 基础加固 (P0)

**目标：** 消除最直接的绕过路径和误报来源。

### 1.1 命令正规化 (Command Normalizer)

**涉及文件：**

| 文件 | 操作 | 说明 |
|------|------|------|
| `packages/sdk/services/command-safety/normalizer.ts` | 新建 | 命令正规化核心逻辑 |
| `packages/sdk/services/command-safety/command-safety-checker.ts` | 修改 | 集成正规化步骤 |
| `packages/sdk/services/command-safety/__tests__/normalizer.test.ts` | 新建 | 单元测试 |

**实现内容：**

```typescript
// 剥离的包装器类型
type StrippedWrapper = 
  | { type: "sudo"; text: string }      // sudo [-EHnkKSb] [-u user] ... -- cmd
  | { type: "env"; text: string }       // env [-i] [-u name] [VAR=VAL]... cmd
  | { type: "command"; text: string }   // command [-p] [--] cmd
  | { type: "backslash"; text: string } // \git → git
  | { type: "path"; text: string };     // /usr/bin/git → git

interface NormalizedCommand {
  original: string;          // 原始命令
  normalized: string;        // 剥离包装器后的命令
  wrappers: StrippedWrapper[]; // 被剥离的包装器列表
}

function normalizeCommand(command: string): NormalizedCommand;
```

**关键行为：**
- 递归剥离，上限 32 层(防 DoS)
- 保留 `command -v`/`command -V` 查询模式(不被剥离)
- 同时返回原始和正规化后的命令，用于日志/explain 输出
- 正规化在模式匹配之前执行

**核心正则模式：**

```typescript
// sudo: sudo [-EHnkKSb]+ [-ug user/group]* [--] cmd
const SUDO_RE = /^sudo(?:\s+-[EHnkKSbisAB]+)*(?:\s+-[ug]\s+\S+)*(?:\s+--)?\s+/i;

// env: env [-i] [-u name]* [VAR=VAL]* cmd
const ENV_RE = /^env(?:\s+-i)?(?:\s+-u\s+\S+)*(?:\s+\w+=\S+)*\s+/i;

// command: command [-p] [--] cmd (但保留 command -v/-V)
const COMMAND_RE = /^command(?:\s+-p)?\s+(?:--\s+)?/i;

// 反斜杠别名: \git → git
const BACKSLASH_RE = /^\\([a-zA-Z0-9_-]+)(\s|$)/;

// 路径前缀: /usr/bin/git → git
const PATH_RE = /^(\/[a-zA-Z0-9_.-]+)+\/([a-zA-Z0-9_-]+)/;
```

### 1.2 命令上下文分类 (Command Context Classifier)

**涉及文件：**

| 文件 | 操作 | 说明 |
|------|------|------|
| `packages/sdk/services/command-safety/context-classifier.ts` | 新建 | 上下文分类器 |
| `packages/sdk/services/command-safety/command-safety-checker.ts` | 修改 | 集成上下文感知匹配 |
| `packages/sdk/services/command-safety/__tests__/context-classifier.test.ts` | 新建 | 单元测试 |

**实现内容：**

```typescript
enum SpanKind {
  Executed,    // 命令词/未引用参数 → 必须检查
  InlineCode,  // -c/-e 后的内联代码 → 必须检查
  HeredocBody, // Heredoc 体 → 升级分析
  Argument,    // 已知安全命令的引用参数 → 低优先级
  Data,        // 单引号字符串 → 跳过
  Comment,     // Shell 注释 → 跳过
  Unknown,     // 模糊上下文 → 保守当 Executed
}

interface Span {
  kind: SpanKind;
  range: [number, number]; // 在原命令中的位置
}

interface CommandSpans {
  spans: Span[];
  executableSpans(): Span[];  // 需要检查模式的 span
  dataSpans(): Span[];        // 可以跳过的 span
  hasExecutableContent(): boolean;
}

function classifyCommand(command: string): CommandSpans;
```

**分类规则（按优先级）：**

1. 双引号外的 `#` 开头 → Comment (跳过)
2. 单引号内的内容 → Data (跳过)
3. `-c`/`-e` 标志后的参数 → InlineCode (必须检查)
4. `<<` heredoc 后的内容 → HeredocBody (升级分析)
5. 已知安全命令(`git commit -m`、`grep -e` 等)的引用参数 → Argument (低优先级)
6. 未匹配的命令词 → Executed (必须检查)
7. 其余 → Unknown (保守按 Executed 处理)

**核心价值：**

```bash
# 之前：整个命令被检查，"rm -rf" 在 commit message 中触发误报
git commit -m "fix rm -rf detection in our codebase"

# 之后："fix rm -rf detection in our codebase" 被标记为 Data，跳过模式匹配
```

**集成方式：**
- `getCommandDecision()` 在匹配前先调 `classifyCommand()` 获取 spans
- 白名单/黑名单匹配仅应用于 `executableSpans()` 中的内容
- `dataSpans()` 中的内容完全跳过

---

## Phase 2: 性能优化 + 决策分级 (P1)

**目标：** 提升 99% 命令的处理速度，丰富决策信息维度。

### 2.1 Quick Reject 快速过滤

**涉及文件：**

| 文件 | 操作 | 说明 |
|------|------|------|
| `packages/sdk/services/command-safety/quick-reject.ts` | 新建 | 基于关键词的快速过滤 |
| `packages/sdk/services/command-safety/command-safety-checker.ts` | 修改 | 在正规化后、分类前插入 |
| `packages/sdk/services/command-safety/__tests__/quick-reject.test.ts` | 新建 | 单元测试 |

**实现思路：**

```typescript
interface QuickRejectFilter {
  // 构建关键词索引(从所有启用规则中提取关键词)
  buildKeywordIndex(keywords: string[]): void;
  // 检查命令是否可能触发任何规则(不包含任何关键词 → 安全跳过)
  hasAnyKeyword(command: string): boolean;
}
```

**实现方案选择：**

| 方案 | 复杂度 | 性能 | 推荐度 |
|------|--------|------|--------|
| `Set<string>` + `words.includes` | 极低 | O(n*m) | 低 |
| 手动前缀树(Trie) | 低 | O(n) | 中 |
| `aho-corasick-node` 库 | 中 | O(n) | 高 |
| 自实现简化版 AC 自动机 | 中 | O(n) | 高 |

**推荐：** 先使用原生 `Set` + 遍历方案快速实现(约 50 行)，后续根据性能数据决定是否升级为 AC 自动机。

**关键词来源：** 从白名单/黑名单中提取命令名(如 `git`、`npm`、`docker`、`rm`、`curl` 等)。

### 2.2 Severity 分级 + 置信度

**涉及文件：**

| 文件 | 操作 | 说明 |
|------|------|------|
| `packages/sdk/services/command-safety/types.ts` | 新建/修改 | Severity/Confidence 类型定义 |
| `packages/sdk/services/command-safety/command-safety-checker.ts` | 修改 | 返回增强决策 |
| `packages/sdk/services/auto-approval/auto-approval-checker.ts` | 修改 | 消费增强决策 |

**类型定义：**

```typescript
enum Severity {
  Critical = "critical",  // 不可逆操作, 永不自动审批
  High = "high",           // 默认阻止, 可经 allowlist 降级
  Medium = "medium",       // 默认警告但允许, 可升级为阻止
  Low = "low",             // 仅记录
}

interface CommandDecision {
  decision: "auto_approve" | "auto_deny" | "ask_user";
  severity: Severity;        // 新增
  confidence: number;        // 新增: 0.0 ~ 1.0
  matchedRuleId?: string;    // 新增: 匹配的规则 ID
  matchedPattern?: string;   // 新增: 匹配的模式
  suggestion?: string;       // 新增: 安全替代方案
}
```

**DecisionMode 映射：**

| Severity | 默认 Decision | 可通过 allowlist 降级 |
|----------|-------------|---------------------|
| Critical | auto_deny | 否 |
| High | auto_deny | 是 (→ ask_user) |
| Medium | ask_user | 是 (→ auto_approve) |
| Low | auto_approve | N/A |

**置信度计算规则：**
- 精确命令名匹配(如 `rm`、`git`): 0.95
- 前缀匹配(如 `git reset`): 0.85
- 通配符匹配: 0.70
- 正则模式匹配: 0.80
- 危险替换检测: 0.90
- 无匹配: 0.95(auto_approve)

**置信度对决策的影响：**
- 高置信度(>0.9): 直接执行决策
- 中置信度(0.5-0.9): 降级为 ask_user
- 低置信度(<0.5): 升级为 auto_deny(保守)

### 2.3 Fail-Closed + 超时预算

**涉及文件：**

| 文件 | 操作 | 说明 |
|------|------|------|
| `packages/sdk/services/command-safety/timeout-guard.ts` | 新建 | 命令安全检查的超时包装 |
| `packages/sdk/services/command-safety/command-safety-checker.ts` | 修改 | 套用超时保护 |
| `packages/sdk/services/command-safety/__tests__/timeout-guard.test.ts` | 新建 | 单元测试 |

**设计要点：**

```typescript
interface TimeoutGuardOptions {
  budget: number;          // 超时预算(默认 200ms)
  onTimeout: "ask_user";   // wf-agent 超时策略: 转为 ask_user
                           // (dcg 策略是 "deny", wf-agent 选择更宽松的策略)
}

async function withCommandSafetyTimeout<T>(
  fn: () => T | Promise<T>,
  options: TimeoutGuardOptions
): Promise<T | CommandDecision> {
  const result = await Promise.race([
    Promise.resolve(fn()),
    new Promise<CommandDecision>(resolve =>
      setTimeout(() => resolve({
        decision: "ask_user",
        severity: Severity.Medium,
        confidence: 0.3,
        matchedRuleId: "command-safety:evaluation-deadline",
        suggestion: "Command safety analysis timed out. Please review manually."
      }), options.budget)
    )
  ]);
  return result;
}
```

> **关键设计决策：** wf-agent 对超时的处理策略为 `ask_user` 而非 dcg 的 `deny`。原因是 wf-agent 作为 agent 框架，超时后直接 deny 会误杀正在进行的合法任务，让用户自行判断更为合理。dcg 作为独立 hook 工具采用更保守的 deny 策略，两者的使用场景不同。

**集成位置：** 在 `handleExecuteApproval()` 中调 `getCommandDecision()` 时套用超时包装。

---

## Phase 3: 深度防御 (P2 + P3)

**目标：** 建立 Pack 系统的架构基础，实现内联脚本检测。

### 3.1 Pack 系统基础架构

**涉及文件：**

| 文件 | 操作 | 说明 |
|------|------|------|
| `packages/sdk/services/command-safety/pack/types.ts` | 新建 | Pack 类型定义 |
| `packages/sdk/services/command-safety/pack/registry.ts` | 新建 | Pack 注册和管理 |
| `packages/sdk/services/command-safety/pack/loader.ts` | 新建 | Pack 加载器(内置 + 外部 YAML) |
| `packages/sdk/services/command-safety/pack/builtin/git.pack.ts` | 新建 | Git 命令保护 Pack |
| `packages/sdk/services/command-safety/pack/builtin/filesystem.pack.ts` | 新建 | 文件系统保护 Pack |
| `packages/sdk/services/command-safety/pack/builtin/network.pack.ts` | 新建 | 网络安全 Pack |
| `packages/sdk/services/command-safety/command-safety-checker.ts` | 修改 | 从 Pack 系统读取规则 |

**Pack 类型定义：**

```typescript
interface CommandPack {
  id: string;                    // 例: "core.git"
  name: string;                  // 例: "Git Command Protection"
  version: string;
  keywords: string[];            // Quick Reject 关键词
  enabled: boolean;
  
  // 规则定义
  rules: CommandRule[];
  
  // 安全模式(白名单)
  safePatterns: SafePattern[];
  
  // 破坏性模式(黑名单)
  destructivePatterns: DestructivePattern[];
}

interface DestructivePattern {
  id: string;            // 例: "core.git:reset-hard"
  name: string;
  pattern: RegExp;       // 正则模式
  severity: Severity;
  description: string;
  suggestion?: string;   // 安全替代方案
}
```

**内置 Pack 规划：**

| Pack ID | 内容 | 规则数(预估) |
|---------|------|-------------|
| `core.git` | `reset --hard`、`push --force`、`branch -D`、`clean -f`、`stash drop` 等 | 8+ |
| `core.filesystem` | `rm -rf /`、`find -delete`、`shred`、`chmod 777 /` 等 | 10+ |
| `core.network` | `curl ... \| bash`、`wget ... -O - \| sh` 等 | 5+ |
| `core.process` | `kill -9`、`pkill`、`killall` 等 | 5+ |
| `core.permissions` | `chmod 777`、`chown root:root` 等 | 5+ |

**Pack 注册和使用：**

```typescript
// 内置 Pack 自动加载
const registry = new PackRegistry();
registry.register(gitPack);
registry.register(filesystemPack);

// 在 getCommandDecision 中使用
const decision = registry.evaluate(normalizedCommand, { spans });
```

### 3.2 Heredoc / 内联脚本 Tier 1 检测

**涉及文件：**

| 文件 | 操作 | 说明 |
|------|------|------|
| `packages/sdk/services/command-safety/inline-script-detector.ts` | 新建 | 内联脚本检测器(Tier 1) |
| `packages/sdk/services/command-safety/command-safety-checker.ts` | 修改 | 集成内联脚本检测 |
| `packages/sdk/services/command-safety/__tests__/inline-script-detector.test.ts` | 新建 | 单元测试 |

**17 种触发模式：**

```typescript
const INLINE_SCRIPT_TRIGGERS = [
  // Shell 内联执行
  { pattern: /\bbash\s+-c\b/, language: "bash" },
  { pattern: /\bsh\s+-c\b/, language: "bash" },
  { pattern: /\bzsh\s+-c\b/, language: "bash" },
  
  // 脚本语言 -c/-e 模式
  { pattern: /\bpython3?\s+-c\b/, language: "python" },
  { pattern: /\bnode\s+-e\b/, language: "javascript" },
  { pattern: /\bruby\s+-e\b/, language: "ruby" },
  { pattern: /\bperl\s+-e\b/, language: "perl" },
  { pattern: /\bphp\s+-r\b/, language: "php" },
  { pattern: /\blua\s+-e\b/, language: "lua" },
  
  // PowerShell
  { pattern: /\bpowershell\s+-(?:Command|EncodedCommand)\b/i, language: "powershell" },
  { pattern: /\bpwsh\s+-(?:Command|EncodedCommand)\b/i, language: "powershell" },
  
  // Heredoc / Here-string
  { pattern: /<<<\s*[\$\(`]/, language: "shell" },   // here-string with command sub
  { pattern: /<<\s*['"]?\w+['"]?/, language: "shell" }, // heredoc start
];
```

**检测流程：**

```
命令 → Tier 1 触发检测 → 命中? 
                              ├─ 否 → 继续常规流程
                              └─ 是 → Tier 2 内容提取 → 
                                          ├─ 提取成功 → 检查内联代码内容
                                          │              ├─ 包含危险模式 → deny
                                          │              └─ 安全 → 继续
                                          └─ 提取失败 → ask_user
```

---

## Phase 4: 体验增强 (P3)

**目标：** 增加临时例外机制，减少用户摩擦。

### 4.1 Allow-Once 机制

**涉及文件：**

| 文件 | 操作 | 说明 |
|------|------|------|
| `packages/sdk/services/command-safety/allow-once.ts` | 新建 | Allow-Once 核心逻辑 |
| `packages/sdk/services/command-safety/command-safety-checker.ts` | 修改 | 拒绝时生成短码 |
| `packages/sdk/services/auto-approval/auto-approval-checker.ts` | 修改 | 消费短码 |

**设计：**

```typescript
interface AllowOnceConfig {
  ttl: number;               // 有效期(默认 86400000ms = 24h)
  scope: "cwd" | "project"; // 作用域
  singleUse: boolean;        // 是否仅一次
}

interface AllowOnceStore {
  generate(command: string, ruleId: string): AllowOnceCode;
  validate(code: string, command: string): boolean;
  revoke(code: string): void;
  listActive(): AllowOnceEntry[];
  // 持久化到 JSONL 文件
  flush(): Promise<void>;
  load(): Promise<void>;
}
```

**拒绝输出增强：**

```typescript
// 当前
{ decision: "auto_deny" }

// Phase 4 增强后
{
  decision: "auto_deny",
  severity: Severity.High,
  confidence: 0.95,
  matchedRuleId: "core.git:reset-hard",
  allowOnceCode: "a1b2c",                    // 5 位短码
  suggestion: "Use git stash to save changes first, then git reset --hard if needed.",
  // 告知用户可用此码临时放行
}
```

---

## 文件变更汇总

### 新建文件

```
packages/sdk/services/command-safety/
├── normalizer.ts                      # Phase 1.1
├── context-classifier.ts              # Phase 1.2
├── quick-reject.ts                    # Phase 2.1
├── types.ts                           # Phase 2.2
├── timeout-guard.ts                   # Phase 2.3
├── inline-script-detector.ts          # Phase 3.2
├── allow-once.ts                      # Phase 4.1
├── pack/
│   ├── types.ts                       # Phase 3.1
│   ├── registry.ts                    # Phase 3.1
│   ├── loader.ts                      # Phase 3.1
│   └── builtin/
│       ├── git.pack.ts                # Phase 3.1
│       ├── filesystem.pack.ts         # Phase 3.1
│       ├── network.pack.ts            # Phase 3.1
│       ├── process.pack.ts            # Phase 3.1
│       └── permissions.pack.ts        # Phase 3.1
└── __tests__/
    ├── normalizer.test.ts             # Phase 1.1
    ├── context-classifier.test.ts     # Phase 1.2
    ├── quick-reject.test.ts           # Phase 2.1
    ├── timeout-guard.test.ts          # Phase 2.3
    └── inline-script-detector.test.ts # Phase 3.2
```

### 修改文件

```
packages/sdk/services/command-safety/
└── command-safety-checker.ts          # 所有 Phase 集成修改

packages/sdk/services/auto-approval/
└── auto-approval-checker.ts           # Phase 2.2, 4.1 消费增强决策
```

### 外部依赖（可选）

| 包名 | 用途 | Phase | 必要性 |
|------|------|-------|--------|
| `aho-corasick-node` | Quick Reject 高性能关键词匹配 | 2.1 | 可选(可自实现) |
| `tree-sitter` + 语言包 | Tier 3 AST 匹配 | 后续 | 可选(Phase 3.2 只需 Tier 1) |
