# destructive_command_guard 参考分析报告

> 分析 dcg (destructive_command_guard) 项目中可被 wf-agent 命令权限控制模块借鉴的实现。

## 一、对比总览

### wf-agent 当前四层防线

```
用户输入 → CommandSafetyChecker(白名单/黑名单最长前缀匹配)
       → AutoApprovalChecker(工具风险级别 7 级分类)
       → SandboxRuntime(Shell 静态分析器, 4 层分析流程)
       → ProtectController(敏感文件 glob 保护)
```

### dcg 核心架构

```
JSON 输入 → Quick Reject(Aho-Corasick) → 命令正规化 → 安全模式 → 破坏性模式 → 默认允许
                    ↑                          ↑
              上下文分类(Heredoc/AST)       Pack 系统(30+ 工具)
```

### 能力维度对比

| 能力维度 | wf-agent 当前 | dcg 实现 | 可借鉴程度 |
|---------|-------------|---------|----------|
| 命令正规化 | 无 | normalize.rs: 剥离 sudo/env/command/反斜杠/路径前缀 | 高 |
| 上下文分类 | 无 | context.rs: 区分 Executed/Data/InlineCode/Comment | 高 |
| 命令链解析 | 有(简单 split) | evaluate.rs: 更精细的链处理 | 中 |
| 危险模式检测 | 有(6 类 shell 模式) | heredoc.rs + AST: 三层检测(Tier1/2/3) | 高 |
| 快速过滤 | 无 | Aho-Corasick Quick Reject (99%+ 命令热路径) | 高 |
| 白名单/黑名单 | 通配符前缀匹配 | Pack 系统: 按工具/领域模块化, 30+ Pack | 高 |
| 严重性分级 | 无 | Severity: Critical/High/Medium/Low | 高 |
| 置信度评分 | 无 | confidence: 0.0~1.0 | 高 |
| Allow-Once | 无 | pending_exceptions.rs: HMAC-SHA256 短码, 24h 过期 | 中 |
| 分层配置 | 安全预设(SAFE/BALANCED/PERMISSIVE) | Agent > Project > User > System 四层 | 中 |
| Fail-Closed | 无 | 超时/错误 → deny/indeterminate | 高 |
| 拒绝建议 | 无 | remediation.safeAlternative | 中 |

## 二、dcg 八大可参考实现

### 2.1 命令正规化 (normalize.rs)

dcg 在模式匹配前剥离包装器，防止绕过。wf-agent 当前完全没有这一步，攻击者可轻易用 `sudo git reset --hard` 绕过。

**可移植的能力：**

- 剥离 `sudo [-EHnkKSb] [-u user] [-g group] ...` 前缀
- 剥离 `env [-i] [-u name] [VAR=VAL] ...` 前缀
- 剥离 `\git` 反斜杠别名绕过
- 剥离 `command [-p] [--] cmd` (保留 `command -v`/`-V` 查询模式)
- 路径前缀剥离: `/usr/bin/git` → `git`
- 递归剥离(最多 32 层, 防止 DoS)
- 返回原始命令和正规化后命令(用于 explain/debug)
- Shell 方言感知(Bash/PowerShell/cmd)

**关键设计原则：**
- 保守: 仅在语法无歧义时剥离
- 非破坏: 绝不改变非包装器命令的语义
- 保留原始: 同时返回原始和正规化形式

**TypeScript 实现预估：** 约 200 行，纯正则实现，无需外部依赖。

### 2.2 命令上下文分类 (context.rs)

dcg 将命令行分词后标记每个 token 的上下文类型(SpanKind)，仅对"可执行"的 span 进行模式匹配，跳过"纯数据"span。

**七种 SpanKind：**

| SpanKind | 含义 | 是否匹配 |
|----------|------|---------|
| Executed | 命令词或未引用参数 | 是 |
| InlineCode | `-c`/`-e` 标志后的代码 | 是 |
| HeredocBody | Heredoc 体 | 升级到 Tier 2/3 |
| Unknown | 模糊上下文 | 是(保守) |
| Argument | 已知安全命令的引用参数 | 否(低优先级) |
| Data | 单引号字符串 | 否(跳过) |
| Comment | Shell 注释 | 否 |

**核心价值：** 消除 `git commit -m 'fix rm -rf detection'` 类误报 -- "fix rm -rf detection" 被标记为 Data，不会触发 `rm -rf` 规则。

**设计原则：**
- 模糊 → 按 Executed 处理(保守策略)
- 宁可误报(阻止安全命令)也不漏报(放过危险命令)

**TypeScript 实现预估：** 约 200 行。

### 2.3 Heredoc / 内联脚本三层检测 (heredoc.rs + ast_matcher.rs)

dcg 对 `bash -c "..."`、`python -c "..."`、`node -e "..."`、`<<EOF...` 做了三层深入检测。wf-agent 当前的 `containsDangerousSubstitution` 只检测了 `<<<$(...)` 形式的 here-string，没有覆盖更常见的内联脚本攻击路径。

**三层架构：**

| 层级 | 技术 | 延迟 | 说明 |
|------|------|------|------|
| Tier 1 触发检测 | `RegexSet` 17 种模式 | <100us | 检测是否需要深入分析 |
| Tier 2 内容提取 | 引号感知扫描器 | <1ms | 提取内联代码内容 |
| Tier 3 AST 匹配 | ast-grep + tree-sitter | <5ms | 结构化代码匹配 |

**Tier 1 的 17 种触发模式：**
`<<<` here-string、`python -c`、`bash -c`、`sh -c`、`node -e`、`ruby -e`、`perl -e`、`php -r`、`lua -e`、`powershell -Command`、`pwsh -Command` 等。

**TypeScript 实现预估：** Tier 1-2 约 300 行。Tier 3 需要 tree-sitter 集成，可后续迭代。

### 2.4 Aho-Corasick Quick Reject (快速过滤)

dcg 使用 Aho-Corasick 自动机做 O(n) 单次关键词扫描。99%+ 的安全命令在热路径上直接放行，不回退到正则匹配或命令分析。

**工作原理：**
- 收集所有启用 Pack 中的关键词构建 AC 自动机
- 命令中不包含任何关键词 → 立即放行
- 含引号/转义的命令不跳过(保守策略)

**对 wf-agent 的价值：** wf-agent 当前对每个命令都做完整的 command chain parse + prefix match + substitution check。引入 Quick Reject 可以让 99%+ 的日常命令(如 `ls`、`cat`、`echo`、`cd` 等)跳过所有安全检查。

**TypeScript 实现预估：** 约 80 行，可使用 `aho-corasick-node` 或自行实现简化版。

### 2.5 Pack 系统 (模块化规则管理)

dcg 将 30+ 个工具/领域的规则组织为 Pack 系统，每个 Pack 独立定义安全模式和破坏性模式。

**结构对比：**

| | wf-agent 当前 | dcg |
|---|---|---|
| 规则组织 | 全局通配符前缀列表 | 按工具/领域分 Pack |
| 规则粒度 | 命令前缀 + `*` | 命名规则 ID (`core.git:reset-hard`) |
| 启用/禁用 | 无 | 可按 Pack 启用/禁用 |
| 外部扩展 | 无 | YAML 外部 Pack |
| 规则数量 | 少量预设 | 每个 Pack 几十到上百条模式 |
| 按 Shell 类型 | 有(Bash/PS/Cmd) | 有(Windows Pack) |

**核心 Pack 示例：**

| Pack | 保护的规则数量 | 典型规则 |
|------|-------------|---------|
| `core.git` | 8 | `reset --hard`、`push --force`、`branch -D` |
| `core.filesystem` | 5 | `rm -rf /`、`find -delete`、`shred` |
| `database.postgresql` | 多个 | `DROP TABLE`、`TRUNCATE`、`DROP DATABASE` |
| `containers.docker` | 多个 | `docker rm -f`、`docker system prune` |
| `kubernetes.kubectl` | 多个 | `kubectl delete namespace` |

**外部 Pack YAML 示例：**

```yaml
schema_version: 1
id: mycompany.deploy
name: MyCompany Deployment Policies
version: 1.0.0
keywords: [deploy, release]
destructive_patterns:
  - name: prod-direct
    pattern: deploy\s+--env\s*=?\s*prod
    severity: critical
    description: Direct production deployment
safe_patterns:
  - name: staging-deploy
    pattern: deploy\s+--env\s*=?\s*(staging|dev)
```

### 2.6 Severity 分级 + 置信度 + DecisionMode

**dcg 的三维决策模型：**

```
Severity: Critical → High → Medium → Low
DecisionMode: Deny → Warn → Log
Confidence: 0.0 ~ 1.0 (float)
```

**wf-agent 当前状态：** 只有 `auto_approve` / `auto_deny` / `ask_user` 三元决策，所有拒绝都是同等对待。

**Severity → DecisionMode 映射：**

| Severity | 默认 Decision | 可降级为 |
|----------|-------------|---------|
| Critical | Deny | 不可降级 |
| High | Deny | Warn(经 allowlist) |
| Medium | Warn | Log |
| Low | Log | - |

**置信度的作用：**
- 高置信度(>0.9) → 直接执行决策
- 中置信度(0.5-0.9) → 降级为 ask 模式
- 低置信度(<0.5) → 升级为 Deny(保守)

### 2.7 Fail-Closed 策略 + 评估预算

dcg 的硬规则：评估预算用尽时(默认 200ms)返回 `Indeterminate`，绝不平移到 `Allow`。

**关键设计：**
- 命令安全分析设置超时预算(例如 200ms)
- 超时后：wf-agent 降级为 `ask_user`（让用户自行判断，而非直接 deny 或 allow）
- 使用 `evaluation-deadline` 作为内部规则 ID 记录
- 输出中明确标注"因分析超时，转为人工审批"

> **wf-agent 差异说明：** dcg 对超时采用 deny/indeterminate 策略。但 wf-agent 作为 agent 框架，超时后更适合转换为 `ask_user` 让用户决策 -- 避免因超时误杀正在进行的任务。

**wf-agent 当前状态：** 有工具失败保护(ToolFailureProtectionState, 连续 3 次失败后冷却 60s)，以及 TimeoutManager(通用超时管理)，但没有命令安全检查本身的超时预算。

### 2.8 Allow-Once 临时例外机制

dcg 使用 HMAC-SHA256 短码生成一次性例外，24 小时过期。

**工作流程：**
- 拒绝时附带 5 位 `allowOnceCode` 短码
- 用户执行 `allow-once <code>` 临时放行
- Cwd/Project 作用域隔离
- 支持 `single_use: true` 模式
- JSONL 持久化

---

## 三、dcg 性能预算参考

| 阶段 | 预算 | 说明 |
|------|------|------|
| Quick Reject | < 50us | Aho-Corasick 关键词扫描 |
| 快路径(安全命令) | < 500us | 正则匹配后放行 |
| 模式匹配 | < 1ms | 破坏性模式检测 |
| Heredoc 提取 | < 2ms | Tier 2 内容提取 |
| 完整 Heredoc 管道 | < 20ms | Tier 1-3 全流程 |
| Hook 评估总额 | 200ms | 超时 → 转人工审批 |

---

## 四、参考优先级建议

按实现难度和收益排序：

| 优先级 | 能力 | 难度 | 收益 | 说明 |
|--------|------|------|------|------|
| P0 | 命令上下文分类 | 中 | 高 | 直接消除 git commit message 类误报 |
| P0 | 命令正规化 | 中 | 高 | 封堵 sudo/env/反斜杠绕过 |
| P1 | Quick Reject | 低 | 高 | 大幅提升 99% 命令的处理性能 |
| P1 | Severity 分级 + 置信度 | 低 | 高 | 让决策信息更丰富 |
| P2 | Fail-Closed + 超时预算 | 低 | 中 | 超时转 ask_user，避免误杀 |
| P2 | Pack 系统化 | 中 | 中 | 规则可管理性和可扩展性 |
| P3 | Heredoc/内联脚本检测 | 高 | 高 | 封堵高级绕过路径 |
| P3 | Allow-Once | 中 | 中 | 提升用户交互体验 |
