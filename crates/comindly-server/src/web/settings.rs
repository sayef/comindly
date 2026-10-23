//! Settings from the UI: read and write `config.toml`, and switch the
//! loaded index without a restart.

use axum::extract::State;
use axum::Json;

use super::{bad as fail, Cur, Reply, Shared};
use crate::api;
use crate::config::Setting;

pub(super) async fn get(State(app): State<Shared>, Cur(s): Cur) -> Json<api::Settings> {
    let (server, cfg, settings) = (app.server.clone(), app.config(), app.settings());
    let root = s.data.root.clone().unwrap_or_default();
    let team = app.team().name;
    let key = tokio::task::spawn_blocking(move || {
        crate::credentials::llm_key(team.as_deref(), server.as_deref()).1
    })
    .await
    .unwrap_or(crate::credentials::Source::None);
    Json(api::Settings {
        config_path: crate::config::store_path().to_string_lossy().into_owned(),
        index_dir: cfg.index_dir.clone(),
        index_effective: root,
        author_name: cfg.author_name.clone(),
        author_effective: crate::config::local_identity().name,
        record_names: cfg.record_names.unwrap_or(true),
        llm_model: settings.llm_model,
        defaults: super::team::defaults(),
        llm_key: key,
        cache_ttl_secs: cfg.cache_ttl_secs.unwrap_or(crate::cache::DEFAULT_TTL),
    })
}

/// Save the given settings; a new index location is loaded before it is saved.
pub(super) async fn save(
    State(app): State<Shared>,
    Json(c): Json<api::SettingsChange>,
) -> Reply<api::Done> {
    let run = async {
        let text = |v: &Option<String>| match v.as_deref().map(str::trim) {
            Some("") => Setting::Remove,
            Some(t) => Setting::Text(t.to_string()),
            None => unreachable!(),
        };
        let mut changes: Vec<(&str, Setting)> = Vec::new();
        anyhow::ensure!(
            app.server.is_none(),
            "a server takes its settings from the deployment and the index's shared settings"
        );
        for (key, v) in [("author_name", &c.author_name)] {
            if v.is_some() {
                changes.push((key, text(v)));
            }
        }
        if let Some(v) = c.record_names {
            changes.push(("record_names", Setting::Flag(v)));
        }
        if let Some(v) = c.cache_ttl_secs {
            changes.push(("cache_ttl_secs", Setting::Number(v.max(30) as i64)));
        }
        let mut message = "saved".to_string();
        if let Some(dir) = c
            .index_dir
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
        {
            let dir = crate::config::expand_tilde(dir);
            app.reload(&dir).await?;
            changes.push(("index_dir", Setting::Text(dir.clone())));
            message = format!("saved; now showing {dir}");
        }
        crate::config::update(&changes)?;
        app.config_changed();
        anyhow::Ok(message)
    };
    run.await.map(|m| Json(api::Done::new(m))).map_err(fail)
}

/// Whether an index location can be read, without switching to it.
pub(super) async fn test(Json(p): Json<api::IndexDirRequest>) -> Reply<api::Done> {
    let root = crate::config::expand_tilde(p.index_dir.trim());
    let graph = format!("{}/_graph", root.trim_end_matches('/'));
    let message = match crate::index::graph_versions(&graph).await {
        Ok(v) if !v.is_empty() => format!("index found: {} version(s)", v.len()),
        Ok(_) | Err(_) if !root.contains("://") && !std::path::Path::new(&root).exists() => {
            "empty location: an index will be created here when you index repositories".into()
        }
        Ok(_) => "reachable, no index yet".into(),
        Err(e) => return Err(fail(e)),
    };
    Ok(Json(api::Done::new(message)))
}
