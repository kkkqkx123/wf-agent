use std::collections::{HashMap, HashSet};

use super::types::{
    BranchExecutionResult, FlowBranch, FlowBranchExecutionResult, FlowExecutionResult, ScriptFlow,
};
use crate::error::ScriptResult;

pub struct ScriptFlowEngine;

impl Default for ScriptFlowEngine {
    fn default() -> Self {
        Self
    }
}

impl ScriptFlowEngine {
    pub fn new() -> Self {
        Self
    }

    pub async fn execute<F, Fut>(&self, flow: &ScriptFlow, execute_module: F) -> FlowExecutionResult
    where
        F: Fn(String, String, Option<HashMap<String, serde_json::Value>>) -> Fut + Sync,
        Fut: std::future::Future<Output = ScriptResult<String>>,
    {
        let start = std::time::Instant::now();
        let mut branches: HashMap<String, BranchExecutionResult> = HashMap::new();

        let levels = match self.topological_levels(flow) {
            Ok(levels) => levels,
            Err(e) => {
                return FlowExecutionResult {
                    success: false,
                    branches: HashMap::new(),
                    total_execution_time_ms: start.elapsed().as_millis() as u64,
                    error: Some(e),
                };
            }
        };
        let branch_map: HashMap<&str, &FlowBranch> =
            flow.branches.iter().map(|b| (b.key.as_str(), b)).collect();

        for level in levels {
            let mut runnable: Vec<&FlowBranch> = Vec::new();
            for branch_key in &level {
                let Some(branch) = branch_map.get(branch_key.as_str()).copied() else {
                    continue;
                };
                let failed_dep = branch
                    .depends_on
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .find(|dep| {
                        branches
                            .get(*dep)
                            .is_some_and(|r: &BranchExecutionResult| !r.success)
                    });
                if let Some(dep) = failed_dep {
                    branches.insert(
                        branch_key.clone(),
                        BranchExecutionResult {
                            success: false,
                            modules: vec![FlowBranchExecutionResult {
                                success: false,
                                module_key: String::new(),
                                output: None,
                                error: Some(format!(
                                    "Branch '{branch_key}' skipped: dependency '{dep}' failed"
                                )),
                                execution_time_ms: 0,
                            }],
                            execution_time_ms: 0,
                        },
                    );
                } else {
                    runnable.push(branch);
                }
            }
            if runnable.is_empty() {
                continue;
            }
            let futures = runnable
                .iter()
                .map(|branch| Self::run_branch(branch, &execute_module));
            let results: Vec<(String, BranchExecutionResult)> =
                futures::future::join_all(futures).await;
            for (key, result) in results {
                branches.insert(key, result);
            }
        }

        let all_success = branches.values().all(|b| b.success);

        FlowExecutionResult {
            success: all_success,
            branches,
            total_execution_time_ms: start.elapsed().as_millis() as u64,
            error: None,
        }
    }

    async fn run_branch<F, Fut>(
        branch: &FlowBranch,
        execute_module: &F,
    ) -> (String, BranchExecutionResult)
    where
        F: Fn(String, String, Option<HashMap<String, serde_json::Value>>) -> Fut + Sync,
        Fut: std::future::Future<Output = ScriptResult<String>>,
    {
        let branch_start = std::time::Instant::now();
        let mut module_results = Vec::new();
        for module_ref in &branch.modules {
            let module_start = std::time::Instant::now();
            let result = match execute_module(
                module_ref.key.clone(),
                branch.key.clone(),
                module_ref.args.clone(),
            )
            .await
            {
                Ok(output) => FlowBranchExecutionResult {
                    success: true,
                    module_key: module_ref.key.clone(),
                    output: Some(output),
                    error: None,
                    execution_time_ms: module_start.elapsed().as_millis() as u64,
                },
                Err(e) => FlowBranchExecutionResult {
                    success: false,
                    module_key: module_ref.key.clone(),
                    output: None,
                    error: Some(e.to_string()),
                    execution_time_ms: module_start.elapsed().as_millis() as u64,
                },
            };
            module_results.push(result);
        }
        let branch_success = module_results.iter().all(|r| r.success);
        (
            branch.key.clone(),
            BranchExecutionResult {
                success: branch_success,
                modules: module_results,
                execution_time_ms: branch_start.elapsed().as_millis() as u64,
            },
        )
    }

    fn topological_levels(&self, flow: &ScriptFlow) -> Result<Vec<Vec<String>>, String> {
        let order = self.topological_sort(flow)?;
        let branch_map: HashMap<&str, &FlowBranch> =
            flow.branches.iter().map(|b| (b.key.as_str(), b)).collect();
        let mut depths: HashMap<String, usize> = HashMap::new();
        for key in &order {
            let depth = match branch_map.get(key.as_str()).and_then(|b| b.depends_on.as_ref()) {
                None => 0,
                Some(deps) => {
                    deps.iter()
                        .map(|dep| depths.get(dep).copied().unwrap_or(0) + 1)
                        .max()
                        .unwrap_or(0)
                }
            };
            depths.insert(key.clone(), depth);
        }
        let max_depth = depths.values().copied().max().unwrap_or(0);
        let mut levels: Vec<Vec<String>> = vec![Vec::new(); max_depth + 1];
        for key in order {
            let depth = depths.get(&key).copied().unwrap_or(0);
            levels[depth].push(key);
        }
        levels.retain(|level| !level.is_empty());
        Ok(levels)
    }

    fn topological_sort(&self, flow: &ScriptFlow) -> Result<Vec<String>, String> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut visiting: HashSet<String> = HashSet::new();
        let mut order: Vec<String> = Vec::new();

        let branch_map: HashMap<&str, &FlowBranch> =
            flow.branches.iter().map(|b| (b.key.as_str(), b)).collect();

        fn visit(
            key: &str,
            flow_name: &str,
            branch_map: &HashMap<&str, &FlowBranch>,
            visited: &mut HashSet<String>,
            visiting: &mut HashSet<String>,
            order: &mut Vec<String>,
        ) -> Result<(), String> {
            if visited.contains(key) {
                return Ok(());
            }
            if visiting.contains(key) {
                return Err(format!(
                    "Circular dependency detected in flow '{}' involving branch '{}'",
                    flow_name, key
                ));
            }

            visiting.insert(key.to_string());

            if let Some(branch) = branch_map.get(key) {
                if let Some(ref deps) = branch.depends_on {
                    for dep in deps {
                        if !branch_map.contains_key(dep.as_str()) {
                            return Err(format!(
                                "Branch '{}' depends on unknown branch '{}'",
                                key, dep
                            ));
                        }
                        visit(dep, flow_name, branch_map, visited, visiting, order)?;
                    }
                }
            }

            visiting.remove(key);
            visited.insert(key.to_string());
            order.push(key.to_string());

            Ok(())
        }

        for branch in &flow.branches {
            visit(
                &branch.key,
                &flow.name,
                &branch_map,
                &mut visited,
                &mut visiting,
                &mut order,
            )?;
        }

        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_branch(key: &str, depends_on: Option<Vec<String>>, modules: Vec<&str>) -> FlowBranch {
        FlowBranch {
            key: key.to_string(),
            depends_on,
            modules: modules
                .into_iter()
                .map(|m| crate::ModuleRef {
                    key: m.to_string(),
                    args: None,
                })
                .collect(),
        }
    }

    #[test]
    fn test_topological_sort_simple() {
        let flow = ScriptFlow {
            name: "test".to_string(),
            branches: vec![
                make_branch("a", None, vec!["1"]),
                make_branch("b", Some(vec!["a".to_string()]), vec!["2"]),
                make_branch("c", Some(vec!["b".to_string()]), vec!["3"]),
            ],
        };

        let engine = ScriptFlowEngine::new();
        let order = engine.topological_sort(&flow).unwrap();

        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_topological_sort_cycle() {
        let flow = ScriptFlow {
            name: "cycle".to_string(),
            branches: vec![
                make_branch("a", Some(vec!["b".to_string()]), vec!["1"]),
                make_branch("b", Some(vec!["a".to_string()]), vec!["2"]),
            ],
        };

        let engine = ScriptFlowEngine::new();
        let result = engine.topological_sort(&flow);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Circular dependency"));
    }

    #[test]
    fn test_topological_sort_missing_dep() {
        let flow = ScriptFlow {
            name: "missing".to_string(),
            branches: vec![make_branch(
                "a",
                Some(vec!["nonexistent".to_string()]),
                vec!["1"],
            )],
        };

        let engine = ScriptFlowEngine::new();
        let result = engine.topological_sort(&flow);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown branch"));
    }

    #[tokio::test]
    async fn test_execute_passes_module_args() {
        use std::collections::HashMap;

        let mut args = HashMap::new();
        args.insert("env".to_string(), serde_json::json!("prod"));
        let flow = ScriptFlow {
            name: "args".to_string(),
            branches: vec![FlowBranch {
                key: "build".to_string(),
                depends_on: None,
                modules: vec![crate::ModuleRef {
                    key: "compile".to_string(),
                    args: Some(args),
                }],
            }],
        };

        let engine = ScriptFlowEngine::new();
        let result = engine
            .execute(&flow, |module, branch, module_args| async move {
                assert_eq!(module, "compile");
                assert_eq!(branch, "build");
                let env = module_args
                    .as_ref()
                    .and_then(|m| m.get("env"))
                    .expect("module args are forwarded");
                assert_eq!(env, &serde_json::json!("prod"));
                Ok("ok".to_string())
            })
            .await;

        assert!(result.success);
        assert!(result.branches["build"].success);
    }
}
