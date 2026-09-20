# wf-storage

Entity persistence for the agent framework: a key-value store with JSON
metadata, filtered queries, bulk batches, and cross-entity atomic writes,
backed by SQLite, PostgreSQL, or an in-memory store.

## Layout

```text
src/
  lib.rs            Module declarations
  domain/           Shared vocabulary: Store / StoreExt / Maintainable traits,
                    QueryFilter and its compiled plan, entity traits, schema keys
  store/            Backends: memory, sqlite, postgres, entity_store
  decorator/        Cross-cutting wrappers: cache, instrumented
  adapter/          Typed per-entity adapters built on EntityStore
  backend.rs        Single-table StorageBackend enum
  context.rs        Multi-entity StorageContext sharing one pool
  error.rs          StorageError
  util/             Hashing, compression, pool creation, maintenance
```

## Traits

- `Store`: single-key `save` / `load` / `delete` / `exists` / `clear` plus
  filtered `list` / `list_data` / `count`. `list_data` has an N+1 default;
  every production backend overrides it with a single query.
- `StoreExt`: `apply_batch` (mixed atomic save/delete),
  `save_batch` / `load_batch` / `delete_batch` (homogeneous bulk),
  `update_status` (metadata-only status write), `count_by_field` (grouped
  counting with a `GROUP BY` override on SQL backends).
- `Maintainable`: `vacuum` / `wal_checkpoint` / `sync`. No-op by default;
  only backends with real maintenance override it.

`EntityStore<S, T>` sits above `Store`: it serializes entities, applies
gzip compression above a threshold, merges the `entityType` marker and
`compressed` flag into metadata, and skips (counting) corrupted records on
list. One `encode_item` helper serves both single and batch saves.

## Backends

All backends store `(id, data, metadata, hash, data_size, compressed,
created_at, updated_at)`. Every payload carries a full-stream SHA-256 hash
verified on read; `created_at` is preserved across overwrites via upsert
semantics. One internal `__schema_version__:<table>` row per table guards
schema compatibility; application queries always exclude it.

| Backend            | Use for                        | Notes                                            |
| ------------------ | ------------------------------ | ------------------------------------------------ |
| `SqliteStorage`    | Default durable deployment     | WAL mode, shared pool per context, chunked bulk  |
| `PostgresStorage`  | Multi-process durable deploy   | `UNNEST` bulk loads, per-table vacuum            |
| `MemoryStorage`    | One-off debugging and tests    | No durability; see below                         |

### Memory backend responsibilities

`MemoryStorage` exists for one-off debugging and unit tests. It implements
the full `Store` + `StoreExt` contract with identical query semantics
(text-form equality, numeric-only range predicates, numeric-first ordering,
last-wins ordering/pagination, pagination-free counting) so tests written
against memory also hold on SQLite and PostgreSQL.

It deliberately provides no advanced features: no read cache (it already
lives in memory), no maintenance operations (the `Maintainable` defaults
apply), no connection pool, and no durability across restarts. Cross-store
atomic batches lock the involved stores in registry order instead of using
transactions. Do not add caching, vacuuming, or persistence to it; keep it
a small, predictable reference implementation.

## Query semantics

`QueryFilter` collects operations; `compile()` normalizes them into a
`CompiledFilter` (conditions in order, last ordering/pagination wins) that
both SQL renderers and the memory backend consume, so interpretation exists
once. Semantics are equal on all backends:

- `Eq` / `Prefix` / `In` compare the metadata text form: strings as-is,
  numbers in canonical decimal form, booleans as `true` / `false`.
- `Lt` / `Gt` / `Between` match JSON numbers only (float-aware); missing
  keys and non-numeric values never match.
- `OrderBy` sorts numbers numerically first in both directions
  (`NULLS LAST`); ties break by ascending id for stable pagination.
- `count` ignores ordering and pagination and reports total matches.
- Prefix matches use escaped `LIKE ... ESCAPE '\'` patterns, so `%` and
  `_` inside a prefix stay literal.

## Decorators and context

- `CachingStore`: single-key read cache for durable backends. `load` and
  `exists` may be served from memory; `list` / `count` pass through.
  Every write path invalidates the affected ids; `load_batch` serves
  cached rows and fetches only the misses, returning input order.
- `InstrumentedStore`: per-operation counters, latency, and byte totals
  (`save` / `load` / `delete` / `list` / `exists` / `clear` / `batch`).
  Aggregate reads (`count`, `count_by_field`) report under `list`;
  single-key writes (`update_status`) under `save`.
- `StorageBackend`: one table with instrumentation (plus cache for durable
  variants). Standalone; never joins cross-entity batches.
- `StorageContext`: all entity stores sharing one pool (SQLite /
  PostgreSQL) or lock-ordered memory partitions. `apply_atomic` runs
  multi-entity batches in one transaction (one batch observation per
  involved backend, cache invalidated only after commit); `clear_all`
  empties every registered store so new entities cannot be missed.

## Maintenance and indexes

SQLite opens with WAL, `synchronous = NORMAL`, and a busy timeout; a
background service runs `PRAGMA optimize` periodically (`ANALYZE` on
PostgreSQL). Expression indexes cover `entityType`, `status`,
`executionId`, `entityId`, and `timestamp`; SQLite rebuilds the first four
once if they predate boolean normalization.

## Testing

```shell
cargo test -p wf-storage
```

Unit tests live next to each backend (filter semantics, batch atomicity,
prefix escaping, pagination-free counting, chunked bulk transfers);
`tests/` holds adapter coverage plus cross-entity atomic-batch checks on
memory and SQLite. PostgreSQL SQL rendering is covered by offline
`build_select_sql` unit tests; live PostgreSQL tests need a database URL.
