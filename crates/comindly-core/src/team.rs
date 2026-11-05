//! Team settings, stored with the index (`<index>/_team`, a versioned LanceDB table) so everyone
//! using it runs the same LLM, embedding model and LLM steps. No secrets: each member's API key
//! comes from their own credentials file or environment.

use anyhow::Result;
use serde::{Deserialize, Serialize};

const TABLE: &str = "settings";

/// What everyone using an index shares; `None` leaves comindly's default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TeamSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_model: Option<String>,
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

/// Parse a `teams` filter: comma-separated names, `none` for unlabelled.
pub fn parse_selection(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// The stored settings with the table version they came from and who saved them.
#[derive(Debug, Clone, Default)]
pub struct Stored {
    pub settings: TeamSettings,
    pub version: Option<u64>,
    pub changed_by: Option<String>,
    pub changed_at: Option<String>,
}

fn uri(root: &str) -> String {
    format!("{}/_team", root.trim_end_matches('/'))
}

/// The team settings of the index at `root` (empty when none were saved).
pub async fn load(root: &str) -> Result<Stored> {
    let s = crate::docstore::load::<TeamSettings>(&uri(root), TABLE).await?;
    Ok(Stored {
        settings: s.doc,
        version: s.version,
        changed_by: s.changed_by,
        changed_at: s.changed_at,
    })
}

/// Save `settings` as `by` if nobody saved since `expected` (the version they were loaded at).
pub async fn save_as(
    root: &str,
    settings: &TeamSettings,
    expected: Option<u64>,
    by: &str,
) -> Result<u64> {
    crate::docstore::save(&uri(root), TABLE, settings, expected, by)
        .await
        .map_err(|e| match format!("{e}").contains("changed meanwhile") {
            true => anyhow::anyhow!(
                "the team settings were changed by someone else; reload and try again"
            ),
            false => e,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn save_load_and_refuse_a_stale_write() {
        let dir = std::env::temp_dir().join(format!("comindly-team-{}", std::process::id()));
        let root = dir.to_str().unwrap();
        let first = TeamSettings {
            llm_model: Some("m1".into()),
            ..Default::default()
        };
        let v1 = save_as(root, &first, None, "t").await.unwrap();
        assert_eq!(load(root).await.unwrap().settings, first);
        let second = TeamSettings {
            llm_model: Some("m2".into()),
            ..Default::default()
        };
        save_as(root, &second, Some(v1), "t").await.unwrap();
        assert!(
            save_as(root, &first, Some(v1), "t").await.is_err(),
            "stale version"
        );
        assert_eq!(
            load(root).await.unwrap().settings.llm_model.as_deref(),
            Some("m2")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    fn two_teams() -> TeamSettings {
        let mut t = TeamSettings::default();
        t.label("hypnos", &["Skills".into()]);
        t.label("shared-lib", &["Skills".into(), "Backend".into()]);
        t.label("billing", &["Backend".into()]);
        t
    }

    #[test]
    fn labels_select_repositories() {
        let t = two_teams();
        assert_eq!(t.teams_of("shared-lib"), ["Skills", "Backend"]);
        let skills = vec!["skills".to_string()];
        assert!(t.in_view("hypnos", &skills) && t.in_view("shared-lib", &skills));
        assert!(!t.in_view("billing", &skills));
        assert!(t.in_view("billing", &[]), "no selection shows everything");
        let none = vec![UNASSIGNED.to_string()];
        assert!(t.in_view("docs", &none) && !t.in_view("hypnos", &none));
    }

    #[test]
    fn relabel_and_unlabel() {
        let mut t = two_teams();
        t.label("shared-lib", &["Backend".into()]);
        assert_eq!(t.teams_of("shared-lib"), ["Backend"]);
        t.unlabel("billing");
        assert!(t.teams_of("billing").is_empty());
        assert_eq!(t.team_list().len(), 2, "an emptied team stays");
    }

    #[test]
    fn tidy_refuses_duplicates_and_the_reserved_name() {
        let mut t = two_teams();
        t.teams.as_mut().unwrap().push(TeamDef {
            name: " skills ".into(),
            ..Default::default()
        });
        assert!(t.tidy_teams().is_err());
        let mut t = TeamSettings {
            teams: Some(vec![TeamDef {
                name: "None".into(),
                ..Default::default()
            }]),
            ..Default::default()
        };
        assert!(t.tidy_teams().is_err());
    }

    #[test]
    fn closed_teams_refuse_notes_from_outside() {
        let mut t = TeamSettings::default();
        t.label("api", &["Backend".into()]);
        t.label("pay", &["Payments".into(), "Backend".into()]);
        let search = ["Search".to_string()];
        assert_eq!(
            t.outside(&search, &["api"]),
            Some(vec!["Backend".to_string()])
        );
        assert_eq!(t.outside(&["backend".into()], &["api"]), None);
        assert_eq!(t.outside(&[], &["api"]), None);
        assert_eq!(t.outside(&search, &["unlabelled"]), None);
        t.teams.as_mut().unwrap()[0].incoming = Incoming::Closed;
        assert!(t.accepts(&search, &["api"]).is_err());
        // Several owners: the most open decides.
        assert!(t.accepts(&search, &["pay"]).is_ok());
        assert!(t.accepts(&["Payments".into()], &["api"]).is_err());
    }

    #[test]
    fn backfill_only_once() {
        let org = TeamSettings {
            name: Some("Skills".into()),
            ..Default::default()
        };
        let done = org
            .backfill(&["b".into(), "a".into(), "a".into()], &["Ann@X.io".into()])
            .unwrap();
        assert_eq!(done.team_list()[0].name, "Skills");
        assert_eq!(done.team_list()[0].repos, ["a", "b"]);
        assert_eq!(done.teams_of_member("ann@x.io"), ["Skills"]);
        assert!(
            done.backfill(&["c".into()], &[]).is_none(),
            "already set up"
        );
        let emptied = TeamSettings {
            teams: Some(vec![]),
            ..org.clone()
        };
        assert!(
            emptied.backfill(&["a".into()], &[]).is_none(),
            "deliberately none"
        );
        assert!(
            TeamSettings::default()
                .backfill(&["a".into()], &[])
                .is_none(),
            "no org name"
        );
    }

    #[test]
    fn members_pick_existing_teams() {
        let mut t = two_teams();
        t.set_member("Ann@x.io", &["skills".into(), "Backend".into()])
            .unwrap();
        assert_eq!(t.teams_of_member("ann@x.io"), ["Skills", "Backend"]);
        t.set_member("ann@x.io", &["Backend".into()]).unwrap();
        assert_eq!(t.teams_of_member("ann@x.io"), ["Backend"]);
        assert!(t.set_member("ann@x.io", &["Nope".into()]).is_err());
    }
}
