/**
 * OS Hook Strategies — Re-export shim
 *
 * This file has been refactored into a directory (os-hooks/) for better
 * maintainability. This shim re-exports everything from the new location
 * to preserve backwards compatibility.
 *
 * @deprecated Import directly from "./os-hooks/index.js" instead.
 */

export {
  LinuxSeccompStrategy,
  WindowsJobObjectStrategy,
  ProotLikeRedirectStrategy,
  createOsHookStrategy,
} from "./os-hooks/index.js";
