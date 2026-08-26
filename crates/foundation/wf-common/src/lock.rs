//! Poisoned-lock recovery helpers.
//!
//! # Why recover instead of panicking
//!
//! `std::sync` locks become poisoned when a thread panics while holding the
//! lock. Poisoning is a *signal* that a panic escaped a critical section; it
//! does not by itself corrupt the guarded data. In this codebase every
//! critical section performs a single atomic container operation (one
//! `insert` / `remove` / `push` / `pop` / `assign` or a read), so a panic
//! inside the section cannot leave the structure in an invalid state — the
//! panic originates from user code within the section and only unwinds
//! through the guard.
//!
//! Panicking at every subsequent lock site would turn one unrelated panic
//! into a crash of the whole service. Recovering the guard keeps the service
//! available, and the incident stays observable: the original panic is
//! reported by the panic hook, and every recovery logs a warning.
//!
//! # When this is NOT appropriate
//!
//! Recovery is only sound when the guarded data remains self-consistent,
//! i.e. when the critical section is panic-atomic (a single operation, no
//! multi-step invariants). For multi-step mutations under one lock, propagate
//! the error or panic instead — recovering would continue with a torn state.

use std::sync::{LockResult, MutexGuard, RwLockReadGuard, RwLockWriteGuard};

/// Acquire a `std::sync::Mutex` guard, recovering from a poisoned mutex
/// instead of panicking.
pub fn lock_ok<T>(result: LockResult<MutexGuard<'_, T>>) -> MutexGuard<'_, T> {
    result.unwrap_or_else(|poisoned| {
        tracing::warn!("mutex was poisoned; recovering the guard");
        poisoned.into_inner()
    })
}

/// Acquire a `std::sync::RwLock` read guard, recovering from a poisoned lock
/// instead of panicking.
pub fn read_ok<T>(result: LockResult<RwLockReadGuard<'_, T>>) -> RwLockReadGuard<'_, T> {
    result.unwrap_or_else(|poisoned| {
        tracing::warn!("rwlock was poisoned; recovering the read guard");
        poisoned.into_inner()
    })
}

/// Acquire a `std::sync::RwLock` write guard, recovering from a poisoned lock
/// instead of panicking.
pub fn write_ok<T>(result: LockResult<RwLockWriteGuard<'_, T>>) -> RwLockWriteGuard<'_, T> {
    result.unwrap_or_else(|poisoned| {
        tracing::warn!("rwlock was poisoned; recovering the write guard");
        poisoned.into_inner()
    })
}
