/**
 * Coordinator Module
 *
 * The coordinator is a stateless component responsible for coordinating the interactions between various managers.
 *
 * Design Principles:
 * - Coordination Logic: Encapsulate complex coordination logic.
 * - Dependency Injection: Receive dependent managers through the constructor.
 *
 */

export * from "./llm-execution-coordinator.js";
export * from "./tool-approval-coordinator.js";
