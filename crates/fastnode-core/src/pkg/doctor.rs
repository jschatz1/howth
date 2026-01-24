//! Package health diagnostics.
//!
//! Provides read-only analysis of installed `node_modules/` to detect
//! common dependency health issues: orphans, missing edges, duplicates, etc.
//!
//! # JSON Contract (v1.7.1+)
//!
//! - `doctor.schema_version` is currently `1`. Breaking changes require a bump.
//! - Top-level JSON keys are exactly: `{ "ok", "doctor" }` (+ optional `"error"`).
//! - `doctor` keys are exactly: `{ "schema_version", "cwd", "summary", "findings", "notes" }`.
//! - `notes` is **always** serialized as an array (possibly empty).
//! - Each finding includes required keys `{ "code", "severity", "message" }` and may
//!   optionally include `{ "package", "path", "detail", "related" }`. No other keys.
//!
//! # Sort Order (LOCKED v1.7.1+)
//!
//! Findings are sorted by: `severity_rank` desc (error=3, warn=2, info=1),
//! then `code`, then `package`, then `path`. Truncation notice is always last.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::graph::{GraphErrorInfo, PackageGraph};

/// Schema version for doctor report output.
pub const PKG_DOCTOR_SCHEMA_VERSION: u32 = 1;

/// Doctor finding codes.
pub mod codes {
    pub const PKG_DOCTOR_NODE_MODULES_NOT_FOUND: &str = "PKG_DOCTOR_NODE_MODULES_NOT_FOUND";
    pub const PKG_DOCTOR_GRAPH_ERROR: &str = "PKG_DOCTOR_GRAPH_ERROR";
    pub const PKG_DOCTOR_ORPHAN_PACKAGE: &str = "PKG_DOCTOR_ORPHAN_PACKAGE";
    pub const PKG_DOCTOR_MISSING_EDGE_TARGET: &str = "PKG_DOCTOR_MISSING_EDGE_TARGET";
    pub const PKG_DOCTOR_INVALID_PACKAGE_JSON: &str = "PKG_DOCTOR_INVALID_PACKAGE_JSON";
    pub const PKG_DOCTOR_DUPLICATE_PACKAGE_VERSION: &str = "PKG_DOCTOR_DUPLICATE_PACKAGE_VERSION";
    pub const PKG_DOCTOR_MAX_ITEMS_REACHED: &str = "PKG_DOCTOR_MAX_ITEMS_REACHED";
}

/// Severity levels for doctor findings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum DoctorSeverity {
    #[default]
    Info,
    Warn,
    Error,
}

impl DoctorSeverity {
    /// Parse severity from string.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "info" => Some(Self::Info),
            "warn" | "warning" => Some(Self::Warn),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    /// Convert to string for JSON serialization.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    /// Get numeric rank for deterministic sorting.
    /// Higher values sort first (error=3, warn=2, info=1).
    /// This is the LOCKED sort order for v1.7.1+.
    #[must_use]
    pub const fn rank(&self) -> u8 {
        match self {
            Self::Error => 3,
            Self::Warn => 2,
            Self::Info => 1,
        }
    }
}

impl Serialize for DoctorSeverity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DoctorSeverity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("invalid severity: {s}")))
    }
}

/// A single diagnostic finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorFinding {
    /// Stable error code.
    pub code: String,
    /// Severity level.
    pub severity: DoctorSeverity,
    /// Human-readable message.
    pub message: String,
    /// Package name (and optionally @version) when relevant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// Absolute path when relevant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Small deterministic detail payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Small list of related names/paths for context.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub related: Vec<String>,
}

impl DoctorFinding {
    /// Create a new finding.
    #[must_use]
    pub fn new(code: &str, severity: DoctorSeverity, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            severity,
            message: message.into(),
            package: None,
            path: None,
            detail: None,
            related: Vec::new(),
        }
    }

    /// Set the package field.
    #[must_use]
    pub fn with_package(mut self, pkg: impl Into<String>) -> Self {
        self.package = Some(pkg.into());
        self
    }

    /// Set the path field.
    #[must_use]
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Set the detail field.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Set the related field.
    #[must_use]
    pub fn with_related(mut self, related: Vec<String>) -> Self {
        self.related = related;
        self
    }
}

/// Get a deterministic sort key for a finding.
///
/// **LOCKED SORT ORDER (v1.7.1+):**
/// 1. `severity_rank` (error=3, warn=2, info=1) - descending (errors first)
/// 2. `code` (byte lexicographic)
/// 3. `package` (byte lexicographic, `None` sorts last)
/// 4. `path` (byte lexicographic, `None` sorts last)
///
/// This ordering is applied:
/// - Before truncation
/// - Before severity filtering
/// - Before JSON or human rendering
///
/// The truncation notice (`PKG_DOCTOR_MAX_ITEMS_REACHED`) is appended AFTER
/// sorting and is NOT re-sorted, so it always appears last.
#[must_use]
pub fn doctor_sort_key(
    f: &DoctorFinding,
) -> (std::cmp::Reverse<u8>, &str, Option<&str>, Option<&str>) {
    (
        std::cmp::Reverse(f.severity.rank()),
        &f.code,
        f.package.as_deref(),
        f.path.as_deref(),
    )
}

/// Counts of findings by severity.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DoctorCounts {
    pub info: u32,
    pub warn: u32,
    pub error: u32,
}

/// Summary of the doctor report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorSummary {
    /// Overall severity (worst of all findings).
    pub severity: DoctorSeverity,
    /// Counts by severity.
    pub counts: DoctorCounts,
    /// Number of packages indexed in graph.
    pub packages_indexed: u32,
    /// Number of reachable packages.
    pub reachable_packages: u32,
    /// Number of orphan packages.
    pub orphans: u32,
    /// Number of missing edge targets.
    pub missing_edges: u32,
    /// Number of invalid packages.
    pub invalid_packages: u32,
}

impl Default for DoctorSummary {
    fn default() -> Self {
        Self {
            severity: DoctorSeverity::Info,
            counts: DoctorCounts::default(),
            packages_indexed: 0,
            reachable_packages: 0,
            orphans: 0,
            missing_edges: 0,
            invalid_packages: 0,
        }
    }
}

/// The complete doctor report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkgDoctorReport {
    /// Schema version for this output format.
    pub schema_version: u32,
    /// Absolute working directory.
    pub cwd: String,
    /// Summary statistics.
    pub summary: DoctorSummary,
    /// All findings (sorted deterministically).
    pub findings: Vec<DoctorFinding>,
    /// Notes (always present, may be empty array).
    /// **LOCKED (v1.7.1+):** This field is always serialized.
    #[serde(default)]
    pub notes: Vec<String>,
}

impl PkgDoctorReport {
    /// Create a new empty report.
    #[must_use]
    pub fn new(cwd: impl Into<String>) -> Self {
        Self {
            schema_version: PKG_DOCTOR_SCHEMA_VERSION,
            cwd: cwd.into(),
            summary: DoctorSummary::default(),
            findings: Vec::new(),
            notes: Vec::new(),
        }
    }
}

/// Options for doctor analysis.
#[derive(Debug, Clone)]
pub struct DoctorOptions {
    /// Include root devDependencies in graph traversal.
    pub include_dev_root: bool,
    /// Include optionalDependencies in graph traversal.
    pub include_optional: bool,
    /// Maximum traversal depth.
    pub max_depth: usize,
    /// Maximum number of findings to return (default 200, hard cap 2000).
    pub max_items: usize,
    /// Minimum severity to include in output.
    pub min_severity: DoctorSeverity,
}

impl Default for DoctorOptions {
    fn default() -> Self {
        Self {
            include_dev_root: false,
            include_optional: true,
            max_depth: 25,
            max_items: 200,
            min_severity: DoctorSeverity::Info,
        }
    }
}

/// Build a doctor report from a package graph.
///
/// This is the main entry point for package health diagnostics.
/// It analyzes the graph for common issues and returns a deterministic report.
#[must_use]
pub fn build_doctor_report(
    graph: &PackageGraph,
    cwd_abs: &str,
    opts: &DoctorOptions,
) -> PkgDoctorReport {
    let mut report = PkgDoctorReport::new(cwd_abs);
    let mut all_findings: Vec<DoctorFinding> = Vec::new();

    // Track metrics
    let mut missing_edges_count = 0u32;
    let mut invalid_packages_count = 0u32;

    // 2.1: Ingest graph errors
    for error in &graph.errors {
        let (finding, is_invalid) = map_graph_error(error);
        all_findings.push(finding);
        if is_invalid {
            invalid_packages_count += 1;
        }
    }

    // 2.2: Orphan packages
    for orphan in &graph.orphans {
        let hint = format!(
            "installed but not reachable from root dependencies\n  \
             hint: howth pkg why {}",
            orphan.name
        );
        let finding =
            DoctorFinding::new(codes::PKG_DOCTOR_ORPHAN_PACKAGE, DoctorSeverity::Warn, hint)
                .with_package(format!("{}@{}", orphan.name, orphan.version))
                .with_path(&orphan.path);
        all_findings.push(finding);
    }

    // 2.3: Missing edge targets
    for node in &graph.nodes {
        for edge in &node.dependencies {
            if edge.to.is_none() {
                missing_edges_count += 1;
                let detail = format!(
                    "missing: {} req={} kind={}",
                    edge.name,
                    edge.req.as_deref().unwrap_or("-"),
                    edge.kind
                );
                let hint = format!(
                    "dependency is declared but not installed\n  \
                     hint: howth pkg explain {} --parent {}",
                    edge.name, node.id.path
                );
                let finding = DoctorFinding::new(
                    codes::PKG_DOCTOR_MISSING_EDGE_TARGET,
                    DoctorSeverity::Warn,
                    hint,
                )
                .with_package(format!("{}@{}", node.id.name, node.id.version))
                .with_detail(detail)
                .with_related(vec![edge.name.clone()]);
                all_findings.push(finding);
            }
        }
    }

    // 2.4: Duplicate versions (best-effort)
    let duplicates = detect_duplicate_versions(graph);
    for (name, versions) in duplicates {
        let severity = if versions.len() > 3 {
            DoctorSeverity::Warn
        } else {
            DoctorSeverity::Info
        };

        // Build detail string with up to 5 versions
        let detail = build_duplicate_detail(&versions);
        let finding = DoctorFinding::new(
            codes::PKG_DOCTOR_DUPLICATE_PACKAGE_VERSION,
            severity,
            "multiple versions installed",
        )
        .with_package(name)
        .with_detail(detail);
        all_findings.push(finding);
    }

    // Sort all findings deterministically using LOCKED sort order (v1.7.1+)
    all_findings.sort_by(|a, b| doctor_sort_key(a).cmp(&doctor_sort_key(b)));

    // Filter by severity
    let filtered: Vec<DoctorFinding> = all_findings
        .into_iter()
        .filter(|f| f.severity >= opts.min_severity)
        .collect();

    // Compute counts before truncation
    let mut counts = DoctorCounts::default();
    for f in &filtered {
        match f.severity {
            DoctorSeverity::Info => counts.info += 1,
            DoctorSeverity::Warn => counts.warn += 1,
            DoctorSeverity::Error => counts.error += 1,
        }
    }

    // Truncate if needed
    let max_items = opts.max_items.min(2000);
    let (mut findings, truncated) = if filtered.len() > max_items {
        let truncated_count = filtered.len() - max_items;
        (
            filtered.into_iter().take(max_items).collect::<Vec<_>>(),
            Some(truncated_count),
        )
    } else {
        (filtered, None)
    };

    // Add truncation notice if needed (LOCKED v1.7.1+: notice is always last, never re-sorted)
    if let Some(truncated_count) = truncated {
        findings.push(
            DoctorFinding::new(
                codes::PKG_DOCTOR_MAX_ITEMS_REACHED,
                DoctorSeverity::Info,
                "maximum findings limit reached",
            )
            .with_detail(format!("omitted={truncated_count} max_items={max_items}")),
        );
    }

    // Compute summary
    let overall_severity = if counts.error > 0 {
        DoctorSeverity::Error
    } else if counts.warn > 0 {
        DoctorSeverity::Warn
    } else {
        DoctorSeverity::Info
    };

    // Count packages indexed = reachable + orphans
    let reachable = graph.nodes.len() as u32;
    let orphans = graph.orphans.len() as u32;
    let packages_indexed = reachable + orphans;

    report.summary = DoctorSummary {
        severity: overall_severity,
        counts,
        packages_indexed,
        reachable_packages: reachable,
        orphans,
        missing_edges: missing_edges_count,
        invalid_packages: invalid_packages_count,
    };
    report.findings = findings;

    report
}

/// Map a graph error to a doctor finding.
fn map_graph_error(error: &GraphErrorInfo) -> (DoctorFinding, bool) {
    use super::graph::codes as graph_codes;

    let (code, severity, message, is_invalid) = match error.code.as_str() {
        graph_codes::PKG_GRAPH_NODE_MODULES_NOT_FOUND => (
            codes::PKG_DOCTOR_NODE_MODULES_NOT_FOUND,
            DoctorSeverity::Error,
            "node_modules not found",
            false,
        ),
        graph_codes::PKG_GRAPH_PACKAGE_JSON_INVALID => (
            codes::PKG_DOCTOR_INVALID_PACKAGE_JSON,
            DoctorSeverity::Warn,
            "invalid package.json",
            true,
        ),
        graph_codes::PKG_GRAPH_PACKAGE_JSON_MISSING => (
            codes::PKG_DOCTOR_INVALID_PACKAGE_JSON,
            DoctorSeverity::Warn,
            "missing package.json",
            true,
        ),
        _ => (
            codes::PKG_DOCTOR_GRAPH_ERROR,
            DoctorSeverity::Warn,
            "graph construction error",
            false,
        ),
    };

    let finding = DoctorFinding::new(code, severity, message)
        .with_path(&error.path)
        .with_detail(&error.message);

    (finding, is_invalid)
}

/// Detect packages with multiple versions installed.
fn detect_duplicate_versions(graph: &PackageGraph) -> Vec<(String, Vec<(String, String)>)> {
    // Collect all versions by name from both nodes and orphans
    let mut by_name: HashMap<String, Vec<(String, String)>> = HashMap::new();

    for node in &graph.nodes {
        by_name
            .entry(node.id.name.clone())
            .or_default()
            .push((node.id.version.clone(), node.id.path.clone()));
    }

    for orphan in &graph.orphans {
        by_name
            .entry(orphan.name.clone())
            .or_default()
            .push((orphan.version.clone(), orphan.path.clone()));
    }

    // Filter to names with >1 distinct version and sort
    let mut duplicates: Vec<(String, Vec<(String, String)>)> = by_name
        .into_iter()
        .filter(|(_, versions)| {
            // Check if there are distinct versions
            let mut seen: Vec<&str> = versions.iter().map(|(v, _)| v.as_str()).collect();
            seen.sort_unstable();
            seen.dedup();
            seen.len() > 1
        })
        .map(|(name, mut versions)| {
            // Sort versions deterministically by (version, path)
            versions.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
            (name, versions)
        })
        .collect();

    // Sort by name for determinism
    duplicates.sort_by(|a, b| a.0.cmp(&b.0));
    duplicates
}

/// Build detail string for duplicate versions.
fn build_duplicate_detail(versions: &[(String, String)]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let max_show = 5;

    for (version, path) in versions.iter().take(max_show) {
        parts.push(format!("{version}@{path}"));
    }

    let remaining = versions.len().saturating_sub(max_show);
    if remaining > 0 {
        parts.push(format!("...(+{remaining} more)"));
    }

    format!("versions: {}", parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::graph::{DepEdge, PackageId, PackageNode};

    fn make_graph(
        nodes: Vec<PackageNode>,
        orphans: Vec<PackageId>,
        errors: Vec<GraphErrorInfo>,
    ) -> PackageGraph {
        PackageGraph {
            schema_version: 1,
            root: "/test".to_string(),
            nodes,
            orphans,
            errors,
        }
    }

    #[test]
    fn test_doctor_reports_orphans() {
        let orphan = PackageId::new(
            "orphan-pkg".to_string(),
            "1.0.0".to_string(),
            "/test/node_modules/orphan-pkg".to_string(),
        );
        let graph = make_graph(vec![], vec![orphan], vec![]);

        let opts = DoctorOptions::default();
        let report = build_doctor_report(&graph, "/test", &opts);

        assert_eq!(report.schema_version, PKG_DOCTOR_SCHEMA_VERSION);
        assert_eq!(report.summary.orphans, 1);
        assert_eq!(report.findings.len(), 1);

        let finding = &report.findings[0];
        assert_eq!(finding.code, codes::PKG_DOCTOR_ORPHAN_PACKAGE);
        assert_eq!(finding.severity, DoctorSeverity::Warn);
        assert_eq!(finding.package, Some("orphan-pkg@1.0.0".to_string()));
        assert_eq!(
            finding.path,
            Some("/test/node_modules/orphan-pkg".to_string())
        );
    }

    #[test]
    fn test_doctor_reports_missing_edges() {
        let node = PackageNode::new(
            PackageId::new(
                "a".to_string(),
                "1.0.0".to_string(),
                "/test/node_modules/a".to_string(),
            ),
            vec![DepEdge::new(
                "missing-dep".to_string(),
                Some("^1.0.0".to_string()),
                None, // Not resolved
                "dep",
            )],
        );
        let graph = make_graph(vec![node], vec![], vec![]);

        let opts = DoctorOptions::default();
        let report = build_doctor_report(&graph, "/test", &opts);

        assert_eq!(report.summary.missing_edges, 1);
        assert_eq!(report.findings.len(), 1);

        let finding = &report.findings[0];
        assert_eq!(finding.code, codes::PKG_DOCTOR_MISSING_EDGE_TARGET);
        assert_eq!(finding.package, Some("a@1.0.0".to_string()));
        assert!(finding.detail.as_ref().unwrap().contains("missing-dep"));
        assert_eq!(finding.related, vec!["missing-dep"]);
    }

    #[test]
    fn test_doctor_reports_graph_errors() {
        let error = GraphErrorInfo::new(
            crate::pkg::graph::codes::PKG_GRAPH_PACKAGE_JSON_INVALID,
            "/test/node_modules/bad/package.json",
            "Invalid JSON syntax",
        );
        let graph = make_graph(vec![], vec![], vec![error]);

        let opts = DoctorOptions::default();
        let report = build_doctor_report(&graph, "/test", &opts);

        assert_eq!(report.summary.invalid_packages, 1);
        assert_eq!(report.findings.len(), 1);

        let finding = &report.findings[0];
        assert_eq!(finding.code, codes::PKG_DOCTOR_INVALID_PACKAGE_JSON);
        assert_eq!(finding.severity, DoctorSeverity::Warn);
    }

    #[test]
    fn test_doctor_duplicate_versions_best_effort() {
        // Create two nodes with same name but different versions
        let node1 = PackageNode::new(
            PackageId::new(
                "dup".to_string(),
                "1.0.0".to_string(),
                "/test/node_modules/dup".to_string(),
            ),
            vec![],
        );
        let orphan = PackageId::new(
            "dup".to_string(),
            "2.0.0".to_string(),
            "/test/node_modules/other/node_modules/dup".to_string(),
        );
        let graph = make_graph(vec![node1], vec![orphan], vec![]);

        let opts = DoctorOptions::default();
        let report = build_doctor_report(&graph, "/test", &opts);

        // Should have orphan finding + duplicate finding
        assert!(report
            .findings
            .iter()
            .any(|f| f.code == codes::PKG_DOCTOR_DUPLICATE_PACKAGE_VERSION));

        let dup_finding = report
            .findings
            .iter()
            .find(|f| f.code == codes::PKG_DOCTOR_DUPLICATE_PACKAGE_VERSION)
            .unwrap();
        assert_eq!(dup_finding.package, Some("dup".to_string()));
        assert_eq!(dup_finding.severity, DoctorSeverity::Info); // 2 versions = info
    }

    #[test]
    fn test_doctor_deterministic_sorting_and_truncation() {
        // Create multiple findings of different severities
        let mut nodes = Vec::new();
        let mut orphans = Vec::new();

        // Add orphans (warn)
        for i in 0..5 {
            orphans.push(PackageId::new(
                format!("orphan-{}", i),
                "1.0.0".to_string(),
                format!("/test/node_modules/orphan-{}", i),
            ));
        }

        // Add nodes with missing deps (warn)
        for i in 0..5 {
            nodes.push(PackageNode::new(
                PackageId::new(
                    format!("pkg-{}", i),
                    "1.0.0".to_string(),
                    format!("/test/node_modules/pkg-{}", i),
                ),
                vec![DepEdge::new(
                    format!("missing-{}", i),
                    Some("^1.0.0".to_string()),
                    None,
                    "dep",
                )],
            ));
        }

        let graph = make_graph(nodes, orphans, vec![]);

        // Test with max_items = 3
        let opts = DoctorOptions {
            max_items: 3,
            ..Default::default()
        };
        let report = build_doctor_report(&graph, "/test", &opts);

        // Should have 3 findings + 1 truncation notice
        assert_eq!(report.findings.len(), 4);
        assert_eq!(
            report.findings.last().unwrap().code,
            codes::PKG_DOCTOR_MAX_ITEMS_REACHED
        );

        // Counts should reflect all filtered findings (before truncation)
        assert_eq!(report.summary.counts.warn, 10); // 5 orphans + 5 missing edges
    }

    #[test]
    fn test_doctor_sort_key_determinism() {
        // Verify the locked sort order: severity_rank desc, code asc, package asc, path asc
        let f_error = DoctorFinding::new(
            codes::PKG_DOCTOR_GRAPH_ERROR,
            DoctorSeverity::Error,
            "error",
        )
        .with_package("z-pkg");

        let f_warn_a = DoctorFinding::new(
            codes::PKG_DOCTOR_MISSING_EDGE_TARGET,
            DoctorSeverity::Warn,
            "warn a",
        )
        .with_package("a-pkg");

        let f_warn_z = DoctorFinding::new(
            codes::PKG_DOCTOR_ORPHAN_PACKAGE,
            DoctorSeverity::Warn,
            "warn z",
        )
        .with_package("z-pkg");

        let f_info = DoctorFinding::new(
            codes::PKG_DOCTOR_DUPLICATE_PACKAGE_VERSION,
            DoctorSeverity::Info,
            "info",
        )
        .with_package("a-pkg");

        let mut findings = vec![
            f_info.clone(),
            f_warn_z.clone(),
            f_error.clone(),
            f_warn_a.clone(),
        ];
        findings.sort_by(|a, b| doctor_sort_key(a).cmp(&doctor_sort_key(b)));

        // Error should be first (highest rank)
        assert_eq!(findings[0].severity, DoctorSeverity::Error);
        // Then warns, sorted by code
        assert_eq!(findings[1].code, codes::PKG_DOCTOR_MISSING_EDGE_TARGET);
        assert_eq!(findings[2].code, codes::PKG_DOCTOR_ORPHAN_PACKAGE);
        // Info should be last
        assert_eq!(findings[3].severity, DoctorSeverity::Info);
    }

    #[test]
    fn test_truncation_notice_is_last() {
        // Create many findings to trigger truncation
        let mut orphans = Vec::new();
        for i in 0..10 {
            orphans.push(PackageId::new(
                format!("orphan-{}", i),
                "1.0.0".to_string(),
                format!("/test/node_modules/orphan-{}", i),
            ));
        }

        let graph = make_graph(vec![], orphans, vec![]);

        let opts = DoctorOptions {
            max_items: 5,
            ..Default::default()
        };
        let report = build_doctor_report(&graph, "/test", &opts);

        // Truncation notice should be last
        let last = report.findings.last().unwrap();
        assert_eq!(last.code, codes::PKG_DOCTOR_MAX_ITEMS_REACHED);
        assert_eq!(last.message, "maximum findings limit reached");
        assert!(last.detail.as_ref().unwrap().contains("omitted=5"));
    }

    #[test]
    fn test_severity_ordering() {
        // Ensure error > warn > info in sorting
        assert!(DoctorSeverity::Error > DoctorSeverity::Warn);
        assert!(DoctorSeverity::Warn > DoctorSeverity::Info);
    }

    #[test]
    fn test_doctor_node_modules_not_found() {
        let error = GraphErrorInfo::new(
            crate::pkg::graph::codes::PKG_GRAPH_NODE_MODULES_NOT_FOUND,
            "/test/node_modules",
            "node_modules directory not found",
        );
        let graph = make_graph(vec![], vec![], vec![error]);

        let opts = DoctorOptions::default();
        let report = build_doctor_report(&graph, "/test", &opts);

        assert_eq!(report.summary.severity, DoctorSeverity::Error);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(
            report.findings[0].code,
            codes::PKG_DOCTOR_NODE_MODULES_NOT_FOUND
        );
        assert_eq!(report.findings[0].severity, DoctorSeverity::Error);
    }

    #[test]
    fn test_severity_filtering() {
        let orphan = PackageId::new(
            "orphan".to_string(),
            "1.0.0".to_string(),
            "/test/node_modules/orphan".to_string(),
        );
        // Create duplicate which is info severity
        let node1 = PackageNode::new(
            PackageId::new(
                "dup".to_string(),
                "1.0.0".to_string(),
                "/test/node_modules/dup".to_string(),
            ),
            vec![],
        );
        let dup_orphan = PackageId::new(
            "dup".to_string(),
            "2.0.0".to_string(),
            "/test/node_modules/dup2".to_string(),
        );

        let graph = make_graph(vec![node1], vec![orphan, dup_orphan], vec![]);

        // Filter to warn and above
        let opts = DoctorOptions {
            min_severity: DoctorSeverity::Warn,
            ..Default::default()
        };
        let report = build_doctor_report(&graph, "/test", &opts);

        // Should not include duplicate finding (info) but should include orphans (warn)
        assert!(report
            .findings
            .iter()
            .all(|f| f.severity >= DoctorSeverity::Warn));
        assert!(!report
            .findings
            .iter()
            .any(|f| f.code == codes::PKG_DOCTOR_DUPLICATE_PACKAGE_VERSION));
    }

    /// **LOCKED v1.7.1+**: Summary counts and severity reflect filtered findings only.
    #[test]
    fn test_severity_filtering_affects_summary_counts() {
        // Create error, warn, and info findings
        let error = GraphErrorInfo::new(
            crate::pkg::graph::codes::PKG_GRAPH_NODE_MODULES_NOT_FOUND,
            "/test/node_modules",
            "not found",
        );
        let orphan = PackageId::new(
            "orphan".to_string(),
            "1.0.0".to_string(),
            "/test/node_modules/orphan".to_string(),
        );
        let node1 = PackageNode::new(
            PackageId::new(
                "dup".to_string(),
                "1.0.0".to_string(),
                "/test/node_modules/dup".to_string(),
            ),
            vec![],
        );
        let dup_orphan = PackageId::new(
            "dup".to_string(),
            "2.0.0".to_string(),
            "/test/node_modules/dup2".to_string(),
        );

        let graph = make_graph(vec![node1], vec![orphan, dup_orphan], vec![error]);

        // Without filtering: 1 error + 2 orphan warns + 1 duplicate info
        let opts_all = DoctorOptions::default();
        let report_all = build_doctor_report(&graph, "/test", &opts_all);
        assert_eq!(report_all.summary.counts.error, 1);
        assert_eq!(report_all.summary.counts.warn, 2);
        assert_eq!(report_all.summary.counts.info, 1);
        assert_eq!(report_all.summary.severity, DoctorSeverity::Error);

        // Filter to warn+: should NOT include info count
        let opts_warn = DoctorOptions {
            min_severity: DoctorSeverity::Warn,
            ..Default::default()
        };
        let report_warn = build_doctor_report(&graph, "/test", &opts_warn);
        assert_eq!(report_warn.summary.counts.error, 1);
        assert_eq!(report_warn.summary.counts.warn, 2);
        assert_eq!(
            report_warn.summary.counts.info, 0,
            "LOCKED: info count should be 0 when filtered"
        );
        assert_eq!(report_warn.summary.severity, DoctorSeverity::Error);

        // Filter to error only: should only have error count
        let opts_error = DoctorOptions {
            min_severity: DoctorSeverity::Error,
            ..Default::default()
        };
        let report_error = build_doctor_report(&graph, "/test", &opts_error);
        assert_eq!(report_error.summary.counts.error, 1);
        assert_eq!(
            report_error.summary.counts.warn, 0,
            "LOCKED: warn count should be 0 when filtered"
        );
        assert_eq!(
            report_error.summary.counts.info, 0,
            "LOCKED: info count should be 0 when filtered"
        );
        assert_eq!(report_error.summary.severity, DoctorSeverity::Error);
    }

    /// **LOCKED v1.7.1+**: When all findings are filtered out, severity = info.
    #[test]
    fn test_severity_filtering_empty_becomes_info() {
        // Create only info findings
        let node1 = PackageNode::new(
            PackageId::new(
                "dup".to_string(),
                "1.0.0".to_string(),
                "/test/node_modules/dup".to_string(),
            ),
            vec![],
        );
        let dup_orphan = PackageId::new(
            "dup".to_string(),
            "2.0.0".to_string(),
            "/test/node_modules/dup2".to_string(),
        );

        let graph = make_graph(vec![node1], vec![dup_orphan], vec![]);

        // Filter to error: should have no findings
        let opts = DoctorOptions {
            min_severity: DoctorSeverity::Error,
            ..Default::default()
        };
        let report = build_doctor_report(&graph, "/test", &opts);

        // No findings pass the filter
        assert!(report.findings.is_empty());
        // LOCKED: severity defaults to info when no findings
        assert_eq!(report.summary.severity, DoctorSeverity::Info);
        assert_eq!(report.summary.counts.error, 0);
        assert_eq!(report.summary.counts.warn, 0);
        assert_eq!(report.summary.counts.info, 0);
    }

    /// **LOCKED v1.7.1+**: notes field is always serialized (even when empty).
    #[test]
    fn test_notes_always_serialized() {
        let report = PkgDoctorReport::new("/test");
        let json = serde_json::to_value(&report).unwrap();

        // notes field must be present even when empty
        assert!(
            json.get("notes").is_some(),
            "LOCKED: notes field must always be present"
        );
        assert!(json["notes"].as_array().unwrap().is_empty());
    }
}
