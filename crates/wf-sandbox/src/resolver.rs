use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use wf_types::script::sandbox::{SandboxPolicy, ScriptExecutionResult};

pub struct StrategyExecuteOptions {
    pub command: String,
    pub shell_type: Option<String>,
    pub runtime: Option<String>,
    pub workdir: Option<String>,
    pub env_vars: Option<HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
    pub vfs: Option<Arc<dyn VfsProvider>>,
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
            .finish()
    }
}

#[async_trait]
pub trait VfsProvider: Send + Sync {
    async fn read_file(&self, path: &str) -> Result<Vec<u8>, std::io::Error>;
    async fn write_file(&self, path: &str, data: Vec<u8>) -> Result<(), std::io::Error>;
    async fn exists(&self, path: &str) -> bool;
}

#[async_trait]
pub trait StrategyImplementation: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn priority(&self) -> i32;
    fn is_available(&self) -> bool;
    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>>;
}

#[async_trait]
pub trait StrategyResolver: Send + Sync {
    fn resolve_shell_strategy(
        &self,
        id: &str,
    ) -> Option<Arc<dyn StrategyImplementation>>;
    fn resolve_python_strategy(
        &self,
        id: &str,
    ) -> Option<Arc<dyn StrategyImplementation>>;
    fn resolve_js_strategy(&self, id: &str) -> Option<Arc<dyn StrategyImplementation>>;
    fn resolve_lua_strategy(&self, id: &str) -> Option<Arc<dyn StrategyImplementation>>;
    fn register_strategy(
        &mut self,
        language: &str,
        strategy: Arc<dyn StrategyImplementation>,
    );
    fn resolve_best(
        &self,
        language: &str,
        preferred_ids: &[String],
    ) -> Option<Arc<dyn StrategyImplementation>>;
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
        use crate::strategy::shell::static_analyzer::ShellStaticAnalyzerStrategy;
        self.shell_strategies.insert(
            "static-analyzer".to_string(),
            Arc::new(ShellStaticAnalyzerStrategy),
        );
        use crate::strategy::shell::os_hook::LinuxSeccompStrategy;
        self.shell_strategies.insert(
            "os-hook".to_string(),
            Arc::new(LinuxSeccompStrategy),
        );

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
        use crate::strategy::python::os_hook::PythonOsHookStrategy;
        self.python_strategies.insert(
            "os-hook".to_string(),
            Arc::new(PythonOsHookStrategy),
        );

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
        use crate::strategy::js::os_hook::JavaScriptOsHookStrategy;
        self.js_strategies.insert(
            "os-hook".to_string(),
            Arc::new(JavaScriptOsHookStrategy),
        );

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
    fn resolve_shell_strategy(
        &self,
        id: &str,
    ) -> Option<Arc<dyn StrategyImplementation>> {
        self.shell_strategies.get(id).cloned()
    }

    fn resolve_python_strategy(
        &self,
        id: &str,
    ) -> Option<Arc<dyn StrategyImplementation>> {
        self.python_strategies.get(id).cloned()
    }

    fn resolve_js_strategy(&self, id: &str) -> Option<Arc<dyn StrategyImplementation>> {
        self.js_strategies.get(id).cloned()
    }

    fn resolve_lua_strategy(&self, id: &str) -> Option<Arc<dyn StrategyImplementation>> {
        self.lua_strategies.get(id).cloned()
    }

    fn register_strategy(
        &mut self,
        language: &str,
        strategy: Arc<dyn StrategyImplementation>,
    ) {
        let map = match language {
            "shell" => &mut self.shell_strategies,
            "python" => &mut self.python_strategies,
            "javascript" | "js" => &mut self.js_strategies,
            "lua" => &mut self.lua_strategies,
            _ => return,
        };
        map.insert(strategy.id().to_string(), strategy);
    }

    fn resolve_best(
        &self,
        language: &str,
        preferred_ids: &[String],
    ) -> Option<Arc<dyn StrategyImplementation>> {
        let map: &HashMap<String, Arc<dyn StrategyImplementation>> = match language {
            "shell" => &self.shell_strategies,
            "python" => &self.python_strategies,
            "javascript" | "js" => &self.js_strategies,
            "lua" => &self.lua_strategies,
            _ => return None,
        };

        if !preferred_ids.is_empty() {
            for id in preferred_ids {
                if let Some(strategy) = map.get(id) {
                    if strategy.is_available() {
                        return Some(strategy.clone());
                    }
                }
            }
        }

        let mut candidates: Vec<&Arc<dyn StrategyImplementation>> = map.values().collect();
        candidates.sort_by_key(|b| std::cmp::Reverse(b.priority()));
        candidates
            .into_iter()
            .find(|s| s.is_available())
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockStrategy {
        id: &'static str,
        priority_val: i32,
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
        fn priority(&self) -> i32 {
            self.priority_val
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

    #[tokio::test]
    async fn test_resolve_best_preferred_id() {
        let mut resolver = DefaultStrategyResolver::new();
        resolver.register_strategy(
            "shell",
            Arc::new(MockStrategy {
                id: "foo",
                priority_val: 10,
                available: true,
            }),
        );
        resolver.register_strategy(
            "shell",
            Arc::new(MockStrategy {
                id: "bar",
                priority_val: 20,
                available: true,
            }),
        );

        let result = resolver.resolve_best("shell", &["foo".to_string()]);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id(), "foo");
    }

    #[tokio::test]
    async fn test_resolve_best_falls_back_by_priority() {
        let mut resolver = DefaultStrategyResolver::new();
        resolver.register_strategy(
            "lua",
            Arc::new(MockStrategy {
                id: "low",
                priority_val: 10,
                available: true,
            }),
        );
        resolver.register_strategy(
            "lua",
            Arc::new(MockStrategy {
                id: "high",
                priority_val: 100,
                available: true,
            }),
        );

        let result = resolver.resolve_best("lua", &[]);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id(), "high");
    }

    #[tokio::test]
    async fn test_resolve_best_skips_unavailable() {
        let mut resolver = DefaultStrategyResolver::new();
        resolver.register_strategy(
            "lua",
            Arc::new(MockStrategy {
                id: "broken",
                priority_val: 100,
                available: false,
            }),
        );
        resolver.register_strategy(
            "lua",
            Arc::new(MockStrategy {
                id: "working",
                priority_val: 10,
                available: true,
            }),
        );

        let result = resolver.resolve_best("lua", &[]);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id(), "working");
    }
}
