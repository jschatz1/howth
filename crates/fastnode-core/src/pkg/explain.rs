//! Package "why" explanation - dependency chain analysis.
//!
//! Provides functionality to explain why a package is installed by
//! analyzing the dependency graph and finding chains from root deps
//! to the target package.

use super::graph::{DepEdge, PackageGraph, PackageId};
use std::collections::{HashMap, HashSet, VecDeque};

/// Schema version for the why output format.
pub const PKG_WHY_SCHEMA_VERSION: u32 = 1;

/// Error codes for why operations.
pub mod why_codes {
    pub const PKG_WHY_ARGS_INVALID: &str = "PKG_WHY_ARGS_INVALID";
    pub const PKG_WHY_TARGET_NOT_FOUND: &str = "PKG_WHY_TARGET_NOT_FOUND";
    pub const PKG_WHY_TARGET_AMBIGUOUS: &str = "PKG_WHY_TARGET_AMBIGUOUS";
    pub const PKG_WHY_GRAPH_UNAVAILABLE: &str = "PKG_WHY_GRAPH_UNAVAILABLE";
    pub const PKG_WHY_MAX_CHAINS_REACHED: &str = "PKG_WHY_MAX_CHAINS_REACHED";
    pub const PKG_WHY_RESOLVER_TRACE_FAILED: &str = "PKG_WHY_RESOLVER_TRACE_FAILED";
}

/// The target package to explain.
#[derive(Debug, Clone, Default)]
pub struct WhyTarget {
    /// Package name (e.g., "react" or "@scope/foo").
    pub name: String,
    /// Package version if specified or resolved.
    pub version: Option<String>,
    /// Absolute package root path if known.
    pub path: Option<String>,
    /// Original input argument.
    pub input: String,
}

/// A single link in the dependency chain.
#[derive(Debug, Clone)]
pub struct WhyLink {
    /// Source package name (or "<root>" for root deps).
    pub from: String,
    /// Target package name.
    pub to: String,
    /// Version range requirement from package.json.
    pub req: Option<String>,
    /// Resolved version of the target.
    pub resolved_version: Option<String>,
    /// Resolved path of the target.
    pub resolved_path: Option<String>,
    /// Dependency kind: "dep", "dev", "optional", "peer".
    pub kind: String,
}

/// A complete chain from root to target.
#[derive(Debug, Clone)]
pub struct WhyChain {
    /// Links in order from root to target.
    pub links: Vec<WhyLink>,
}

/// Error information for why operations.
#[derive(Debug, Clone)]
pub struct WhyErrorInfo {
    /// Stable error code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Related path if applicable.
    pub path: Option<String>,
}

impl WhyErrorInfo {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            path: None,
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

/// Result of a "why" query.
#[derive(Debug, Clone, Default)]
pub struct PkgWhyResult {
    /// Schema version.
    pub schema_version: u32,
    /// Working directory used.
    pub cwd: String,
    /// The target being explained.
    pub target: WhyTarget,
    /// Whether target was found in `node_modules`.
    pub found_in_node_modules: bool,
    /// Whether target is an orphan (installed but not reachable).
    pub is_orphan: bool,
    /// Dependency chains from root to target.
    pub chains: Vec<WhyChain>,
    /// Additional notes (deterministic).
    pub notes: Vec<String>,
    /// Errors encountered.
    pub errors: Vec<WhyErrorInfo>,
}

impl PkgWhyResult {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self {
            schema_version: PKG_WHY_SCHEMA_VERSION,
            cwd: cwd.into(),
            ..Default::default()
        }
    }
}

/// Options for why computation.
#[derive(Debug, Clone)]
pub struct WhyOptions {
    /// Maximum number of chains to return.
    pub max_chains: usize,
    /// Prefer shortest paths.
    pub prefer_shortest: bool,
}

impl Default for WhyOptions {
    fn default() -> Self {
        Self {
            max_chains: 5,
            prefer_shortest: true,
        }
    }
}

/// Parsed why argument.
#[derive(Debug, Clone)]
pub struct ParsedWhyArg {
    /// Input kind.
    pub kind: WhyArgKind,
    /// Package name.
    pub name: String,
    /// Version if specified.
    pub version: Option<String>,
    /// Path if specified.
    pub path: Option<String>,
    /// Subpath for resolver trace (e.g., "./jsx-runtime").
    pub subpath: Option<String>,
}

/// Kind of why argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhyArgKind {
    /// Just a package name.
    Name,
    /// Package name with version.
    NameVersion,
    /// A filesystem path.
    Path,
    /// Package name with subpath (for resolver trace).
    NameSubpath,
}

/// Parse a why argument into its components.
///
/// Rules:
/// - If input contains path separators and looks like a path, treat as Path.
/// - If it contains `@` after a non-scope position, split as name@version.
/// - If it contains `/` after the package name, treat as name/subpath.
/// - Otherwise, treat as plain name.
#[must_use]
pub fn parse_why_arg(input: &str) -> ParsedWhyArg {
    let input = input.trim();

    // Check if it looks like a filesystem path
    if input.starts_with('/')
        || input.starts_with("./")
        || input.starts_with("../")
        || input.contains("node_modules/")
        || input.ends_with("package.json")
    {
        // Extract package name from path if possible
        let name = extract_package_name_from_path(input);
        return ParsedWhyArg {
            kind: WhyArgKind::Path,
            name: name.unwrap_or_default(),
            version: None,
            path: Some(input.to_string()),
            subpath: None,
        };
    }

    // Windows absolute path check
    if input.len() >= 3 {
        let chars: Vec<char> = input.chars().take(3).collect();
        if chars[0].is_ascii_alphabetic()
            && chars[1] == ':'
            && (chars[2] == '\\' || chars[2] == '/')
        {
            let name = extract_package_name_from_path(input);
            return ParsedWhyArg {
                kind: WhyArgKind::Path,
                name: name.unwrap_or_default(),
                version: None,
                path: Some(input.to_string()),
                subpath: None,
            };
        }
    }

    // Parse as package spec: name[@version][/subpath]
    let (name_part, version, subpath) = parse_package_spec(input);

    let kind = if version.is_some() {
        WhyArgKind::NameVersion
    } else if subpath.is_some() {
        WhyArgKind::NameSubpath
    } else {
        WhyArgKind::Name
    };

    ParsedWhyArg {
        kind,
        name: name_part,
        version,
        path: None,
        subpath,
    }
}

/// Parse a package spec into (name, version, subpath).
fn parse_package_spec(input: &str) -> (String, Option<String>, Option<String>) {
    // Handle scoped packages: @scope/name[@version][/subpath]
    if input.starts_with('@') {
        // Find the scope/name boundary
        let after_scope = input.get(1..).unwrap_or("");
        let scope_end = after_scope.find('/');

        if let Some(scope_slash) = scope_end {
            // We have @scope/...
            let scope = &input[..scope_slash + 2]; // includes @scope/
            let rest = &input[scope_slash + 2..];

            // Find package name end (next @ or /)
            let (pkg_name, remainder) = if let Some(at_pos) = rest.find('@') {
                // @scope/name@version...
                (&rest[..at_pos], Some(&rest[at_pos + 1..]))
            } else if let Some(slash_pos) = rest.find('/') {
                // @scope/name/subpath
                (&rest[..slash_pos], Some(&rest[slash_pos..]))
            } else {
                // Just @scope/name
                (rest, None)
            };

            let full_name = format!("{scope}{pkg_name}");

            match remainder {
                Some(r) if r.starts_with('/') => {
                    // Subpath
                    let subpath = format!(".{r}");
                    (full_name, None, Some(subpath))
                }
                Some(r) => {
                    // Version possibly with subpath
                    if let Some(slash_pos) = r.find('/') {
                        let version = &r[..slash_pos];
                        let subpath = format!(".{}", &r[slash_pos..]);
                        (full_name, Some(version.to_string()), Some(subpath))
                    } else {
                        (full_name, Some(r.to_string()), None)
                    }
                }
                None => (full_name, None, None),
            }
        } else {
            // Malformed scoped package, treat as name
            (input.to_string(), None, None)
        }
    } else {
        // Non-scoped: name[@version][/subpath]
        if let Some(at_pos) = input.find('@') {
            let name = &input[..at_pos];
            let rest = &input[at_pos + 1..];

            if let Some(slash_pos) = rest.find('/') {
                let version = &rest[..slash_pos];
                let subpath = format!(".{}", &rest[slash_pos..]);
                (name.to_string(), Some(version.to_string()), Some(subpath))
            } else {
                (name.to_string(), Some(rest.to_string()), None)
            }
        } else if let Some(slash_pos) = input.find('/') {
            let name = &input[..slash_pos];
            let subpath = format!(".{}", &input[slash_pos..]);
            (name.to_string(), None, Some(subpath))
        } else {
            (input.to_string(), None, None)
        }
    }
}

/// Extract package name from a `node_modules` path.
fn extract_package_name_from_path(path: &str) -> Option<String> {
    // Normalize separators
    let normalized = path.replace('\\', "/");

    // Find node_modules
    if let Some(nm_pos) = normalized.rfind("node_modules/") {
        let after_nm = &normalized[nm_pos + 13..]; // "node_modules/".len() == 13

        // Check for scoped package
        if after_nm.starts_with('@') {
            // @scope/name
            let parts: Vec<&str> = after_nm.splitn(3, '/').collect();
            if parts.len() >= 2 {
                return Some(format!("{}/{}", parts[0], parts[1]));
            }
        } else {
            // Regular package name
            let name = after_nm.split('/').next()?;
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }

    None
}

/// Compute why chains from a package graph.
#[must_use]
pub fn why_from_graph(graph: &PackageGraph, input: &str, opts: &WhyOptions) -> PkgWhyResult {
    let mut result = PkgWhyResult::new(&graph.root);

    // Parse the input
    let parsed = parse_why_arg(input);
    result.target.input = input.to_string();
    result.target.name.clone_from(&parsed.name);
    result.target.version.clone_from(&parsed.version);
    result.target.path.clone_from(&parsed.path);

    // Build lookup indices
    let (by_name, by_path) = build_indices(graph);

    // Find the target package
    let Some(target_id) = find_target(&parsed, &by_name, &by_path, &mut result) else {
        result.found_in_node_modules = false;
        return result;
    };

    result.found_in_node_modules = true;
    result.target.version = Some(target_id.version.clone());
    result.target.path = Some(target_id.path.clone());

    // Check if target is an orphan
    let orphan_paths: HashSet<_> = graph.orphans.iter().map(|o| &o.path).collect();
    result.is_orphan = orphan_paths.contains(&target_id.path);

    if result.is_orphan {
        result.notes.push(format!(
            "{} is installed but not reachable from root dependencies (orphan)",
            format_package_id(&target_id)
        ));
        return result;
    }

    // Build parent adjacency map
    let parents = build_parent_map(graph);

    // Find root-level packages (packages with no parents in the graph)
    let root_level_packages = find_root_level_packages(graph, &parents);

    // Find chains using BFS from target back to roots
    let chains = find_chains(
        &target_id,
        &parents,
        &root_level_packages,
        graph,
        opts.max_chains,
        opts.prefer_shortest,
    );

    if chains.is_empty() && !result.is_orphan {
        result.notes.push(
            "No parent links found (possibly root-level or graph was pruned by max_depth)"
                .to_string(),
        );
    }

    if chains.len() >= opts.max_chains {
        result.notes.push(format!(
            "Showing {} of potentially more chains (max_chains limit)",
            opts.max_chains
        ));
    }

    result.chains = chains;
    result
}

/// Build lookup indices from the graph.
fn build_indices(
    graph: &PackageGraph,
) -> (HashMap<String, Vec<PackageId>>, HashMap<String, PackageId>) {
    let mut by_name: HashMap<String, Vec<PackageId>> = HashMap::new();
    let mut by_path: HashMap<String, PackageId> = HashMap::new();

    // Index nodes
    for node in &graph.nodes {
        by_name
            .entry(node.id.name.clone())
            .or_default()
            .push(node.id.clone());
        by_path.insert(node.id.path.clone(), node.id.clone());
    }

    // Index orphans
    for orphan in &graph.orphans {
        by_name
            .entry(orphan.name.clone())
            .or_default()
            .push(orphan.clone());
        by_path.insert(orphan.path.clone(), orphan.clone());
    }

    // Sort entries for determinism
    for candidates in by_name.values_mut() {
        candidates.sort_by(|a, b| a.version.cmp(&b.version).then_with(|| a.path.cmp(&b.path)));
    }

    (by_name, by_path)
}

/// Find the target package in the indices.
fn find_target(
    parsed: &ParsedWhyArg,
    by_name: &HashMap<String, Vec<PackageId>>,
    by_path: &HashMap<String, PackageId>,
    result: &mut PkgWhyResult,
) -> Option<PackageId> {
    // If path is provided, match by path
    if let Some(ref path) = parsed.path {
        // Try exact match first
        if let Some(id) = by_path.get(path) {
            return Some(id.clone());
        }

        // Try to normalize and match
        let normalized = normalize_path(path);
        for (p, id) in by_path {
            if normalize_path(p) == normalized {
                return Some(id.clone());
            }
        }

        // Path not found
        result.errors.push(WhyErrorInfo::new(
            why_codes::PKG_WHY_TARGET_NOT_FOUND,
            format!("Package at path not found in node_modules: {path}"),
        ));
        return None;
    }

    // Match by name
    let candidates = match by_name.get(&parsed.name) {
        Some(c) if !c.is_empty() => c,
        _ => {
            result.errors.push(WhyErrorInfo::new(
                why_codes::PKG_WHY_TARGET_NOT_FOUND,
                format!("Package not found in node_modules: {}", parsed.name),
            ));
            return None;
        }
    };

    // If version specified, find exact match
    if let Some(ref version) = parsed.version {
        for candidate in candidates {
            if &candidate.version == version {
                return Some(candidate.clone());
            }
        }

        // Version not found
        result.errors.push(WhyErrorInfo::new(
            why_codes::PKG_WHY_TARGET_NOT_FOUND,
            format!(
                "Package {}@{} not found. Available versions: {}",
                parsed.name,
                version,
                candidates
                    .iter()
                    .map(|c| c.version.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
        return None;
    }

    // No version specified
    if candidates.len() == 1 {
        return Some(candidates[0].clone());
    }

    // Ambiguous - multiple versions
    result.errors.push(WhyErrorInfo::new(
        why_codes::PKG_WHY_TARGET_AMBIGUOUS,
        format!(
            "Multiple versions of {} found. Use {}@<version> to disambiguate.",
            parsed.name, parsed.name
        ),
    ));

    // Add stable candidate listing in notes (sorted by version, then path)
    let candidates_list: Vec<String> = candidates
        .iter()
        .map(|c| format!("{}@{} path={}", c.name, c.version, c.path))
        .collect();
    result
        .notes
        .push(format!("candidates: {}", candidates_list.join("; ")));

    // Return deterministic choice (first after sorting by version, then path)
    result.notes.push(format!(
        "Using {} (deterministic: smallest version+path)",
        format_package_id(&candidates[0])
    ));

    Some(candidates[0].clone())
}

/// Build a map of package -> list of (`parent_id`, edge).
fn build_parent_map(graph: &PackageGraph) -> HashMap<String, Vec<(PackageId, DepEdge)>> {
    let mut parents: HashMap<String, Vec<(PackageId, DepEdge)>> = HashMap::new();

    for node in &graph.nodes {
        for edge in &node.dependencies {
            if let Some(ref to) = edge.to {
                parents
                    .entry(to.path.clone())
                    .or_default()
                    .push((node.id.clone(), edge.clone()));
            }
        }
    }

    // Sort parents for determinism
    for p in parents.values_mut() {
        p.sort_by(|(a_id, a_edge), (b_id, b_edge)| {
            a_id.name
                .cmp(&b_id.name)
                .then_with(|| a_id.version.cmp(&b_id.version))
                .then_with(|| a_id.path.cmp(&b_id.path))
                .then_with(|| a_edge.name.cmp(&b_edge.name))
        });
    }

    parents
}

/// Find packages that appear to be root-level (no parents).
fn find_root_level_packages(
    graph: &PackageGraph,
    parents: &HashMap<String, Vec<(PackageId, DepEdge)>>,
) -> HashSet<String> {
    let mut root_level = HashSet::new();

    for node in &graph.nodes {
        if !parents.contains_key(&node.id.path) {
            root_level.insert(node.id.path.clone());
        }
    }

    root_level
}

/// Find chains from target back to roots using BFS.
fn find_chains(
    target: &PackageId,
    parents: &HashMap<String, Vec<(PackageId, DepEdge)>>,
    root_level: &HashSet<String>,
    graph: &PackageGraph,
    max_chains: usize,
    prefer_shortest: bool,
) -> Vec<WhyChain> {
    let mut chains = Vec::new();

    // If target is root-level, return a single chain with just root -> target
    if root_level.contains(&target.path) {
        let chain = WhyChain {
            links: vec![WhyLink {
                from: "<root>".to_string(),
                to: target.name.clone(),
                req: find_root_req(graph, &target.name),
                resolved_version: Some(target.version.clone()),
                resolved_path: Some(target.path.clone()),
                kind: "dep".to_string(),
            }],
        };
        chains.push(chain);
        return chains;
    }

    // BFS from target backwards
    // Each state is (current_path, chain_so_far)
    let mut queue: VecDeque<(String, Vec<(PackageId, DepEdge)>)> = VecDeque::new();
    let mut seen_chains: HashSet<Vec<String>> = HashSet::new();

    queue.push_back((target.path.clone(), Vec::new()));

    while let Some((current_path, path_so_far)) = queue.pop_front() {
        if chains.len() >= max_chains {
            break;
        }

        // Check if we've reached a root-level package
        if root_level.contains(&current_path) && !path_so_far.is_empty() {
            // Build the chain
            let chain = build_chain_from_path(&path_so_far, target, graph);

            // Check for duplicate chain
            let chain_key: Vec<String> = chain.links.iter().map(|l| l.to.clone()).collect();
            if seen_chains.insert(chain_key) {
                chains.push(chain);
            }
            continue;
        }

        // Get parents of current
        if let Some(parent_list) = parents.get(&current_path) {
            for (parent_id, edge) in parent_list {
                // Avoid cycles
                let already_visited = path_so_far.iter().any(|(id, _)| id.path == parent_id.path);
                if already_visited {
                    continue;
                }

                let mut new_path = path_so_far.clone();
                new_path.push((parent_id.clone(), edge.clone()));

                queue.push_back((parent_id.path.clone(), new_path));
            }
        }

        // If prefer_shortest and we have some chains, stop after this BFS layer
        if prefer_shortest && !chains.is_empty() && chains.len() >= max_chains {
            break;
        }
    }

    // Sort chains deterministically:
    // 1) shortest number of links first
    // 2) if tie, lexicographic compare over (to.name, to.version, to.path) sequence
    chains.sort_by(|a, b| {
        a.links.len().cmp(&b.links.len()).then_with(|| {
            // Compare by link contents: (to, resolved_version, resolved_path)
            for (la, lb) in a.links.iter().zip(b.links.iter()) {
                let cmp = la
                    .to
                    .cmp(&lb.to)
                    .then_with(|| {
                        la.resolved_version
                            .as_deref()
                            .unwrap_or("")
                            .cmp(lb.resolved_version.as_deref().unwrap_or(""))
                    })
                    .then_with(|| {
                        la.resolved_path
                            .as_deref()
                            .unwrap_or("")
                            .cmp(lb.resolved_path.as_deref().unwrap_or(""))
                    })
                    .then_with(|| la.from.cmp(&lb.from));
                if cmp != std::cmp::Ordering::Equal {
                    return cmp;
                }
            }
            // If one is longer than the other after zip, shorter comes first (already handled by len comparison)
            std::cmp::Ordering::Equal
        })
    });

    chains
}

/// Build a `WhyChain` from a path of (parent, edge) pairs.
fn build_chain_from_path(
    path: &[(PackageId, DepEdge)],
    target: &PackageId,
    graph: &PackageGraph,
) -> WhyChain {
    let mut links = Vec::new();

    // Path is in reverse order (target -> ... -> root), so we need to reverse
    let reversed: Vec<_> = path.iter().rev().collect();

    // First link: <root> -> first parent
    if let Some((first_parent, _)) = reversed.first() {
        links.push(WhyLink {
            from: "<root>".to_string(),
            to: first_parent.name.clone(),
            req: find_root_req(graph, &first_parent.name),
            resolved_version: Some(first_parent.version.clone()),
            resolved_path: Some(first_parent.path.clone()),
            kind: "dep".to_string(),
        });
    }

    // Middle links
    for i in 0..reversed.len() {
        if i + 1 < reversed.len() {
            let (current, _) = reversed[i];
            let (next, edge) = reversed[i + 1];

            links.push(WhyLink {
                from: current.name.clone(),
                to: next.name.clone(),
                req: edge.req.clone(),
                resolved_version: Some(next.version.clone()),
                resolved_path: Some(next.path.clone()),
                kind: edge.kind.clone(),
            });
        }
    }

    // Last link to target
    if let Some((last, edge)) = path.first() {
        links.push(WhyLink {
            from: last.name.clone(),
            to: target.name.clone(),
            req: edge.req.clone(),
            resolved_version: Some(target.version.clone()),
            resolved_path: Some(target.path.clone()),
            kind: edge.kind.clone(),
        });
    }

    WhyChain { links }
}

/// Find the version requirement for a root-level package.
fn find_root_req(_graph: &PackageGraph, _name: &str) -> Option<String> {
    // In the current graph model, root deps are the top-level nodes
    // We don't have explicit root package.json deps stored, so return None
    // A future improvement could parse root package.json
    None
}

/// Format a package ID for display.
fn format_package_id(id: &PackageId) -> String {
    format!("{}@{}", id.name, id.version)
}

/// Normalize a path for comparison.
fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::super::graph::PackageNode;
    use super::*;

    #[test]
    fn test_parse_why_arg_simple_name() {
        let parsed = parse_why_arg("react");
        assert_eq!(parsed.kind, WhyArgKind::Name);
        assert_eq!(parsed.name, "react");
        assert!(parsed.version.is_none());
        assert!(parsed.path.is_none());
        assert!(parsed.subpath.is_none());
    }

    #[test]
    fn test_parse_why_arg_name_version() {
        let parsed = parse_why_arg("react@18.2.0");
        assert_eq!(parsed.kind, WhyArgKind::NameVersion);
        assert_eq!(parsed.name, "react");
        assert_eq!(parsed.version, Some("18.2.0".to_string()));
    }

    #[test]
    fn test_parse_why_arg_scoped() {
        let parsed = parse_why_arg("@types/node");
        assert_eq!(parsed.kind, WhyArgKind::Name);
        assert_eq!(parsed.name, "@types/node");
    }

    #[test]
    fn test_parse_why_arg_scoped_version() {
        let parsed = parse_why_arg("@types/node@20.0.0");
        assert_eq!(parsed.kind, WhyArgKind::NameVersion);
        assert_eq!(parsed.name, "@types/node");
        assert_eq!(parsed.version, Some("20.0.0".to_string()));
    }

    #[test]
    fn test_parse_why_arg_subpath() {
        let parsed = parse_why_arg("react/jsx-runtime");
        assert_eq!(parsed.kind, WhyArgKind::NameSubpath);
        assert_eq!(parsed.name, "react");
        assert_eq!(parsed.subpath, Some("./jsx-runtime".to_string()));
    }

    #[test]
    fn test_parse_why_arg_path() {
        let parsed = parse_why_arg("./node_modules/react");
        assert_eq!(parsed.kind, WhyArgKind::Path);
        assert_eq!(parsed.name, "react");
        assert_eq!(parsed.path, Some("./node_modules/react".to_string()));
    }

    #[test]
    fn test_parse_why_arg_scoped_path() {
        let parsed = parse_why_arg("node_modules/@types/node");
        assert_eq!(parsed.kind, WhyArgKind::Path);
        assert_eq!(parsed.name, "@types/node");
    }

    #[test]
    fn test_extract_package_name_from_path() {
        assert_eq!(
            extract_package_name_from_path("/project/node_modules/react"),
            Some("react".to_string())
        );
        assert_eq!(
            extract_package_name_from_path("/project/node_modules/@types/node"),
            Some("@types/node".to_string())
        );
        assert_eq!(
            extract_package_name_from_path("/project/node_modules/react/index.js"),
            Some("react".to_string())
        );
    }

    #[test]
    fn test_why_simple_chain() {
        // Build a simple graph: root -> a -> b -> target
        let graph = PackageGraph {
            schema_version: 1,
            root: "/project".to_string(),
            nodes: vec![
                PackageNode {
                    id: PackageId {
                        name: "a".to_string(),
                        version: "1.0.0".to_string(),
                        path: "/project/node_modules/a".to_string(),
                        integrity: None,
                    },
                    dependencies: vec![DepEdge {
                        name: "b".to_string(),
                        req: Some("^1.0.0".to_string()),
                        to: Some(PackageId {
                            name: "b".to_string(),
                            version: "1.0.0".to_string(),
                            path: "/project/node_modules/b".to_string(),
                            integrity: None,
                        }),
                        kind: "dep".to_string(),
                    }],
                },
                PackageNode {
                    id: PackageId {
                        name: "b".to_string(),
                        version: "1.0.0".to_string(),
                        path: "/project/node_modules/b".to_string(),
                        integrity: None,
                    },
                    dependencies: vec![DepEdge {
                        name: "target".to_string(),
                        req: Some("^2.0.0".to_string()),
                        to: Some(PackageId {
                            name: "target".to_string(),
                            version: "2.0.0".to_string(),
                            path: "/project/node_modules/target".to_string(),
                            integrity: None,
                        }),
                        kind: "dep".to_string(),
                    }],
                },
                PackageNode {
                    id: PackageId {
                        name: "target".to_string(),
                        version: "2.0.0".to_string(),
                        path: "/project/node_modules/target".to_string(),
                        integrity: None,
                    },
                    dependencies: vec![],
                },
            ],
            orphans: vec![],
            errors: vec![],
        };

        let opts = WhyOptions::default();
        let result = why_from_graph(&graph, "target", &opts);

        assert!(result.found_in_node_modules);
        assert!(!result.is_orphan);
        assert!(!result.chains.is_empty());

        let chain = &result.chains[0];
        assert!(chain.links.len() >= 2);
        assert_eq!(chain.links[0].from, "<root>");
        assert_eq!(chain.links.last().unwrap().to, "target");
    }

    #[test]
    fn test_why_multiple_parents() {
        // Build graph: root -> a -> c, root -> b -> c
        let graph = PackageGraph {
            schema_version: 1,
            root: "/project".to_string(),
            nodes: vec![
                PackageNode {
                    id: PackageId {
                        name: "a".to_string(),
                        version: "1.0.0".to_string(),
                        path: "/project/node_modules/a".to_string(),
                        integrity: None,
                    },
                    dependencies: vec![DepEdge {
                        name: "c".to_string(),
                        req: Some("^1.0.0".to_string()),
                        to: Some(PackageId {
                            name: "c".to_string(),
                            version: "1.0.0".to_string(),
                            path: "/project/node_modules/c".to_string(),
                            integrity: None,
                        }),
                        kind: "dep".to_string(),
                    }],
                },
                PackageNode {
                    id: PackageId {
                        name: "b".to_string(),
                        version: "1.0.0".to_string(),
                        path: "/project/node_modules/b".to_string(),
                        integrity: None,
                    },
                    dependencies: vec![DepEdge {
                        name: "c".to_string(),
                        req: Some("^1.0.0".to_string()),
                        to: Some(PackageId {
                            name: "c".to_string(),
                            version: "1.0.0".to_string(),
                            path: "/project/node_modules/c".to_string(),
                            integrity: None,
                        }),
                        kind: "dep".to_string(),
                    }],
                },
                PackageNode {
                    id: PackageId {
                        name: "c".to_string(),
                        version: "1.0.0".to_string(),
                        path: "/project/node_modules/c".to_string(),
                        integrity: None,
                    },
                    dependencies: vec![],
                },
            ],
            orphans: vec![],
            errors: vec![],
        };

        let opts = WhyOptions::default();
        let result = why_from_graph(&graph, "c", &opts);

        assert!(result.found_in_node_modules);
        assert!(result.chains.len() >= 2, "Should have at least 2 chains");
    }

    #[test]
    fn test_why_orphan() {
        let graph = PackageGraph {
            schema_version: 1,
            root: "/project".to_string(),
            nodes: vec![],
            orphans: vec![PackageId {
                name: "orphan-pkg".to_string(),
                version: "1.0.0".to_string(),
                path: "/project/node_modules/orphan-pkg".to_string(),
                integrity: None,
            }],
            errors: vec![],
        };

        let opts = WhyOptions::default();
        let result = why_from_graph(&graph, "orphan-pkg", &opts);

        assert!(result.found_in_node_modules);
        assert!(result.is_orphan);
        assert!(result.chains.is_empty());
    }

    #[test]
    fn test_why_target_not_found() {
        let graph = PackageGraph {
            schema_version: 1,
            root: "/project".to_string(),
            nodes: vec![],
            orphans: vec![],
            errors: vec![],
        };

        let opts = WhyOptions::default();
        let result = why_from_graph(&graph, "nonexistent", &opts);

        assert!(!result.found_in_node_modules);
        assert!(!result.errors.is_empty());
        assert_eq!(result.errors[0].code, why_codes::PKG_WHY_TARGET_NOT_FOUND);
    }

    #[test]
    fn test_why_ambiguous_versions() {
        // Two versions of the same package
        let graph = PackageGraph {
            schema_version: 1,
            root: "/project".to_string(),
            nodes: vec![
                PackageNode {
                    id: PackageId {
                        name: "react".to_string(),
                        version: "17.0.2".to_string(),
                        path: "/project/node_modules/react-17".to_string(),
                        integrity: None,
                    },
                    dependencies: vec![],
                },
                PackageNode {
                    id: PackageId {
                        name: "react".to_string(),
                        version: "18.2.0".to_string(),
                        path: "/project/node_modules/react-18".to_string(),
                        integrity: None,
                    },
                    dependencies: vec![],
                },
            ],
            orphans: vec![],
            errors: vec![],
        };

        let opts = WhyOptions::default();
        let result = why_from_graph(&graph, "react", &opts);

        // Should still find a target (deterministic choice)
        assert!(result.found_in_node_modules);

        // Should have ambiguity error
        assert!(result
            .errors
            .iter()
            .any(|e| e.code == why_codes::PKG_WHY_TARGET_AMBIGUOUS));

        // Should have a note about which was chosen
        assert!(!result.notes.is_empty());
    }

    #[test]
    fn test_why_max_chains() {
        // Create a graph with many paths to target
        let graph = PackageGraph {
            schema_version: 1,
            root: "/project".to_string(),
            nodes: vec![
                PackageNode {
                    id: PackageId {
                        name: "a".to_string(),
                        version: "1.0.0".to_string(),
                        path: "/project/node_modules/a".to_string(),
                        integrity: None,
                    },
                    dependencies: vec![DepEdge {
                        name: "target".to_string(),
                        req: None,
                        to: Some(PackageId {
                            name: "target".to_string(),
                            version: "1.0.0".to_string(),
                            path: "/project/node_modules/target".to_string(),
                            integrity: None,
                        }),
                        kind: "dep".to_string(),
                    }],
                },
                PackageNode {
                    id: PackageId {
                        name: "b".to_string(),
                        version: "1.0.0".to_string(),
                        path: "/project/node_modules/b".to_string(),
                        integrity: None,
                    },
                    dependencies: vec![DepEdge {
                        name: "target".to_string(),
                        req: None,
                        to: Some(PackageId {
                            name: "target".to_string(),
                            version: "1.0.0".to_string(),
                            path: "/project/node_modules/target".to_string(),
                            integrity: None,
                        }),
                        kind: "dep".to_string(),
                    }],
                },
                PackageNode {
                    id: PackageId {
                        name: "target".to_string(),
                        version: "1.0.0".to_string(),
                        path: "/project/node_modules/target".to_string(),
                        integrity: None,
                    },
                    dependencies: vec![],
                },
            ],
            orphans: vec![],
            errors: vec![],
        };

        let opts = WhyOptions {
            max_chains: 1,
            prefer_shortest: true,
        };
        let result = why_from_graph(&graph, "target", &opts);

        assert_eq!(result.chains.len(), 1);
    }

    #[test]
    fn test_chains_are_sorted_and_primary_is_shortest_then_lex() {
        // Build graph where two shortest paths exist: a -> target, b -> target
        // Primary should be deterministic based on lex order (a < b)
        let graph = PackageGraph {
            schema_version: 1,
            root: "/project".to_string(),
            nodes: vec![
                PackageNode {
                    id: PackageId {
                        name: "b-pkg".to_string(),
                        version: "1.0.0".to_string(),
                        path: "/project/node_modules/b-pkg".to_string(),
                        integrity: None,
                    },
                    dependencies: vec![DepEdge {
                        name: "target".to_string(),
                        req: Some("^1.0.0".to_string()),
                        to: Some(PackageId {
                            name: "target".to_string(),
                            version: "1.0.0".to_string(),
                            path: "/project/node_modules/target".to_string(),
                            integrity: None,
                        }),
                        kind: "dep".to_string(),
                    }],
                },
                PackageNode {
                    id: PackageId {
                        name: "a-pkg".to_string(),
                        version: "1.0.0".to_string(),
                        path: "/project/node_modules/a-pkg".to_string(),
                        integrity: None,
                    },
                    dependencies: vec![DepEdge {
                        name: "target".to_string(),
                        req: Some("^1.0.0".to_string()),
                        to: Some(PackageId {
                            name: "target".to_string(),
                            version: "1.0.0".to_string(),
                            path: "/project/node_modules/target".to_string(),
                            integrity: None,
                        }),
                        kind: "dep".to_string(),
                    }],
                },
                PackageNode {
                    id: PackageId {
                        name: "target".to_string(),
                        version: "1.0.0".to_string(),
                        path: "/project/node_modules/target".to_string(),
                        integrity: None,
                    },
                    dependencies: vec![],
                },
            ],
            orphans: vec![],
            errors: vec![],
        };

        // Run twice and verify deterministic output
        let opts = WhyOptions {
            max_chains: 10,
            prefer_shortest: true,
        };

        let result1 = why_from_graph(&graph, "target", &opts);
        let result2 = why_from_graph(&graph, "target", &opts);

        // Should have same number of chains
        assert_eq!(result1.chains.len(), result2.chains.len());

        // Primary chain (first) should be the same
        assert!(!result1.chains.is_empty());
        let primary1 = &result1.chains[0];
        let primary2 = &result2.chains[0];

        assert_eq!(primary1.links.len(), primary2.links.len());
        for (l1, l2) in primary1.links.iter().zip(primary2.links.iter()) {
            assert_eq!(l1.from, l2.from);
            assert_eq!(l1.to, l2.to);
            assert_eq!(l1.resolved_version, l2.resolved_version);
        }

        // Primary should be via a-pkg (lexicographically first) since both have same length
        assert_eq!(primary1.links.len(), 2); // <root> -> a-pkg -> target
        assert!(primary1.links.iter().any(|l| l.to == "a-pkg"));
    }

    #[test]
    fn test_target_root_level_chain_is_root_to_target() {
        // Target has no parents - it's a root-level dep
        let graph = PackageGraph {
            schema_version: 1,
            root: "/project".to_string(),
            nodes: vec![PackageNode {
                id: PackageId {
                    name: "root-dep".to_string(),
                    version: "2.0.0".to_string(),
                    path: "/project/node_modules/root-dep".to_string(),
                    integrity: None,
                },
                dependencies: vec![],
            }],
            orphans: vec![],
            errors: vec![],
        };

        let opts = WhyOptions::default();
        let result = why_from_graph(&graph, "root-dep", &opts);

        assert!(result.found_in_node_modules);
        assert!(!result.is_orphan);
        assert_eq!(result.chains.len(), 1);

        // Chain should be <root> -> target
        let chain = &result.chains[0];
        assert_eq!(chain.links.len(), 1);
        assert_eq!(chain.links[0].from, "<root>");
        assert_eq!(chain.links[0].to, "root-dep");
        assert_eq!(chain.links[0].resolved_version, Some("2.0.0".to_string()));
    }

    #[test]
    fn test_ambiguous_target_notes_are_stable() {
        // Multiple candidates - notes should be deterministic and include stable candidate listing
        let graph = PackageGraph {
            schema_version: 1,
            root: "/project".to_string(),
            nodes: vec![
                PackageNode {
                    id: PackageId {
                        name: "ambig".to_string(),
                        version: "2.0.0".to_string(),
                        path: "/project/node_modules/ambig-v2".to_string(),
                        integrity: None,
                    },
                    dependencies: vec![],
                },
                PackageNode {
                    id: PackageId {
                        name: "ambig".to_string(),
                        version: "1.0.0".to_string(),
                        path: "/project/node_modules/ambig-v1".to_string(),
                        integrity: None,
                    },
                    dependencies: vec![],
                },
            ],
            orphans: vec![],
            errors: vec![],
        };

        let opts = WhyOptions::default();
        let result1 = why_from_graph(&graph, "ambig", &opts);
        let result2 = why_from_graph(&graph, "ambig", &opts);

        // Notes should be identical
        assert_eq!(result1.notes.len(), result2.notes.len());
        for (n1, n2) in result1.notes.iter().zip(result2.notes.iter()) {
            assert_eq!(n1, n2);
        }

        // Should have a "candidates:" note
        assert!(result1.notes.iter().any(|n| n.starts_with("candidates:")));

        // Should have a "Using" note
        assert!(result1.notes.iter().any(|n| n.starts_with("Using ")));

        // Chosen should be deterministic (1.0.0 < 2.0.0 lexicographically)
        assert_eq!(result1.target.version, Some("1.0.0".to_string()));
        assert_eq!(result2.target.version, Some("1.0.0".to_string()));
    }
}
