/**
 * Interrupt-Related Error Types (DEPRECATED)
 * 
 * ⚠️ DEPRECATED: These types have been moved to SDK internal implementation.
 * 
 * Migration Guide:
 * - InterruptionType → Use sdk/core/types/interruption-types.ts
 * - InterruptedException → Use sdk/core/types/interruption-types.ts
 * - WorkflowExecutionInterruptedException → Use sdk/core/types/interruption-types.ts
 * - AbortError → Use sdk/core/types/interruption-types.ts
 * 
 * Reason for Deprecation:
 * - Interrupt handling is an internal SDK control flow mechanism
 * - External applications should use ExecutionInterruptionCheckResult from SDK
 * - Exception classes are implementation details that change frequently
 * - Keeping them in public API creates unnecessary coupling
 * 
 * @deprecated Will be removed in next major version
 */

// This file is intentionally left empty.
// All interruption-related types have been moved to SDK internal implementation.
// See: sdk/core/types/interruption-types.ts

