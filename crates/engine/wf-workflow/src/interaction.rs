//! Re-export of the shared interaction mechanism from
//! `wf_execution_shared::interaction`. The registry, wait channel and
//! process-wide singleton now live in the shared execution-infrastructure
//! crate; this module keeps the historical public path stable for the
//! handler-side and application-side consumers.

pub use wf_execution_shared::interaction::{
    complete_interaction, interaction_registry, register_interaction, remove_interaction,
    InteractionRegistry, InteractionWait,
};

// The dev-dependency entry enables `wf-execution-shared/test-util`, so the
// test-only registry helpers are visible while this crate's tests compile.
#[cfg(test)]
pub(crate) use wf_execution_shared::interaction::acquire_registry_test_lock;
