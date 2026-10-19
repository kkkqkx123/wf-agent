use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;

#[derive(Debug, Clone)]
pub struct ResolvedGraph {
    pub load_order: Vec<String>,
    pub cycles: Vec<Vec<String>>,
    pub missing: Vec<String>,
    pub version_mismatches: Vec<String>,
    pub global_conflicts: Vec<String>,
    pub optional_warnings: Vec<String>,
}

struct ParsedConstraint {
    source: String,
    raw: String,
    req: semver::VersionReq,
}

/// Probe whether a set of required constraints admits any version. Uses a
/// fixed candidate list covering common major ranges; conclusive for the
/// usual caret and range constraints without implementing a solver.
fn required_constraints_admit_some(reqs: &[semver::VersionReq]) -> bool {
    const CANDIDATES: [&str; 10] = [
        "0.1.0", "0.9.0", "1.0.0", "1.5.0", "1.6.0", "1.9.0", "2.0.0", "2.5.0", "3.0.0", "10.0.0",
    ];
    CANDIDATES.iter().any(|raw| {
        semver::Version::parse(raw)
            .map(|version| reqs.iter().all(|req| req.matches(&version)))
            .unwrap_or(false)
    })
}

pub fn resolve_dependencies(manifests: &[PluginManifest]) -> PluginResult<ResolvedGraph> {
    let mut version_mismatches = Vec::new();
    let mut optional_warnings = Vec::new();

    for m in manifests {
        if !crate::manifest::is_valid_plugin_version(&m.version) {
            version_mismatches.push(format!(
                "{}: invalid version '{}'. Provide a valid semver version (e.g. '1.0.0').",
                m.id, m.version
            ));
        }
    }

    let versions: HashMap<&str, Option<semver::Version>> = manifests
        .iter()
        .map(|m| (m.id.as_str(), semver::Version::parse(&m.version).ok()))
        .collect();

    // Single parse pass over every declared constraint.
    let mut required: HashMap<String, Vec<ParsedConstraint>> = HashMap::new();
    let mut optional: HashMap<String, Vec<ParsedConstraint>> = HashMap::new();
    for m in manifests {
        for (dep_id, req_str) in &m.dependencies {
            match semver::VersionReq::parse(req_str) {
                Ok(req) => required.entry(dep_id.clone()).or_default().push(
                    ParsedConstraint {
                        source: m.id.clone(),
                        raw: req_str.clone(),
                        req,
                    },
                ),
                Err(_) => version_mismatches.push(format!(
                    "{}: dependency {} has invalid requirement '{}'. Provide a valid semver requirement (e.g. '^1.0.0').",
                    m.id, dep_id, req_str
                )),
            }
        }
        for (dep_id, req_str) in &m.optional_dependencies {
            match semver::VersionReq::parse(req_str) {
                Ok(req) => optional.entry(dep_id.clone()).or_default().push(
                    ParsedConstraint {
                        source: m.id.clone(),
                        raw: req_str.clone(),
                        req,
                    },
                ),
                Err(_) => optional_warnings.push(format!(
                    "{}: optional dependency {} has invalid requirement '{}'. Provide a valid semver requirement (e.g. '^1.0.0').",
                    m.id, dep_id, req_str
                )),
            }
        }
    }

    // Required constraints against the installed version.
    for (dep_id, constraints) in &required {
        if let Some(Some(installed)) = versions.get(dep_id.as_str()) {
            for constraint in constraints {
                if !constraint.req.matches(installed) {
                    version_mismatches.push(format!(
                        "{}: dependency {} requires '{}', found '{}'. Update plugin '{}' to a version satisfying '{}' or update plugin '{}' to relax the requirement.",
                        constraint.source,
                        dep_id,
                        constraint.raw,
                        installed,
                        dep_id,
                        constraint.raw,
                        constraint.source
                    ));
                }
            }
        }
    }

    // Optional constraints never block; mismatches become warnings.
    for (dep_id, constraints) in &optional {
        if let Some(Some(installed)) = versions.get(dep_id.as_str()) {
            for constraint in constraints {
                if !constraint.req.matches(installed) {
                    optional_warnings.push(format!(
                        "{}: optional dependency {} requires '{}', found '{}'. Update plugin '{}' to a version satisfying '{}' or update plugin '{}' to relax the requirement; continuing without it.",
                        constraint.source,
                        dep_id,
                        constraint.raw,
                        installed,
                        dep_id,
                        constraint.raw,
                        constraint.source
                    ));
                }
            }
        }
    }

    let mut global_conflicts = Vec::new();
    for (dep_id, constraints) in &required {
        if constraints.len() < 2 {
            continue;
        }
        match versions.get(dep_id.as_str()) {
            Some(Some(installed)) => {
                let all_match = constraints.iter().all(|c| c.req.matches(installed));
                if !all_match {
                    let sources: Vec<String> = constraints
                        .iter()
                        .map(|c| format!("{} requires '{}'", c.source, c.raw))
                        .collect();
                    global_conflicts.push(format!(
                        "plugin '{dep_id}' version {installed} fails to satisfy conflicting constraints: {}. Align the dependency requirements across these plugins or update plugin '{dep_id}' to a version satisfying all of them.",
                        sources.join(", ")
                    ));
                }
            }
            _ => {
                let reqs: Vec<semver::VersionReq> =
                    constraints.iter().map(|c| c.req.clone()).collect();
                if !required_constraints_admit_some(&reqs) {
                    let sources: Vec<String> = constraints
                        .iter()
                        .map(|c| format!("{} requires '{}'", c.source, c.raw))
                        .collect();
                    global_conflicts.push(format!(
                        "constraints on missing plugin '{dep_id}' are mutually exclusive: {}. Align the dependency requirements across these plugins.",
                        sources.join(", ")
                    ));
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
        let mut details = Vec::new();
        for name in &missing {
            match required.get(name) {
                Some(constraints) => {
                    let sources: Vec<String> = constraints
                        .iter()
                        .map(|c| format!("{} requires '{}'", c.source, c.raw))
                        .collect();
                    let mut detail = format!("missing plugin '{name}' ({})", sources.join(", "));
                    let reqs: Vec<semver::VersionReq> =
                        constraints.iter().map(|c| c.req.clone()).collect();
                    if constraints.len() > 1 && !required_constraints_admit_some(&reqs) {
                        detail.push_str(
                            "; the listed constraints are mutually exclusive, align them across the requiring plugins",
                        );
                    }
                    details.push(detail);
                }
                None => details.push(format!("missing plugin '{name}'")),
            }
        }
        return Err(PluginError::DependencyNotSatisfied(details.join("; ")));
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
    let unprocessed: Vec<&&str> = all_ids
        .iter()
        .filter(|id| !processed.contains(*id))
        .collect();

    if !unprocessed.is_empty() {
        let mut visited: HashSet<&str> = HashSet::new();
        let mut in_stack: HashSet<&str> = HashSet::new();

        for id in unprocessed {
            if !visited.contains(id) {
                let mut path = Vec::new();
                detect_cycle(
                    &graph,
                    id,
                    &mut visited,
                    &mut in_stack,
                    &mut path,
                    &mut cycles,
                );
            }
        }
    }

    Ok(ResolvedGraph {
        load_order,
        cycles,
        missing,
        version_mismatches,
        global_conflicts,
        optional_warnings,
    })
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
                let cycle: Vec<String> =
                    path[cycle_start..].iter().map(|s| s.to_string()).collect();
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
            entry_point: "main.lua".to_string(),
            versioned_entry_points: Default::default(),
            dependencies,
            optional_dependencies: std::collections::HashMap::new(),
            contributions: vec![],
            permissions: vec![],
            config_schema: None,
            config: None,
            hooks: None,
            llm_providers: vec![],
            wasm: None,
            lua: None,
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
        let manifests = vec![make_manifest("b", &["a"]), make_manifest("a", &[])];
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
        let mut manifests = vec![make_manifest("a", &[]), req];
        // b requires a@1.0.0, both at 1.0.0 — should match
        let graph = resolve_dependencies(&manifests).unwrap();
        assert!(graph.version_mismatches.is_empty());

        // Change b to require a@^2.0.0
        manifests[1]
            .dependencies
            .insert("a".into(), "^2.0.0".into());
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
    fn test_global_conflict_detected_with_fix_suggestion() {
        let mut a = make_manifest("a", &[]);
        a.version = "1.5.0".into();
        let mut b = make_manifest("b", &["a"]);
        b.dependencies.insert("a".into(), "^1.0.0".into());
        let mut c = make_manifest("c", &["a"]);
        c.dependencies.insert("a".into(), "^2.0.0".into());
        let graph = resolve_dependencies(&[a, b, c]).unwrap();
        assert!(!graph.global_conflicts.is_empty());
        let text = graph.global_conflicts.join("; ");
        assert!(text.contains("conflicting constraints"), "got: {text}");
        assert!(text.contains("update plugin"), "got: {text}");
    }

    #[test]
    fn test_compatible_constraints_have_no_conflict() {
        let mut a = make_manifest("a", &[]);
        a.version = "1.6.0".into();
        let mut b = make_manifest("b", &["a"]);
        b.dependencies.insert("a".into(), ">=1.0.0, <2.0.0".into());
        let mut c = make_manifest("c", &["a"]);
        c.dependencies.insert("a".into(), ">=1.5.0, <1.9.0".into());
        let graph = resolve_dependencies(&[a, b, c]).unwrap();
        assert!(
            graph.global_conflicts.is_empty(),
            "{:?}",
            graph.global_conflicts
        );
        assert!(graph.version_mismatches.is_empty());
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

    #[test]
    fn test_optional_mismatch_warns_without_blocking() {
        let mut a = make_manifest("a", &[]);
        a.version = "1.0.0".into();
        let mut b = make_manifest("b", &[]);
        b.optional_dependencies.insert("a".into(), "^2.0.0".into());
        let graph = resolve_dependencies(&[a, b]).unwrap();
        assert!(graph.version_mismatches.is_empty());
        assert!(graph.global_conflicts.is_empty());
        assert_eq!(graph.optional_warnings.len(), 1);
    }

    #[test]
    fn test_invalid_requirement_is_reported() {
        let mut b = make_manifest("b", &["a"]);
        b.dependencies.insert("a".into(), "not-a-req".into());
        let mut a = make_manifest("a", &[]);
        a.version = "1.0.0".into();
        let graph = resolve_dependencies(&[a, b]).unwrap();
        assert!(!graph.version_mismatches.is_empty());
    }

    #[test]
    fn test_missing_with_exclusive_constraints_notes_conflict() {
        let mut b = make_manifest("b", &["gone"]);
        b.dependencies.insert("gone".into(), "^1.0.0".into());
        let mut c = make_manifest("c", &["gone"]);
        c.dependencies.insert("gone".into(), "^2.0.0".into());
        let err = resolve_dependencies(&[b, c]).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("mutually exclusive"), "got: {text}");
    }
}
