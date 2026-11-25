use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};

mod audit;
mod flags;
mod ingest;
mod ranges;
mod store;
mod units;

use store::{LabResult, ResultStore};

pub struct App {
    pub store: ResultStore,
    pub s3: aws_sdk_s3::Client,
    pub sns: aws_sdk_sns::Client,
}

async fn results_for_subject(State(app): State<Arc<App>>, Path(subject_id): Path<String>) -> Json<Vec<LabResult>> {
    Json(app.store.for_subject(&subject_id).await.unwrap_or_default())
}

async fn flags_for_subject(State(app): State<Arc<App>>, Path(subject_id): Path<String>) -> Json<Vec<LabResult>> {
    let all = app.store.for_subject(&subject_id).await.unwrap_or_default();
    Json(all.into_iter().filter(|r| r.flag.is_some()).collect())
}

fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/api/lab-results/:subject_id", get(results_for_subject))
        .route("/api/lab-results/:subject_id/flags", get(flags_for_subject))
        .with_state(app)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = aws_config::load_from_env().await;
    let app = Arc::new(App {
        store: ResultStore::connect(&std::env::var("LAB_DATABASE_URL")?).await?,
        s3: aws_sdk_s3::Client::new(&cfg),
        sns: aws_sdk_sns::Client::new(&cfg),
    });
    tokio::spawn(ingest::run(app.clone()));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    axum::serve(listener, router(app)).await?;
    Ok(())
}
