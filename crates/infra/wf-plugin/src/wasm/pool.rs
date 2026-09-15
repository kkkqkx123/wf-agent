use std::collections::VecDeque;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use moka::sync::Cache;
use tokio::sync::Mutex;
use wasmtime::{
    Config, Engine, Instance, InstancePre, Linker, Memory, ResourceLimiter, Store, StoreLimits,
    StoreLimitsBuilder,
};
use wasmtime_wasi::p2::WasiCtxBuilder;
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wf_plugin_sdk::wasm::export;

use super::policy::{WasiGrants, WasmLimits};
use super::stats::WasmStats;
use super::stdio::{GuestLogPipe, GUEST_STDIO_CAP_BYTES};
use crate::error::{PluginError, PluginResult};

/// Per-call store state: WASI context plus the memory limiter.
///
/// A fresh `Store` is built for every guest call so concurrent invocations
/// never share linear memory. The `Module` and `Linker` are shared.
pub struct WasmStoreState {
    wasi: WasiP1Ctx,
    limits: StoreLimits,
    /// Owning plugin id, used to attribute guest-to-host log records.
    plugin_id: String,
    /// Guest `stdout` capture. Drained to the host log after every call so
    /// pooled sessions never accumulate output across calls.
    pub(crate) stdout: GuestLogPipe,
    /// Guest `stderr` capture, drained like `stdout`.
    pub(crate) stderr: GuestLogPipe,
}

/// Live guest instance plus its private store. When the pool is disabled
/// the session is dropped after each call so no linear memory is ever
/// shared between invocations; when pooling is enabled the session may be
/// returned to the pool after a successful heap reset.
pub struct PooledSession {
    pub store: Store<WasmStoreState>,
    pub instance: Instance,
    pub memory: Memory,
}

/// Build one fresh session: new store plus instantiation from the shared
/// pre-resolved imports. Linear memory is never shared by construction.
pub async fn new_session(
    engine: &Engine,
    pre: &InstancePre<WasmStoreState>,
    plugin_id: &str,
    grants: &WasiGrants,
    limits: &WasmLimits,
) -> PluginResult<PooledSession> {
    let mut store = build_store(engine, plugin_id, grants, limits)?;
    let instance = pre
        .instantiate_async(&mut store)
        .await
        .map_err(|e| wasm_err(&format!("plugin '{plugin_id}' instantiate failed"), e))?;
    let memory = instance
        .get_memory(&mut store, export::MEMORY)
        .ok_or_else(|| {
            PluginError::WasmError(format!(
                "plugin '{plugin_id}' does not export '{}'",
                export::MEMORY
            ))
        })?;
    Ok(PooledSession {
        store,
        instance,
        memory,
    })
}

/// Bounded pool of idle guest sessions for one plugin.
///
/// The pool only retains idle sessions; it never blocks acquisition.
/// When no idle session is available a fresh ephemeral session is built,
/// so concurrency bursts degrade to the unpooled path instead of queueing.
/// Sessions are only retained when the guest exports `wf_heap_reset`
/// (probed once at load time) and the manifest enables pooling; otherwise
/// every call builds a fresh session exactly as before.
pub struct SessionPool {
    plugin_id: String,
    engine: Engine,
    pre: InstancePre<WasmStoreState>,
    grants: WasiGrants,
    limits: WasmLimits,
    idle: Option<Mutex<VecDeque<PooledSession>>>,
    stats: Arc<WasmStats>,
}

impl SessionPool {
    /// Create the pool. The idle queue exists only when the manifest
    /// requests pooling (`pool_size > 0`) and the guest exports the
    /// heap-reset hook; otherwise acquisition always builds fresh sessions.
    pub fn new(
        plugin_id: &str,
        engine: &Engine,
        pre: &InstancePre<WasmStoreState>,
        grants: &WasiGrants,
        limits: &WasmLimits,
        stats: &Arc<WasmStats>,
        supports_reset: bool,
    ) -> Self {
        let idle = (limits.pool_size > 0 && supports_reset).then(|| {
            tracing::info!(
                "wasm plugin '{plugin_id}' session pool enabled (cap {})",
                limits.pool_size
            );
            Mutex::new(VecDeque::new())
        });
        Self {
            plugin_id: plugin_id.to_owned(),
            engine: engine.clone(),
            pre: pre.clone(),
            grants: grants.clone(),
            limits: limits.clone(),
            idle,
            stats: stats.clone(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.idle.is_some()
    }

    /// Check out a session: reuse an idle one when available, otherwise
    /// build a fresh session. Never blocks.
    pub async fn acquire(&self) -> PluginResult<PooledSession> {
        if let Some(idle) = &self.idle {
            if let Some(session) = idle.lock().await.pop_front() {
                self.stats.record_pool_hit();
                return Ok(session);
            }
            self.stats.record_pool_miss();
        }
        new_session(
            &self.engine,
            &self.pre,
            &self.plugin_id,
            &self.grants,
            &self.limits,
        )
        .await
    }

    /// Return a session after use. `healthy` must be true only when the
    /// last guest call completed successfully; sessions from failed calls
    /// are always discarded because post-trap linear memory is untrusted.
    /// A successful heap reset is required before the session rejoins the
    /// idle queue, otherwise it is dropped.
    pub async fn release(&self, mut session: PooledSession, healthy: bool) {
        let Some(idle) = &self.idle else {
            return;
        };
        if !healthy {
            self.stats.record_pool_drop();
            return;
        }
        if !run_reset(self, &mut session).await {
            self.stats.record_pool_drop();
            return;
        }
        let mut idle = idle.lock().await;
        if idle.len() >= self.limits.pool_size {
            self.stats.record_pool_drop();
            return;
        }
        idle.push_back(session);
    }
}

/// Run the guest `wf_heap_reset` export on a session being returned to the
/// pool. Fuel is replenished first so reset never starves on leftovers from
/// the previous call; the same epoch deadline and outer timeout apply, so
/// a runaway reset discards the session instead of stalling the host.
/// Returns true only when reset reports success (code 0).
async fn run_reset(pool: &SessionPool, session: &mut PooledSession) -> bool {
    let id = &pool.plugin_id;
    let reset = match session
        .instance
        .get_typed_func::<(), u32>(&mut session.store, export::HEAP_RESET)
    {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!("wasm plugin '{id}' heap reset lookup failed: {e}");
            return false;
        }
    };
    if let Err(e) = session
        .store
        .set_fuel(pool.limits.fuel_limit.unwrap_or(u64::MAX))
    {
        tracing::warn!("wasm plugin '{id}' heap reset fuel setup failed: {e}");
        return false;
    }
    arm_epoch(
        &pool.engine,
        &mut session.store,
        pool.limits.call_timeout_ms,
    );
    let call = reset.call_async(&mut session.store, ());
    let outcome = match outer_timeout_ms(pool.limits.call_timeout_ms) {
        Some(ms) => match tokio::time::timeout(Duration::from_millis(ms), call).await {
            Ok(Ok(code)) => Some(code),
            Ok(Err(e)) => {
                tracing::warn!("wasm plugin '{id}' heap reset trapped: {e:#}");
                None
            }
            Err(_) => {
                tracing::warn!("wasm plugin '{id}' heap reset timed out");
                None
            }
        },
        None => match call.await {
            Ok(code) => Some(code),
            Err(e) => {
                tracing::warn!("wasm plugin '{id}' heap reset trapped: {e:#}");
                None
            }
        },
    };
    match outcome {
        Some(0) => true,
        Some(code) => {
            tracing::warn!("wasm plugin '{id}' heap reset refused reuse (code {code})");
            false
        }
        None => false,
    }
}

/// Epoch tick interval driving wall-clock interruption.
pub const EPOCH_TICK_MS: u64 = 10;

/// Slack added to the outer backstop timeout on top of the epoch deadline.
const TIMEOUT_SLACK_MS: u64 = 5_000;

/// Process-wide shared engine. Engine construction compiles no guest code;
/// sharing it across plugins amortizes config cost and lets the file cache
/// (enabled below) serve every plugin.
fn shared_engine() -> Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut config = Config::new();
            config.async_support(true);
            config.consume_fuel(true);
            config.epoch_interruption(true);
            Engine::new(&config).expect("wasmtime engine construction")
        })
        .clone()
}

/// Shorten a wasmtime error (which may embed a full backtrace) for logs.
pub fn wasm_err(context: &str, err: impl std::fmt::Display) -> PluginError {
    let mut text = format!("{context}: {err}");
    if text.len() > 2000 {
        text.truncate(2000);
        text.push_str("...(truncated)");
    }
    PluginError::WasmError(text)
}

/// Linker pre-loaded with WASI imports plus the `wf_host` guest-to-host
/// namespace. Guests that do not import WASI or `wf_host` still
/// instantiate: unused imports are simply never resolved.
pub fn new_linker(engine: &Engine) -> PluginResult<Linker<WasmStoreState>> {
    let mut linker = Linker::new(engine);
    preview1::add_to_linker_async(&mut linker, |state: &mut WasmStoreState| &mut state.wasi)
        .map_err(|e| wasm_err("wasi linker setup failed", e))?;
    linker
        .func_wrap(
            wf_plugin_sdk::wasm::host::MODULE,
            wf_plugin_sdk::wasm::host::LOG,
            |mut caller: wasmtime::Caller<'_, WasmStoreState>, level: u32, ptr: u32, len: u32| {
                // Best-effort observability path: unreadable input is
                // dropped instead of trapping the guest call.
                let plugin_id = caller.data().plugin_id.clone();
                let len = (len as usize).min(super::stdio::HOST_LOG_MESSAGE_CAP_BYTES);
                if len == 0 {
                    return;
                }
                let mut buf = vec![0u8; len];
                let read = caller
                    .get_export(export::MEMORY)
                    .and_then(|extern_ref| extern_ref.into_memory())
                    .is_some_and(|memory| memory.read(&mut caller, ptr as usize, &mut buf).is_ok());
                if read {
                    super::stdio::emit_host_log(&plugin_id, level, &buf);
                }
            },
        )
        .map_err(|e| wasm_err("host log linker setup failed", e))?;
    Ok(linker)
}

/// Build a per-call store: WASI context from grants, fuel budget and memory
/// cap from limits. Guest `stdout`/`stderr` are captured to bounded pipes
/// and drained to the host log after each call. Missing preopen directories
/// are skipped with a warning so one stale grant does not break every call.
pub fn build_store(
    engine: &Engine,
    plugin_id: &str,
    grants: &WasiGrants,
    limits: &WasmLimits,
) -> PluginResult<Store<WasmStoreState>> {
    let stdout = GuestLogPipe::new(GUEST_STDIO_CAP_BYTES);
    let stderr = GuestLogPipe::new(GUEST_STDIO_CAP_BYTES);
    let mut builder = WasiCtxBuilder::new();
    builder.stdout(stdout.clone()).stderr(stderr.clone());
    for (key, value) in &grants.env_vars {
        builder.env(key, value);
    }
    for (host_path, guest_path) in &grants.preopened_dirs {
        if !std::path::Path::new(host_path).is_dir() {
            tracing::warn!(
                "wasm plugin '{plugin_id}' preopen skipped, not a directory: {host_path}"
            );
            continue;
        }
        if let Err(e) = builder.preopened_dir(
            host_path,
            guest_path,
            wasmtime_wasi::DirPerms::READ,
            wasmtime_wasi::FilePerms::READ,
        ) {
            tracing::warn!("wasm plugin '{plugin_id}' preopen failed for '{host_path}': {e}");
        }
    }
    let wasi = builder.build_p1();
    let mut store = Store::new(
        engine,
        WasmStoreState {
            wasi,
            limits: StoreLimitsBuilder::new()
                .memory_size(limits.memory_max_bytes as usize)
                .build(),
            plugin_id: plugin_id.to_owned(),
            stdout,
            stderr,
        },
    );
    store.limiter(|state: &mut WasmStoreState| &mut state.limits as &mut dyn ResourceLimiter);
    // A fresh store starts with zero fuel and would trap immediately, so an
    // unbounded budget is expressed as fuel that cannot be exhausted.
    store
        .set_fuel(limits.fuel_limit.unwrap_or(u64::MAX))
        .map_err(|e| wasm_err("fuel setup failed", e))?;
    Ok(store)
}

/// Ensure the global epoch ticker is running. The ticker lives on a
/// dedicated OS thread (not a tokio task) so it keeps advancing even when a
/// spinning guest occupies every async worker or the runtime is
/// single-threaded: without this, a `#[tokio::test]`-style current-thread
/// runtime would starve a tokio-based ticker and epoch deadlines would
/// never fire.
///
/// Deadlines are relative (`ticks_beyond_current`), so a constantly
/// advancing clock is harmless to stores without a deadline.
fn ensure_epoch_ticker(engine: &Engine) {
    static TICKER: OnceLock<()> = OnceLock::new();
    TICKER.get_or_init(|| {
        let ticker_engine = engine.clone();
        std::thread::Builder::new()
            .name("wf-wasm-epoch".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_millis(EPOCH_TICK_MS));
                ticker_engine.increment_epoch();
            })
            .expect("wasm epoch ticker thread spawns");
    });
}

/// Set a relative epoch deadline on any store. `None` timeout means no
/// deadline; the caller still gets fuel metering and the outer
/// `PluginGuard` timeout. Generic over the store state so both the
/// core-module and component-model paths share one ticker.
pub fn arm_epoch<T>(engine: &Engine, store: &mut Store<T>, timeout_ms: Option<u64>) {
    let Some(timeout_ms) = timeout_ms else {
        return;
    };
    ensure_epoch_ticker(engine);
    let ticks = timeout_ms.div_ceil(EPOCH_TICK_MS).max(1);
    store.set_epoch_deadline(ticks);
}

/// Outer backstop for a guest call: the epoch deadline should always fire
/// first inside the guest; this only guards against host-side stalls.
pub fn outer_timeout_ms(call_timeout_ms: Option<u64>) -> Option<u64> {
    call_timeout_ms.map(|ms| ms.saturating_add(TIMEOUT_SLACK_MS))
}

/// Shared engine accessor for the loader and tests.
pub fn engine_handle() -> Engine {
    shared_engine()
}

/// Maximum compiled artifacts retained per cache. Each entry holds machine
/// code for one distinct module/component (up to tens of MiB), so the cap
/// keeps long-running hosts from growing without bound. Eviction follows
/// the cache's approximate-LRU policy; evicted bytes recompile on demand.
pub const MAX_CACHED_ARTIFACTS: u64 = 32;

fn module_cache() -> &'static Cache<String, wasmtime::Module> {
    static CACHE: OnceLock<Cache<String, wasmtime::Module>> = OnceLock::new();
    CACHE.get_or_init(|| Cache::builder().max_capacity(MAX_CACHED_ARTIFACTS).build())
}

fn compile_count() -> &'static std::sync::atomic::AtomicU64 {
    static COUNT: OnceLock<std::sync::atomic::AtomicU64> = OnceLock::new();
    COUNT.get_or_init(|| std::sync::atomic::AtomicU64::new(0))
}

/// Compile `bytes` to a `Module`, reusing a content-keyed bounded cache so
/// reloads of an unchanged plugin skip recompilation. Keyed by the blake3
/// digest of the exact bytes, so a key hit always means identical code.
pub fn cached_module(engine: &Engine, bytes: &[u8]) -> PluginResult<wasmtime::Module> {
    let digest = blake3::hash(bytes).to_hex().to_string();
    if let Some(module) = module_cache().get(&digest) {
        return Ok(module);
    }
    let module = wasmtime::Module::new(engine, bytes)
        .map_err(|e| wasm_err("wasm module compile failed", e))?;
    compile_count().fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    module_cache().insert(digest, module.clone());
    Ok(module)
}

#[cfg(test)]
pub fn compile_count_value() -> u64 {
    compile_count().load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_applies_fuel_limit() {
        let engine = engine_handle();
        let limits = WasmLimits {
            memory_max_bytes: 64 * 1024 * 1024,
            fuel_limit: Some(100),
            call_timeout_ms: None,
            max_module_bytes: 1024,
            pool_size: 0,
        };
        let store =
            build_store(&engine, "test", &WasiGrants::default(), &limits).expect("store builds");
        assert_eq!(store.get_fuel().expect("fuel readable"), 100);
    }

    #[test]
    fn module_cache_is_bounded() {
        let engine = engine_handle();
        for i in 0..(MAX_CACHED_ARTIFACTS + 8) {
            let wat = format!("(module (memory 1) (data (i32.const 0) \"bound{i}\"))");
            let bytes = wat::parse_str(&wat).expect("valid wat");
            cached_module(&engine, &bytes).expect("compile");
        }
        module_cache().run_pending_tasks();
        assert!(
            module_cache().entry_count() <= MAX_CACHED_ARTIFACTS,
            "cache holds {} entries, cap is {MAX_CACHED_ARTIFACTS}",
            module_cache().entry_count()
        );
    }

    /// Guest writes to fd 1/2 via WASI `fd_write` must land in the host
    /// capture pipes instead of vanishing.
    #[tokio::test]
    async fn guest_stdout_and_stderr_are_captured() {
        let wat = r#"(module
  (import "wasi_snapshot_preview1" "fd_write"
    (func $fd_write (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 128))
  (data (i32.const 64) "hi-stdout")
  (data (i32.const 80) "hi-stderr")
  (func (export "alloc") (param $n i32) (result i32)
    (local $p i32) (global.get $heap) (local.set $p)
    (global.set $heap (i32.add (global.get $heap) (local.get $n)))
    (local.get $p))
  (func $write (param $fd i32) (param $ptr i32) (param $len i32)
    (i32.store (i32.const 16) (local.get $ptr))
    (i32.store (i32.const 20) (local.get $len))
    (drop (call $fd_write (local.get $fd) (i32.const 16) (i32.const 1) (i32.const 24))))
  (func (export "wf_on_load") (param $p i32) (param $n i32) (result i32)
    (call $write (i32.const 1) (i32.const 64) (i32.const 9))
    (call $write (i32.const 2) (i32.const 80) (i32.const 9))
    (i32.const 0)))
"#;
        let bytes = wat::parse_str(wat).expect("valid wat");
        let engine = engine_handle();
        let module = cached_module(&engine, &bytes).expect("compile");
        let linker = new_linker(&engine).expect("linker");
        let pre = linker.instantiate_pre(&module).expect("pre-instantiate");
        let limits = WasmLimits {
            memory_max_bytes: 64 * 1024 * 1024,
            fuel_limit: Some(1_000_000),
            call_timeout_ms: Some(10_000),
            max_module_bytes: 1024 * 1024,
            pool_size: 0,
        };
        let mut session = new_session(
            &engine,
            &pre,
            "stdio-probe",
            &WasiGrants::default(),
            &limits,
        )
        .await
        .expect("session");
        let hook = session
            .instance
            .get_typed_func::<(u32, u32), u32>(&mut session.store, export::ON_LOAD)
            .expect("hook");
        arm_epoch(&engine, &mut session.store, limits.call_timeout_ms);
        let code = hook
            .call_async(&mut session.store, (0, 0))
            .await
            .expect("hook runs");
        assert_eq!(code, 0);
        let state = session.store.data();
        assert_eq!(state.stdout.take_contents(), b"hi-stdout");
        assert_eq!(state.stderr.take_contents(), b"hi-stderr");
    }
}
