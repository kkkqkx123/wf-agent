/**
 * Lua Sandbox Strategies — Barrel Export
 *
 * Re-exports all Lua sandbox strategy implementations.
 *
 * Usage:
 *   import { LuaBuiltinHookStrategy, LuaStaticAnalyzerStrategy } from "./lua-strategies/index.js";
 */

export { LuaBuiltinHookStrategy } from "./builtin-hook.js";
export { LuaStaticAnalyzerStrategy } from "./static-analyzer.js";
export { checkLuaAvailable, checkLuaJITAvailable, DEFAULT_DENIED_MODULES } from "./base.js";
