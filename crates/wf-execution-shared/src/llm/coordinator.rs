use wf_llm::LlmWrapper;
use wf_types::llm::LlmRequest;

use crate::error::ExecutionSharedResult;

pub struct LlmExecutionCoordinator {
    llm_wrapper: std::sync::Arc<LlmWrapper>,
}

impl LlmExecutionCoordinator {
    pub fn new(llm_wrapper: std::sync::Arc<LlmWrapper>) -> Self {
        Self { llm_wrapper }
    }

    pub async fn execute_llm_call(
        &self,
        request: LlmRequest,
    ) -> ExecutionSharedResult<wf_types::llm::LlmResult> {
        self.llm_wrapper.generate(&request).await.map_err(Into::into)
    }

    pub fn llm_wrapper(&self) -> &LlmWrapper {
        &self.llm_wrapper
    }
}
