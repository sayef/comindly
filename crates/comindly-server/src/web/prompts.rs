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
                    format: t.fixed(),
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

/// What the next index writes again when `task` changes prompt.
fn redone(task: Task) -> &'static str {
    match task {
        Task::Summary => "writes every summary again",
        Task::Guide => "writes every style guide again",
        Task::Flow => "writes every walkthrough again",
    }
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
                format!("version {n} is in use; the next index {}", redone(task))
            }
            PromptAction::Deactivate { task } => {
                lib.deactivate(task);
                format!(
                    "back to the built-in prompt; the next index {}",
                    redone(task)
                )
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
                let path = node.location.rsplit_once(':').map_or(&*node.location, |(p, _)| p);
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
            Task::Guide => anyhow::bail!(
                "a style guide section is written from evidence gathered while indexing; activate the draft and index to see it"
            ),
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
