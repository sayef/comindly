use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageRow {
    pub repo: String,
    /// `pypi` | `npm` | `go` | `cargo`.
    pub ecosystem: String,
    /// Normalized package name.
    pub name: String,
    /// `true`: this repo publishes the package; `false`: it depends on it.
    pub publishes: bool,
    /// Published version, or the declared requirement for a dependency.
    pub version: Option<String>,
    /// Version pinned by the lockfile next to the manifest.
    pub locked: Option<String>,
    /// Repo-relative manifest path.
    pub manifest: String,
}

/// An internal dependency: `consumer` uses `name`, which `publisher` publishes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InternalDep {
    pub ecosystem: String,
    pub name: String,
    pub publisher: String,
    pub published: Option<String>,
    pub consumer: String,
    pub requirement: Option<String>,
    pub locked: Option<String>,
}

impl InternalDep {
    /// The consumer pins a version other than the one the publisher's source declares.
    pub fn skewed(&self) -> bool {
        matches!((&self.locked, &self.published), (Some(l), Some(p)) if l != p)
    }
}
