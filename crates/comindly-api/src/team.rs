use anyhow::Result;
use serde::{Deserialize, Serialize};

/// What everyone using an index shares; `None` leaves comindly's default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TeamSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_model: Option<String>,
    /// The model for one LLM task in place of `llm_model`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_model_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_model_guide: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_model_flow: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embed_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enrich: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flows: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide: Option<bool>,
    /// Search embeddings when indexing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embed: Option<bool>,
    /// Most symbols summarised by the LLM, most central first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_enrich: Option<usize>,
    /// Most flows given a written walkthrough, furthest-reaching first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_flows: Option<usize>,
    /// LLM requests in flight at once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_concurrency: Option<usize>,
    /// The LLM's price in USD per million input tokens, to show what indexing costs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_price_in: Option<f64>,
    /// The LLM's price in USD per million output tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_price_out: Option<f64>,
    /// What MCP tools return as text: `md` (default) or `json`; the JSON is always attached too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// SCIP use when indexing: `auto` (default), `require` or `off`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scip: Option<String>,
    /// Re-index when a listed repository's default branch moves on its remote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_index: Option<bool>,
    /// Also run the LLM steps on automatic re-indexes, for new or changed code only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_index_llm: Option<bool>,
    /// Minutes between checks whether a default branch moved (default 5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_index_minutes: Option<usize>,
    /// Clone URLs of the team's repositories, so everyone can get the same set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<String>,
    /// Teams labelling repositories; `None` until set up, so an upgrade can backfill once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teams: Option<Vec<TeamDef>>,
}

/// A team: a label on repositories (by indexed name); a repository may carry several.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamDef {
    pub name: String,
    #[serde(default)]
    pub repos: Vec<String>,
    /// Members' emails, lowercase; what they see first and how their notes are marked.
    #[serde(default)]
    pub members: Vec<String>,
    /// How notes from people outside the team on its code count.
    #[serde(default)]
    pub incoming: Incoming,
}

impl TeamSettings {
    pub fn team_list(&self) -> &[TeamDef] {
        self.teams.as_deref().unwrap_or_default()
    }

    /// Teams labelling `repo`, in the order they were defined.
    pub fn teams_of(&self, repo: &str) -> Vec<String> {
        self.team_list()
            .iter()
            .filter(|t| t.repos.iter().any(|r| r == repo))
            .map(|t| t.name.clone())
            .collect()
    }

    /// The teams owning `repos` when the writer is in none of them; `None` inside or unlabelled.
    pub fn outside(&self, writer: &[String], repos: &[&str]) -> Option<Vec<String>> {
        if writer.is_empty() {
            return None;
        }
        let mut owners: Vec<String> = Vec::new();
        for r in repos {
            for t in self.teams_of(r) {
                if !owners.contains(&t) {
                    owners.push(t);
                }
            }
        }
        let inside = owners
            .iter()
            .any(|o| writer.iter().any(|w| w.eq_ignore_ascii_case(o)));
        (!owners.is_empty() && !inside).then_some(owners)
    }

    /// Refuses a note from outside when every team owning `repos` is closed to them.
    pub fn accepts(&self, writer: &[String], repos: &[&str]) -> Result<()> {
        if let Some(owners) = self.outside(writer, repos) {
            let (takes, their) = if owners.len() == 1 {
                ("takes", "its")
            } else {
                ("take", "their")
            };
            anyhow::ensure!(
                self.incoming(&owners) != Incoming::Closed,
                "{} {takes} notes on this code only from {their} members; ask one of them to record it",
                owners.join(" and ")
            );
        }
        Ok(())
    }

    /// The most open policy among `owners` for notes from outside.
    pub fn incoming(&self, owners: &[String]) -> Incoming {
        self.team_list()
            .iter()
            .filter(|t| owners.iter().any(|o| o.eq_ignore_ascii_case(&t.name)))
            .map(|t| t.incoming)
            .min()
            .unwrap_or_default()
    }

    /// Teams `email` belongs to.
    pub fn teams_of_member(&self, email: &str) -> Vec<String> {
        let email = email.trim().to_lowercase();
        self.team_list()
            .iter()
            .filter(|t| t.members.contains(&email))
            .map(|t| t.name.clone())
            .collect()
    }

    /// Put `email` in exactly the existing teams named in `teams`.
    pub fn set_member(&mut self, email: &str, teams: &[String]) -> Result<()> {
        let email = email.trim().to_lowercase();
        for n in teams {
            anyhow::ensure!(
                self.team_list()
                    .iter()
                    .any(|t| t.name.eq_ignore_ascii_case(n.trim())),
                "there is no team called {n}"
            );
        }
        for t in self.teams.iter_mut().flatten() {
            let want = teams.iter().any(|n| n.trim().eq_ignore_ascii_case(&t.name));
            t.members.retain(|m| *m != email);
            if want {
                t.members.push(email.clone());
            }
        }
        Ok(())
    }

    /// Whether `repo` is in view for the `selected` teams (empty: everything; `none`: unlabelled).
    pub fn in_view(&self, repo: &str, selected: &[String]) -> bool {
        if selected.is_empty() {
            return true;
        }
        let labels = self.teams_of(repo);
        selected.iter().any(|s| match s.as_str() {
            UNASSIGNED => labels.is_empty(),
            name => labels.iter().any(|l| l.eq_ignore_ascii_case(name)),
        })
    }

    /// Trim names, drop empty and duplicate teams and repeated repositories.
    pub fn tidy_teams(&mut self) -> Result<()> {
        let Some(teams) = self.teams.as_mut() else {
            return Ok(());
        };
        let mut seen = std::collections::HashSet::new();
        for t in teams.iter_mut() {
            t.name = t.name.trim().to_string();
            anyhow::ensure!(!t.name.is_empty(), "a team needs a name");
            anyhow::ensure!(
                !t.name.eq_ignore_ascii_case(UNASSIGNED),
                "`{UNASSIGNED}` is reserved for repositories without a team"
            );
            anyhow::ensure!(
                seen.insert(t.name.to_lowercase()),
                "two teams are called {}",
                t.name
            );
            let mut repos = std::collections::BTreeSet::new();
            t.repos.retain(|r| repos.insert(r.clone()));
            for m in t.members.iter_mut() {
                *m = m.trim().to_lowercase();
            }
            let mut members = std::collections::BTreeSet::new();
            t.members
                .retain(|m| !m.is_empty() && members.insert(m.clone()));
        }
        Ok(())
    }

    /// Remove `repo` from every team.
    pub fn unlabel(&mut self, repo: &str) {
        for t in self.teams.iter_mut().flatten() {
            t.repos.retain(|r| r != repo);
        }
    }

    /// Set exactly `teams` on `repo`, creating teams that don't exist yet.
    pub fn label(&mut self, repo: &str, teams: &[String]) {
        let list = self.teams.get_or_insert_with(Vec::new);
        for t in list.iter_mut() {
            let want = teams.iter().any(|n| n.trim().eq_ignore_ascii_case(&t.name));
            let has = t.repos.iter().any(|r| r == repo);
            match (want, has) {
                (true, false) => t.repos.push(repo.to_string()),
                (false, true) => t.repos.retain(|r| r != repo),
                _ => {}
            }
        }
        for n in teams.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
            if !list.iter().any(|t| t.name.eq_ignore_ascii_case(n)) {
                list.push(TeamDef {
                    name: n.to_string(),
                    repos: vec![repo.to_string()],
                    ..Default::default()
                });
            }
        }
    }

    /// A first team named after the org with these repos and members, when teams were never set up.
    pub fn backfill(&self, repos: &[String], members: &[String]) -> Option<TeamSettings> {
        let name = self.name.as_deref()?.trim();
        if self.teams.is_some() || name.is_empty() {
            return None;
        }
        let mut repos = repos.to_vec();
        repos.sort();
        repos.dedup();
        let mut next = TeamSettings {
            teams: Some(vec![TeamDef {
                name: name.to_string(),
                repos,
                members: members.to_vec(),
                ..Default::default()
            }]),
            ..self.clone()
        };
        next.tidy_teams().ok()?;
        Some(next)
    }
}

/// Ordered from most to least open.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Incoming {
    Open,
    #[default]
    Review,
    Closed,
}

/// The `none` filter value: repositories no team labels.
pub const UNASSIGNED: &str = "none";
