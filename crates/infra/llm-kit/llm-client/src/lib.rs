// Transport layer: client trait and HTTP implementation, stream accumulation,
// dead-loop guard, generic usage sink and feature-gated mocks.
pub mod client;
pub mod dead_loop_detector;
pub mod stream;
pub mod token_stream;

#[cfg(feature = "mock")]
pub mod http_mock;
#[cfg(feature = "mock")]
pub mod mock;

pub use client::LlmClient;
pub use dead_loop_detector::{DeadLoopDetectionResult, DeadLoopDetector, DeadLoopDetectorConfig};
pub use stream::MessageStream;
pub use token_stream::{
    LlmMetricsSink, SharedLlmMetricsSink, SharedTokenUsageSink, TokenRecordingStream, TokenUsageSink,
};
#[cfg(feature = "mock")]
pub use mock::{LlmResponseSpec, MockLlmClient, MockMessageStream};
