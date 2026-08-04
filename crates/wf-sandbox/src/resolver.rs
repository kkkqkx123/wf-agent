use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use wf_types::script::sandbox::{SandboxPolicy, ScriptExecutionResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyKind {
    /// Static analysis gate. Never executes user code.
    Analysis,
    /// Actually executes user code.
    Execution,
}

/// Per-language default chains used when the config does not specify a chain.
///
/// Chain semantics are three-phase: every `Analysis` strategy runs in chain
/// order as a gate (Strict rejects on first denial, Lenient records and
/// continues); then the first available `Execution` strategy runs. Analysis
/// strategies therefore always precede execution regardless of their
/// position in the chain list.
pub const DEFAULT_SHELL_CHAIN: &[&str] = &["static-analyzer", "vfs-gate", "os-hook"];
pub const DEFAULT_PYTHON_CHAIN: &[&str] = &["ast-analyzer", "builtin-hook"];
pub const DEFAULT_JS_CHAIN: &[&str] = &["vm-context"];
pub const DEFAULT_LUA_CHAIN: &[&str] = &["static-analyzer", "mlua-sandbox"];

pub fn default_chain(language: &str) -> &'static [&'static str] {
    match language {
        "shell" => DEFAULT_SHELL_CHAIN,
        "python" => DEFAULT_PYTHON_CHAIN,
        "javascript" | "js" => DEFAULT_JS_CHAIN,
        "lua" => DEFAULT_LUA_CHAIN,
        _ => &[],
    }
}

/// Whether a language requires at least one `Analysis` strategy in its
/// chain (gate guarantee). JavaScript is exempt because its only strategy
/// (`vm-context`) enforces policy at runtime inside the wrapped execution.
pub fn analysis_gate_required(language: &str) -> bool {
    matches!(language, "shell" | "python" | "lua")
}

#[derive(Clone)]
pub struct StrategyExecuteOptions {
    pub command: String,
    pub shell_type: Option<String>,
    pub runtime: Option<String>,
    pub workdir: Option<String>,
    pub env_vars: Option<HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
    pub vfs: Option<Arc<dyn VfsProvider>>,
    /// Set by the runtime when the resolved chain contains a dedicated
    /// `vfs-gate` strategy, so `static-analyzer` skips its duplicated VFS
    /// path checks (the gate runs them once).
    pub skip_vfs_check: bool,
}

impl fmt::Debug for StrategyExecuteOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StrategyExecuteOptions")
            .field("command", &self.command)
            .field("shell_type", &self.shell_type)
            .field("runtime", &self.runtime)
            .field("workdir", &self.workdir)
            .field("env_vars", &self.env_vars)
            .field("timeout_ms", &self.timeout_ms)
            .field("vfs", &self.vfs.as_ref().map(|_| "VfsProvider"))
            .field("skip_vfs_check", &self.skip_vfs_check)
            .finish()
    }
}

#[async_trait]
pub trait VfsProvider: Send + Sync {
    async fn read_file(&self, path: &str) -> Result<Vec<u8>, std::io::Error>;
    async fn write_file(&self, path: &str, data: Vec<u8>) -> Result<(), std::io::Error>;
    async fn exists(&self, path: &str) -> bool;
    /// Pure access check; must not mutate VFS state. Used by pre-execution
    /// analysis gates to validate read paths.
    async fn check_read(&self, path: &str) -> Result<(), std::io::Error>;
    /// Pure access check; must not mutate VFS state. Used by pre-execution
    /// analysis gates to validate write paths (e.g. redirect targets).
    async fn check_write(&self, path: &str) -> Result<(), std::io::Error>;
}

#[async_trait]
pub trait StrategyImplementation: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn kind(&self) -> StrategyKind;
    fn is_available(&self) -> bool;
    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>>;
}

#[async_trait]
pub trait StrategyResolver: Send + Sync {
    fn resolve_shell_strategy(&self, id: &str) -> Option<Arc<dyn StrategyImplementation>>;
    fn resolve_python_strategy(&self, id: &str) -> Option<Arc<dyn StrategyImplementation>>;
    fn resolve_js_strategy(&self, id: &str) -> Option<Arc<dyn StrategyImplementation>>;
    fn resolve_lua_strategy(&self, id: &str) -> Option<Arc<dyn StrategyImplementation>>;
    fn register_strategy(&mut self, language: &str, strategy: Arc<dyn StrategyImplementation>);
    /// Resolve an ordered strategy chain for a language.
    ///
    /// `preferred_ids` (if non-empty) is the chain order; otherwise the
    /// per-language default chain is used. Unknown strategy IDs are a
    /// fail-closed error rather than a silent drop.
    fn resolve_chain(
        &self,
        language: &str,
        preferred_ids: &[String],
    ) -> Result<Vec<Arc<dyn StrategyImplementation>>, String>;
}

pub struct DefaultStrategyResolver {
    shell_strategies: HashMap<String, Arc<dyn StrategyImplementation>>,
    python_strategies: HashMap<String, Arc<dyn StrategyImplementation>>,
    js_strategies: HashMap<String, Arc<dyn StrategyImplementation>>,
    lua_strategies: HashMap<String, Arc<dyn StrategyImplementation>>,
}

impl Default for DefaultStrategyResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultStrategyResolver {
    pub fn new() -> Self {
        Self {
            shell_strategies: HashMap::new(),
            python_strategies: HashMap::new(),
            js_strategies: HashMap::new(),
            lua_strategies: HashMap::new(),
        }
    }

    pub fn with_defaults() -> Self {
        let mut resolver = Self::new();
        resolver.register_default_strategies();
        resolver
    }

    fn register_default_strategies(&mut self) {
        // Shell strategies
        use crate::strategy::shell::os_hook::LinuxSeccompStrategy;
        self.shell_strategies
            .insert("os-hook".to_string(), Arc::new(LinuxSeccompStrategy));
        use crate::strategy::shell::static_analyzer::ShellStaticAnalyzerStrategy;
        self.shell_strategies.insert(
            "static-analyzer".to_string(),
            Arc::new(ShellStaticAnalyzerStrategy::new()),
        );
        use crate::strategy::vfs_gate::VfsGateStrategy;
        self.shell_strategies
            .insert("vfs-gate".to_string(), Arc::new(VfsGateStrategy));
        use crate::strategy::container::ContainerStrategy;
        self.shell_strategies
            .insert("container".to_string(), Arc::new(ContainerStrategy));

        // Python strategies
        use crate::strategy::python::builtin_hook::PythonBuiltinHookStrategy;
        self.python_strategies.insert(
            "builtin-hook".to_string(),
            Arc::new(PythonBuiltinHookStrategy),
        );
        use crate::strategy::python::ast_analyzer::PythonAstAnalyzerStrategy;
        self.python_strategies.insert(
            "ast-analyzer".to_string(),
            Arc::new(PythonAstAnalyzerStrategy),
        );
        use crate::strategy::python::direct::PythonDirectStrategy;
        self.python_strategies
            .insert("direct".to_string(), Arc::new(PythonDirectStrategy));

        // JavaScript strategies
        use crate::strategy::js::vm_context::JavaScriptVmContextStrategy;
        self.js_strategies.insert(
            "vm-context".to_string(),
            Arc::new(JavaScriptVmContextStrategy),
        );
        use crate::strategy::js::subprocess::JavaScriptSubprocessStrategy;
        self.js_strategies.insert(
            "subprocess".to_string(),
            Arc::new(JavaScriptSubprocessStrategy),
        );
        use crate::strategy::js::direct::JavaScriptDirectStrategy;
        self.js_strategies
            .insert("direct".to_string(), Arc::new(JavaScriptDirectStrategy));

        // Lua strategies
        use crate::strategy::lua::static_analyzer::LuaStaticAnalyzerStrategy;
        self.lua_strategies.insert(
            "static-analyzer".to_string(),
            Arc::new(LuaStaticAnalyzerStrategy),
        );
        #[cfg(feature = "lua-mlua-sandbox")]
        {
            use crate::strategy::lua::mlua_sandbox::LuaMluaSandboxStrategy;
            self.lua_strategies
                .insert("mlua-sandbox".to_string(), Arc::new(LuaMluaSandboxStrategy));
        }
    }
}

#[async_trait]
impl StrategyResolver for DefaultStrategyResolver {
    fn resolve_shell_strategy(&self, id: &str) -> Option<Arc<dyn StrategyImplementation>> {
        self.shell_strategies.get(id).cloned()
    }

    fn resolve_python_strategy(&self, id: &str) -> Option<Arc<dyn StrategyImplementation>> {
        self.python_strategies.get(id).cloned()
    }

    fn resolve_js_strategy(&self, id: &str) -> Option<Arc<dyn StrategyImplementation>> {
        self.js_strategies.get(id).cloned()
    }

    fn resolve_lua_strategy(&self, id: &str) -> Option<Arc<dyn StrategyImplementation>> {
        self.lua_strategies.get(id).cloned()
    }

    fn register_strategy(&mut self, language: &str, strategy: Arc<dyn StrategyImplementation>) {
        let map = match language {
            "shell" => &mut self.shell_strategies,
            "python" => &mut self.python_strategies,
            "javascript" | "js" => &mut self.js_strategies,
            "lua" => &mut self.lua_strategies,
            _ => return,
        };
        map.insert(strategy.id().to_string(), strategy);
    }

    fn resolve_chain(
        &self,
        language: &str,
        preferred_ids: &[String],
    ) -> Result<Vec<Arc<dyn StrategyImplementation>>, String> {
        let map: &HashMap<String, Arc<dyn StrategyImplementation>> = match language {
            "shell" => &self.shell_strategies,
            "python" => &self.python_strategies,
            "javascript" | "js" => &self.js_strategies,
            "lua" => &self.lua_strategies,
            _ => return Err(format!("Unsupported sandbox language: {language}")),
        };

        let ids: Vec<&str> = if preferred_ids.is_empty() {
            default_chain(language).to_vec()
        } else {
            preferred_ids.iter().map(|s| s.as_str()).collect()
        };

        if ids.is_empty() {
            return Err(format!(
                "No strategy chain configured for language: {language}"
            ));
        }

        let mut chain = Vec::with_capacity(ids.len());
        let mut missing = Vec::new();
        for id in ids {
            match map.get(id) {
                Some(strategy) => chain.push(strategy.clone()),
                None => missing.push(id.to_string()),
            }
        }

        if !missing.is_empty() {
            return Err(format!(
                "Strategy chain for language '{language}' contains unregistered strategies: {}",
                missing.join(", ")
            ));
        }

        Ok(chain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockStrategy {
        id: &'static str,
        kind: StrategyKind,
        available: bool,
    }

    #[async_trait]
    impl StrategyImplementation for MockStrategy {
        fn id(&self) -> &str {
            self.id
        }
        fn name(&self) -> &str {
            self.id
        }
        fn description(&self) -> &str {
            "mock"
        }
        fn kind(&self) -> StrategyKind {
            self.kind
        }
        fn is_available(&self) -> bool {
            self.available
        }
        async fn execute(
            &self,
            _options: StrategyExecuteOptions,
            _policy: &SandboxPolicy,
        ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
            unimplemented!()
        }
    }

    fn register(resolver: &mut DefaultStrategyResolver, language: &str, id: &'static str) {
        resolver.register_strategy(
            language,
            Arc::new(MockStrategy {
                id,
                kind: StrategyKind::Execution,
                available: true,
            }),
        );
    }

    #[test]
    fn test_resolve_chain_preferred_order() {
        let mut resolver = DefaultStrategyResolver::new();
        register(&mut resolver, "shell", "foo");
        register(&mut resolver, "shell", "bar");

        let chain = resolver
            .resolve_chain("shell", &["bar".to_string(), "foo".to_string()])
            .unwrap();
        let ids: Vec<&str> = chain.iter().map(|s| s.id()).collect();
        assert_eq!(ids, vec!["bar", "foo"]);
    }

    #[test]
    fn test_resolve_chain_default_chain() {
        let resolver = DefaultStrategyResolver::with_defaults();
        let chain = resolver.resolve_chain("shell", &[]).unwrap();
        let ids: Vec<&str> = chain.iter().map(|s| s.id()).collect();
        assert_eq!(ids, vec!["static-analyzer", "vfs-gate", "os-hook"]);
        assert_eq!(
            chain[0].kind(),
            StrategyKind::Analysis,
            "static-analyzer must be an analysis gate"
        );
        assert_eq!(
            chain[1].kind(),
            StrategyKind::Analysis,
            "vfs-gate must be an analysis gate"
        );
        assert_eq!(
            chain[2].kind(),
            StrategyKind::Execution,
            "os-hook must be an execution strategy"
        );
    }

    #[test]
    fn test_resolve_chain_missing_strategy_is_error() {
        let resolver = DefaultStrategyResolver::with_defaults();
        let err = resolver
            .resolve_chain("shell", &["os-hook".to_string(), "nope".to_string()])
            .err()
            .expect("missing strategy must fail resolution");
        assert!(err.contains("nope"), "error should list missing id: {err}");
    }

    #[test]
    fn test_resolve_chain_unknown_language() {
        let resolver = DefaultStrategyResolver::with_defaults();
        assert!(resolver.resolve_chain("cobol", &[]).is_err());
    }

    #[test]
    fn test_default_chain_js_is_vm_context_only() {
        let resolver = DefaultStrategyResolver::with_defaults();
        let chain = resolver.resolve_chain("js", &[]).unwrap();
        let ids: Vec<&str> = chain.iter().map(|s| s.id()).collect();
        assert_eq!(ids, vec!["vm-context"]);
    }
}
