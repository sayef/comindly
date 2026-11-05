//! Team settings from the UI, plus your own API key in your credentials file (never in the index).

use anyhow::Context as _;
use axum::extract::State;
use axum::Json;

use super::{bad as fail, Reply, Shared};
use crate::api;
use crate::team::{self, TeamSettings};

fn done(r: anyhow::Result<impl Into<String>>) -> Reply<api::Done> {
    r.map(|m| Json(api::Done::new(m))).map_err(fail)
}

async fn root(app: &Shared) -> anyhow::Result<String> {
    use anyhow::Context as _;
    app.current
        .read()
        .await
        .data
        .root
        .clone()
        .context("read-only UI")
}

/// comindly's built-in values, shown where a setting is left empty.
pub(super) fn defaults() -> api::Defaults {
    api::Defaults {
        llm_model: crate::llm::DEFAULT_MODEL.into(),
        llm_base_url: crate::llm::DEFAULT_BASE_URL.into(),
        embed_model: crate::embed::DEFAULT_MODEL.into(),
        llm_concurrency: 8,
        format: "md".into(),
    }
}

pub(super) async fn get(State(app): State<Shared>) -> Reply<api::Team> {
    let run = async {
        let root = root(&app).await?;
        let stored = team::load(&root).await?;
        let key =
            crate::credentials::llm_key(stored.settings.name.as_deref(), app.server.as_deref()).1;
        anyhow::Ok(api::Team {
            settings: stored.settings,
            key_source: key,
            version: stored.version,
            changed_by: stored.changed_by,
            changed_at: stored.changed_at,
            defaults: defaults(),
        })
    };
    run.await.map(Json).map_err(fail)
}

/// Save the shared settings for everyone using this index.
pub(super) async fn save(
    who: super::Member,
    State(app): State<Shared>,
    Json(b): Json<api::TeamSave>,
) -> Reply<api::Done> {
    let run = async {
        let root = root(&app).await?;
        let current = team::load(&root).await?;
        let mut next = b.settings;
        for v in [
            &mut next.name,
            &mut next.llm_base_url,
            &mut next.llm_model,
            &mut next.embed_model,
        ] {
            if v.as_deref().is_some_and(|s| s.trim().is_empty()) {
                *v = None;
            }
        }
        if let Some(f) = next.format.as_deref() {
            anyhow::ensure!(matches!(f, "md" | "json"), "the format is md or json");
        }
        anyhow::ensure!(
            next.llm_concurrency.is_none_or(|n| (1..=64).contains(&n)),
            "LLM requests at once: between 1 and 64"
        );
        // Teams have their own endpoints; this form never changes them.
        next.teams = current.settings.teams.clone();
        let reindex = next.embed_model != current.settings.embed_model;
        let format_changed = next.format != current.settings.format;
        match who.as_deref() {
            Some(w) => team::save_as(&root, &next, b.version, &w.email).await?,
            None => crate::config::save_team(&root, &next, b.version).await?,
        };
        app.set_team(next);
        if format_changed {
            app.reload_mcp(&root);
        }
        anyhow::Ok(if reindex {
            "saved; the new embedding model needs a full re-index"
        } else {
            "saved for everyone using this index"
        })
    };
    done(run.await)
}

/// Store your API key for this team (or as your default) in your credentials file.
pub(super) async fn key(
    State(app): State<Shared>,
    Json(b): Json<api::KeyRequest>,
) -> Reply<api::Done> {
    let run = async {
        anyhow::ensure!(
            app.server.is_none(),
            "on a team server the LLM key is set under Settings → Integrations"
        );
        let root = root(&app).await?;
        let name = team::load(&root).await?.settings.name;
        let (n, k) = (name.clone(), b.key.clone());
        tokio::task::spawn_blocking(move || crate::credentials::set_llm_key(n.as_deref(), &k))
            .await??;
        anyhow::Ok(match name {
            Some(n) if !b.key.trim().is_empty() => format!("saved as your key for {n}"),
            None if !b.key.trim().is_empty() => "saved as your default key".to_string(),
            _ => "removed your key".to_string(),
        })
    };
    done(run.await)
}

/// Use a team's index: check it, remember it and load it.
pub(super) async fn join(
    State(app): State<Shared>,
    Json(b): Json<api::JoinRequest>,
) -> Reply<api::Done> {
    let run = async {
        let root = crate::config::expand_tilde(b.index.trim());
        let indexed = has_index(&root).await?;
        let stored = team::load(&root).await?;
        anyhow::ensure!(
            indexed || stored.version.is_some(),
            "no team or index at {root}"
        );
        use_location(&app, &root, stored.settings.clone()).await?;
        anyhow::Ok(format!("joined {}", stored.settings.name.unwrap_or(root)))
    };
    done(run.await)
}

/// Start a team at an empty location: write its settings there and switch to it.
pub(super) async fn create(
    State(app): State<Shared>,
    Json(b): Json<api::CreateTeam>,
) -> Reply<api::Done> {
    let run = async {
        let name = b.name.trim().to_string();
        anyhow::ensure!(!name.is_empty(), "give the team a name");
        let root = crate::config::expand_tilde(b.index.trim());
        anyhow::ensure!(!root.is_empty(), "choose where the team's index lives");
        if !root.contains("://") {
            std::fs::create_dir_all(&root)?;
        }
        anyhow::ensure!(
            !has_index(&root).await?,
            "{root} already has an index; join it instead"
        );
        anyhow::ensure!(
            team::load(&root).await?.version.is_none(),
            "{root} already has a team; join it instead"
        );
        let settings = TeamSettings {
            name: Some(name.clone()),
            ..Default::default()
        };
        crate::config::save_team(&root, &settings, None).await?;
        use_location(&app, &root, settings).await?;
        anyhow::Ok(format!("created {name}"))
    };
    done(run.await)
}

async fn has_index(root: &str) -> anyhow::Result<bool> {
    let graph = format!("{}/_graph", root.trim_end_matches('/'));
    match crate::index::graph_versions(&graph).await {
        Ok(v) => Ok(!v.is_empty()),
        Err(e) if format!("{e:#}").contains("was not found") => Ok(false),
        Err(e) => Err(e),
    }
}

/// Remember `root` in the personal config and load it with its team settings.
async fn use_location(app: &Shared, root: &str, settings: TeamSettings) -> anyhow::Result<()> {
    if app.server.is_none() {
        crate::config::update(&[("index_dir", crate::config::Setting::Text(root.to_string()))])?;
        app.config_changed();
    }
    app.set_team(settings);
    app.reload(root).await
}

/// Load the settings, change them with `edit`, and save them as `who`.
async fn change_teams(
    app: &Shared,
    who: &super::Member,
    expected: Option<Option<u64>>,
    edit: impl FnOnce(&mut TeamSettings) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let root = root(app).await?;
    let current = team::load(&root).await?;
    let mut next = current.settings.clone();
    edit(&mut next)?;
    next.tidy_teams()?;
    let version = expected.unwrap_or(current.version);
    match who.as_deref() {
        Some(w) => team::save_as(&root, &next, version, &w.email).await?,
        None => crate::config::save_team(&root, &next, version).await?,
    };
    app.set_team(next);
    Ok(())
}

/// Replace the team list (admins).
pub(super) async fn save_teams(
    who: super::Member,
    State(app): State<Shared>,
    Json(b): Json<api::TeamsSave>,
) -> Reply<api::Done> {
    let run = change_teams(&app, &who, Some(b.version), |t| {
        t.teams = Some(b.teams);
        Ok(())
    });
    done(run.await.map(|_| "teams saved"))
}

/// Set the teams labelling one repository (admins); new names become new teams.
pub(super) async fn label_repo(
    who: super::Member,
    State(app): State<Shared>,
    Json(b): Json<api::RepoTeams>,
) -> Reply<api::Done> {
    let repo = b.repo.trim().to_string();
    let run = change_teams(&app, &who, None, |t| {
        anyhow::ensure!(!repo.is_empty(), "which repository?");
        t.label(&repo, &b.teams);
        Ok(())
    });
    done(run.await.map(|_| format!("teams of {repo} saved")))
}

/// Choose the teams you belong to; on a server only.
pub(super) async fn my_teams(
    who: super::Member,
    State(app): State<Shared>,
    Json(b): Json<api::MyTeams>,
) -> Reply<api::Done> {
    let run = async {
        let email = who
            .as_deref()
            .map(|w| w.email.clone())
            .context("teams per person need a server with sign-in")?;
        change_teams(&app, &who, None, |t| t.set_member(&email, &b.teams)).await
    };
    done(run.await.map(|_| "your teams saved"))
}
