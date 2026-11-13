pub mod code_context;
pub mod transport;

pub use code_context::fold::{
    apply_fold_results, build_fold_notice, file_base_name, infer_language_hint,
    select_fold_candidates, tool_call_paths, FoldBatchItem, FoldBatchRequest, FoldBatchResponse,
    FoldBatchResultItem, FoldBatchStats, FoldCandidate, FoldClient,
};
pub use code_context::navigation::{
    definition, definition_request_body, optional_column, optional_symbol, references,
    references_request_body, require_line, require_path, resolve_paths,
    summarize_definition_payload, summarize_references_payload, summarize_symbols_payload, symbols,
    symbols_request_body, LocationQuery, DEFINITION_ENDPOINT, MAX_SYMBOLS_PATHS,
    MAX_SYMBOL_CHILDREN, MAX_SYMBOL_DEPTH, REFERENCES_ENDPOINT, SYMBOLS_ENDPOINT,
};
pub use code_context::retrieval::{
    clamp_limit, keyword_request_body, keyword_search, require_query, search, search_request_body,
    summarize_keyword_payload, summarize_search_payload, DEFAULT_RESULT_LIMIT, MAX_SNIPPET_CHARS,
    SERVICE_RESULT_CAP,
};
pub use code_context::{
    cce_server_command, prepare_managed_service, write_cce_server_config, CallScope,
    CodeContextAvailability, CodeContextConfig, CodeContextService, CodeContextStatus,
    DeadLetterAction, DeadLetterFile, DeadLetterList, DeadLetterModule, FoldPolicy,
    IndexSupplyMode, OperationError, ProjectPolicy, ProjectState, ProjectStatus,
    ReconfigureOutcome, RetrievalPolicy, SharedCodeContextService,
};
pub use transport::{
    breaker_for, get_json, http_client, origin_of, parse_health, pick_loopback_port, post_json,
    probe_health, register_service_credential, service_client, start_sidecar, BreakerPhase,
    BreakerSnapshot, CircuitBreaker, HealthReport, RunningSidecar, ServiceError, ServiceErrorKind,
    ServiceTransport, SidecarPhase, SidecarSpec, SidecarStatus, SidecarStatusHandle, TransportMode,
    REQUEST_ID_HEADER,
};
