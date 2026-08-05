pub mod base;
pub mod builtin;
pub mod cli;
pub mod mcp;
pub mod remote;
pub mod rest;
pub mod stateful;
pub mod stateless;
pub mod trait_def;

pub use base::BaseExecutor;
pub use builtin::BuiltinExecutor;
pub use cli::{
    CliExecutionOptions, CliExecutionResult, CliExecutor, CliToolExecutor, ExecutorConfig,
    ExecutorInfo, ExecutorStatus, RipgrepExecutor,
};
pub use mcp::McpExecutor;
#[cfg(feature = "remote-layertwine")]
pub use remote::{
    register_layertwine_tools, LayertwineDeployMode, LayertwineExecutor, LayertwineExecutorConfig,
};
pub use remote::{
    ReconnectPolicy, RemoteConnectionConfig, RemoteErrorInfo, RemoteExecutionResult,
    RemoteExecutor, RemoteExecutorStatus,
};
pub use rest::{
    build_full_url, classify_status, ErrorInterceptor, RequestInterceptor, ResponseInterceptor,
    RestErrorKind, RestExecutor, RestRequestSpec,
};
pub use stateful::{InstanceFactory, StatefulExecutor, StatefulInstance};
pub use stateless::{StatelessAsyncHandler, StatelessExecutor, StatelessHandler};
pub use trait_def::{ToolExecutor, ToolExecutorExt};
