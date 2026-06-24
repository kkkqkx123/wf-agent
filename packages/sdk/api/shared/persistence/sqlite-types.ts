/**
 * SQLite Type Mappings
 *
 * Defines strongly-typed interfaces for SQLite query results
 * to eliminate the need for `as any` type assertions.
 */

/**
 * Execution state row from database
 * Represents the state_data column from execution_states table
 */
export interface SQLiteExecutionStateRow {
  state_data: string;
  execution_id: string;
  iteration: number;
}

/**
 * Event row from database
 * Represents a row from events table
 */
export interface SQLiteEventRow {
  event_id: string;
  event_data: string;
  execution_id: string;
  timestamp: number;
}

/**
 * Checkpoint row from database
 * Represents a row from checkpoints table
 */
export interface SQLiteCheckpointRow {
  checkpoint_id: string;
  timestamp: number;
  execution_id?: string;
}

/**
 * System metrics row from database
 * Represents a row from system_metrics table
 */
export interface SQLiteSystemMetricsRow {
  iteration: number;
  cpu_time_ms: number;
  memory_peak_mb: number;
  duration_ms: number;
  execution_id: string;
  timestamp: number;
}

/**
 * LLM metrics row from database
 * Represents a row from llm_metrics table
 */
export interface SQLiteLLMMetricsRow {
  iteration: number;
  input_tokens: number;
  output_tokens: number;
  cost_usd: number;
  model: string;
  duration_ms: number;
  execution_id: string;
  timestamp: number;
  tool_call_id?: string;
  cache_read_tokens?: number;
  cache_creation_tokens?: number;
}

/**
 * Resource usage row from database (legacy)
 * Represents a row from resource_usage table
 */
export interface SQLiteResourceUsageRow {
  execution_id: string;
  iteration: number;
  cpu_time: number;
  memory_used: number;
  timestamp: number;
}
