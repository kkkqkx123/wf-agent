# 变量保护机制: readonly vs freeze

**版本**: 1.0  
**最后更新**: 2026-05-12  
**适用对象**: 开发者、工作流配置者、维护人员

---

## 📋 目录

- [概述](#概述)
- [readonly - 防止重新赋值](#readonly---防止重新赋值)
- [freeze - 防止内容修改](#freeze---防止内容修改)
- [对比总结](#对比总结)
- [使用场景](#使用场景)
- [最佳实践](#最佳实践)
- [常见问题](#常见问题)
- [示例代码](#示例代码)

---

## 概述

VariableManager 提供两种变量保护机制:

| 机制 | 作用 | 适用类型 | 保护级别 |
|------|------|----------|----------|
| **readonly** | 防止变量重新赋值 | 所有类型 | 变量级别 |
| **freeze** | 防止对象/数组内容修改 | 仅 object/array | 内容级别 |

**可以组合使用**,提供双重保护:
```typescript
{
  name: "CONFIG",
  readonly: true,  // 不能重新赋值变量
  freeze: true,    // 不能修改对象内容
  value: { timeout: 5000 }
}
```

---

## readonly - 防止重新赋值

### 作用

`readonly: true` 阻止对变量的**重新赋值**操作。

### 行为

```typescript
// 定义只读变量
manager.registerVariable({
  name: "MAX_RETRIES",
  type: "number",
  value: 3,
  readonly: true  // ← 设置为只读
});

// ✅ 允许: 读取值
const maxRetries = manager.getVariable("MAX_RETRIES"); // 3

// ❌ 禁止: 重新赋值
manager.setVariable("MAX_RETRIES", 5); 
// 抛出 RuntimeValidationError: Variable 'MAX_RETRIES' is readonly
```

### 适用类型

- ✅ number
- ✅ string
- ✅ boolean
- ✅ object
- ✅ array

**对所有类型都有效**。

### 典型场景

#### 1. 常量定义

```toml
[[variables]]
name = "API_VERSION"
type = "string"
value = "v1"
scope = "global"
readonly = true
```

```typescript
// 尝试修改会失败
manager.setVariable("API_VERSION", "v2"); // ❌ Error
```

#### 2. 配置引用

```toml
[[variables]]
name = "databaseConfig"
type = "object"
value = { host = "localhost", port = 5432 }
scope = "global"
readonly = true  # 不能替换整个配置对象
```

```typescript
// ❌ 不能替换整个对象
manager.setVariable("databaseConfig", { host: "remote", port: 3306 });

// ✅ 但可以修改对象内容 (除非同时设置 freeze)
const config = manager.getVariable("databaseConfig");
config.host = "remote"; // 允许!
```

#### 3. 循环计数器保护

```toml
[[variables]]
name = "totalIterations"
type = "number"
value = 0
scope = "execution"
readonly = true  # 由系统管理,不允许手动修改
```

---

## freeze - 防止内容修改

### 作用

`freeze: true` 使用 `Object.freeze()` 冻结对象/数组,阻止对其**内容的修改**。

### 行为

```typescript
// 定义冻结变量
manager.registerVariable({
  name: "config",
  type: "object",
  value: { timeout: 5000, retries: 3 },
  freeze: true  // ← 设置为冻结
});

// ✅ 允许: 读取值
const config = manager.getVariable("config");
console.log(config.timeout); // 5000

// ❌ 禁止: 修改对象内容
config.timeout = 10000; 
// 抛出 TypeError: Cannot assign to read only property 'timeout'

// ❌ 禁止: 添加新属性
config.newProp = "value";
// 抛出 TypeError: Cannot add property newProp

// ❌ 禁止: 删除属性
delete config.timeout;
// 抛出 TypeError: Cannot delete property 'timeout'
```

### 重要特性

#### 1. 浅冻结 (Shallow Freeze)

`Object.freeze()` 只冻结对象本身,**不递归冻结嵌套对象**。

```typescript
manager.registerVariable({
  name: "nested",
  type: "object",
  value: { 
    outer: { 
      inner: "value" 
    } 
  },
  freeze: true
});

const data = manager.getVariable("nested");

// ❌ 外层属性不能修改
data.outer = "modified"; // TypeError

// ✅ 但内层对象仍可修改!
data.outer.inner = "new value"; // 允许!
```

**如需深冻结**,需要手动处理:
```typescript
import deepFreeze from 'deep-freeze';

manager.registerVariable({
  name: "deepConfig",
  type: "object",
  value: deepFreeze({ nested: { deep: "value" } }),
  freeze: true
});
```

#### 2. 不可逆性

一旦对象被冻结,**无法解冻**。

```typescript
const obj = { value: 1 };
Object.freeze(obj);

// 没有 Object.unfreeze() 方法!
// 如果需要修改,必须创建新对象
const newObj = { ...obj, value: 2 };
manager.setVariable("data", newObj);
```

#### 3. 仅适用于对象类型

对原始类型(number, string, boolean)设置 `freeze: true` **不会产生错误**,但也**没有实际效果**。

```typescript
manager.registerVariable({
  name: "counter",
  type: "number",
  value: 0,
  freeze: true  // 无实际效果,但不报错
});

// 仍然可以正常修改
manager.setVariable("counter", 10); // ✅ 允许
```

### 适用类型

- ✅ object (主要用途)
- ✅ array (主要用途)
- ⚠️ number/string/boolean (无实际效果,但不报错)

---

## 对比总结

### 核心区别

| 维度 | readonly | freeze |
|------|----------|--------|
| **保护目标** | 变量引用 | 对象内容 |
| **实现方式** | 运行时检查 | `Object.freeze()` |
| **作用时机** | `setVariable()` 时检查 | 注册/设置时立即冻结 |
| **可逆性** | 可通过修改定义解除 | **不可逆** |
| **错误类型** | `RuntimeValidationError` | `TypeError` |
| **适用类型** | 所有类型 | 主要 object/array |
| **性能影响** | 微小(条件检查) | 中等(冻结操作) |

### 行为对比表

```typescript
// 场景1: readonly + object
manager.registerVariable({
  name: "config1",
  readonly: true,
  freeze: false,
  value: { timeout: 5000 }
});

manager.setVariable("config1", { timeout: 10000 }); // ❌ Error: readonly
const c1 = manager.getVariable("config1");
c1.timeout = 10000; // ✅ 允许! (对象未冻结)


// 场景2: freeze + object
manager.registerVariable({
  name: "config2",
  readonly: false,
  freeze: true,
  value: { timeout: 5000 }
});

manager.setVariable("config2", { timeout: 10000 }); // ✅ 允许! (可以替换)
const c2 = manager.getVariable("config2");
c2.timeout = 10000; // ❌ TypeError: frozen


// 场景3: readonly + freeze (双重保护)
manager.registerVariable({
  name: "config3",
  readonly: true,
  freeze: true,
  value: { timeout: 5000 }
});

manager.setVariable("config3", { timeout: 10000 }); // ❌ Error: readonly
const c3 = manager.getVariable("config3");
c3.timeout = 10000; // ❌ TypeError: frozen
```

### 选择指南

```
需要保护什么?
├─ 防止变量被重新赋值?
│  └─ ✅ 使用 readonly
│
├─ 防止对象内容被修改?
│  └─ ✅ 使用 freeze
│
└─ 两者都需要?
   └─ ✅ 同时使用 readonly + freeze
```

---

## 使用场景

### 场景1: 全局配置 (推荐 readonly + freeze) ⭐⭐⭐⭐⭐

**需求**: 应用配置在整个生命周期中不应被修改

```toml
[[variables]]
name = "APP_CONFIG"
type = "object"
value = { 
  api_url = "https://api.example.com",
  timeout = 5000,
  max_retries = 3 
}
scope = "global"
readonly = true
freeze = true
```

**保护效果**:
- ❌ 不能替换配置对象 (`readonly`)
- ❌ 不能修改配置项 (`freeze`)
- ✅ 只能读取配置

---

### 场景2: API响应缓存 (推荐 freeze) ⭐⭐⭐⭐

**需求**: 缓存的API响应应保持完整,不被意外修改

```toml
[[variables]]
name = "cachedResponse"
type = "object"
value = null
scope = "execution"
readonly = false  # 允许更新缓存
freeze = true     # 但缓存内容不可修改
```

```typescript
// 更新缓存 (允许,因为 readonly=false)
manager.setVariable("cachedResponse", await fetchApi());

// 尝试修改缓存内容 (禁止,因为 freeze=true)
const response = manager.getVariable("cachedResponse");
response.data.modified = true; // ❌ TypeError
```

---

### 场景3: 常量定义 (推荐 readonly) ⭐⭐⭐⭐⭐

**需求**: 定义不会改变的常量值

```toml
[[variables]]
name = "MAX_PAGE_SIZE"
type = "number"
value = 100
scope = "global"
readonly = true
```

**说明**: 对原始类型,number/string/boolean,只需 `readonly`,无需 `freeze`。

---

### 场景4: 共享数据结构 (谨慎使用 freeze) ⭐⭐⭐

**需求**: 多个组件共享一个数据结构,但各组件可能需要独立修改

```toml
[[variables]]
name = "sharedData"
type = "object"
value = { items = [] }
scope = "global"
readonly = false
freeze = false  # ⚠️ 不要冻结,允许多方修改
```

**警告**: 如果冻结,第一个修改的组件会成功,后续组件会失败。

---

### 场景5: 临时计算结果 (不推荐 freeze) ⭐

**需求**: 逐步构建的计算结果

```toml
[[variables]]
name = "accumulator"
type = "array"
value = []
scope = "subgraph"
readonly = false
freeze = false  # 需要逐步添加元素
```

```typescript
const acc = manager.getVariable("accumulator");
acc.push(item1); // ✅
acc.push(item2); // ✅
acc.push(item3); // ✅

// 如果 freeze=true,第一次 push 后就会失败
```

---

## 最佳实践

### 1. 优先使用 readonly

对于大多数场景,`readonly` 已经足够:

```toml
# ✅ 推荐: 简单明了
[[variables]]
name = "CONSTANT"
type = "number"
value = 42
readonly = true
```

仅在需要保护对象内容时才添加 `freeze`。

---

### 2. 明确意图

在变量名或描述中说明保护原因:

```toml
[[variables]]
name = "IMMUTABLE_CONFIG"  # 命名暗示不可变
type = "object"
value = { ... }
readonly = true
freeze = true
metadata = { description = "Application config - should never change" }
```

---

### 3. 组合使用要谨慎

`readonly + freeze` 提供最强保护,但也最严格:

```typescript
// 一旦设置,完全无法修改
{
  readonly: true,  // 不能 setVariable
  freeze: true,    // 不能修改内容
  value: { ... }
}

// 如果需要更新,必须:
// 1. 删除变量
manager.deleteVariable("config");

// 2. 重新注册
manager.registerVariable({
  name: "config",
  value: { ...new value... }
});
```

---

### 4. 注意浅冻结限制

如果需要深冻结,明确说明:

```typescript
import deepFreeze from 'deep-freeze';

// 方式1: 在注册前深冻结
const config = deepFreeze({ nested: { deep: "value" } });
manager.registerVariable({
  name: "config",
  value: config,
  freeze: true  // 虽然已经冻结,但仍标记为freeze
});

// 方式2: 在文档中说明是浅冻结
/**
 * @note This variable uses shallow freeze. 
 * Nested objects can still be modified.
 */
```

---

### 5. 测试冻结行为

编写测试验证保护是否生效:

```typescript
it("should prevent modification of frozen config", () => {
  manager.registerVariable({
    name: "config",
    type: "object",
    value: { timeout: 5000 },
    freeze: true
  });

  const config = manager.getVariable("config");
  
  expect(() => {
    config.timeout = 10000;
  }).toThrow(TypeError);
});
```

---

### 6. 提供更新策略

如果变量需要定期更新,设计清晰的更新流程:

```typescript
// ❌ 不好: 直接修改冻结对象
const config = manager.getVariable("config");
config.timeout = 10000; // TypeError

// ✅ 好: 创建新对象并替换
const oldConfig = manager.getVariable("config");
const newConfig = { ...oldConfig, timeout: 10000 };
manager.setVariable("config", newConfig); // 如果 readonly=false
```

---

## 常见问题

### Q1: readonly 和 freeze 可以同时使用吗?

**A**: 可以,而且经常一起使用以提供最强保护。

```typescript
{
  readonly: true,  // 防止变量重新赋值
  freeze: true,    // 防止对象内容修改
  value: { ... }
}
```

---

### Q2: freeze 会影响性能吗?

**A**: 轻微影响,通常可忽略。

- **冻结操作**: ~0.01ms (小对象) 到 ~10ms (大对象)
- **冻结后读取**: 无影响
- **冻结后写入**: 立即抛出错误 (比检查更快)

**建议**: 仅在必要时使用,对性能敏感场景进行基准测试。

---

### Q3: 如何解冻一个冻结的对象?

**A**: **无法解冻**。JavaScript 不提供 `Object.unfreeze()` 方法。

**解决方案**:
```typescript
// 创建新对象
const oldObj = manager.getVariable("data");
const newObj = { ...oldObj, modified: "value" };
manager.setVariable("data", newObj);
```

---

### Q4: freeze 对数组有效吗?

**A**: 有效,数组也是对象。

```typescript
manager.registerVariable({
  name: "items",
  type: "array",
  value: [1, 2, 3],
  freeze: true
});

const items = manager.getVariable("items");
items.push(4);        // ❌ TypeError
items[0] = 10;        // ❌ TypeError
items.length = 0;     // ❌ TypeError
```

---

### Q5: 嵌套对象如何处理?

**A**: `Object.freeze()` 是浅冻结,嵌套对象不受影响。

```typescript
const obj = { 
  level1: { 
    level2: { 
      value: "deep" 
    } 
  } 
};

Object.freeze(obj);

obj.level1 = "changed";        // ❌ TypeError
obj.level1.level2.value = "x"; // ✅ 允许! (level2 未冻结)
```

**如需深冻结**:
```typescript
import deepFreeze from 'deep-freeze';
manager.registerVariable({
  name: "deepObj",
  value: deepFreeze(obj),
  freeze: true
});
```

---

### Q6: freeze 和 immutable.js 有什么区别?

**A**: 

| 特性 | Object.freeze() | Immutable.js |
|------|----------------|--------------|
| **深度** | 浅冻结 | 持久化数据结构 |
| **修改** | 抛出错误 | 返回新实例 |
| **性能** | 快 | 中等 |
| **复杂度** | 简单 | 需要学习曲线 |
| **生态** | 原生支持 | 额外依赖 |

**建议**: 简单场景用 `freeze`,复杂场景考虑 Immutable.js 或 Immer。

---

### Q7: 如何在开发环境检测意外的冻结?

**A**: 启用严格模式或使用调试工具:

```typescript
// 开发环境: 添加警告
if (process.env.NODE_ENV === "development") {
  if (Object.isFrozen(value)) {
    console.warn(`Variable '${name}' is frozen. Modifications will fail.`);
  }
}
```

---

### Q8: freeze 会影响序列化/反序列化吗?

**A**: 不会。`JSON.stringify()` 和 `JSON.parse()` 正常工作。

```typescript
const obj = { value: 1 };
Object.freeze(obj);

const json = JSON.stringify(obj); // '{"value":1}'
const parsed = JSON.parse(json);  // { value: 1 } (新对象,未冻结)
```

**注意**: 反序列化后得到的是**新对象**,不再冻结。

---

## 示例代码

### 完整示例1: 应用配置

```toml
# workflow.toml
[[variables]]
name = "APP_CONFIG"
type = "object"
value = { 
  api_url = "https://api.example.com",
  timeout = 5000,
  max_retries = 3,
  features = { enable_cache = true, debug_mode = false }
}
scope = "global"
readonly = true
freeze = true
metadata = { 
  description = "Application configuration - immutable throughout lifecycle" 
}
```

```typescript
// 使用配置
const config = manager.getVariable("APP_CONFIG");
console.log(config.api_url); // ✅ 读取

// 尝试修改
config.timeout = 10000;           // ❌ TypeError
manager.setVariable("APP_CONFIG", {}); // ❌ RuntimeValidationError
```

---

### 完整示例2: API缓存

```toml
[[variables]]
name = "apiCache"
type = "object"
value = {}
scope = "execution"
readonly = false  # 允许更新缓存
freeze = true     # 但缓存内容不可修改
```

```typescript
// 更新缓存
async function updateCache(key: string, data: any) {
  const cache = manager.getVariable("apiCache") as Record<string, any>;
  const newCache = { ...cache, [key]: data };
  manager.setVariable("apiCache", newCache);
}

// 使用缓存
const cache = manager.getVariable("apiCache");
const userData = cache["user_123"]; // ✅ 读取

// 尝试修改缓存内容
userData.name = "Modified"; // ❌ TypeError
```

---

### 完整示例3: 常量集合

```toml
[[variables]]
name = "ERROR_CODES"
type = "object"
value = { 
  NOT_FOUND = 404,
  UNAUTHORIZED = 401,
  SERVER_ERROR = 500 
}
scope = "global"
readonly = true
# 不需要 freeze,因为是常量对象,不会修改内容
```

---

### 完整示例4: 双重保护

```toml
[[variables]]
name = "SECURE_CONFIG"
type = "object"
value = { 
  api_key = "sk-xxx",
  secret = "encrypted_value" 
}
scope = "global"
readonly = true
freeze = true
metadata = { 
  description = "Security-sensitive config - must never change" 
}
```

```typescript
// 任何修改尝试都会失败
manager.setVariable("SECURE_CONFIG", {}); // ❌ readonly
const config = manager.getVariable("SECURE_CONFIG");
config.api_key = "new-key"; // ❌ freeze
```

---

## 维护人员指南

### 调试冻结相关错误

#### 错误1: TypeError: Cannot assign to read only property

**原因**: 尝试修改冻结对象的属性

**解决**:
```typescript
// 检查对象是否冻结
const obj = manager.getVariable("name");
console.log(Object.isFrozen(obj)); // true/false

// 如果需要修改,创建新对象
const newObj = { ...obj, modified: "value" };
manager.setVariable("name", newObj);
```

---

#### 错误2: RuntimeValidationError: Variable is readonly

**原因**: 尝试重新赋值只读变量

**解决**:
```typescript
// 检查变量定义
const def = manager.getVariableDefinition("name");
console.log(def?.readonly); // true/false

// 如果需要修改,先删除再重新注册
manager.deleteVariable("name");
manager.registerVariable({
  name: "name",
  value: newValue,
  readonly: false  // 或者保持 readonly,根据需求
});
```

---

### 性能优化建议

1. **避免频繁冻结大对象**
   ```typescript
   // ❌ 不好: 每次迭代都冻结
   for (let i = 0; i < 1000; i++) {
     manager.setVariable("data", largeObject, true);
   }
   
   // ✅ 好: 只在最终结果时冻结
   manager.setVariable("data", finalResult, true);
   ```

2. **批量操作时暂缓冻结**
   ```typescript
   // 构建阶段: 不冻结
   manager.setVariable("accumulator", [], false);
   const acc = manager.getVariable("accumulator");
   acc.push(...items);
   
   // 完成后: 冻结
   manager.setVariable("accumulator", acc, true);
   ```

---

### 版本兼容性

| 版本 | readonly | freeze | 备注 |
|------|----------|--------|------|
| < 1.0 | ✅ | ❌ | freeze 尚未实现 |
| 1.0+ | ✅ | ✅ | 完整支持 |

**迁移指南**:
```typescript
// 旧代码 (只有 readonly)
{ name: "config", readonly: true, value: {...} }

// 新代码 (添加 freeze)
{ name: "config", readonly: true, freeze: true, value: {...} }
```

---

## 总结

### 快速决策树

```
需要保护变量吗?
├─ 是
│  ├─ 防止重新赋值?
│  │  └─ ✅ 设置 readonly: true
│  │
│  ├─ 防止对象内容修改?
│  │  └─ ✅ 设置 freeze: true
│  │
│  └─ 两者都需要?
│     └─ ✅ 同时设置 readonly: true 和 freeze: true
│
└─ 否
   └─ 保持默认 (readonly: false, freeze: false)
```

### 核心原则

1. **最小权限**: 只在必要时启用保护
2. **明确意图**: 通过命名和文档说明保护原因
3. **组合使用**: readonly + freeze 提供最强保护
4. **注意限制**: freeze 是浅冻结,不可逆
5. **测试验证**: 编写测试确保保护生效

---

**参考资料**:
- [MDN: Object.freeze()](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Object/freeze)
- [VariableManager API 文档](../../sdk/workflow/state-managers/variable-manager.ts)
- [freeze 实施报告](./freeze-implementation-report.md)
