# wf-storage Crate Issues

Analysis date: 2026-07-27

## High Priority (Correctness)

### H1: `save_batch` silently drops serialization errors
- **File:** `crates/wf-storage/src/store/entity_store.rs:122-124`
- **Issue:** Uses `unwrap_or_default()` on `serde_json::to_value()` and `to_bytes()`. If serialization fails, the entity is stored with empty data and no metadata — no error propagated.
- **Fix:** Replace with `?` operator to propagate errors.

### H2: `delete` always returns `Ok(true)` even when entity doesn't exist
- **File:** `crates/wf-storage/src/adapter/macro.rs:60-63`
- **Issue:** The `make_base_adapter!` macro bridges `Store::delete` (returns `Result<()>`) to `BaseStorageAdapter::delete` (returns `Result<bool>`) by discarding the inner result and returning `Ok(true)` unconditionally.
- **Fix:** Track existence before deletion or use a lower-level store that returns deletion count.

### H3: `wf-common` dependency unused; `StorageError` duplicates `CommonError`
- **File:** `crates/wf-storage/Cargo.toml` + `crates/wf-storage/src/error.rs`
- **Issue:** `wf-common` is declared as a dependency but never imported. `StorageError` defines variants (`NotFound`, `Internal`, `Serialization`, `Io`) that overlap with `CommonError` from `wf-common`.
- **Fix:** Either use `CommonError` as the error foundation (wrap it in `StorageError`), or remove the `wf-common` dependency from `Cargo.toml`.

---

## Medium Priority (Design/Consistency)

### M1: `list()` has N+1 query pattern
- **File:** `crates/wf-storage/src/store/entity_store.rs:80-88`
- **Issue:** `list()` calls `storage.list()` to get IDs, then `self.load(&id)` for each — 2N queries total.
- **Fix:** Add a `load_data()` method to `Store` that returns `(id, data, metadata)` in one query, or use a bulk load path.

### M2: `data_size` column type mismatch between SQLite and Postgres
- **File:** `crates/wf-storage/src/store/sqlite.rs:146` vs `crates/wf-storage/src/store/postgres.rs:112`
- **Issue:** SQLite uses `i64` for `data_size`, Postgres uses `i32`. Risk of truncation for data > 2GB and schema inconsistency.
- **Fix:** Use `i64` consistently across both backends.

### M3: `AgentProfileStorage` uses raw `ListOptions`
- **File:** `crates/wf-storage/src/adapter/concrete.rs:42`
- **Issue:** All 10 other adapters have dedicated `*ListOptions` structs with entity-specific filter fields. `AgentProfile` uses the base `ListOptions` (only `offset`/`limit`), preventing profile-specific filtering.
- **Fix:** Create `AgentProfileListOptions` with domain-specific fields.

### M4: `MetricsStorage` bypasses `make_base_adapter!` macro
- **File:** `crates/wf-storage/src/adapter/concrete.rs:433-503`
- **Issue:** `MetricsStorage` is a standalone struct with manual `new()`, `inner()`, and hand-written `MetricsStorageAdapter` impl. It doesn't get `initialize()`, `close()`, `save_batch()` from `BaseStorageAdapter`.
- **Fix:** Either refactor to use the macro, or document why it's intentionally excluded.

### M5: `MetricsStorage::query` filters timestamps in memory
- **File:** `crates/wf-storage/src/adapter/concrete.rs:475-488`
- **Issue:** Queries by `metricName`, then loads each entry and filters by timestamp range in Rust. Inefficient for large datasets.
- **Fix:** Push timestamp range filter down to SQL query.

### M6: `exists()` and `clear()` not instrumented
- **File:** `crates/wf-storage/src/decorator/instrumented.rs:124-131`
- **Issue:** `save`/`load`/`delete`/`list` are all instrumented with timing and byte counts, but `exists()` and `clear()` delegate directly to inner store without metrics.
- **Fix:** Add instrumentation for these two methods.

### M7: `sqlx::Error::RowNotFound` not mapped to `NotFound`
- **File:** `crates/wf-storage/src/error.rs:66-73`
- **Issue:** All `sqlx::Error` variants are flattened into `StorageError::General`. `RowNotFound` should map to `NotFound` for better ergonomics.
- **Fix:** Add specific mapping for `sqlx::Error::RowNotFound`.

---

## Low Priority (Dead Code / Minor)

### L1: `should_compress` never called
- **File:** `crates/wf-storage/src/domain/entity.rs:21-23`
- **Issue:** The trait method exists but is never invoked. Actual threshold is hardcoded in `util/compression.rs:39` (`data_len < 1024`).
- **Fix:** Remove the method or refactor compression logic to use it.

### L2: `SharedEntityStore` type alias unused
- **File:** `crates/wf-storage/src/store/entity_store.rs:149`
- **Fix:** Remove or use it.

### L3: `name` field dead code in `MemoryStorage`
- **File:** `crates/wf-storage/src/store/memory.rs:34-35`
- **Fix:** Remove the field or use it (e.g., in Debug impl).

### L4: `checkpoint()` not implemented for Postgres
- **File:** `crates/wf-storage/src/store/postgres.rs:371-387`
- **Issue:** SQLite has a real `checkpoint()` implementation; Postgres inherits the no-op default.
- **Fix:** Implement with `CHECKPOINT` command or document as SQLite-specific.

### L5: `get_default` returns arbitrary first item
- **File:** `crates/wf-storage/src/adapter/concrete.rs:423-428`
- **Issue:** Method name implies business logic ("default") but implementation returns `all.into_iter().next()`.
- **Fix:** Rename to `get_first` or implement actual default-selection logic.

### L6: Enum metadata uses `format!("{:?}", ...)` instead of serde
- **File:** `crates/wf-storage/src/entity_impl.rs:19`
- **Issue:** Fragile — if `Debug` output changes, metadata changes. Should use serde serialization for consistency.
- **Fix:** Use `serde_json::to_value()` or implement `Serialize` for the enum.

### L7: `verify_integrity` passes empty `id` on error
- **File:** `crates/wf-storage/src/util/hash.rs:25`
- **Fix:** Pass the actual id to `StorageError::Integrity`.

### L8: `*StorageConfig` types defined but unused
- **File:** `crates/wf-types/src/storage/*.rs` (all 12 modules)
- **Issue:** Each file defines a `*StorageConfig` struct that is never referenced anywhere.
- **Fix:** Remove or document as reserved for future use.

### L9: No cross-backend integration tests
- **File:** `crates/wf-storage/tests/adapter_tests.rs`
- **Issue:** All tests use `MemoryStorage`. No integration tests exercise `SqliteStorage` or `PostgresStorage`.
- **Fix:** Add at least one cross-backend test parameterized by storage type.

### L10: `StorageMetrics` not re-exported from decorator module
- **File:** `crates/wf-storage/src/decorator.rs:4-6`
- **Issue:** Users must import `wf_storage::decorator::instrumented::StorageMetrics` directly.
- **Fix:** Add re-export in `decorator.rs`.
