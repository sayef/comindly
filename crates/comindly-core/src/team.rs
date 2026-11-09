//! Team settings, stored with the index (`<index>/_team`, a versioned LanceDB table) so everyone
//! using it runs the same LLM, embedding model and LLM steps. No secrets: each member's API key
//! comes from their own credentials file or environment.

use anyhow::Result;

const TABLE: &str = "settings";

pub use comindly_api::team::{TeamDef, TeamSettings};

pub use comindly_api::team::Incoming;

pub use comindly_api::team::UNASSIGNED;

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
        t.label("catalog", &["Search".into()]);
        t.label("shared-lib", &["Search".into(), "Backend".into()]);
        t.label("billing", &["Backend".into()]);
        t
    }

    #[test]
    fn labels_select_repositories() {
        let t = two_teams();
        assert_eq!(t.teams_of("shared-lib"), ["Search", "Backend"]);
        let search = vec!["search".to_string()];
        assert!(t.in_view("catalog", &search) && t.in_view("shared-lib", &search));
        assert!(!t.in_view("billing", &search));
        assert!(t.in_view("billing", &[]), "no selection shows everything");
        let none = vec![UNASSIGNED.to_string()];
        assert!(t.in_view("docs", &none) && !t.in_view("catalog", &none));
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
            name: " search ".into(),
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
            name: Some("Search".into()),
            ..Default::default()
        };
        let done = org
            .backfill(&["b".into(), "a".into(), "a".into()], &["Ann@X.io".into()])
            .unwrap();
        assert_eq!(done.team_list()[0].name, "Search");
        assert_eq!(done.team_list()[0].repos, ["a", "b"]);
        assert_eq!(done.teams_of_member("ann@x.io"), ["Search"]);
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
        t.set_member("Ann@x.io", &["search".into(), "Backend".into()])
            .unwrap();
        assert_eq!(t.teams_of_member("ann@x.io"), ["Search", "Backend"]);
        t.set_member("ann@x.io", &["Backend".into()]).unwrap();
        assert_eq!(t.teams_of_member("ann@x.io"), ["Backend"]);
        assert!(t.set_member("ann@x.io", &["Nope".into()]).is_err());
    }
}
