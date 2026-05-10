# Ask Follow-up Question - Design Evolution

## 设计演进历程

### V1: 初始设计（复杂嵌套）
```typescript
{
  questions: [{
    text: string;
    options: [{
      label: string;
      value?: string;
    }];
  }]
}
```

**问题**：
- ❌ `label`/`value` 分离冗余
- ❌ 过度设计，增加 LLM 理解成本

---

### V2: 共享选项（扁平化）
```typescript
{
  questions: string[];      // ["问题1", "问题2"]
  options: string[];        // ["选项A", "选项B", "选项C"]
}
```

**问题**：
- ❌ 问题和选项分离，LLM 需要跨字段关联
- ❌ 不同问题的选项可能完全不同，共享不合理
- ❌ 容易导致选项与问题错位
- ❌ UI 展示时选项和问题关系不直观

**用户反馈**：
> "选项应该要与问题绑定，是每个问题独立对应几个选项。选项应该嵌套在问题里，问题、选项分离可能影响llm注意力，导致错位的概率增加"

---

### V3: 嵌套结构（最终方案）✅
```typescript
{
  questions: [
    {
      text: "使用哪个配置文件？",
      options: ["./src/config.json", "./config/app.json"]
    },
    {
      text: "实现优先级？",
      options: ["立即实现", "先规划再实现"]
    }
  ]
}
```

**优势**：
- ✅ 语义完整：问题和选项在同一层级
- ✅ LLM 理解更容易：不需要跨字段关联
- ✅ 灵活性高：每个问题可以有完全不同的选项集
- ✅ 减少错误：避免选项与问题错位
- ✅ UI 清晰：每个问题下方直接显示其选项

---

## 关键决策对比

| 维度 | V1 复杂嵌套 | V2 共享选项 | V3 嵌套结构 |
|------|------------|------------|------------|
| **结构复杂度** | 高（三层嵌套） | 低（两层数组） | 中（两层嵌套） |
| **LLM 理解难度** | 中 | 高（需关联） | 低（自包含） |
| **语义清晰度** | 中 | 低 | 高 |
| **灵活性** | 高 | 低 | 高 |
| **UI 实现难度** | 中 | 低 | 中 |
| **出错概率** | 中 | 高 | 低 |

---

## 为什么选择 V3？

### 1. LLM 注意力机制考虑

LLM 在处理 JSON 时：
- **局部性原则**：相邻的字段更容易被关联理解
- **上下文完整性**：问题和选项在一起形成完整的语义单元
- **减少跳跃**：不需要在 `questions[0]` 和 `options[2]` 之间跳转

### 2. 实际场景需求

不同问题通常需要不同的选项：

```json
{
  "questions": [
    {
      "text": "使用哪个数据库？",
      "options": ["PostgreSQL", "MySQL", "MongoDB"]
    },
    {
      "text": "部署环境？",
      "options": ["Development", "Staging", "Production"]
    }
  ]
}
```

如果使用共享选项，会变成：
```json
{
  "questions": ["使用哪个数据库？", "部署环境？"],
  "options": ["PostgreSQL", "MySQL", "MongoDB", "Development", "Staging", "Production"]
}
```

**问题**：
- 6 个选项混在一起，LLM 难以判断哪些属于哪个问题
- 用户 UI 也会混乱

### 3. 用户体验

**V3 的 UI 更直观**：
```
Q1: 使用哪个数据库？
  ○ PostgreSQL
  ○ MySQL
  ○ MongoDB
  
Q2: 部署环境？
  ○ Development
  ○ Staging
  ○ Production
```

**V2 的 UI 会困惑**：
```
可用选项：
1. PostgreSQL  2. MySQL  3. MongoDB
4. Development  5. Staging  6. Production

Q1: 使用哪个数据库？
  ○ Option ?  ○ Option ?  ...
  
Q2: 部署环境？
  ○ Option ?  ○ Option ?  ...
```

用户需要思考："我应该选 1、2、3 还是 4、5、6？"

---

## 数据结构设计

### 请求格式
```typescript
interface AskFollowupQuestionParams {
  questions: Array<{
    text: string;      // 问题文本
    options: string[]; // 该问题的选项（1-4个）
  }>;
  additionalInfoLabel?: string;
}
```

### 响应格式
```typescript
interface UserInteractionResponse {
  answers: Array<{
    questionIndex: number;        // 问题索引
    selectedOptionIndex: number;  // 选项索引（-1表示自定义）
    customInput?: string;         // 自定义输入
    answer: string;               // 最终答案
  }>;
  additionalInfo?: string;
}
```

### 格式化输出（给 LLM）
```
User Responses:

Q1: 使用哪个数据库？
A1: PostgreSQL (selected from options)

Q2: 部署环境？
A2: Production (selected from options)

Additional Information:
需要支持高可用

--- End of User Response ---
```

---

## 实施要点

### SDK 层
1. Schema 验证：确保每个问题都有 1-4 个选项
2. Handler 逻辑：遍历问题数组，为每个问题构建交互请求
3. 响应处理：通过 `questionIndex` 映射回原始问题文本

### Apps 层
1. UI 组件：每个问题卡片独立渲染其选项
2. 事件订阅：监听 `ASK_FOLLOWUP_QUESTION` 事件
3. 响应格式：返回 `questionIndex` 而非问题文本（SDK 负责映射）

---

## 总结

**核心原则**：
> "让 LLM 最容易理解的结构，就是最好的结构"

**V3 嵌套结构**通过保持问题和选项的语义完整性，实现了：
- ✅ 降低 LLM 认知负担
- ✅ 提高回答准确性
- ✅ 改善用户体验
- ✅ 增强系统可靠性

这是经过三次迭代后的最优设计方案。
