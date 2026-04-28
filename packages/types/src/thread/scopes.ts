/**
 * Variable Scope Definitions
 *
 * Contains two core concepts:
 * 1. VariableScope - enumerated values for a single scope
 * 2. VariableScopes - a container structure with four levels of scopes
 */

/**
 * Variable Scope Type (Single Value)
 * Define the scope level of the variable in the workflow
 */
export type VariableScope = "global" | "thread" | "local" | "loop";

/**
 * 变量作用域结构（运行时状态）
 *
 * 用途：
 * 1. Thread.variableScopes - 线程的运行时变量存储
 * 2. ThreadStateSnapshot.variableScopes - 检查点快照的状态
 * 3. VariableState 中的运行时变量管理
 *
 * 作用域特点：
 * - **global**: 工作流级别的全局变量，多线程共享同一对象引用
 * - **thread**: 线程级别的变量，每个线程独享，fork 时深拷贝
 * - **local**: 本地作用域栈，支持嵌套（如进入子图）
 * - **loop**: 循环作用域栈，支持嵌套循环
 *
 * 访问优先级（从低到高）：
 * global < thread < local[...] < loop[...]
 *
 * 说明：
 * - 访问变量时，优先在高优先级作用域查找
 * - local 和 loop 是栈结构，访问时使用栈顶（最内层）的值
 * - 使用场景示例：
 *   - global: 工作流配置、常量、全局状态
 *   - thread: 工作流执行期间的临时变量、中间结果
 *   - local: 子图内的局部变量（子图结束后销毁）
 *   - loop: 循环迭代变量（循环结束后销毁）
 */
export interface VariableScopes {
  /**
   * 全局作用域 - 多线程共享
   *
   * 特点：
   * - 在工作流初始化时设置
   * - 所有线程（包括 fork 创建的子线程）共享同一对象引用
   * - 修改在所有线程中可见
   * - 线程间的全局变量需要同步控制
   *
   * 示例：
   * ```typescript
   * thread.variableScopes.global['API_KEY'] = 'xxx';
   * // 在其他线程中也可见
   * ```
   */
  global: Record<string, unknown>;

  /**
   * 线程作用域 - 单线程内部
   *
   * 特点：
   * - 每个线程有独立的对象，互不干扰
   * - fork 时执行深拷贝，子线程有独立副本
   * - 修改不影响其他线程
   * - 最常用的变量存储位置
   *
   * 示例：
   * ```typescript
   * thread.variableScopes.thread['result'] = data;
   * // 仅在该线程中可见
   * ```
   */
  thread: Record<string, unknown>;

  /**
   * 本地作用域栈 - 支持嵌套
   *
   * 特点：
   * - 是数组形式的栈结构，每个元素是一个作用域
   * - 进入本地作用域（如进入子图）时 push 新对象
   * - 退出本地作用域时 pop
   * - 优先级高于 global/thread 作用域
   * - 本地作用域结束后自动销毁，不影响父作用域
   *
   * 使用场景：
   * - 子图的局部变量
   * - 临时计算结果
   * - 作用域隔离
   *
   * 示例：
   * ```typescript
   * // 进入子图
   * thread.variableScopes.local.push({ tempVar: 'value' });
   * // 在子图中可以访问 tempVar
   * // 退出子图
   * thread.variableScopes.local.pop();
   * // tempVar 不再可见
   * ```
   */
  local: Record<string, unknown>[];

  /**
   * 循环作用域栈 - 支持嵌套循环
   *
   * 特点：
   * - 是数组形式的栈结构，每个元素是一个循环的作用域
   * - 每次进入循环时 push 新对象
   * - 循环结束时 pop
   * - 优先级最高，覆盖其他三层作用域
   * - 循环结束后作用域销毁
   *
   * 使用场景：
   * - 循环迭代变量（item, index 等）
   * - 循环内的临时计算结果
   * - 嵌套循环的变量隔离
   *
   * 示例：
   * ```typescript
   * // 进入循环（迭代数组 [1, 2, 3]）
   * thread.variableScopes.loop.push({ item: 1, index: 0 });
   * // 在循环体中可以访问 item 和 index
   * // 迭代完成，退出循环
   * thread.variableScopes.loop.pop();
   * // item 和 index 不再可见
   * ```
   */
  loop: Record<string, unknown>[];
}
