/**
 * Shared Error Analysis Module
 *
 * Provides common interfaces and types for error analysis across execution types.
 * Specific implementations are in their respective execution type modules:
 * - Agent: sdk/api/agent/resources/errors/
 * - Workflow: sdk/api/workflow/resources/errors/ (future)
 */

export type { IErrorAnalysisProvider, ErrorAnalysisProviderResult, ExecutionTypeDiscriminant } from "./error-analysis-provider.js";



