use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Convention {
    /// Repo the convention holds in, or [`ORG`] for the org baseline.
    pub repo: String,
    /// `naming`, `signatures`, `models`, `errors`, `layout`, `logging`, `config`, `libraries`,
    /// `testing`.
    pub dimension: String,
    /// Stable identity of the measured choice, shared across repos (`naming:functions:snake_case`).
    pub key: String,
    pub rule: String,
    /// The rule without repo-specific counts, names or paths (used for the org baseline).
    pub generic: String,
    pub support: usize,
    pub total: usize,
    /// What `support` and `total` count, e.g. `functions with parameters`.
    #[serde(default)]
    pub unit: String,
    /// Org baseline only: the repositories that share it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<String>,
    /// `path:line` of representative, central instances.
    pub exemplars: Vec<String>,
    /// `path:line` of instances that deviate.
    pub counterexamples: Vec<String>,
    #[serde(default)]
    pub origin: Origin,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    #[default]
    Measured,
    /// Measured, with the rule rewritten by a person.
    Edited,
    /// Written by a person.
    Manual,
}

impl Convention {
    /// Stable handle for edits: a manual entry's id, or a hash of repo and key.
    pub fn id(&self) -> String {
        match self.key.strip_prefix(MANUAL) {
            Some(id) => id.to_string(),
            None => measured_id(&self.repo, &self.key),
        }
    }
}

/// A person's change: a new convention, or an edit or hiding (`replaces`) of a measured one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manual {
    pub id: String,
    /// Repo, or [`ORG`] for the whole org.
    pub repo: String,
    pub dimension: String,
    /// The rule; for a hidden convention, the rule that was hidden.
    pub rule: String,
    /// Key of the measured convention this overrides.
    #[serde(default)]
    pub replaces: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    pub actor: String,
    pub updated: String,
}

impl Manual {
    pub fn new(repo: &str, dimension: &str, rule: &str, actor: &str) -> Manual {
        let id = uuid::Uuid::now_v7().simple().to_string();
        Manual {
            id: format!("c{}", &id[id.len() - 10..]),
            repo: repo.into(),
            dimension: dimension.trim().to_lowercase(),
            rule: rule.trim().into(),
            replaces: None,
            hidden: false,
            actor: actor.into(),
            updated: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.rule.trim().is_empty() || self.rule.chars().count() > 2000 {
            bail!("rule must be 1–2000 characters");
        }
        if self.dimension.is_empty() || self.dimension.len() > 100 {
            bail!("dimension must be 1–100 characters");
        }
        if let Some(why) = crate::ledger::suspicious(&self.rule) {
            bail!("rule rejected: {why}");
        }
        Ok(())
    }
}

impl Convention {
    /// How strictly the code follows it: `Always`, `Usually`, `Often` or `Sometimes`; `None` when
    /// nothing was counted (the team's own).
    pub fn strength(&self) -> Option<&'static str> {
        if self.total == 0 || self.counts_only() {
            return None;
        }
        let p = self.support as f64 / self.total as f64;
        Some(match p {
            p if p >= 0.95 => "Always",
            p if p >= 0.8 => "Usually",
            p if p >= 0.6 => "Often",
            _ => "Sometimes",
        })
    }

    /// Too few instances to call it settled.
    pub fn few(&self) -> bool {
        self.repos.is_empty() && self.total > 0 && self.total < FEW
    }

    /// What it was measured from, e.g. `Followed by 412 of the 530 functions with parameters.`
    pub fn evidence(&self) -> Option<String> {
        if !self.repos.is_empty() {
            return Some(format!("Shared by {}.", and_list(&self.repos)));
        }
        (self.total > 0).then(|| {
            let unit = if self.unit.is_empty() {
                "places"
            } else {
                &self.unit
            };
            let (n, of) = (thousands(self.support), thousands(self.total));
            match (self.counts_only(), self.support == self.total) {
                (true, _) => format!("Extended by {n} {unit}."),
                (false, true) => format!("Followed by all {of} {unit}."),
                (false, false) => format!("Followed by {n} of the {of} {unit}."),
            }
        })
    }

    /// A pattern counted where it occurs, with nothing that could deviate from it.
    fn counts_only(&self) -> bool {
        self.key.starts_with("models:base:")
    }
}

pub const MANUAL: &str = "manual:";

pub fn measured_id(repo: &str, key: &str) -> String {
    let h = blake3::hash(format!("{repo}\n{key}").as_bytes()).to_hex();
    format!("m{}", &h[..10])
}

/// `1689` as `1,689`.
pub fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Below this many instances a convention is marked as having few examples.
pub const FEW: usize = 10;

pub fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Repo name used for org-wide conventions.
pub const ORG: &str = "*";
