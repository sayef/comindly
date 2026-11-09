use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::{Fingerprint, Symbol};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Intent,
    Decision,
    Attempt,
    Constraint,
    Gotcha,
    Question,
    /// A relation to another record (supersede, retract, confirm, dispute).
    Relation,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "intent" => Kind::Intent,
            "decision" => Kind::Decision,
            "attempt" => Kind::Attempt,
            "constraint" => Kind::Constraint,
            "gotcha" => Kind::Gotcha,
            "question" => Kind::Question,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Intent => "intent",
            Kind::Decision => "decision",
            Kind::Attempt => "attempt",
            Kind::Constraint => "constraint",
            Kind::Gotcha => "gotcha",
            Kind::Question => "question",
            Kind::Relation => "relation",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    Supersedes,
    Retracts,
    Confirms,
    Disputes,
}

impl RelationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RelationKind::Supersedes => "supersede",
            RelationKind::Retracts => "retract",
            RelationKind::Confirms => "confirm",
            RelationKind::Disputes => "dispute",
        }
    }

    pub fn parse(s: &str) -> Option<RelationKind> {
        Some(match s {
            "confirm" | "confirms" => RelationKind::Confirms,
            "retract" | "retracts" => RelationKind::Retracts,
            "supersede" | "supersedes" => RelationKind::Supersedes,
            "dispute" | "disputes" => RelationKind::Disputes,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relation {
    pub kind: RelationKind,
    pub target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorKind {
    Human,
    Agent,
    /// Legacy (removed git harvest and distiller); kept so older records still read.
    Harvested,
    Distilled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub author: AuthorKind,
    /// Pseudonymous actor id (no names or emails).
    pub actor: String,
    pub source: String,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub session: Option<String>,
    /// The person's name, or the person an agent works for; absent with `record_names = false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The writer's teams when it was written; empty for older records and outside teams.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub teams: Vec<String>,
}

/// Who writes records: a pseudonymous actor id and, when names are kept, a name.
#[derive(Debug, Clone, Default)]
pub struct Identity {
    pub actor: String,
    pub name: Option<String>,
}

impl Provenance {
    pub fn human(source: &str, who: &Identity) -> Provenance {
        Provenance {
            author: AuthorKind::Human,
            actor: who.actor.clone(),
            source: source.into(),
            agent: None,
            session: None,
            name: who.name.clone(),
            teams: Vec::new(),
        }
    }

    pub fn agent(source: &str, agent: Option<String>, who: &Identity) -> Provenance {
        Provenance {
            author: AuthorKind::Agent,
            agent,
            ..Provenance::human(source, who)
        }
    }

    /// Who, for people to read: "Ada Lovelace", "claude-code for Ada Lovelace", "an agent".
    pub fn who(&self) -> String {
        match (self.author, self.name.as_deref(), self.agent.as_deref()) {
            (AuthorKind::Agent, Some(n), Some(a)) => format!("{a} for {n}"),
            (AuthorKind::Agent, Some(n), None) => format!("an agent for {n}"),
            (AuthorKind::Agent, None, Some(a)) => a.to_string(),
            (AuthorKind::Agent, None, None) => "an agent".into(),
            (AuthorKind::Human, Some(n), _) => n.to_string(),
            (AuthorKind::Human, None, _) => "a person".into(),
            (AuthorKind::Harvested, ..) => "git history".into(),
            (AuthorKind::Distilled, ..) => "the distiller".into(),
        }
    }
}

/// Where a record points: a symbol as it was when the record was written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    pub symbol: String,
    pub repo: String,
    pub path: String,
    pub name: String,
    pub line: u32,
    #[serde(default)]
    pub fingerprint: Option<Fingerprint>,
    /// The indexed commit of the anchor's repository when the record was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

impl Anchor {
    pub fn of(s: &Symbol) -> Anchor {
        Anchor {
            symbol: s.id.render(),
            repo: s.repo.0.clone(),
            path: s.file_path.clone(),
            name: s.name.clone(),
            line: s.range.start.line,
            fingerprint: s.fingerprint,
            commit: None,
        }
    }

    /// With the commit `commits` (by repository) records for the anchor's repository.
    pub fn at(mut self, commits: &std::collections::HashMap<String, String>) -> Anchor {
        self.commit = commits.get(&self.repo).cloned();
        self
    }
}

/// Code that shaped a record, as it was at `commit`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snippet {
    pub path: String,
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub schema: u32,
    pub kind: Kind,
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// When a future reader needs this ("relevant when adding caching to auth paths").
    #[serde(default)]
    pub trigger: Option<String>,
    /// Kind-specific fields (`alternatives`, `outcome`, `rule`, `severity`, …).
    #[serde(default)]
    pub fields: BTreeMap<String, serde_json::Value>,
    pub anchors: Vec<Anchor>,
    /// `commit:<sha>`, `mr:!412`, `test:<path>::<name>`, `url:…`, or free text.
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snippets: Vec<Snippet>,
    #[serde(default)]
    pub relation: Option<Relation>,
    pub provenance: Provenance,
    /// RFC 3339, UTC.
    pub created: String,
    /// Optional SSH signature over the rest of the record .
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<Signature>,
}

impl Record {
    pub fn new(kind: Kind, title: &str, anchors: Vec<Anchor>, provenance: Provenance) -> Record {
        Record {
            id: uuid::Uuid::now_v7().to_string(),
            schema: SCHEMA,
            kind,
            title: title.trim().to_string(),
            body: String::new(),
            trigger: None,
            fields: BTreeMap::new(),
            anchors,
            evidence: Vec::new(),
            snippets: Vec::new(),
            relation: None,
            provenance,
            created: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            signature: None,
        }
    }

    /// The relation record for a review `action` (see [`review_action`]) on `target`.
    pub fn review(
        target: &Record,
        action: &str,
        reason: &str,
        provenance: Provenance,
    ) -> Result<Record> {
        let (kind, retire) = review_action(action).with_context(|| {
            format!("unknown action `{action}`: confirm, archive, invalidate, supersede, dispute or retract")
        })?;
        let mut r = Record::relation(kind, target, reason, provenance);
        if let Some(why) = retire {
            r.fields
                .insert("retire".into(), serde_json::Value::String(why.into()));
        }
        Ok(r)
    }

    /// A relation record (`confirm`, `retract`, …) about `target`.
    pub fn relation(
        kind: RelationKind,
        target: &Record,
        reason: &str,
        provenance: Provenance,
    ) -> Record {
        let title = if reason.trim().is_empty() {
            format!("{kind:?}: {}", target.title)
        } else {
            reason.trim().to_string()
        };
        let mut r = Record::new(Kind::Relation, &title, Vec::new(), provenance);
        r.relation = Some(Relation {
            kind,
            target: target.id.clone(),
        });
        r
    }

    /// All free text of a record, for content checks.
    fn text(&self) -> String {
        let fields: Vec<String> = self.fields.values().map(|v| v.to_string()).collect();
        let code: Vec<&str> = self.snippets.iter().map(|s| s.code.as_str()).collect();
        format!(
            "{} {} {} {} {} {}",
            self.title,
            self.body,
            self.trigger.as_deref().unwrap_or(""),
            self.evidence.join(" "),
            fields.join(" "),
            code.join(" ")
        )
    }

    /// A record that slipped past write-time checks (planted straight into the store).
    pub fn unsafe_content(&self) -> bool {
        let t = self.text();
        suspicious(&t).is_some()
            || (self.provenance.author != AuthorKind::Human && injection(&t).is_some())
    }

    /// Reject records that would mislead or carry secrets; caps keep them reviewable.
    pub fn validate(&self) -> Result<()> {
        if self.title.is_empty() || self.title.chars().count() > 160 {
            bail!("title must be 1–160 characters");
        }
        if self.body.chars().count() > 4000 {
            bail!("body is limited to 4000 characters");
        }
        if self.kind != Kind::Relation && self.anchors.is_empty() {
            bail!("a record needs at least one anchor (a symbol it is about)");
        }
        if self.fields.contains_key("check") && self.kind != Kind::Constraint {
            bail!("only constraints carry a check");
        }
        if self.snippets.len() > MAX_SNIPPETS {
            bail!("at most {MAX_SNIPPETS} snippets");
        }
        if self.snippets.iter().any(|s| {
            s.path.trim().is_empty()
                || s.code.trim().is_empty()
                || s.code.lines().count() > MAX_SNIPPET_LINES
                || s.code.chars().count() > 4000
        }) {
            bail!("a snippet needs a path and 1–{MAX_SNIPPET_LINES} lines of code (4000 characters at most)");
        }
        if self.kind == Kind::Attempt && self.evidence.is_empty() {
            bail!("an attempt needs evidence (a revert commit, failing test, error, MR, …)");
        }
        let text = self.text();
        if let Some(why) = suspicious(&text) {
            bail!("refusing to store record: {why}");
        }
        // Ledger text is injected into agents' context: machine-written records must not carry
        // instructions aimed at an agent or shell payloads.
        if self.provenance.author != AuthorKind::Human {
            if let Some(why) = injection(&text) {
                bail!("refusing to store record: {why}");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Active,
    /// The anchored symbol moved or was renamed; its body is unchanged.
    Moved,
    /// The anchored symbol's body changed since the record was written: re-verify.
    Stale,
    /// The anchored symbol no longer exists.
    Orphaned,
    Superseded,
    Retracted,
}

/// T4 constraint enforced by a passing check, T3 human or human-confirmed, T2 agent with evidence,
/// T1 agent assertion, T0 untrusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Trust {
    T0,
    T1,
    T2,
    T3,
    T4,
}

/// A record with its derived state against the current index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub record: Record,
    pub status: Status,
    pub trust: Trust,
    /// Current symbol ids the record's anchors map to (after moves/renames).
    pub current: Vec<String>,
    /// Ids of records that supersede, retract, confirm or dispute this one.
    pub related: Vec<(RelationKind, String)>,
    /// Signature check: verified, failed (record treated as T0), or unsigned/unchecked.
    pub verified: Option<bool>,
    /// Who wrote it, for people to read.
    pub by: String,
    /// Reviews of this record, oldest first.
    pub history: Vec<Event>,
    /// Why it was retracted: `outdated` (archived) or `wrong` (invalidated).
    pub retired: Option<String>,
    /// The writer's teams, when the note is about another team's code and none of its members confirmed it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outside: Vec<String>,
}

pub const SCHEMA: u32 = 1;

/// A review action: `confirm`, `supersede`, `dispute`, `retract`, `archive` (no longer applies)
/// or `invalidate` (was wrong).
pub fn review_action(action: &str) -> Option<(RelationKind, Option<&'static str>)> {
    match action {
        "archive" => Some((RelationKind::Retracts, Some("outdated"))),
        "invalidate" => Some((RelationKind::Retracts, Some("wrong"))),
        other => RelationKind::parse(other).map(|k| (k, None)),
    }
}

pub const MAX_SNIPPETS: usize = 3;

pub const MAX_SNIPPET_LINES: usize = 60;

/// Instruction or payload text that would steer an agent reading the record.
pub fn injection(text: &str) -> Option<&'static str> {
    let t = text.to_lowercase();
    const STEERING: &[&str] = &[
        "ignore previous",
        "ignore all previous",
        "ignore the above",
        "disregard previous",
        "disregard the above",
        "system prompt",
        "you are now",
        "new instructions",
        "<system",
        "</system",
        "<instructions",
        "assistant:",
        "do not tell the user",
        "without telling the user",
    ];
    const PAYLOADS: &[&str] = &[
        "| sh",
        "| bash",
        "|sh",
        "|bash",
        "rm -rf",
        "base64 -d",
        "base64 --decode",
        "<script",
        "javascript:",
        "chmod +x",
        "eval(",
        "/etc/passwd",
        ".ssh/",
    ];
    if STEERING.iter().any(|p| t.contains(p)) {
        return Some("reads like instructions to an agent");
    }
    if PAYLOADS.iter().any(|p| t.contains(p)) {
        return Some("contains a shell or script payload");
    }
    None
}

/// Secrets and invisible/bidi characters never enter the shared ledger.
pub fn suspicious(text: &str) -> Option<&'static str> {
    if text
        .chars()
        .any(|c| matches!(c as u32, 0x202A..=0x202E | 0x2066..=0x2069 | 0xE0000..=0xE007F))
    {
        return Some("contains invisible or bidirectional control characters");
    }
    let lower = text.to_ascii_lowercase();
    let markers = [
        "-----begin",
        "aws_secret_access_key",
        "xoxb-",
        "ghp_",
        "glpat-",
        "sk-ant-",
        "password=",
    ];
    if markers.iter().any(|m| lower.contains(m))
        || text.split_whitespace().any(|w| {
            w.starts_with("AKIA") && w.len() == 20 && w.chars().all(|c| c.is_ascii_alphanumeric())
        })
    {
        return Some("looks like it contains a secret");
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    /// Public key, `ssh-ed25519 AAAA…`.
    pub key: String,
    /// Armored SSHSIG over the record without its signature.
    pub sig: String,
}

/// One review of a record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    /// `confirm`, `supersede`, `dispute`, `retract`, `archive` or `invalidate`.
    pub action: String,
    pub who: String,
    pub at: String,
    pub reason: String,
    /// False when it could not take effect (a machine retiring a human-backed record).
    pub counted: bool,
}
