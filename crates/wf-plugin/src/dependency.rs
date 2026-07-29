use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;

#[derive(Debug, Clone)]
pub struct ResolvedGraph {
    pub load_order: Vec<String>,
    pub cycles: Vec<Vec<String>>,
    pub missing: Vec<String>,
    pub version_mismatches: Vec<String>,
}

pub fn resolve_dependencies(manifests: &[PluginManifest]) -> PluginResult<ResolvedGraph> {
    let mut version_mismatches = Vec::new();

    // Validate semver for all plugin versions
    for m in manifests {
        if semver::Version::parse(&m.version).is_err() {
            version_mismatches.push(format!("{}: invalid version '{}'", m.id, m.version));
        }
        if let Some(ref sdk) = m.sdk_version {
            if semver::VersionReq::parse(sdk).is_err() {
                version_mismatches.push(format!("{}: invalid sdk_version '{}'", m.id, sdk));
            }
        }
    }

    // Validate dependency version requirements
    for m in manifests {
        for (dep_id, req_str) in &m.dependencies {
            if let Some(dep) = manifests.iter().find(|d| d.id == *dep_id) {
                if let Ok(req) = semver::VersionReq::parse(req_str) {
                    if let Ok(ver) = semver::Version::parse(&dep.version) {
                        if !req.matches(&ver) {
                            version_mismatches.push(
                                format!("{}: dependency {} requires '{}', found '{}'", m.id, dep_id, req_str, dep.version)
                            );
                        }
                    }
                }
            }
        }
    }

    // Validate optional dependencies version when present
    for m in manifests {
        for (dep_id, req_str) in &m.optional_dependencies {
            if let Some(dep) = manifests.iter().find(|d| d.id == *dep_id) {
                if let Ok(req) = semver::VersionReq::parse(req_str) {
                    if let Ok(ver) = semver::Version::parse(&dep.version) {
                        if !req.matches(&ver) {
                            version_mismatches.push(
                                format!("{}: optional dependency {} requires '{}', found '{}'", m.id, dep_id, req_str, dep.version)
                            );
                        }
                    }
                }
            }
        }
    }

    let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut all_ids: HashSet<&str> = HashSet::new();

    for m in manifests {
        all_ids.insert(&m.id);
    }

    for m in manifests {
        let deps: Vec<&str> = m.dependencies.keys().map(|s| s.as_str()).collect();
        for dep in &deps {
            graph.entry(dep).or_default().push(&m.id);
        }
        graph.entry(&m.id).or_default();
    }

    let mut missing: Vec<String> = Vec::new();
    for key in graph.keys() {
        if !all_ids.contains(key) {
            let s = (*key).to_string();
            if !missing.contains(&s) {
                missing.push(s);
            }
        }
    }

    if !missing.is_empty() {
        let err = missing.join(", ");
        return Err(PluginError::DependencyNotSatisfied(err));
    }

    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    for id in &all_ids {
        in_degree.insert(id, 0);
    }
    for targets in graph.values() {
        for t in targets {
            *in_degree.entry(t).or_insert(0) += 1;
        }
    }

    let mut queue: VecDeque<&str> = VecDeque::new();
    for (id, degree) in &in_degree {
        if *degree == 0 {
            queue.push_back(id);
        }
    }

    let mut load_order = Vec::new();
    while let Some(node) = queue.pop_front() {
        load_order.push(node.to_string());
        if let Some(targets) = graph.get(node) {
            for t in targets {
                if let Some(degree) = in_degree.get_mut(t) {
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(t);
                    }
                }
            }
        }
    }

    let mut cycles = Vec::new();
    let processed: HashSet<&str> = load_order.iter().map(|s| s.as_str()).collect();
    let unprocessed: Vec<&&str> = all_ids.iter().filter(|id| !processed.contains(*id)).collect();

    if !unprocessed.is_empty() {
        let mut visited: HashSet<&str> = HashSet::new();
        let mut in_stack: HashSet<&str> = HashSet::new();

        for id in unprocessed {
            if !visited.contains(id) {
                let mut path = Vec::new();
                detect_cycle(&graph, id, &mut visited, &mut in_stack, &mut path, &mut cycles);
            }
        }
    }

    Ok(ResolvedGraph { load_order, cycles, missing, version_mismatches })
}

fn detect_cycle<'a>(
    graph: &HashMap<&'a str, Vec<&'a str>>,
    node: &'a str,
    visited: &mut HashSet<&'a str>,
    in_stack: &mut HashSet<&'a str>,
    path: &mut Vec<&'a str>,
    cycles: &mut Vec<Vec<String>>,
) {
    visited.insert(node);
    in_stack.insert(node);
    path.push(node);

    if let Some(targets) = graph.get(node) {
        for t in targets {
            if in_stack.contains(t) {
                let cycle_start = path.iter().position(|n| *n == *t).unwrap_or(0);
                let cycle: Vec<String> = path[cycle_start..].iter().map(|s| s.to_string()).collect();
                cycles.push(cycle);
            } else if !visited.contains(t) {
                detect_cycle(graph, t, visited, in_stack, path, cycles);
            }
        }
    }

    path.pop();
    in_stack.remove(node);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manifest(id: &str, deps: &[&str]) -> PluginManifest {
        let mut dependencies = std::collections::HashMap::new();
        for d in deps {
            dependencies.insert(d.to_string(), "1.0.0".to_string());
        }
        PluginManifest {
            id: id.to_string(),
            version: "1.0.0".to_string(),
            name: None,
            description: None,
            plugin_type: None,
            sdk_version: None,
            entry_point: "main.lua".to_string(),
            dependencies,
            optional_dependencies: std::collections::HashMap::new(),
            contributions: vec![],
            config: None,
            hooks: None,
        }
    }

    #[test]
    fn test_no_deps() {
        let manifests = vec![make_manifest("a", &[])];
        let graph = resolve_dependencies(&manifests).unwrap();
        assert_eq!(graph.load_order, vec!["a"]);
        assert!(graph.cycles.is_empty());
        assert!(graph.missing.is_empty());
    }

    #[test]
    fn test_linear_deps() {
        let manifests = vec![
            make_manifest("b", &["a"]),
            make_manifest("a", &[]),
        ];
        let graph = resolve_dependencies(&manifests).unwrap();
        assert_eq!(graph.load_order, vec!["a", "b"]);
        assert!(graph.cycles.is_empty());
    }

    #[test]
    fn test_missing_dep() {
        let manifests = vec![make_manifest("a", &["missing"])];
        let err = resolve_dependencies(&manifests).unwrap_err();
        assert!(matches!(err, PluginError::DependencyNotSatisfied(_)));
    }

    #[test]
    fn test_cycle_detection() {
        let manifests = vec![
            make_manifest("a", &["b"]),
            make_manifest("b", &["c"]),
            make_manifest("c", &["a"]),
        ];
        let graph = resolve_dependencies(&manifests).unwrap();
        assert!(!graph.cycles.is_empty());
        assert!(graph.load_order.len() < 3);
    }

    #[test]
    fn test_version_mismatch_detected() {
        let req = make_manifest("b", &["a"]);
        let mut manifests = vec![
            make_manifest("a", &[]),
            req,
        ];
        // b requires a@1.0.0, both at 1.0.0 — should match
        let graph = resolve_dependencies(&manifests).unwrap();
        assert!(graph.version_mismatches.is_empty());

        // Change b to require a@^2.0.0
        manifests[1].dependencies.insert("a".into(), "^2.0.0".into());
        let graph = resolve_dependencies(&manifests).unwrap();
        assert!(!graph.version_mismatches.is_empty());
    }

    #[test]
    fn test_invalid_plugin_version() {
        let mut m = make_manifest("bad-ver", &[]);
        m.version = "not-semver".into();
        let graph = resolve_dependencies(&[m]).unwrap();
        assert!(!graph.version_mismatches.is_empty());
    }

    #[test]
    fn test_complex_dag() {
        let manifests = vec![
            make_manifest("a", &[]),
            make_manifest("b", &["a"]),
            make_manifest("c", &["a"]),
            make_manifest("d", &["b", "c"]),
        ];
        let graph = resolve_dependencies(&manifests).unwrap();
        assert_eq!(graph.load_order.len(), 4);
        assert!(graph.load_order[0] == "a");
        assert!(graph.cycles.is_empty());

        let a_pos = graph.load_order.iter().position(|x| x == "a").unwrap();
        let b_pos = graph.load_order.iter().position(|x| x == "b").unwrap();
        let c_pos = graph.load_order.iter().position(|x| x == "c").unwrap();
        let d_pos = graph.load_order.iter().position(|x| x == "d").unwrap();
        assert!(a_pos < b_pos);
        assert!(a_pos < c_pos);
        assert!(b_pos < d_pos);
        assert!(c_pos < d_pos);
    }
}
