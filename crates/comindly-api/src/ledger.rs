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
        // Records written through a gateway before beta.9 carry the MCP library's name, not the agent's.
        let agent = self.agent.as_deref().filter(|a| *a != "rmcp");
        match (self.author, self.name.as_deref(), agent) {
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
        anyhow::ensure!(
            !reason.trim().is_empty(),
            "say why: every change to the ledger carries a reason other agents can read"
        );
        let mut r = Record::relation(kind, target, reason, provenance);
        if let Some(why) = retire {
            r.fields
                .insert("retire".into(), serde_json::Value::String(why.into()));
        }
        Ok(r)
    }

    /// Mark a review an agent makes on its person's explicit say.
    pub fn from_person(mut self) -> Record {
        self.fields
            .insert("from_person".into(), serde_json::Value::Bool(true));
        self
    }

    /// A confirmation vouches for the code as indexed now: it takes the target's anchors from `now`.
    pub fn vouching(mut self, target: &Record, now: impl Fn(&str) -> Option<Anchor>) -> Record {
        if self.relation.as_ref().map(|rel| rel.kind) == Some(RelationKind::Confirms) {
            self.anchors = target
                .anchors
                .iter()
                .filter_map(|a| now(&a.symbol))
                .collect();
        }
        self
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
        // A review keeps its reason in the title.
        let (cap, what) = if self.kind == Kind::Relation {
            (MAX_REASON, "a reason")
        } else {
            (MAX_TITLE, "a title")
        };
        if self.title.is_empty() || self.title.chars().count() > cap {
            bail!("{what} must be 1–{cap} characters");
        }
        if self.body.chars().count() > MAX_BODY {
            bail!("the body is limited to {MAX_BODY} characters");
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
                || s.code.chars().count() > MAX_SNIPPET_CHARS
        }) {
            bail!("a snippet needs a path and 1–{MAX_SNIPPET_LINES} lines of code ({MAX_SNIPPET_CHARS} characters at most)");
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
    /// Standing until someone retires it; code changes only add notes. Older engines said moved or stale.
    #[serde(alias = "moved", alias = "stale")]
    Active,
    /// None of the anchored symbols exist any more.
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
    /// People other than the writer who confirmed it, themselves or through their agents.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub confirmers: Vec<String>,
    /// Where the code is now, when a person's confirmation re-anchored the record.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vouched: Vec<Anchor>,
    /// An older standing record this one reads like, until a person keeps or retires it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alike: Option<Alike>,
    /// What happened to the anchored code since the record, or its latest confirmation, saw it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Note>,
    /// Evidence (a merge or pull request, a branch commit) and the default-branch commits it became.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub landed: std::collections::BTreeMap<String, Vec<String>>,
}

/// A change to one anchored symbol: `changed`, `moved` (another file), `renamed` or `gone`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub what: String,
    /// The symbol's name when the record saw it.
    pub name: String,
    /// Where it is now (`path:line`); absent when gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now: Option<String>,
    /// Its name now, when renamed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renamed: Option<String>,
    /// The indexed commit the record saw, to compare against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
}

impl Note {
    /// One line for agents and people: "`merge` changed since 0cdb88b, now at app/x.py:40".
    pub fn text(&self) -> String {
        let since = self
            .since
            .as_deref()
            .map(|c| format!(" since {}", &c[..c.len().min(8)]))
            .unwrap_or_default();
        let now = self
            .now
            .as_deref()
            .map(|n| format!(", now at {n}"))
            .unwrap_or_default();
        match self.what.as_str() {
            "renamed" => format!(
                "`{}` renamed to `{}`{since}{now}",
                self.name,
                self.renamed.as_deref().unwrap_or("?")
            ),
            "gone" => format!("`{}` no longer exists{since}", self.name),
            what => format!("`{}` {what}{since}{now}", self.name),
        }
    }
}

impl Entry {
    /// The anchors to show: where a confirmation last saw the code, else where it was written.
    pub fn anchors(&self) -> &[Anchor] {
        if self.vouched.is_empty() {
            &self.record.anchors
        } else {
            &self.vouched
        }
    }
}

/// The older record a record may repeat, and how alike they read (0 to 1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alike {
    pub id: String,
    pub title: String,
    pub score: f32,
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

pub const MAX_TITLE: usize = 500;
pub const MAX_REASON: usize = 4000;
pub const MAX_BODY: usize = 20000;
pub const MAX_SNIPPET_CHARS: usize = 16000;
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
