/**
 * Analysis Module
 * Provides execution analysis capabilities: comparison, progress tracking, etc.
 */

export { ComparisonAnalysis } from './comparison.js';
export type {
  ExecutionComparison,
  RangeComparison,
  Delta,
  DurationDelta,
  ThroughputDelta,
  SuccessRateDelta,
  ErrorComparison,
  InterruptionComparison,
} from './comparison.js';

export { ProgressTracker, ProgressAnalysis, ProgressEventType } from './progress.js';
export type {
  ProgressMetrics,
  ProgressListener,
} from './progress.js';
