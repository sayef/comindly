//! The CLI's client for a comindly server's `/api/v1`: typed requests, its error shape, job events
//! and signing in by browser approval.

use anyhow::{bail, Context, Result};
use serde::{de::DeserializeOwned, Serialize};

use crate::events::Event;

pub struct Client {
    base: String,
    token: Option<String>,
    http: reqwest::Client,
}

impl Client {
    pub fn new(base: &str, token: Option<String>) -> Client {
        Client {
            base: base.trim_end_matches('/').to_string(),
            token,
            http: reqwest::Client::new(),
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let r = self.http.request(method, format!("{}{path}", self.base));
        match &self.token {
            Some(t) => r.bearer_auth(t),
            None => r,
        }
    }

    pub async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T> {
        let r = self
            .request(reqwest::Method::GET, path)
            .query(query)
            .send()
            .await;
        self.read(r).await
    }

    pub async fn post<T: DeserializeOwned>(&self, path: &str, body: &impl Serialize) -> Result<T> {
        let r = self
            .request(reqwest::Method::POST, path)
            .json(body)
            .send()
            .await;
        self.read(r).await
    }

    async fn read<T: DeserializeOwned>(&self, r: reqwest::Result<reqwest::Response>) -> Result<T> {
        let r = r.with_context(|| format!("cannot reach {}", self.base))?;
        let status = r.status();
        if status.is_success() {
            return r.json().await.context("unexpected answer from the server");
        }
        let body: serde_json::Value = r.json().await.unwrap_or_default();
        let message = body["error"]["message"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| format!("the server answered {status}"));
        if status == reqwest::StatusCode::UNAUTHORIZED && self.token.is_none() {
            bail!("{message}: sign in with `comindly login {}`", self.base);
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            bail!(
                "{message}: your sign-in has ended; run `comindly login {}`",
                self.base
            );
        }
        bail!(message)
    }

    /// Follow job `id`'s events until it ends.
    pub async fn follow(&self, id: &str, mut on: impl FnMut(Event)) -> Result<()> {
        let mut r = self
            .request(reqwest::Method::GET, &format!("/api/v1/jobs/{id}/events"))
            .send()
            .await
            .with_context(|| format!("cannot reach {}", self.base))?;
        anyhow::ensure!(
            r.status().is_success(),
            "the server answered {}",
            r.status()
        );
        let mut buf = String::new();
        let mut kind = String::new();
        while let Some(chunk) = r.chunk().await? {
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(i) = buf.find('\n') {
                let line = buf[..i].trim_end_matches('\r').to_string();
                buf.drain(..=i);
                if let Some(k) = line.strip_prefix("event:") {
                    kind = k.trim().to_string();
                } else if let Some(d) = line.strip_prefix("data:") {
                    if kind == "end" {
                        return Ok(());
                    }
                    if let Ok(e) = serde_json::from_str::<Event>(d.trim()) {
                        on(e);
                    }
                }
            }
        }
        Ok(())
    }
}

/// What a sign-in by browser approval returns.
pub struct Approved {
    pub token: String,
    pub email: String,
}

/// Sign in to `base` as `client`: `show` gets the address and code to approve in the browser, then
/// the server is polled until someone approves or denies it, or it expires.
pub async fn login(base: &str, client: &str, show: impl FnOnce(&str, &str)) -> Result<Approved> {
    #[derive(serde::Deserialize)]
    struct Start {
        device_code: String,
        user_code: String,
        verify_url: String,
        interval: u64,
    }
    #[derive(serde::Deserialize)]
    struct Poll {
        status: String,
        token: Option<String>,
        email: Option<String>,
    }
    let c = Client::new(base, None);
    let start: Start = c
        .post(
            "/api/v1/auth/device/start",
            &serde_json::json!({"client": client}),
        )
        .await?;
    show(&start.verify_url, &start.user_code);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(start.interval.max(1))).await;
        let p: Poll = c
            .post(
                "/api/v1/auth/device/token",
                &serde_json::json!({"device_code": start.device_code}),
            )
            .await?;
        if p.status == "approved" {
            return Ok(Approved {
                token: p.token.context("no token in the answer")?,
                email: p.email.unwrap_or_default(),
            });
        }
    }
}
