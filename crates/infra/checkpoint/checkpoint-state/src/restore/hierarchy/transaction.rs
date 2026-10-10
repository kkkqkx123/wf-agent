//! Multi-operation recovery transaction with LIFO compensating actions.

use checkpoint_base::error::CheckpointError;

pub struct RecoveryTransaction {
    operations: Vec<RecoveryOperation>,
    rollback_strategy: RollbackStrategy,
    /// Compensating actions executed LIFO on rollback
    /// (`addCompensatingAction`).
    compensating_actions: Vec<Box<dyn Fn() -> Result<(), String> + Send + Sync>>,
    status: RecoveryTransactionStatus,
    rolled_back: bool,
}

impl RecoveryTransaction {
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
            rollback_strategy: RollbackStrategy::AllOrNothing,
            compensating_actions: Vec::new(),
            status: RecoveryTransactionStatus::Pending,
            rolled_back: false,
        }
    }

    pub fn with_rollback_strategy(strategy: RollbackStrategy) -> Self {
        Self {
            operations: Vec::new(),
            rollback_strategy: strategy,
            compensating_actions: Vec::new(),
            status: RecoveryTransactionStatus::Pending,
            rolled_back: false,
        }
    }

    /// Transition the transaction into the in-progress state.
    pub fn begin(&mut self) {
        self.status = RecoveryTransactionStatus::InProgress;
    }

    pub fn register(&mut self, operation: RecoveryOperation) {
        self.operations.push(operation);
    }

    /// Register a compensating action executed (LIFO) during rollback.
    pub fn add_compensating_action(
        &mut self,
        action: Box<dyn Fn() -> Result<(), String> + Send + Sync>,
    ) {
        self.compensating_actions.push(action);
    }

    /// Execute all pending operations through the provided executor.
    /// Successful operations are marked `Completed`; failed ones are marked
    /// `Failed` and, under `AllOrNothing`, previously completed operations are
    /// rolled back.
    pub async fn execute<F, Fut>(&mut self, mut executor: F) -> Result<(), CheckpointError>
    where
        F: FnMut(&RecoveryOperation) -> Fut,
        Fut: std::future::Future<Output = Result<(), CheckpointError>>,
    {
        self.begin();
        for operation in self.operations.iter_mut() {
            if operation.status != RecoveryOperationStatus::Pending {
                continue;
            }
            match executor(operation).await {
                Ok(()) => {
                    operation.status = RecoveryOperationStatus::Completed;
                }
                Err(e) => {
                    operation.status = RecoveryOperationStatus::Failed(e.to_string());
                    if self.rollback_strategy == RollbackStrategy::AllOrNothing {
                        self.rollback();
                        return Err(CheckpointError::Coordinator(format!(
                            "recovery transaction failed: {}",
                            e
                        )));
                    }
                }
            }
        }
        if self.status == RecoveryTransactionStatus::InProgress {
            self.status = RecoveryTransactionStatus::Completed;
        }
        Ok(())
    }

    /// Mark the operation at `index` as completed.
    pub fn complete(&mut self, index: usize) {
        if let Some(operation) = self.operations.get_mut(index) {
            operation.status = RecoveryOperationStatus::Completed;
        }
    }

    /// Mark the operation at `index` as failed with the given error message.
    pub fn fail(&mut self, index: usize, error: impl Into<String>) {
        if let Some(operation) = self.operations.get_mut(index) {
            operation.status = RecoveryOperationStatus::Failed(error.into());
        }
    }

    /// Commit the transaction: with `AllOrNothing`, any failed operation
    /// triggers a rollback; otherwise the transaction commits with partial
    /// success.
    pub fn commit(&mut self) -> RecoveryTransactionResult {
        let has_failed = self
            .operations
            .iter()
            .any(|op| matches!(op.status, RecoveryOperationStatus::Failed(_)));
        match (self.rollback_strategy, has_failed) {
            (RollbackStrategy::AllOrNothing, true) => self.rollback(),
            _ => {
                self.status = RecoveryTransactionStatus::Completed;
                RecoveryTransactionResult {
                    status: RecoveryTransactionStatus::Completed,
                    errors: Vec::new(),
                }
            }
        }
    }

    /// Rollback the transaction: only executed operations are marked failed,
    /// pending operations stay pending, and the registered compensating
    /// actions run LIFO once. Repeat rollbacks report an explicit error
    /// instead of running compensations twice.
    pub fn rollback(&mut self) -> RecoveryTransactionResult {
        if self.rolled_back {
            return RecoveryTransactionResult {
                status: self.status.clone(),
                errors: vec!["transaction already rolled back".to_string()],
            };
        }
        self.rolled_back = true;
        for operation in self.operations.iter_mut() {
            match operation.status {
                RecoveryOperationStatus::Pending => {}
                RecoveryOperationStatus::Completed | RecoveryOperationStatus::Failed(_) => {
                    operation.status = RecoveryOperationStatus::Failed("rolled back".to_string());
                }
            }
        }
        let mut errors = Vec::new();
        for action in self.compensating_actions.drain(..).rev() {
            if let Err(err) = action() {
                errors.push(err);
            }
        }
        self.status = if errors.is_empty() {
            RecoveryTransactionStatus::RolledBack
        } else {
            RecoveryTransactionStatus::RolledBackWithErrors
        };
        RecoveryTransactionResult {
            status: self.status.clone(),
            errors,
        }
    }

    pub fn operations(&self) -> &[RecoveryOperation] {
        &self.operations
    }

    pub fn status(&self) -> &RecoveryTransactionStatus {
        &self.status
    }

    pub fn len(&self) -> usize {
        self.operations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// Number of operations that completed successfully.
    pub fn completed_count(&self) -> usize {
        self.operations
            .iter()
            .filter(|op| op.status == RecoveryOperationStatus::Completed)
            .count()
    }

    /// Number of operations that failed.
    pub fn failed_count(&self) -> usize {
        self.operations
            .iter()
            .filter(|op| matches!(op.status, RecoveryOperationStatus::Failed(_)))
            .count()
    }

    pub fn rollback_strategy(&self) -> &RollbackStrategy {
        &self.rollback_strategy
    }
}

/// Result of a commit/rollback.
#[derive(Debug, Clone)]
pub struct RecoveryTransactionResult {
    pub status: RecoveryTransactionStatus,
    pub errors: Vec<String>,
}

/// Transaction lifecycle.
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryTransactionStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    RolledBack,
    RolledBackWithErrors,
}

impl Default for RecoveryTransaction {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct RecoveryOperation {
    pub checkpoint_id: String,
    pub operation_type: RecoveryOperationType,
    pub status: RecoveryOperationStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryOperationType {
    Restore,
    Delete,
    Reconstruct,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryOperationStatus {
    Pending,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RollbackStrategy {
    AllOrNothing,
    BestEffort,
}
