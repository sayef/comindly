//! The CLI's client for a comindly server's `/api/v1`: typed requests, its error shape, job events
//! and signing in by browser approval.

use anyhow::{bail, Context, Result};
use serde::{de::DeserializeOwned, Serialize};

use crate::api;
use crate::events::Event;

#[derive(Clone)]
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
        query: &(impl Serialize + ?Sized),
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

    /// Revoke the token this client signs in with; `false` when the server no longer accepts it.
    pub async fn revoke_own_token(&self) -> Result<bool> {
        let r = self
            .request(reqwest::Method::POST, "/api/v1/auth/token/revoke")
            .json(&serde_json::Map::new())
            .send()
            .await;
        if r.as_ref()
            .is_ok_and(|r| r.status() == reqwest::StatusCode::UNAUTHORIZED)
        {
            return Ok(false);
        }
        self.read::<api::Done>(r).await.map(|_| true)
    }

    async fn read<T: DeserializeOwned>(&self, r: reqwest::Result<reqwest::Response>) -> Result<T> {
        let r = r.with_context(|| format!("cannot reach {}", self.base))?;
        let status = r.status();
        if status.is_success() {
            return r.json().await.context("unexpected answer from the server");
        }
        let message = r
            .json::<api::ErrorBody>()
            .await
            .map(|b| b.error.message)
            .unwrap_or_else(|_| format!("the server answered {status}"));
        if status == reqwest::StatusCode::UNAUTHORIZED && self.token.is_none() {
            bail!("{message}: sign in with `comindly login {}`", self.base);
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            bail!(
                "{message}: your sign-in has ended; run `comindly login {}` to sign in again",
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
    /// When the token stops working; `None` when it does not expire.
    pub expires: Option<String>,
}

/// Sign in to `base` as `client`: `show` gets the address and code to approve in the browser, then
/// the server is polled until someone approves or denies it, or it expires.
pub async fn login(base: &str, client: &str, show: impl FnOnce(&str, &str)) -> Result<Approved> {
    let c = Client::new(base, None);
    let start: api::DeviceStart = c
        .post(
            "/api/v1/auth/device/start",
            &api::DeviceStartRequest {
                client: client.to_string(),
            },
        )
        .await?;
    show(&start.verify_url, &start.user_code);
    let poll = api::DeviceTokenRequest {
        device_code: start.device_code.clone(),
    };
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(start.interval.max(1))).await;
        let p: api::DeviceToken = c.post("/api/v1/auth/device/token", &poll).await?;
        if let api::DeviceToken::Approved {
            token,
            email,
            expires,
        } = p
        {
            return Ok(Approved {
                token,
                email,
                expires,
            });
        }
    }
}
