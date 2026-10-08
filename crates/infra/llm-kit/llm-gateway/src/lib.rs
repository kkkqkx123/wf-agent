// Gateway orchestration: mandatory profile resolution, request merging,
// client caching, mock routing and metrics hooks.
pub mod gateway;

pub use gateway::LlmGateway;
