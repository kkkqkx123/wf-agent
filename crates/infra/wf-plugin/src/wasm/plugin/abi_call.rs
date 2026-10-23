use super::*;

pub(crate) fn guest_call_error(
    inner: &WasmPluginInner,
    export_name: &str,
    err: wasmtime::Error,
) -> PluginError {
    let id = &inner.manifest.id;
    if let Some(trap) = err.downcast_ref::<wasmtime::Trap>() {
        if matches!(trap, wasmtime::Trap::Interrupt) {
            return PluginError::Timeout {
                plugin_id: id.clone(),
            };
        }
        if matches!(trap, wasmtime::Trap::OutOfFuel) {
            return PluginError::WasmError(format!(
                "plugin '{id}' export '{export_name}' exhausted its fuel budget"
            ));
        }
    }
    pool::wasm_err(&format!("plugin '{id}' export '{export_name}' failed"), err)
}

pub(crate) async fn with_call_timeout<T, F>(inner: &WasmPluginInner, fut: F) -> PluginResult<T>
where
    F: std::future::Future<Output = PluginResult<T>>,
{
    match pool::outer_timeout_ms(inner.limits.call_timeout_ms) {
        Some(ms) => tokio::time::timeout(Duration::from_millis(ms), fut)
            .await
            .map_err(|_| PluginError::Timeout {
                plugin_id: inner.manifest.id.clone(),
            })?,
        None => fut.await,
    }
}

pub(crate) fn observe<T>(
    inner: &WasmPluginInner,
    session: &PooledSession,
    result: &PluginResult<T>,
) {
    let fuel_used = inner.limits.fuel_limit.and_then(|budget| {
        session
            .store
            .get_fuel()
            .ok()
            .map(|remaining| budget.saturating_sub(remaining))
    });
    inner.stats.record(result, fuel_used);
}

pub(crate) async fn write_input(
    inner: &WasmPluginInner,
    session: &mut PooledSession,
    input: &[u8],
) -> PluginResult<u32> {
    let id = &inner.manifest.id;
    let alloc = session
        .instance
        .get_typed_func::<u32, u32>(&mut session.store, export::ALLOC)
        .map_err(|_| {
            PluginError::WasmError(format!("plugin '{id}' does not export '{}'", export::ALLOC))
        })?;
    let ptr = with_call_timeout(inner, async {
        alloc
            .call_async(&mut session.store, input.len() as u32)
            .await
            .map_err(|e| guest_call_error(inner, export::ALLOC, e))
    })
    .await?;
    abi::write_bytes(&mut session.store, &session.memory, ptr, input)?;
    Ok(ptr)
}

pub(crate) async fn release_output(
    _inner: &WasmPluginInner,
    session: &mut PooledSession,
    ptr: u32,
    len: u32,
) {
    let Ok(dealloc) = session
        .instance
        .get_typed_func::<(u32, u32), ()>(&mut session.store, export::DEALLOC)
    else {
        return;
    };
    let _ = dealloc.call_async(&mut session.store, (ptr, len)).await;
}

pub(crate) async fn read_output(
    inner: &WasmPluginInner,
    session: &mut PooledSession,
    packed: u64,
) -> PluginResult<Vec<u8>> {
    let (ptr, len) = abi::unpack_ptr_len(packed);
    let bytes = abi::read_bytes(&session.store, &session.memory, ptr, len)?;
    release_output(inner, session, ptr, len).await;
    Ok(bytes)
}

pub(crate) fn drain_stdio(inner: &WasmPluginInner, op: &str, session: &PooledSession) {
    let state = session.store.data();
    super::super::stdio::drain_guest_stdio(&inner.manifest.id, op, &state.stdout, &state.stderr);
}

pub(crate) async fn invoke_hook(
    inner: &WasmPluginInner,
    export_name: &str,
    input: &[u8],
) -> PluginResult<()> {
    let mut session = inner.pool.acquire().await?;
    let result = invoke_hook_inner(inner, &mut session, export_name, input).await;
    drain_stdio(inner, export_name, &session);
    observe(inner, &session, &result);
    inner.pool.release(session, result.is_ok()).await;
    result
}

pub(crate) async fn invoke_hook_inner(
    inner: &WasmPluginInner,
    session: &mut PooledSession,
    export_name: &str,
    input: &[u8],
) -> PluginResult<()> {
    if session
        .instance
        .get_func(&mut session.store, export_name)
        .is_none()
    {
        tracing::debug!(
            "wasm plugin '{}' has no export '{}'",
            inner.manifest.id,
            export_name
        );
        return Ok(());
    }
    let hook = session
        .instance
        .get_typed_func::<(u32, u32), u32>(&mut session.store, export_name)
        .map_err(|e| {
            pool::wasm_err(
                &format!(
                    "plugin '{}' export '{export_name}' has an unexpected signature",
                    inner.manifest.id
                ),
                e,
            )
        })?;
    pool::arm_epoch(
        &inner.engine,
        &mut session.store,
        inner.limits.call_timeout_ms,
    );
    let ptr = write_input(inner, &mut *session, input).await?;
    let code = with_call_timeout(inner, async {
        hook.call_async(&mut session.store, (ptr, input.len() as u32))
            .await
            .map_err(|e| guest_call_error(inner, export_name, e))
    })
    .await;
    let code = code?;
    release_output(inner, &mut *session, ptr, input.len() as u32).await;
    if code == 0 {
        return Ok(());
    }
    let detail = read_last_error(inner, &mut *session).await;
    abi::check_hook_status_with_detail(export_name, &inner.manifest.id, code, detail.as_deref())
}

pub(crate) async fn read_last_error(
    inner: &WasmPluginInner,
    session: &mut PooledSession,
) -> Option<String> {
    session
        .instance
        .get_func(&mut session.store, export::LAST_ERROR)?;
    let detail_fn = session
        .instance
        .get_typed_func::<(), u64>(&mut session.store, export::LAST_ERROR)
        .ok()?;
    pool::arm_epoch(
        &inner.engine,
        &mut session.store,
        inner.limits.call_timeout_ms,
    );
    let packed = with_call_timeout(inner, async {
        detail_fn
            .call_async(&mut session.store, ())
            .await
            .map_err(|e| guest_call_error(inner, export::LAST_ERROR, e))
    })
    .await
    .ok()?;
    let (ptr, len) = abi::unpack_ptr_len(packed);
    let bytes = abi::read_bytes(&session.store, &session.memory, ptr, len).ok()?;
    release_output(inner, session, ptr, len).await;
    let mut text = String::from_utf8_lossy(&bytes).trim().to_owned();
    if text.is_empty() {
        return None;
    }
    if text.len() > 512 {
        text.truncate(512);
        text.push_str("...(truncated)");
    }
    Some(text)
}

pub(crate) async fn invoke_dispatch(
    inner: &WasmPluginInner,
    handler_type: &str,
    handler_name: &str,
    input_json: &str,
) -> PluginResult<Vec<u8>> {
    let mut session = inner.pool.acquire().await?;
    let result =
        invoke_dispatch_inner(inner, &mut session, handler_type, handler_name, input_json).await;
    drain_stdio(inner, export::DISPATCH, &session);
    observe(inner, &session, &result);
    inner.pool.release(session, result.is_ok()).await;
    result
}

pub(crate) async fn invoke_dispatch_inner(
    inner: &WasmPluginInner,
    session: &mut PooledSession,
    handler_type: &str,
    handler_name: &str,
    input_json: &str,
) -> PluginResult<Vec<u8>> {
    let id = &inner.manifest.id;
    let dispatch = session
        .instance
        .get_typed_func::<(u32, u32, u32, u32, u32, u32), u64>(&mut session.store, export::DISPATCH)
        .map_err(|_| {
            PluginError::WasmError(format!(
                "plugin '{id}' declares contributions but does not export '{}'",
                export::DISPATCH
            ))
        })?;
    pool::arm_epoch(
        &inner.engine,
        &mut session.store,
        inner.limits.call_timeout_ms,
    );
    let type_ptr = write_input(inner, &mut *session, handler_type.as_bytes()).await?;
    let name_ptr = write_input(inner, &mut *session, handler_name.as_bytes()).await?;
    let input_ptr = write_input(inner, &mut *session, input_json.as_bytes()).await?;
    let packed = with_call_timeout(inner, async {
        dispatch
            .call_async(
                &mut session.store,
                (
                    type_ptr,
                    handler_type.len() as u32,
                    name_ptr,
                    handler_name.len() as u32,
                    input_ptr,
                    input_json.len() as u32,
                ),
            )
            .await
            .map_err(|e| guest_call_error(inner, export::DISPATCH, e))
    })
    .await;
    let packed = packed?;
    let out = read_output(inner, &mut *session, packed).await?;
    Ok(out)
}

pub(crate) fn hook_input(inner: &WasmPluginInner, config: &Value) -> PluginResult<Vec<u8>> {
    abi::encode_hook_input(&inner.manifest.id, config)
}
