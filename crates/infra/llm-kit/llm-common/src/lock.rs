use std::sync::{LockResult, MutexGuard};

/// Acquire a `std::sync::Mutex` guard, recovering from a poisoned mutex
/// instead of panicking.
///
/// Every critical section guarded this way performs a single atomic container
/// operation, so a panic inside it cannot leave the structure inconsistent;
/// recovering the guard keeps the service available while the original panic
/// stays observable through the panic hook and the error log below.
pub fn lock_ok<T>(result: LockResult<MutexGuard<'_, T>>) -> MutexGuard<'_, T> {
    result.unwrap_or_else(|poisoned| {
        tracing::error!("mutex was poisoned; recovering the guard");
        poisoned.into_inner()
    })
}
