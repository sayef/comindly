use axum::routing::{get, post};
use axum::Router;

const ACCOUNTS: &str = "http://accounts/api/accounts";

async fn list_items() {}

async fn create_item() {}

#[actix_web::get("/api/reports/{id}")]
async fn report() {}

fn app() -> Router {
    Router::new().route("/api/items", get(list_items).post(create_item))
}

async fn account(id: &str) {
    let client = reqwest::Client::new();
    let _ = client.get(format!("{ACCOUNTS}/{id}")).send().await;
}
