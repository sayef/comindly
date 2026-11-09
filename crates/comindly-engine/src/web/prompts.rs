//! The LLM prompts indexing uses: saved versions, which one is active, and trying a draft.

use axum::extract::State;
use axum::Json;

use super::{bad as fail, Cur, Reply, Shared};
use crate::api::{self, PromptAction};
use crate::prompts::{self, Prompts, Task};

pub(super) async fn get(State(app): State<Shared>) -> Reply<api::Prompts> {
    let run = async {
        let root = super::team::root(&app).await?;
        let stored = prompts::load(&root).await?;
        let lib = stored.doc;
        anyhow::Ok(api::Prompts {
            tasks: Task::ALL
                .into_iter()
                .map(|t| api::PromptTask {
                    task: t,
                    builtin: t.builtin().into(),
                    format: crate::prompts::fixed(t),
                    active: lib.active.get(&t).copied(),
                    builtin_changed: lib.builtin_changed(t),
                    sections: match t {
                        Task::Guide => crate::styleguide::SECTIONS
                            .iter()
                            .map(|(title, g)| api::GuideSectionText {
                                title: title.to_string(),
                                guidance: g.to_string(),
                            })
                            .collect(),
                        _ => Vec::new(),
                    },
                    versions: lib.of(t).cloned().collect(),
                })
                .collect(),
            version: stored.version,
            changed_by: stored.changed_by,
            changed_at: stored.changed_at,
        })
    };
    run.await.map(Json).map_err(fail)
}

pub(super) async fn change(
    who: super::Member,
    State(app): State<Shared>,
    Json(b): Json<api::PromptChange>,
) -> Reply<api::Done> {
    let run = async {
        let root = super::team::root(&app).await?;
        let by = match who.as_deref() {
            Some(w) => w.email.clone(),
            None => {
                let me = crate::config::local_identity();
                me.name.unwrap_or(me.actor)
            }
        };
        let mut lib = prompts::load(&root).await?.doc;
        let msg_detail = match &b.action {
            PromptAction::Save { task, .. } => format!("{} saved", task.name()),
            PromptAction::Activate { task, n } => format!("{} v{n} activated", task.name()),
            PromptAction::Deactivate { task } => format!("{} back to built-in", task.name()),
            PromptAction::Delete { task, n } => format!("{} v{n} deleted", task.name()),
        };
        let msg = match b.action {
            PromptAction::Save {
                task,
                text,
                sections,
                note,
            } => {
                let n = lib.add(task, &text, &sections, note.as_deref(), &by)?;
                format!("saved as version {n}; not used until you activate it")
            }
            PromptAction::Activate { task, n } => {
                lib.activate(task, n)?;
                format!("version {n} is in use for what the next index writes")
            }
            PromptAction::Deactivate { task } => {
                lib.deactivate(task);
                "back to the built-in prompt for what the next index writes".to_string()
            }
            PromptAction::Delete { task, n } => {
                lib.remove(task, n)?;
                format!("version {n} deleted")
            }
        };
        prompts::save_as(&root, &lib, b.version, &by).await?;
        if let Some(server) = app.server.as_deref() {
            server.audit.record(&by, "prompt", &msg_detail);
        }
        anyhow::Ok(msg)
    };
    run.await.map(|m| Json(api::Done::new(m))).map_err(fail)
}

/// Run a draft on one sample, next to what the index holds for it now.
pub(super) async fn try_draft(
    State(app): State<Shared>,
    Cur(s): Cur,
    Json(b): Json<api::PromptTry>,
) -> Reply<api::PromptTried> {
    let run = async {
        anyhow::ensure!(!b.text.trim().is_empty(), "the prompt is empty");
        let team = app.team().name;
        let llm =
            crate::config::llm_client(&app.settings(), team.as_deref(), app.server.as_deref())
                .ok_or_else(|| anyhow::anyhow!("no LLM key; set one under Settings"))?;
        let draft = Prompts::draft(b.task, &b.text, b.sections.clone());
        let g = &s.data.graph;
        let extras = s.extras().await;
        let pick = |id: Option<&str>| id.filter(|i| !i.is_empty()).and_then(|i| g.lookup(i));
        let shown = |(text, q): &(String, Vec<String>)| answer(text, q);
        match b.task {
            Task::Summary => {
                let i = match pick(b.sample.as_deref()) {
                    Some(i) => i,
                    None => most_used(&s.data)
                        .and_then(|id| g.lookup(&id))
                        .ok_or_else(|| anyhow::anyhow!("nothing indexed to try it on"))?,
                };
                let node = g.node(i);
                let path = node
                    .location
                    .rsplit_once(':')
                    .map_or(&*node.location, |(p, _)| p);
                let sig = node.signature.clone().unwrap_or_default();
                let e = llm.enrich_symbol(&draft, &node.name, &sig, path).await?;
                Ok(api::PromptTried {
                    sample: format!("{} ({})", node.name, node.repo),
                    output: answer(&e.summary, &e.queries),
                    current: extras.enrichment.get(&node.id).map(shown),
                })
            }
            Task::Flow => {
                let i = match pick(b.sample.as_deref()) {
                    Some(i) => i,
                    None => g
                        .entry_points(1)
                        .first()
                        .map(|(i, _)| *i)
                        .ok_or_else(|| anyhow::anyhow!("no flows indexed to try it on"))?,
                };
                let node = g.node(i);
                let sig = node.signature.clone().unwrap_or_default();
                let (text, q) = llm
                    .narrate_flow(&draft, &node.name, &sig, &g.flow_trace(i))
                    .await?;
                Ok(api::PromptTried {
                    sample: format!("{} ({})", node.name, node.repo),
                    output: answer(&text, &q),
                    current: extras.flows.get(&node.id).map(shown),
                })
            }
            Task::Guide => {
                let (repo, title, body) = guide_sample(&app, &s, &b, &draft, &llm).await?;
                let uri = super::graph_uri(&s).ok_or_else(|| anyhow::anyhow!("no index"))?;
                let current = crate::index::read_keys(&uri, "guide_sections")
                    .await
                    .ok()
                    .and_then(|m| m.get(&format!("{repo}\0{title}")).cloned());
                Ok(api::PromptTried {
                    sample: format!("{title} ({repo})"),
                    output: body.unwrap_or_else(|| "No consistent convention observed.".into()),
                    current,
                })
            }
        }
    };
    run.await.map(Json).map_err(fail)
}

fn answer(text: &str, queries: &[String]) -> String {
    let mut out = text.trim().to_string();
    if !queries.is_empty() {
        out.push_str("\n\nQuestions it answers:");
        for q in queries {
            out.push_str(&format!("\n- {q}"));
        }
    }
    out
}

/// The function or type most others depend on, as indexing summarises first.
fn most_used(data: &crate::view::Data) -> Option<String> {
    use crate::model::SymbolKind as K;
    data.symbols
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                K::Function | K::Method | K::Class | K::Struct | K::Interface | K::Enum
            )
        })
        .map(|s| s.id.render())
        .max_by_key(|id| data.graph.dependents_count(id))
}

/// One style guide section of a repository, written with the draft from evidence measured now in
/// comindly's checkout of it: `(repo, section title, body)`.
async fn guide_sample(
    app: &Shared,
    s: &super::Session,
    b: &api::PromptTry,
    draft: &Prompts,
    llm: &crate::llm::LlmClient,
) -> anyhow::Result<(String, String, Option<String>)> {
    let entries = app.repos();
    let named = |e: &crate::config::RepoEntry| crate::worktree::name(std::path::Path::new(&e.path));
    let wanted = b.sample.as_deref().map(str::trim).filter(|x| !x.is_empty());
    let entry = match wanted {
        Some(w) => entries.iter().find(|e| named(e) == w),
        None => entries.first(),
    }
    .ok_or_else(|| anyhow::anyhow!("no repository {} in the list", wanted.unwrap_or("")))?;
    let repo = named(entry);
    let root = crate::worktree::checkout_dir(
        &crate::config::cache_dir(),
        std::path::Path::new(&entry.path),
    )
    .ok_or_else(|| anyhow::anyhow!("{repo} has no checkout here yet; index it once"))?;
    let symbols: Vec<crate::model::Symbol> = s
        .data
        .symbols
        .iter()
        .filter(|x| x.id.package == repo)
        .cloned()
        .collect();
    let edges: Vec<crate::model::Edge> = s
        .data
        .edges
        .iter()
        .filter(|e| e.src.package == repo)
        .cloned()
        .collect();
    let conventions: Vec<_> = s
        .data
        .conventions
        .iter()
        .filter(|c| c.repo == repo)
        .cloned()
        .collect();
    let title = b
        .section
        .clone()
        .unwrap_or_else(|| crate::styleguide::SECTIONS[0].0.to_string());
    let t = title.clone();
    let mut sec = tokio::task::spawn_blocking(move || {
        let refs: Vec<&crate::model::Symbol> = symbols.iter().collect();
        crate::styleguide::build_sections(&root, &refs, &edges, &conventions)
            .into_iter()
            .find(|x| x.title == t)
    })
    .await?
    .ok_or_else(|| anyhow::anyhow!("{repo} has no evidence for {title}"))?;
    if let Some(g) = draft.guidance(&sec.title) {
        sec.guidance = g.to_string();
    }
    let body = llm
        .guide_section(draft, &repo, &sec.title, &sec.guidance, &sec.evidence)
        .await?;
    Ok((repo, title, body))
}
