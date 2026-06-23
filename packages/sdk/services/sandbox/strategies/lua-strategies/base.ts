/**
 * Lua Sandbox Strategies — Shared Base
 *
 * Common utilities and constants shared across Lua sandbox strategy implementations.
 */

import { spawnSync } from "node:child_process";

/**
 * Default denied Lua modules when policy.lua.deniedModules is empty.
 * These modules provide system access and should be restricted by default.
 */
export const DEFAULT_DENIED_MODULES: string[] = [
  "os",
  "io",
  "package",
  "debug",
  "ffi",
  "socket",
  "lfs",
  "luaposix",
];

/**
 * Check if Lua is available on the system by running `lua -v`.
 */
export function checkLuaAvailable(): boolean {
  try {
    const result = spawnSync("lua", ["-v"], {
      timeout: 5000,
      stdio: "pipe",
      encoding: "utf-8",
    });
    return result.status === 0;
  } catch {
    return false;
  }
}

/**
 * Check if LuaJIT is available on the system.
 * LuaJIT provides better performance but has different sandboxing considerations.
 */
export function checkLuaJITAvailable(): boolean {
  try {
    const result = spawnSync("luajit", ["-v"], {
      timeout: 5000,
      stdio: "pipe",
      encoding: "utf-8",
    });
    return result.status === 0;
  } catch {
    return false;
  }
}
