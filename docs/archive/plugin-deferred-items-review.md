# Plugin 待决项决策回顾

源自 `docs/plan/plugin-deferred-items.md`，对 3 项放弃修改的理由进行合理性分析。

## 1. Lua Mutex 串行化优化 — **放弃合理**

`Arc<Mutex<Lua>>` 是正确设计。Lua VM 本身单线程，不同插件各有独立 state+Mutex 已可并行。按 category 拆分 state 会导致 `RegistryKey` 跨 state 无效，且无法共享闭包。当前不存在性能问题，优化成本 > 收益。

## 2. ContributionBridge 实装 — **放弃合理（但有隐含风险）**

`ContributionBridge` 是可选外部同步接口，内部存储已由 `ContributionManager` 完整实现（7 个 registry + query methods），当前无功能缺失。在外部消费系统接口稳定前，bridge 保持 trait 定义即可，不需要默认实现。

**隐含风险**：如果外部消费系统最终发现需要 bridge 做跨插件通信或同步，当前内聚查询方法可能不够。届时再实装也不迟。

## 3. `register_contributions` 保持同步 — **放弃合理**

注册仅做内存级操作（Lua 拿 `RegistryKey`、Native 保存函数指针），无 I/O/阻塞，仅在 `activate` 时执行一次。用 `spawn_blocking` 包装反而增加线程池切换开销。

## 总结

三项放弃决策全部合理。第 1 项涉及正确设计而非缺陷；第 2 项是时机问题（接口未稳定）；第 3 项是不必要抽象。无错误决策。
