/**
 * Runtime Adapters Module Exports
 */

export { BaseAppAdapter, AdapterError } from "./base-adapter.js";
export {
  NotFoundError,
  findByIdOrThrow,
  scanDirForConfigs,
  batchRegisterFromDir,
  executeWithErrorHandling,
  createErrorHandler,
} from "./utils.js";
export type { BatchResult } from "./utils.js";