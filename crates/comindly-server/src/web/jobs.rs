//! Jobs over HTTP: the list, one job with its events, its live events and cancelling it.

use std::convert::Infallible;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event as Sse, KeepAlive};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures::stream::{self, BoxStream, StreamExt};
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;

use super::Shared;
use crate::events::{Event, Kind, Level};
use crate::jobs::Status;

/// Most recent first; waiting jobs carry their `position`, 1 for the next to run.
pub(super) async fn list(State(app): State<Shared>) -> Response {
    let jobs: Vec<_> = app
        .jobs
        .list(crate::jobs::MAX_JOBS)
        .into_iter()
        .map(|j| {
            let mut v = json!(j);
            if j.status == Status::Queued {
                v["position"] = json!(app.jobs.position(&j.id));
            }
            v
        })
        .collect();
    Json(json!({"jobs": jobs})).into_response()
}

pub(super) async fn one(State(app): State<Shared>, Path(id): Path<String>) -> Response {
    match app.jobs.get(&id) {
        Some((job, events)) => {
            let position = app.jobs.position(&id);
            Json(json!({"job": job, "events": events, "position": position})).into_response()
        }
        None => missing(),
    }
}

/// The job's events so far, then new ones as they happen; ends when the job does. While it waits
/// in the queue, a note says its position each time that changes.
pub(super) async fn events(State(app): State<Shared>, Path(id): Path<String>) -> Response {
    let events: BoxStream<'static, Result<Sse, Infallible>> = match app.jobs.live(&id) {
        Some(job) => {
            let (past, rx) = job.follow();
            let state = (rx, job, app.clone(), None::<usize>);
            let live = stream::unfold(state, |(mut rx, job, app, mut at)| async move {
                loop {
                    if job.record().status == Status::Queued {
                        let now = app.jobs.position(&job.record().id);
                        if now.is_some() && now != at {
                            at = now;
                            let mut e = Event::new(
                                Level::Info,
                                Kind::Note,
                                format!("queued (position {})", now.unwrap_or(1)),
                            );
                            e.job_id = Some(job.record().id);
                            return Some((event(e), (rx, job, app, at)));
                        }
                    }
                    tokio::select! {
                        biased;
                        r = rx.recv() => match r {
                            Ok(e) => return Some((event(e), (rx, job, app, at))),
                            Err(RecvError::Lagged(_)) => continue,
                            Err(RecvError::Closed) => return None,
                        },
                        _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                            if job.record().status.finished() {
                                return None;
                            }
                        }
                    }
                }
            });
            stream::iter(past.into_iter().map(event))
                .chain(live)
                .boxed()
        }
        None => match app.jobs.get(&id) {
            Some((_, past)) => stream::iter(past.into_iter().map(event)).boxed(),
            None => return missing(),
        },
    };
    let all = events.chain(stream::once(async {
        Ok(Sse::default().event("end").data(""))
    }));
    axum::response::Sse::new(all)
        .keep_alive(KeepAlive::default())
        .into_response()
}

pub(super) async fn cancel(State(app): State<Shared>, Path(id): Path<String>) -> Response {
    let message = match app.jobs.cancel(&id) {
        Some(Status::Queued) => "removed from the queue",
        Some(_) => "cancelling; work already saved stays",
        None => "that job is not running or queued",
    };
    Json(json!({"ok": true, "message": message})).into_response()
}

fn event(e: Event) -> Result<Sse, Infallible> {
    Ok(Sse::default()
        .event("event")
        .json_data(&e)
        .unwrap_or_default())
}

fn missing() -> Response {
    super::error_body(StatusCode::NOT_FOUND, "no such job").into_response()
}
