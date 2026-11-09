//! Sign-in through an OpenID Connect provider: the authorization code flow with PKCE.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet, EndpointNotSet,
    EndpointSet, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
    TokenResponse,
};

use crate::server::OidcConfig;

type Client = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

/// How long someone may take at the provider before the sign-in has to start again.
const PENDING_FOR: Duration = Duration::from_secs(600);
/// Signing keys and endpoints are fetched again after this long, or when a token fails to verify.
const DISCOVERY_FOR: Duration = Duration::from_secs(3600);
const MAX_PENDING: usize = 1000;

/// Where the provider sends people back.
pub const CALLBACK: &str = "/auth/oidc/callback";

struct Pending {
    nonce: Nonce,
    verifier: PkceCodeVerifier,
    started: Instant,
}

pub struct Oidc {
    pub config: OidcConfig,
    redirect: RedirectUrl,
    http: reqwest::Client,
    client: tokio::sync::Mutex<Option<(Client, Instant)>>,
    pending: Mutex<HashMap<String, Pending>>,
}

#[derive(Debug)]
struct HttpError(String);

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for HttpError {}

async fn send(
    http: reqwest::Client,
    req: openidconnect::HttpRequest,
) -> Result<openidconnect::HttpResponse, HttpError> {
    let err = |e: &dyn std::fmt::Display| HttpError(e.to_string());
    let req = reqwest::Request::try_from(req).map_err(|e| err(&e))?;
    let res = http.execute(req).await.map_err(|e| err(&e))?;
    let mut out = openidconnect::http::Response::builder().status(res.status());
    for (k, v) in res.headers() {
        out = out.header(k, v);
    }
    let body = res.bytes().await.map_err(|e| err(&e))?;
    out.body(body.to_vec()).map_err(|e| err(&e))
}

impl Oidc {
    pub fn new(config: OidcConfig, public_url: &str) -> Result<Oidc> {
        IssuerUrl::new(config.issuer.clone()).context("COMINDLY_OIDC_ISSUER is not a URL")?;
        let redirect = RedirectUrl::new(format!("{public_url}{CALLBACK}"))
            .context("COMINDLY_PUBLIC_URL is not a URL")?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .build()?;
        Ok(Oidc {
            config,
            redirect,
            http,
            client: Default::default(),
            pending: Default::default(),
        })
    }

    /// The provider's endpoints and signing keys, fetched again when old or when `fresh`.
    async fn client(&self, fresh: bool) -> Result<Client> {
        let mut cached = self.client.lock().await;
        if let Some((c, at)) = cached.as_ref() {
            if !fresh && at.elapsed() < DISCOVERY_FOR {
                return Ok(c.clone());
            }
        }
        let http = self.http.clone();
        let call = move |r| send(http.clone(), r);
        let issuer = IssuerUrl::new(self.config.issuer.clone())?;
        let meta = CoreProviderMetadata::discover_async(issuer, &call)
            .await
            .with_context(|| {
                format!(
                    "cannot reach the sign-in provider at {}",
                    self.config.issuer
                )
            })?;
        let client = CoreClient::from_provider_metadata(
            meta,
            ClientId::new(self.config.client_id.clone()),
            Some(ClientSecret::new(self.config.client_secret.clone())),
        )
        .set_redirect_uri(self.redirect.clone());
        *cached = Some((client.clone(), Instant::now()));
        Ok(client)
    }

    /// Where to send the browser, and the state it must bring back.
    pub async fn start(&self) -> Result<(String, String)> {
        let client = self.client(false).await?;
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let (url, state, nonce) = client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .add_scope(Scope::new("email".into()))
            .add_scope(Scope::new("profile".into()))
            .set_pkce_challenge(challenge)
            .url();
        let mut pending = self.pending.lock().unwrap();
        pending.retain(|_, p| p.started.elapsed() < PENDING_FOR);
        anyhow::ensure!(
            pending.len() < MAX_PENDING,
            "Too many sign-ins in progress; try again in a few minutes"
        );
        pending.insert(
            state.secret().clone(),
            Pending {
                nonce,
                verifier,
                started: Instant::now(),
            },
        );
        Ok((url.to_string(), state.secret().clone()))
    }

    /// The verified email address of someone coming back from the provider.
    pub async fn finish(&self, state: &str, code: &str) -> Result<String> {
        let pending = self
            .pending
            .lock()
            .unwrap()
            .remove(state)
            .filter(|p| p.started.elapsed() < PENDING_FOR)
            .context("This sign-in expired or was already used; start again")?;
        let client = self.client(false).await?;
        let http = self.http.clone();
        let call = move |r| send(http.clone(), r);
        let tokens = client
            .exchange_code(AuthorizationCode::new(code.to_string()))?
            .set_pkce_verifier(pending.verifier)
            .request_async(&call)
            .await
            .map_err(|e| {
                crate::ui::warn(&format!("sign-in provider refused the code: {e:?}"));
                anyhow::anyhow!("The sign-in provider did not accept this sign-in; start again")
            })?;
        let id_token = tokens
            .id_token()
            .context("The sign-in provider sent no ID token")?;
        let claims = match id_token.claims(&client.id_token_verifier(), &pending.nonce) {
            Ok(c) => c.clone(),
            // The provider may have rotated its signing keys since they were fetched.
            Err(_) => {
                let client = self.client(true).await?;
                let verified = id_token
                    .claims(&client.id_token_verifier(), &pending.nonce)
                    .cloned();
                verified.map_err(|e| {
                    crate::ui::warn(&format!("ID token rejected: {e}"));
                    anyhow::anyhow!("The sign-in could not be verified; start again")
                })?
            }
        };
        if claims.email_verified() == Some(false) {
            bail!("The sign-in provider has not verified this email address");
        }
        let email = claims
            .email()
            .map(|e| e.as_str().to_string())
            .or_else(|| {
                claims
                    .preferred_username()
                    .map(|u| u.as_str().to_string())
                    .filter(|u| u.contains('@'))
            })
            .context("The sign-in provider sent no email address; ask its admin to include the email claim")?;
        Ok(crate::server::normalize(&email))
    }

    /// `microsoft` or `google` when the issuer is theirs.
    pub fn brand(&self) -> Option<&'static str> {
        let host = self.config.issuer.split("://").nth(1)?.split('/').next()?;
        match host {
            "login.microsoftonline.com" | "sts.windows.net" => Some("microsoft"),
            "accounts.google.com" => Some("google"),
            _ => None,
        }
    }

    /// Whether someone at `email` may join without an invitation.
    pub fn may_join(&self, email: &str) -> bool {
        email
            .rsplit_once('@')
            .is_some_and(|(_, d)| self.config.allowed_domains.iter().any(|a| a == d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidconnect::core::{
        CoreIdToken, CoreIdTokenClaims, CoreJsonWebKeySet, CoreJwsSigningAlgorithm,
        CoreRsaPrivateSigningKey,
    };
    use openidconnect::{
        Audience, EmptyAdditionalClaims, EndUserEmail, PrivateSigningKey, StandardClaims,
        SubjectIdentifier,
    };
    use std::sync::Arc;

    /// What the provider puts in the next ID token.
    #[derive(Clone)]
    struct Next {
        email: Option<String>,
        verified: Option<bool>,
        audience: String,
        nonce: Option<String>,
        expires_in: i64,
    }

    struct Provider {
        issuer: String,
        key: CoreRsaPrivateSigningKey,
        next: Mutex<Next>,
        /// The PKCE verifier each token request carried.
        verifiers: Mutex<Vec<String>>,
    }

    fn signing_key() -> CoreRsaPrivateSigningKey {
        use rsa::pkcs1::EncodeRsaPrivateKey;
        let key = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
        let pem = key.to_pkcs1_pem(rsa::pkcs8::LineEnding::LF).unwrap();
        CoreRsaPrivateSigningKey::from_pem(
            &pem,
            Some(openidconnect::JsonWebKeyId::new("k1".into())),
        )
        .unwrap()
    }

    async fn provider() -> Arc<Provider> {
        use axum::routing::{get, post};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let p = Arc::new(Provider {
            issuer: issuer.clone(),
            key: signing_key(),
            next: Mutex::new(Next {
                email: Some("Dev@Example.com".into()),
                verified: None,
                audience: "comindly".into(),
                nonce: None,
                expires_in: 300,
            }),
            verifiers: Default::default(),
        });
        let (meta, jwks, token) = (p.clone(), p.clone(), p.clone());
        let app = axum::Router::new()
            .route(
                "/.well-known/openid-configuration",
                get(move || async move {
                    let i = &meta.issuer;
                    axum::Json(serde_json::json!({
                        "issuer": i,
                        "authorization_endpoint": format!("{i}/authorize"),
                        "token_endpoint": format!("{i}/token"),
                        "jwks_uri": format!("{i}/jwks"),
                        "response_types_supported": ["code"],
                        "subject_types_supported": ["public"],
                        "id_token_signing_alg_values_supported": ["RS256"],
                    }))
                }),
            )
            .route(
                "/jwks",
                get(move || async move {
                    axum::Json(CoreJsonWebKeySet::new(vec![jwks.key.as_verification_key()]))
                }),
            )
            .route(
                "/token",
                post(move |body: String| async move {
                    let form: HashMap<String, String> =
                        openidconnect::url::form_urlencoded::parse(body.as_bytes())
                            .into_owned()
                            .collect();
                    token
                        .verifiers
                        .lock()
                        .unwrap()
                        .push(form.get("code_verifier").cloned().unwrap_or_default());
                    let n = token.next.lock().unwrap().clone();
                    let now = chrono::Utc::now();
                    let mut standard = StandardClaims::new(SubjectIdentifier::new("u1".into()));
                    standard = standard
                        .set_email(n.email.map(EndUserEmail::new))
                        .set_email_verified(n.verified);
                    let claims = CoreIdTokenClaims::new(
                        IssuerUrl::new(token.issuer.clone()).unwrap(),
                        vec![Audience::new(n.audience)],
                        now + chrono::Duration::seconds(n.expires_in),
                        now - chrono::Duration::seconds(60),
                        standard,
                        EmptyAdditionalClaims {},
                    )
                    .set_nonce(n.nonce.map(Nonce::new));
                    let id = CoreIdToken::new(
                        claims,
                        &token.key,
                        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
                        None,
                        None,
                    )
                    .unwrap();
                    axum::Json(serde_json::json!({
                        "access_token": "at",
                        "token_type": "Bearer",
                        "expires_in": 3600,
                        "id_token": id,
                    }))
                }),
            );
        tokio::spawn(async move { axum::serve(listener, app).await });
        p
    }

    fn oidc(issuer: &str) -> Oidc {
        Oidc::new(
            OidcConfig {
                issuer: issuer.into(),
                client_id: "comindly".into(),
                client_secret: "client secret".into(),
                label: "Example".into(),
                allowed_domains: vec!["example.com".into()],
            },
            "https://comindly.example.com",
        )
        .unwrap()
    }

    /// Start a sign-in; the provider will answer with this sign-in's nonce.
    async fn begin(p: &Provider, o: &Oidc) -> (String, String) {
        let (url, state) = o.start().await.unwrap();
        let url = openidconnect::url::Url::parse(&url).unwrap();
        let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
        assert_eq!(q["state"], state);
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(
            q["redirect_uri"],
            "https://comindly.example.com/auth/oidc/callback"
        );
        assert!(q["scope"].contains("openid") && q["scope"].contains("email"));
        p.next.lock().unwrap().nonce = Some(q["nonce"].clone());
        (state, q["code_challenge"].clone())
    }

    #[tokio::test]
    async fn a_signed_and_matching_id_token_gives_the_verified_email_once() {
        let p = provider().await;
        let o = oidc(&p.issuer);
        let (state, challenge) = begin(&p, &o).await;
        assert_eq!(o.finish(&state, "code-1").await.unwrap(), "dev@example.com");
        let verifier = p.verifiers.lock().unwrap()[0].clone();
        let expected =
            PkceCodeChallenge::from_code_verifier_sha256(&PkceCodeVerifier::new(verifier));
        assert_eq!(
            expected.as_str(),
            challenge,
            "the code is redeemed with this sign-in's PKCE secret"
        );
        assert!(
            o.finish(&state, "code-1").await.is_err(),
            "a state works once"
        );
        assert!(o.finish("made-up", "code-1").await.is_err());
        assert!(o.may_join("dev@example.com"));
        assert!(!o.may_join("dev@example.com.evil.org"));
        assert!(!o.may_join("dev@other.org"));
        assert_eq!(o.brand(), None);
        assert_eq!(
            oidc("https://login.microsoftonline.com/tenant/v2.0").brand(),
            Some("microsoft")
        );
        assert_eq!(oidc("https://accounts.google.com").brand(), Some("google"));
    }

    #[tokio::test]
    async fn tokens_for_someone_else_stale_or_replayed_are_refused() {
        let p = provider().await;
        let o = oidc(&p.issuer);
        type Change = fn(&mut Next);
        let cases: [(&str, Change); 5] = [
            ("another client's token", |n| {
                n.audience = "other-app".into()
            }),
            ("an expired token", |n| n.expires_in = -120),
            ("another sign-in's nonce", |n| n.nonce = Some("old".into())),
            ("an unverified email", |n| n.verified = Some(false)),
            ("no email", |n| n.email = None),
        ];
        for (what, change) in cases {
            let (state, _) = begin(&p, &o).await;
            change(&mut p.next.lock().unwrap());
            assert!(
                o.finish(&state, "code").await.is_err(),
                "{what} was accepted"
            );
            *p.next.lock().unwrap() = Next {
                email: Some("dev@example.com".into()),
                verified: None,
                audience: "comindly".into(),
                nonce: None,
                expires_in: 300,
            };
        }
    }

    #[tokio::test]
    async fn an_unreachable_provider_is_an_error_not_a_panic() {
        let o = oidc("http://127.0.0.1:9");
        let e = o.start().await.unwrap_err();
        assert!(format!("{e:#}").contains("cannot reach the sign-in provider"));
    }
}
