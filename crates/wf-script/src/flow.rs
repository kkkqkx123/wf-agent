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

    pub async fn execute<F, Fut>(
        &self,
        flow: &ScriptFlow,
        execute_module: F,
    ) -> FlowExecutionResult
    where
        F: Fn(&str, &str) -> Fut,
        Fut: std::future::Future<Output = ScriptResult<String>>,
    {
        let start = std::time::Instant::now();
        let mut branches = HashMap::new();

        let order = match self.topological_sort(flow) {
            Ok(o) => o,
            Err(e) => {
                return FlowExecutionResult {
                    success: false,
                    branches: HashMap::new(),
                    total_execution_time_ms: start.elapsed().as_millis() as u64,
                    error: Some(e),
                };
            }
        };

        for branch_key in &order {
            let Some(branch) = flow.branches.iter().find(|b| b.key == *branch_key) else {
                continue;
            };

            let branch_start = std::time::Instant::now();
            let mut module_results = Vec::new();

            for module_ref in &branch.modules {
                let result = match execute_module(&module_ref.key, branch_key).await {
                    Ok(output) => FlowBranchExecutionResult {
                        success: true,
                        module_key: module_ref.key.clone(),
                        output: Some(output),
                        error: None,
                        execution_time_ms: branch_start.elapsed().as_millis() as u64,
                    },
                    Err(e) => FlowBranchExecutionResult {
                        success: false,
                        module_key: module_ref.key.clone(),
                        output: None,
                        error: Some(e.to_string()),
                        execution_time_ms: branch_start.elapsed().as_millis() as u64,
                    },
                };
                module_results.push(result);
            }

            let branch_success = module_results.iter().all(|r| r.success);

            branches.insert(
                branch_key.clone(),
                BranchExecutionResult {
                    success: branch_success,
                    modules: module_results,
                    execution_time_ms: branch_start.elapsed().as_millis() as u64,
                },
            );
        }

        let all_success = branches.values().all(|b| b.success);

        FlowExecutionResult {
            success: all_success,
            branches,
            total_execution_time_ms: start.elapsed().as_millis() as u64,
            error: None,
        }
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
        assert!(result.unwrap_err().contains("Circular dependency"));    }

    #[test]
    fn test_topological_sort_missing_dep() {
        let flow = ScriptFlow {
            name: "missing".to_string(),
            branches: vec![make_branch("a", Some(vec!["nonexistent".to_string()]), vec!["1"])],
        };

        let engine = ScriptFlowEngine::new();
        let result = engine.topological_sort(&flow);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown branch"));
    }
}
