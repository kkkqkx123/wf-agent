pub mod orchestrator;
pub mod scanner;

pub use orchestrator::RecoveryOrchestrator;
pub use scanner::RecoveryScanner;

pub struct RecoveryResult {
    pub recovered: Vec<RecoveryItem>,
    pub failed: Vec<(String, String)>,
}

pub struct RecoveryItem {
    pub execution_id: String,
    pub status: String,
    pub current_node_id: Option<String>,
}
