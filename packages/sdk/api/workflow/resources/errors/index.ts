/**
 * Workflow Error Analysis Module
 *
 * Workflow-specific error analysis implementation
 */

export {
  WorkflowErrorAnalysisAPI,
  type WorkflowRootCauseAnalysis,
  type WorkflowErrorStatistics,
  type WorkflowRecoveryProposal,
  type WorkflowErrorContext,
} from "./workflow-error-analysis-api.js";

// Re-export common interface from shared
export type { IErrorAnalysisProvider } from "../../../shared/resources/errors/error-analysis-provider.js";
