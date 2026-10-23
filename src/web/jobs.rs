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

pub(super) async fn list(State(app): State<Shared>) -> Response {
    Json(json!({"jobs": app.jobs.list(crate::jobs::MAX_JOBS)})).into_response()
}

pub(super) async fn one(State(app): State<Shared>, Path(id): Path<String>) -> Response {
    match app.jobs.get(&id) {
        Some((job, events)) => Json(json!({"job": job, "events": events})).into_response(),
        None => missing(),
    }
}

/// The job's events so far, then new ones as they happen; ends when the job does.
pub(super) async fn events(State(app): State<Shared>, Path(id): Path<String>) -> Response {
    let events: BoxStream<'static, Result<Sse, Infallible>> = match app.jobs.live(&id) {
        Some(job) => {
            let (past, rx) = job.follow();
            let live = stream::unfold((rx, job), |(mut rx, job)| async move {
                loop {
                    tokio::select! {
                        biased;
                        r = rx.recv() => match r {
                            Ok(e) => return Some((event(e), (rx, job))),
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
    match app.jobs.live(&id) {
        Some(job) => {
            job.cancel.cancel();
            Json(json!({"ok": true, "message": "cancelling; work already saved stays"}))
                .into_response()
        }
        None => Json(json!({"ok": true, "message": "that job is not running"})).into_response(),
    }
}

fn event(e: crate::events::Event) -> Result<Sse, Infallible> {
    Ok(Sse::default()
        .event("event")
        .json_data(&e)
        .unwrap_or_default())
}

fn missing() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({"ok": false, "message": "no such job"})),
    )
        .into_response()
}
