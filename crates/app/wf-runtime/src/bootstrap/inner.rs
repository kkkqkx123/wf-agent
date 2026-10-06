use std::sync::Arc;

use tracing::{info, warn};

use wf_core::event::EventBus;
use wf_core::internal_signal::InternalSignalBus;
use wf_core::registry::{MutableRegistry, Registry};
use wf_execution_shared::hooks::HookHandlerRegistry;
use wf_resource::registry::ResourceRegistries;
use wf_storage::adapter::base::BaseStorageAdapter;

use crate::error::RuntimeResult;
use crate::lifecycle::shutdown_channel;
use crate::logger::init_tracing;
use crate::mode::detect_all;
use crate::storage_manager::StorageManager;
use crate::trigger_listener::TriggerExecutionRecorder;

use super::code_context::resolve_code_context_transport;
use super::config::{adjust_log_config, resolve_infra_config, InfraSourceConfig, RuntimeConfig};
use super::file_checkpoint::init_file_checkpoint_stack;
use super::llm::{create_llm_gateway, register_llm_config};
use super::mcp::init_mcp;
use super::metrics::init_metrics_context;
#[cfg(feature = "plugins")]
use super::plugin::init_plugins;
use super::plugin::init_plugins_and_resources;
use super::runtime::Runtime;
use super::storage::init_event_persistence;
use super::tool_registry::{hydrate_tool_registry_from_storage, init_tool_registry_with_mcp};
use super::trigger::{assemble_trigger_subsystem, TriggerSubsystemDeps};

impl Runtime {
    /// Bootstrap from fully programmatic values (no file layer).
    pub async fn bootstrap(config: RuntimeConfig) -> RuntimeResult<Self> {
        Self::bootstrap_inner(config, None).await
    }

    /// Bootstrap with a file-layer source: user dotfiles plus the
    /// infrastructure preset fill the values the caller left at defaults.
    /// Programmatic values always win over the file layer.
    pub async fn bootstrap_with_source(
        config: RuntimeConfig,
        source: InfraSourceConfig,
    ) -> RuntimeResult<Self> {
        Self::bootstrap_inner(config, Some(source)).await
    }

    async fn bootstrap_inner(
        mut config: RuntimeConfig,
        source: Option<InfraSourceConfig>,
    ) -> RuntimeResult<Self> {
        // Stage 1: configuration resolution. The file layer (user dotfiles +
        // orchestrator-assembled infrastructure preset, plus the skill
        // settings chain) fills storage / metrics / output /
        // sandbox / presets / tools / file_checkpoint from disk; values the
        // caller already set keep their higher priority.
        let config_metrics = std::sync::Arc::new(wf_metrics::ConfigMetricsCollector::new(
            wf_metrics::CollectorConfig::default(),
        ));
        if let Some(infra) = source {
            config = resolve_infra_config(config, &infra, Some(&config_metrics)).await?;
        }

        // Stage 2: mode detection, logging and durable backends.
        let mode_info = detect_all(config.mode_override);
        let effective_log_config = adjust_log_config(config.log_config, &mode_info);

        let _guard = init_tracing(&effective_log_config)?;

        info!("Bootstrapping runtime in {:?} mode", mode_info.mode);

        // Stage 2 (continued): durable event persistence and storage backends.
        // Durable event persistence backend: engine events published
        // on the shared bus are buffered and flushed to the same backend as
        // the runtime storage, so history survives restarts. `None` (memory
        // storage, or a failed open) keeps events in memory only.
        let event_persistence = init_event_persistence(&config.storage).await;

        let mut storage_manager = StorageManager::new(config.storage);
        storage_manager.initialize().await?;

        // Durable checkpoint store backend: reuse the checkpoint table of the
        // shared storage context so execution checkpoints survive restarts
        // without opening a second pool on the same file/table.
        let checkpoint_store = Arc::new(
            storage_manager
                .shared_context()
                .expect("storage initialized")
                .checkpoint
                .store()
                .clone(),
        );

        let registries = Arc::new(ResourceRegistries::new());

        // Stage 3: foundation pieces. Skills, MCP, the shared event/signal
        // buses and the hook registry are created before the tool registry
        // and executors that consume them.
        let skill_loader = Arc::new(wf_tools::SkillLoader::new(config.skills));
        let skill_count = skill_loader.list_skills().len();
        if skill_count > 0 {
            info!("Skill registry initialized: {} skills", skill_count);
        }

        // MCP: load merged settings (global + project), register servers and
        // connect eager/keep-alive ones. Lazy servers connect on first use.
        let mcp_manager = init_mcp(&config.mcp).await;
        if let Some(manager) = &mcp_manager {
            info!(
                "MCP manager initialized: {} servers registered",
                manager.registry().list().len()
            );
        }

        // Shared event bus: shell event bridge depends on it, so it is
        // created before the tool registry.
        let event_bus = Arc::new(EventBus::new(1024));
        // Shared typed signal bus: internal workflow/agent control signals
        // (stop/pause/resume/skip, async results) replace the `__`-prefixed
        // variable protocol. Created beside the event bus so trigger actions
        // and coordinators share one instance.
        let signal_bus = Arc::new(InternalSignalBus::new());

        // Shared hook handler registry: hook points and engine signals
        // (context compression) fire through it. The builtin compression
        // handler is registered once the execution write-back registry and
        // the trigger shutdown token exist (below).
        let hook_handler_registry = Arc::new(HookHandlerRegistry::new());

        // Stage 4: execution pieces. The sandbox runtime compiles first so
        // configuration errors surface at bootstrap, not at script
        // execution; the tool registry, LLM gateway, plugins and persisted
        // templates build on top in dependency order.
        // Shared sandbox runtime: compile the global config (profiles +
        // routing rules) up front so configuration errors surface at
        // bootstrap, not at script execution. Created before the tool
        // registry so its default policy can harden every external command
        // (shell tool, CLI executors) through the shared execution gateway.
        let sandbox_runtime = Arc::new(match &config.sandbox {
            Some(global) => wf_sandbox::SandboxRuntime::with_global_config(global.clone())
                .map_err(|e| {
                    crate::error::RuntimeError::Config(format!(
                        "Invalid sandbox global config: {e}"
                    ))
                })?,
            None => wf_sandbox::SandboxRuntime::new(),
        });
        if let Some(global) = &config.sandbox {
            info!(
                "Sandbox runtime initialized: {} profiles, {} routing rules",
                global.profiles.len(),
                global.rules.len()
            );
        }

        let mut shell_config = config.shell.clone();
        // Code-context transport resolves before the tool registry builds:
        // managed mode starts the supervised process here so the registry
        // handlers and retrieval tools observe the effective address.
        let (code_context, code_context_sidecar) =
            resolve_code_context_transport(config.tools.code_context.clone()).await;
        let tool_registry = init_tool_registry_with_mcp(
            &mut shell_config,
            &sandbox_runtime,
            skill_loader.clone(),
            &mcp_manager,
            &event_bus,
            code_context.clone(),
        )
        .await?;

        // Fixed startup order for LLM extensibility: the gateway exists
        // before plugin activation so the bridge can sync plugin codecs
        // and provider definitions into it; file-layer providers and
        // profiles register afterwards. No lazy backfill.
        let agent_registry = std::sync::Arc::new(wf_agent::registry::AgentLoopRegistry::new());
        let gate_stats = agent_registry.gate_stats();
        info!(
            "Agent capacity gate at startup: max_concurrent={}, active={}, available={}",
            gate_stats.max_concurrent, gate_stats.active_count, gate_stats.available_permits
        );

        let metrics = init_metrics_context(
            &config.metrics,
            &storage_manager,
            &event_bus,
            &agent_registry,
            Some(config_metrics),
        )
        .await?;

        if let Some(ref metrics) = metrics {
            tool_registry.set_tool_metrics(metrics.registry().tool());
        }

        let llm_gateway = create_llm_gateway(metrics.as_ref().map(|m| m.registry().as_ref()));

        #[cfg(feature = "plugins")]
        let plugin_engine = init_plugins(
            &config.plugins,
            registries.clone(),
            tool_registry.clone(),
            llm_gateway.clone(),
        )
        .await?;
        config.resource.apply_custom_source();
        init_plugins_and_resources(
            &config.resource.options,
            &registries,
            &tool_registry,
            #[cfg(feature = "plugins")]
            &plugin_engine,
        )
        .await?;

        // Build the fold-summary chain with the effective code-context
        // snapshot at construction time, so execution reads only node
        // config. Absent service keeps the safe skip behavior.
        if let Some(service) = code_context.clone() {
            let template =
                wf_resource::predefined::workflow::create_fold_summary_workflow_with_service(
                    None, &service,
                );
            if let Err(e) = registries.upsert_workflow_template(template) {
                warn!("code-context snapshot bake skipped: {e}");
            }
        }

        register_llm_config(&llm_gateway, &config.llm)?;

        // Hydrate persisted agent templates into the runtime registry so
        // templates created through the API survive restarts. Predefined
        // and plugin-owned entries keep their registry version.
        if let Some(storage_ctx) = storage_manager.shared_context() {
            if let Ok(templates) = storage_ctx.agent_template.list(None).await {
                let mut restored = 0usize;
                for template in templates {
                    let key = template.id.to_string();
                    if !registries.agent_templates.has(&key) {
                        if let Err(e) = registries
                            .agent_templates
                            .register(key, std::sync::Arc::new(template))
                        {
                            warn!(error = %e, "hydrate persisted agent template skipped");
                        } else {
                            restored += 1;
                        }
                    }
                }
                if restored > 0 {
                    info!(
                        "Restored {} persisted agent template(s) from storage",
                        restored
                    );
                }
            }
        }

        let (shutdown_handle, _shutdown_waiter) = shutdown_channel();

        // Stage 5: execution dispatch. The composite agent/workflow callback
        // is registered on both the global callback singleton and the shared
        // tool registry so builtin dispatch tools resolve at runtime.
        // Execution callback assembly: a composite covering agent and
        // workflow dispatch, registered on both the global callback
        // singleton and the shared tool registry. Fixes the production path
        // where builtin dispatch tools previously failed with
        // CallbackNotRegistered.
        let agent_limits = config.limits.agent.clone().unwrap_or_default();
        let agent_executor = std::sync::Arc::new(
            wf_agent::executor::AgentLoopExecutor::new(llm_gateway.clone(), tool_registry.clone())
                .with_shared_registry(agent_registry.clone())
                .with_max_iterations_cap(
                    agent_limits
                        .max_iterations_cap
                        .unwrap_or(wf_agent::constants::AGENT_MAX_ITERATIONS_CAP),
                )
                .with_max_iterations(
                    agent_limits
                        .default_max_iterations
                        .unwrap_or(wf_agent::constants::DEFAULT_MAX_ITERATIONS),
                )
                .with_max_sub_agent_depth(
                    agent_limits
                        .max_sub_agent_depth
                        .unwrap_or(wf_types::execution::MAX_EXECUTION_DEPTH),
                )
                .with_max_concurrent({
                    let max = agent_limits.max_concurrent.unwrap_or(0);
                    if max == 0 {
                        std::thread::available_parallelism()
                            .map(|n| n.get())
                            .unwrap_or(4)
                    } else {
                        max as usize
                    }
                })
                .with_hook_handler_registry(hook_handler_registry.clone())
                .with_signal_bus(signal_bus.clone()),
        );
        let mut workflow_callback =
            wf_workflow::execution_callback::WorkflowExecutionCallback::new(tool_registry.clone())
                .with_gateway(llm_gateway.clone())
                .with_event_bus(event_bus.clone())
                .with_sandbox(sandbox_runtime.clone())
                .with_hook_handler_registry(hook_handler_registry.clone())
                .with_signal_bus(signal_bus.clone());
        if let Some(metrics) = metrics.as_ref() {
            workflow_callback = workflow_callback.with_metrics(metrics.registry().clone());
        }
        let workflow_callback = Arc::new(workflow_callback);
        // Register every registered workflow template so the execute_workflow
        // tool can resolve it at runtime. Definition-level hooks travel with
        // the template and are executed per node (BEFORE_EXECUTE /
        // AFTER_EXECUTE). The same executable graph also goes into the
        // process-wide graph registry, which is where SUBGRAPH nodes resolve
        // their child workflow by id at execution time.
        for id in wf_core::registry::Registry::list(&registries.workflows) {
            if let Some(template) = wf_core::registry::Registry::get(&registries.workflows, &id) {
                let graph = crate::trigger_listener::template_to_graph(&template);
                wf_workflow::register_graph(&id, graph.clone());
                let hooks = template
                    .definition
                    .hooks
                    .as_ref()
                    .map(|hooks| hooks.iter().map(Into::into).collect())
                    .unwrap_or_default();
                workflow_callback.register_workflow_with_hooks(
                    wf_types::Id::from(id.clone()),
                    graph,
                    hooks,
                );
            }
        }
        let composite = std::sync::Arc::new(
            crate::execution_callback::CompositeExecutionCallback::new()
                .with_agent(agent_executor.clone())
                .with_workflow(workflow_callback),
        );
        wf_tools::callback::register_execution_callback(composite.clone()).map_err(|e| {
            crate::error::RuntimeError::Config(format!(
                "Failed to register execution callback: {}",
                e
            ))
        })?;
        tool_registry.set_builtin_callback(composite);

        hydrate_tool_registry_from_storage(&tool_registry, &storage_manager).await;

        // Stage 6: event-driven trigger subsystem (context compression chain
        // plus user trigger templates) over the shared write-back registries.
        // Event-driven trigger subsystem: powers the nested-agent-execution
        // action (HookTriggered etc.) and user trigger templates. The context
        // compression chain is now served by the hook registry: the engine
        // fires the CONTEXT_COMPRESSION_REQUESTED signal synchronously
        // and the compression handler (assembled below) takes over
        // immediately.
        let trigger_subsystem = assemble_trigger_subsystem(TriggerSubsystemDeps {
            registries: registries.clone(),
            event_bus: event_bus.clone(),
            signal_bus: signal_bus.clone(),
            llm_gateway: llm_gateway.clone(),
            tool_registry: tool_registry.clone(),
            sandbox_runtime: sandbox_runtime.clone(),
            agent_executor: agent_executor.clone(),
            hook_handler_registry: hook_handler_registry.clone(),
            storage: storage_manager.shared_context().map(|ctx| {
                Arc::new(ctx.trigger_execution.clone()) as Arc<dyn TriggerExecutionRecorder>
            }),
            limits: config.limits.clone(),
        });
        let execution_contexts = trigger_subsystem.execution_contexts;
        let trigger_state_registry = trigger_subsystem.trigger_state_registry;
        let timer_bindings = trigger_subsystem.timer_bindings;
        let listener = trigger_subsystem.listener;

        // Stage 7: file-checkpoint stack (manager + manual watcher + GC
        // timer). The manager is attached to the API context so workflow /
        // agent executions create and restore file snapshots through it and
        // script handlers capture workspace changes.
        let checkpoint_stack =
            init_file_checkpoint_stack(&config.file_checkpoint, event_bus.clone())?;
        let file_checkpoint_manager = checkpoint_stack.manager;
        let checkpoint_event_bridge_handle = checkpoint_stack.event_bridge_handle;
        let manual_change_service = checkpoint_stack.manual_change_service;
        let gc_timer_handle = checkpoint_stack.gc_timer_handle;
        if let (Some(manager), Some(metrics)) = (file_checkpoint_manager.as_ref(), metrics.as_ref())
        {
            manager.set_checkpoint_metrics(metrics.registry().checkpoint());
        }
        // Approval tool (policy `llm` / `manual`): an LLM node can call
        // `approve_changes` to resolve a pending agent approval in-workflow.
        // Registered only when a file checkpoint manager is attached.
        if let Some(manager) = &file_checkpoint_manager {
            crate::approval_tool::register_approval_tools(&tool_registry, manager.clone());
        }

        info!("Runtime bootstrap complete");

        Ok(Self {
            storage_manager,
            mode_info,
            shutdown_handle,
            _shutdown_waiter,
            registries,
            skill_loader,
            tool_registry,
            mcp_manager,
            event_bus,
            metrics,
            llm_gateway,
            sandbox_runtime,
            execution_contexts,
            trigger_listener: Some(listener.listener),
            trigger_listener_shutdown: Some(listener.shutdown),
            trigger_listener_handle: Some(listener.handle),
            trigger_state_registry,
            timer_bindings,
            hook_handler_registry,
            agent_registry,
            #[cfg(feature = "plugins")]
            plugin_engine,
            event_persistence,
            checkpoint_store,
            api_ctx: std::sync::OnceLock::new(),
            file_checkpoint_manager,
            tool_approval: config.tool_approval.clone(),
            manual_change_service,
            checkpoint_event_bridge_handle,
            gc_timer_handle,
            code_context_sidecar,
            output: config.output.clone(),
            presets: config.presets.clone(),
            tools: config.tools.clone(),
            limits: config.limits.clone(),
        })
    }
}
