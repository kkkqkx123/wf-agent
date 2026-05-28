/**
 * OS Hook Strategies — Barrel Export
 *
 * Re-exports all platform-specific OS hook strategies and provides a
 * convenience factory that returns the best strategy for the current platform.
 *
 * Usage:
 *   // Import specific strategies for explicit registration
 *   import { LinuxSeccompStrategy } from "./strategies/os-hooks/index.js";
 *
 *   // Or use the factory to auto-select by platform
 *   import { createOsHookStrategy } from "./strategies/os-hooks/index.js";
 *   const strategy = createOsHookStrategy();
 */

import type { StrategyImplementation } from "../../types.js";
import type { ScriptExecutionResult } from "@wf-agent/types";
import type { TerminalService } from "../../../terminal/index.js";

import { LinuxSeccompStrategy } from "./linux-seccomp.js";
import { WindowsJobObjectStrategy } from "./windows-job-object.js";
import { ProotLikeRedirectStrategy } from "./proot-redirect.js";

export { LinuxSeccompStrategy };
export { WindowsJobObjectStrategy };
export { ProotLikeRedirectStrategy };

/**
 * Create the best OS hook strategy for the current platform.
 *
 * Platform mapping:
 *   - win32 → WindowsJobObjectStrategy
 *   - linux → LinuxSeccompStrategy
 *   - darwin / others → ProotLikeRedirectStrategy (best-effort path redirect)
 *
 * Usage:
 *   const strategy = createOsHookStrategy();
 *   const result = await strategy.execute(options, policy);
 *
 * @param terminalService Optional TerminalService instance (auto-resolved if omitted)
 */
export function createOsHookStrategy(
  terminalService?: TerminalService,
): StrategyImplementation<ScriptExecutionResult> {
  switch (process.platform) {
    case "win32":
      return new WindowsJobObjectStrategy(terminalService);
    case "linux":
      return new LinuxSeccompStrategy(terminalService);
    default:
      return new ProotLikeRedirectStrategy(terminalService);
  }
}
