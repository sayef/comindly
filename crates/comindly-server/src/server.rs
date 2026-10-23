//! Server mode (on when `COMINDLY_PUBLIC_URL` is set): members, invitations, sign-in links,
//! sessions, personal access tokens and encrypted secrets, in a LanceDB table next to the index.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use chrono::{Duration, Utc};
use lancedb::arrow::arrow_array::{RecordBatch, RecordBatchIterator, StringArray};
use lancedb::arrow::arrow_schema::{DataType, Field, Schema};
use serde::{Deserialize, Serialize};

use crate::backends::{
    AccountStore, AuditLog, Authenticator, DeviceSignIn, Mailer, SecretStore, SettingsStore,
};

/// `NAME` from the environment, or the contents of the file `NAME_FILE` names.
pub fn env_secret(name: &str) -> Option<String> {
    if let Some(v) = std::env::var(name).ok().filter(|v| !v.trim().is_empty()) {
        return Some(v.trim().to_string());
    }
    let path = std::env::var(format!("{name}_FILE")).ok()?;
    match std::fs::read_to_string(&path) {
        Ok(v) => Some(v.trim().to_string()).filter(|v| !v.is_empty()),
        Err(e) => {
            crate::ui::warn(&format!("{name}_FILE ({path}) cannot be read: {e}"));
            None
        }
    }
}

/// The deployment's settings for server mode, read once when the server starts.
#[derive(Clone)]
pub struct ServerConfig {
    /// The address members reach this server at, without a trailing slash.
    pub public_url: String,
    /// Always admins, whatever the database says.
    pub admin_emails: Vec<String>,
    pub sign_in: SignIn,
    pub admin_token: Option<String>,
    pub secret_key: String,
    pub secret_key_previous: Option<String>,
    pub smtp_url: Option<String>,
    pub smtp_from: String,
    /// Take the client address from `X-Forwarded-For`, behind a proxy you run.
    pub trust_proxy: bool,
    /// The deployment's LLM key, which wins over one stored here.
    pub llm_key: Option<String>,
}

impl ServerConfig {
    /// `None` without `COMINDLY_PUBLIC_URL`; an error when server mode is set up wrong.
    pub fn from_env() -> Result<Option<ServerConfig>> {
        let Some(url) = env_secret("COMINDLY_PUBLIC_URL") else {
            return Ok(None);
        };
        let admin_emails: Vec<String> = env_secret("COMINDLY_ADMIN_EMAILS")
            .unwrap_or_default()
            .split([',', ' ', '\n'])
            .map(normalize)
            .filter(|e| e.contains('@'))
            .collect();
        anyhow::ensure!(
            !admin_emails.is_empty(),
            "server mode needs COMINDLY_ADMIN_EMAILS: who administers this server"
        );
        let admin_token = env_secret("COMINDLY_ADMIN_TOKEN");
        if let Some(t) = &admin_token {
            anyhow::ensure!(
                t.len() >= 32,
                "COMINDLY_ADMIN_TOKEN is too short: use at least 32 random characters (e.g. `openssl rand -base64 32`)"
            );
        }
        let smtp_url = env_secret("COMINDLY_SMTP_URL");
        let sign_in = match env_secret("COMINDLY_SIGN_IN").as_deref() {
            Some("email") => {
                anyhow::ensure!(
                    smtp_url.is_some(),
                    "COMINDLY_SIGN_IN=email needs COMINDLY_SMTP_URL"
                );
                SignIn::Email
            }
            Some("code") => SignIn::Code,
            Some(other) => bail!("COMINDLY_SIGN_IN is `email` or `code`, not `{other}`"),
            None if smtp_url.is_some() => SignIn::Email,
            None => SignIn::Code,
        };
        Ok(Some(ServerConfig {
            public_url: url.trim_end_matches('/').to_string(),
            admin_emails,
            sign_in,
            admin_token,
            secret_key: env_secret("COMINDLY_SECRET_KEY")
                .context("server mode needs COMINDLY_SECRET_KEY")?,
            secret_key_previous: env_secret("COMINDLY_SECRET_KEY_PREVIOUS"),
            smtp_url,
            smtp_from: env_secret("COMINDLY_SMTP_FROM")
                .unwrap_or_else(|| "comindly <comindly@localhost>".into()),
            trust_proxy: env_secret("COMINDLY_TRUST_PROXY")
                .is_some_and(|v| v == "true" || v == "1"),
            llm_key: env_secret("OPENAI_API_KEY"),
        }))
    }

    pub fn https(&self) -> bool {
        self.public_url.starts_with("https://")
    }
}

/// A running server: its settings and its backends.
pub struct Server {
    pub config: ServerConfig,
    pub accounts: Arc<dyn AccountStore>,
    pub audit: Arc<dyn AuditLog>,
    pub auth: Arc<dyn Authenticator>,
    pub devices: Arc<dyn DeviceSignIn>,
    pub secrets: Arc<dyn SecretStore>,
    pub settings: Arc<dyn SettingsStore>,
    pub mailer: Arc<dyn Mailer>,
    pub jobs: Arc<dyn crate::jobs::JobStore>,
}

impl Server {
    /// The default backends: a signed table under `index_root`, and SMTP.
    pub fn open(index_root: &str, config: ServerConfig) -> Result<Server> {
        let store = Arc::new(Store::open(index_root, &config)?);
        let mailer = Arc::new(crate::backends::Smtp {
            url: config.smtp_url.clone(),
            from: config.smtp_from.clone(),
        });
        Ok(Server {
            config,
            accounts: store.clone(),
            audit: store.clone(),
            auth: store.clone(),
            devices: store.clone(),
            secrets: store.clone(),
            settings: store.clone(),
            jobs: store,
            mailer,
        })
    }

    /// A git host's token: the deployment's, else the one stored here.
    pub fn git_token(&self, host: &str) -> Option<String> {
        crate::config::env_token(host).or_else(|| self.secrets.secret(&git_secret(host)))
    }
}

/// How members sign in: everything by email, or everything by codes admins hand out. Email when
/// the server can send it, else codes; `COMINDLY_SIGN_IN=email|code` decides when set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SignIn {
    Email,
    Code,
}

pub fn normalize(email: &str) -> String {
    email.trim().to_lowercase()
}

/// What someone may do, each role including the ones below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Members, repositories, settings, secrets and indexing.
    Admin,
    /// Writes to the ledger and conventions.
    Member,
    /// Reads.
    Viewer,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Member => "member",
            Role::Viewer => "viewer",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Role::Admin => 2,
            Role::Member => 1,
            Role::Viewer => 0,
        }
    }

    /// This role may do what `other` may.
    pub fn at_least(self, other: Role) -> bool {
        self.rank() >= other.rank()
    }
}

/// How a request proved who it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    Session,
    AdminToken,
    /// A personal access token: enough for agents, not for managing the account.
    Token,
}

/// Who a request comes from.
#[derive(Debug, Clone, Serialize)]
pub struct Who {
    pub email: String,
    pub role: Role,
}

#[derive(Debug, Serialize)]
pub struct Member {
    pub email: String,
    pub role: Role,
    pub fixed: bool,
    pub created: String,
    pub last_seen: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Invite {
    pub email: String,
    pub role: Role,
    pub created_by: String,
    pub expires: String,
}

#[derive(Debug, Serialize)]
pub struct AccessToken {
    pub id: String,
    pub name: String,
    pub created: String,
    pub last_used: Option<String>,
    /// `None` for a token that never expires.
    pub expires: Option<String>,
    pub expired: bool,
}

/// One row of the audit log; details never hold token values or codes.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    pub at: String,
    pub actor: String,
    pub action: String,
    pub detail: String,
}

/// Which audit entries to list: `before` is the `next` cursor of the previous page.
#[derive(Debug, Default, Deserialize)]
pub struct AuditQuery {
    pub limit: Option<usize>,
    pub before: Option<String>,
    pub actor: Option<String>,
    pub action: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuditPage {
    pub entries: Vec<AuditEntry>,
    /// The cursor for the next (older) page, when there is one.
    pub next: Option<String>,
}

/// A secret's state for the UI; its value never leaves the server.
#[derive(Debug, Serialize)]
pub struct SecretInfo {
    pub name: String,
    pub source: &'static str,
    pub hint: Option<String>,
    pub updated: Option<String>,
    pub updated_by: Option<String>,
}

const SESSION_DAYS: i64 = 30;
const LINK_MINUTES: i64 = 15;
const INVITE_DAYS: i64 = 7;
const MAX_FAILED: i64 = 5;
const MAX_REDEEM_FAILED: i64 = 20;
const LOCK_MINUTES: i64 = 15;
const MAX_LINKS_PER_HOUR: i64 = 5;
const DEVICE_MINUTES: i64 = 10;
/// Command-line sign-ins one address may start in `LOCK_MINUTES`.
const MAX_DEVICE_STARTS: i64 = 10;
/// How long a token lasts unless chosen otherwise, and a command-line sign-in's always.
pub const TOKEN_DAYS: u32 = 90;
/// Expired tokens stay listed (as expired) this long before they are removed.
const EXPIRED_TOKEN_DAYS: i64 = 30;
const AUDIT_PAGE: usize = 50;
const AUDIT_PAGE_MAX: usize = 500;
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

fn now() -> String {
    Utc::now().to_rfc3339()
}

/// A random URL-safe token with `prefix`.
pub fn new_token(prefix: &str) -> String {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).expect("system randomness");
    format!("{prefix}{}", blake3::Hash::from(b).to_hex())
}

/// A sign-in or invitation code people type: 20 characters (100 bits) in groups of four.
pub fn new_code() -> String {
    let mut b = [0u8; 20];
    getrandom::fill(&mut b).expect("system randomness");
    let chars: Vec<char> = b
        .iter()
        .map(|x| CODE_ALPHABET[(*x as usize) % CODE_ALPHABET.len()] as char)
        .collect();
    chars
        .chunks(4)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

/// A code as typed, without case, spaces or dashes mattering.
fn code_hash(code: &str) -> String {
    let plain: String = code
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    hash(&format!("code:{plain}"))
}

fn hash(token: &str) -> String {
    blake3::hash(token.as_bytes()).to_hex().to_string()
}

fn cipher(key: &str) -> XChaCha20Poly1305 {
    let k = blake3::derive_key("comindly secrets v1", key.as_bytes());
    XChaCha20Poly1305::new((&k).into())
}

/// Encrypt `value`, bound to the secret's `name` so a value cannot be moved to another name.
fn seal(key: &str, name: &str, value: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut nonce = [0u8; 24];
    getrandom::fill(&mut nonce).context("randomness")?;
    let payload = Payload {
        msg: value.as_bytes(),
        aad: name.as_bytes(),
    };
    let sealed = cipher(key)
        .encrypt(&XNonce::from(nonce), payload)
        .map_err(|_| anyhow::anyhow!("encryption failed"))?;
    Ok((sealed, nonce.to_vec()))
}

fn open(key: &str, name: &str, sealed: &[u8], nonce: &[u8]) -> Option<String> {
    let payload = Payload {
        msg: sealed,
        aad: name.as_bytes(),
    };
    let plain = cipher(key)
        .decrypt(&XNonce::try_from(nonce).ok()?, payload)
        .ok()?;
    String::from_utf8(plain).ok()
}

/// The last four characters, to recognise a token without showing it.
fn hint(value: &str) -> String {
    let tail: String = value
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

#[derive(Clone, Serialize, Deserialize)]
struct User {
    role: Role,
    created: String,
    last_seen: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Invitation {
    email: String,
    role: Role,
    created_by: String,
    created: String,
    expires: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct Link {
    email: String,
    created: String,
    expires: String,
}

/// A sign-in asked for by a command line, until a member approves it and the command collects it.
#[derive(Clone, Serialize, Deserialize)]
struct Device {
    /// The hash of the code the member confirms in the browser.
    user_code: String,
    client: String,
    created: String,
    expires: String,
    approved_by: Option<String>,
    denied: bool,
    /// The token issued on approval, sealed until it is collected: `(sealed, nonce)` in hex.
    token: Option<(String, String)>,
}

/// What a member sees before approving a command-line sign-in.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceRequest {
    pub client: String,
    pub expires: String,
}

/// Where a command-line sign-in stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevicePoll {
    Pending,
    Approved {
        token: String,
        email: String,
        expires: Option<String>,
    },
    Denied,
    Expired,
}

#[derive(Clone, Serialize, Deserialize)]
struct Session {
    email: String,
    created: String,
    expires: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct Token {
    hash: String,
    email: String,
    name: String,
    created: String,
    last_used: Option<String>,
    #[serde(default)]
    expires: Option<String>,
}

impl Token {
    fn expired(&self, now: &str) -> bool {
        self.expires.as_deref().is_some_and(|e| e <= now)
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Secret {
    sealed: String,
    nonce: String,
    hint: String,
    updated: String,
    updated_by: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct Code {
    hash: String,
    created: String,
}

/// Everything the server knows, as loaded from its table. Failed attempts stay in memory.
#[derive(Default)]
struct State {
    users: BTreeMap<String, User>,
    invites: HashMap<String, Invitation>,
    links: HashMap<String, Link>,
    sessions: HashMap<String, Session>,
    tokens: HashMap<String, Token>,
    secrets: BTreeMap<String, Secret>,
    codes: HashMap<String, Code>,
    /// When admins named by the deployment were last seen; they have no `user` row.
    seen: HashMap<String, String>,
    /// The server's `config.toml`, for containers that start without it.
    config: HashMap<String, String>,
    devices: HashMap<String, Device>,
    jobs: HashMap<String, crate::jobs::JobRecord>,
    job_events: HashMap<String, Vec<crate::events::Event>>,
    failures: Vec<(String, String)>,
}

/// One row to write (`Some` json) or delete (`None`), by kind and key.
type Change = (&'static str, String, Option<String>);

fn put(kind: &'static str, key: &str, value: &impl Serialize) -> Change {
    (kind, key.to_string(), serde_json::to_string(value).ok())
}

fn del(kind: &'static str, key: &str) -> Change {
    (kind, key.to_string(), None)
}

impl State {
    fn load(&mut self, kind: &str, key: String, json: &str) {
        fn add<T: serde::de::DeserializeOwned>(
            m: &mut impl Extend<(String, T)>,
            key: String,
            json: &str,
        ) {
            if let Ok(v) = serde_json::from_str(json) {
                m.extend([(key, v)]);
            }
        }
        match kind {
            "user" => add(&mut self.users, key, json),
            "invite" => add(&mut self.invites, key, json),
            "link" => add(&mut self.links, key, json),
            "session" => add(&mut self.sessions, key, json),
            "token" => add(&mut self.tokens, key, json),
            "secret" => add(&mut self.secrets, key, json),
            "code" => add(&mut self.codes, key, json),
            "seen" => add(&mut self.seen, key, json),
            "config" => add(&mut self.config, key, json),
            "device" => add(&mut self.devices, key, json),
            "job" => add(&mut self.jobs, key, json),
            "job-events" => add(&mut self.job_events, key, json),
            _ => {}
        }
    }

    /// Apply changes already written to the table.
    fn apply(&mut self, changes: &[Change]) {
        for (kind, key, json) in changes {
            match json {
                Some(j) => self.load(kind, key.clone(), j),
                None => match *kind {
                    "user" => drop(self.users.remove(key)),
                    "invite" => drop(self.invites.remove(key)),
                    "link" => drop(self.links.remove(key)),
                    "session" => drop(self.sessions.remove(key)),
                    "token" => drop(self.tokens.remove(key)),
                    "secret" => drop(self.secrets.remove(key)),
                    "code" => drop(self.codes.remove(key)),
                    "seen" => drop(self.seen.remove(key)),
                    "config" => drop(self.config.remove(key)),
                    "device" => drop(self.devices.remove(key)),
                    "job" => drop(self.jobs.remove(key)),
                    "job-events" => drop(self.job_events.remove(key)),
                    _ => {}
                },
            }
        }
    }
}

const TABLE: &str = "server";
const CONFIG: &str = "config.toml";
const REPOS: &str = "repos";
const KINDS: [&str; 12] = [
    "user",
    "invite",
    "link",
    "session",
    "token",
    "secret",
    "code",
    "seen",
    "config",
    "device",
    "job",
    "job-events",
];

/// LanceDB work from the store's synchronous methods.
fn run<T: Send + 'static>(
    f: impl std::future::Future<Output = Result<T>> + Send + 'static,
) -> Result<T> {
    crate::rt::block_on(f)
}

fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("kind", DataType::Utf8, false),
        Field::new("key", DataType::Utf8, false),
        Field::new("json", DataType::Utf8, false),
        Field::new("mac", DataType::Utf8, false),
    ]))
}

/// The key that proves a row was written by this server, from `COMINDLY_SECRET_KEY`.
fn mac_key(secret: &str) -> [u8; 32] {
    blake3::derive_key("comindly rows v1", secret.as_bytes())
}

fn mac(key: &[u8; 32], kind: &str, k: &str, json: &str) -> String {
    blake3::keyed_hash(key, format!("{kind}\0{k}\0{json}").as_bytes())
        .to_hex()
        .to_string()
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

async fn table(uri: &str) -> Result<lancedb::Table> {
    let db = crate::storage::connect(uri).execute().await?;
    match db.open_table(TABLE).execute().await {
        Ok(t) => Ok(t),
        Err(_) => match db.create_empty_table(TABLE, schema()).execute().await {
            Ok(t) => Ok(t),
            // Another instance created it first.
            Err(_) => Ok(db.open_table(TABLE).execute().await?),
        },
    }
}

/// Every row except the audit log, and the table version they came from.
async fn read_all(uri: String) -> Result<(u64, Vec<[String; 4]>)> {
    read_rows(uri, "kind != 'audit'").await
}

/// Rows matching `filter` as `[kind, key, json, mac]`, and the table version they came from.
async fn read_rows(uri: String, filter: &str) -> Result<(u64, Vec<[String; 4]>)> {
    use futures::TryStreamExt;
    use lancedb::query::{ExecutableQuery, QueryBase};
    let tbl = table(&uri).await?;
    let version = tbl.version().await?;
    let mut st = tbl.query().only_if(filter).execute().await?;
    let mut rows = Vec::new();
    while let Some(b) = st.try_next().await? {
        let col = |n: &str| {
            b.column_by_name(n)
                .and_then(|c| c.as_any().downcast_ref::<StringArray>().cloned())
        };
        let (Some(k), Some(key), Some(j), Some(m)) =
            (col("kind"), col("key"), col("json"), col("mac"))
        else {
            continue;
        };
        for i in 0..b.num_rows() {
            rows.push([k.value(i), key.value(i), j.value(i), m.value(i)].map(str::to_string));
        }
    }
    Ok((version, rows))
}

async fn write(uri: String, key: [u8; 32], changes: Vec<Change>) -> Result<u64> {
    let tbl = table(&uri).await?;
    let (puts, dels): (Vec<_>, Vec<_>) = changes.into_iter().partition(|(_, _, j)| j.is_some());
    if !dels.is_empty() {
        let pred = dels
            .iter()
            .map(|(k, key, _)| format!("(kind = {} AND key = {})", quote(k), quote(key)))
            .collect::<Vec<_>>()
            .join(" OR ");
        tbl.delete(&pred).await?;
    }
    if !puts.is_empty() {
        let batch = RecordBatch::try_new(
            schema(),
            vec![
                Arc::new(StringArray::from_iter_values(
                    puts.iter().map(|(k, _, _)| *k),
                )),
                Arc::new(StringArray::from_iter_values(
                    puts.iter().map(|(_, key, _)| key.as_str()),
                )),
                Arc::new(StringArray::from_iter_values(
                    puts.iter()
                        .map(|(_, _, j)| j.as_deref().unwrap_or_default()),
                )),
                Arc::new(StringArray::from_iter_values(puts.iter().map(
                    |(k, key_, j)| mac(&key, k, key_, j.as_deref().unwrap_or_default()),
                ))),
            ],
        )?;
        let reader = RecordBatchIterator::new(vec![Ok(batch)], schema());
        let mut m = tbl.merge_insert(&["kind", "key"]);
        m.when_matched_update_all(None)
            .when_not_matched_insert_all();
        m.execute(Box::new(reader)).await?;
    }
    Ok(tbl.version().await?)
}

/// Fold small writes together and drop versions older than a day.
async fn optimize(uri: String) -> Result<()> {
    use lancedb::table::OptimizeAction;
    let tbl = table(&uri).await?;
    tbl.optimize(OptimizeAction::Compact {
        options: Default::default(),
        remap_options: None,
    })
    .await?;
    tbl.optimize(OptimizeAction::Prune {
        older_than: Duration::try_days(1),
        delete_unverified: Some(false),
        error_if_tagged_old_versions: Some(false),
    })
    .await?;
    Ok(())
}

const REFRESH_SECONDS: i64 = 15;

pub struct Store {
    uri: String,
    /// Row keys from `COMINDLY_SECRET_KEY`, then `COMINDLY_SECRET_KEY_PREVIOUS`.
    keys: Vec<[u8; 32]>,
    /// Seals stored secrets; the previous one still opens them until they are sealed again.
    key: String,
    previous: Option<String>,
    admins: Vec<String>,
    admin_token: Option<String>,
    state: Mutex<State>,
    /// The table version `state` reflects, and when it was last compared with the table.
    seen: Mutex<(u64, chrono::DateTime<Utc>)>,
    /// Serialises writes, so a single-use invitation or link is used once.
    writing: Mutex<()>,
}

impl Store {
    /// The server's table under the index location (`<index>/_server`), created when missing.
    pub fn open(index_root: &str, config: &ServerConfig) -> Result<Store> {
        let mut store = Store::open_keyed(
            index_root,
            &config.secret_key,
            config.secret_key_previous.as_deref(),
        )?;
        store.admins = config.admin_emails.clone();
        store.admin_token = config.admin_token.clone();
        Ok(store)
    }

    fn open_keyed(index_root: &str, key: &str, previous: Option<&str>) -> Result<Store> {
        let store = Store::attach_keyed(index_root, key, previous)?;
        store.rotate_key()?;
        store.prune();
        let unreadable = store.unreadable_secrets();
        if !unreadable.is_empty() {
            crate::ui::warn(&format!(
                "{} stored secret(s) cannot be decrypted with COMINDLY_SECRET_KEY ({}); \
                 set COMINDLY_SECRET_KEY_PREVIOUS to the old key, or save them again",
                unreadable.len(),
                unreadable.join(", ")
            ));
        }
        Ok(store)
    }

    /// The table as it is, without the server's upkeep.
    fn attach_keyed(index_root: &str, key: &str, previous: Option<&str>) -> Result<Store> {
        let uri = format!("{}/_server", index_root.trim_end_matches('/'));
        let store = Store {
            uri,
            keys: [Some(key), previous]
                .into_iter()
                .flatten()
                .map(mac_key)
                .collect(),
            key: key.to_string(),
            previous: previous.map(str::to_string),
            admins: Vec::new(),
            admin_token: None,
            state: Mutex::new(State::default()),
            seen: Mutex::new((0, Utc::now())),
            writing: Mutex::new(()),
        };
        store.reload()?;
        Ok(store)
    }

    /// Re-read the table; rows without a valid mac (not written by this server) are ignored,
    /// and rows under the previous key are signed again with the current one.
    fn reload(&self) -> Result<()> {
        let (version, rows) = run(read_all(self.uri.clone()))?;
        let mut fresh = State::default();
        let (mut forged, mut resign) = (0, Vec::new());
        for [kind, key, json, m] in rows {
            let Some(i) = self
                .keys
                .iter()
                .position(|k| constant_eq(&mac(k, &kind, &key, &json), &m))
            else {
                forged += 1;
                continue;
            };
            if i > 0 {
                if let Some(k) = KINDS.iter().find(|k| **k == kind) {
                    resign.push((*k, key.clone(), Some(json.clone())));
                }
            }
            fresh.load(&kind, key, &json);
        }
        if forged > 0 {
            crate::ui::warn(&format!(
                "ignored {forged} server row(s) not signed with COMINDLY_SECRET_KEY"
            ));
        }
        let mut st = self.lock();
        fresh.failures = std::mem::take(&mut st.failures);
        *st = fresh;
        *self.seen.lock().unwrap_or_else(|e| e.into_inner()) = (version, Utc::now());
        drop(st);
        self.commit(resign)
    }

    /// The current state, re-read when another instance changed the table.
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        let stale = {
            let seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
            Utc::now() - seen.1 > Duration::seconds(REFRESH_SECONDS)
        };
        if stale {
            let uri = self.uri.clone();
            let version = run(async move { Ok(table(&uri).await?.version().await?) });
            let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
            seen.1 = Utc::now();
            let changed = version.is_ok_and(|v| v != seen.0);
            drop(seen);
            if changed {
                if let Err(e) = self.reload() {
                    crate::ui::warn(&format!("re-reading the server table: {e:#}"));
                }
            }
        }
        self.lock()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Write `changes` to the table, then to memory.
    fn commit(&self, changes: Vec<Change>) -> Result<()> {
        if changes.is_empty() {
            return Ok(());
        }
        let version = run(write(self.uri.clone(), self.keys[0], changes.clone()))
            .context("save to the server table")?;
        self.lock().apply(&changes);
        let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        if version == seen.0 + 1 || version == seen.0 + 2 {
            seen.0 = version;
        }
        Ok(())
    }

    /// Drop expired links, invitations, sessions and tokens and old failed attempts; tidy the table.
    fn prune(&self) {
        let t = now();
        let day = (Utc::now() - Duration::days(1)).to_rfc3339();
        let mut changes = Vec::new();
        {
            let mut st = self.state();
            changes.extend(
                st.links
                    .iter()
                    .filter(|(_, l)| l.expires < t)
                    .map(|(k, _)| del("link", k)),
            );
            changes.extend(
                st.invites
                    .iter()
                    .filter(|(_, i)| i.expires < t)
                    .map(|(k, _)| del("invite", k)),
            );
            changes.extend(
                st.sessions
                    .iter()
                    .filter(|(_, s)| s.expires < t)
                    .map(|(k, _)| del("session", k)),
            );
            changes.extend(
                st.devices
                    .iter()
                    .filter(|(_, d)| d.expires < t)
                    .map(|(k, _)| del("device", k)),
            );
            changes.extend(long_expired(&st.tokens, |_| true));
            st.failures.retain(|(_, at)| *at >= day);
        }
        if let Err(e) = self.commit(changes) {
            crate::ui::warn(&format!("pruning the server table: {e:#}"));
        }
        if let Err(e) = run(optimize(self.uri.clone())) {
            crate::ui::warn(&format!("optimizing the server table: {e:#}"));
        }
    }

    /// Names of stored secrets the current key cannot open.
    fn unreadable_secrets(&self) -> Vec<String> {
        self.state()
            .secrets
            .iter()
            .filter(|(n, s)| unseal(&self.key, n, s).is_none())
            .map(|(n, _)| n.clone())
            .collect()
    }

    /// Failed attempts under `key` in the last few minutes.
    fn failures(&self, key: &str) -> i64 {
        let since = (Utc::now() - Duration::minutes(LOCK_MINUTES)).to_rfc3339();
        self.lock()
            .failures
            .iter()
            .filter(|(k, at)| k == key && *at > since)
            .count() as i64
    }

    fn fail(&self, key: &str) {
        self.lock().failures.push((key.to_string(), now()));
    }

    fn forgive(&self, key: &str) {
        self.lock().failures.retain(|(k, _)| k != key);
    }

    pub fn audit(&self, actor: &str, action: &str, detail: &str) {
        let row =
            serde_json::json!({"at": now(), "actor": actor, "action": action, "detail": detail});
        let key = uuid::Uuid::new_v4().simple().to_string();
        if let Err(e) = run(write(
            self.uri.clone(),
            self.keys[0],
            vec![("audit", key, Some(row.to_string()))],
        )) {
            crate::ui::warn(&format!("audit log: {e:#}"));
        }
    }

    /// Audit rows this server signed, with the index of the key that signed each.
    fn audit_rows(&self) -> Result<Vec<(String, AuditEntry, String, usize)>> {
        let (_, rows) = run(read_rows(self.uri.clone(), "kind = 'audit'"))?;
        Ok(rows
            .into_iter()
            .filter_map(|[kind, key, json, m]| {
                let i = self
                    .keys
                    .iter()
                    .position(|k| constant_eq(&mac(k, &kind, &key, &json), &m))?;
                let v: serde_json::Value = serde_json::from_str(&json).ok()?;
                let s = |n: &str| v[n].as_str().unwrap_or_default().to_string();
                let entry = AuditEntry {
                    at: s("at"),
                    actor: s("actor"),
                    action: s("action"),
                    detail: s("detail"),
                };
                Some((key, entry, json, i))
            })
            .collect())
    }

    fn session_for(&self, email: &str) -> Result<(String, Who)> {
        let who = self.member(email)?.context("not a member")?;
        let token = new_token("ses_");
        let session = Session {
            email: who.email.clone(),
            created: now(),
            expires: (Utc::now() + Duration::days(SESSION_DAYS)).to_rfc3339(),
        };
        self.commit(vec![put("session", &hash(&token), &session)])?;
        Ok((token, who))
    }

    /// Re-seal secrets stored under `COMINDLY_SECRET_KEY_PREVIOUS` with the current key.
    fn rotate_key(&self) -> Result<()> {
        let (key, Some(old)) = (self.key.clone(), self.previous.clone()) else {
            return Ok(());
        };
        let rows: Vec<(String, Secret)> = self
            .state()
            .secrets
            .iter()
            .map(|(n, s)| (n.clone(), s.clone()))
            .collect();
        let mut changes = Vec::new();
        for (name, row) in rows {
            if unseal(&key, &name, &row).is_some() {
                continue;
            }
            let Some(plain) = unseal(&old, &name, &row) else {
                continue;
            };
            let mut again = sealed(&key, &name, &plain, &row.updated_by)?;
            again.updated = row.updated;
            changes.push(put("secret", &name, &again));
        }
        self.commit(changes)?;
        let audit: Vec<Change> = self
            .audit_rows()?
            .into_iter()
            .filter(|(.., i)| *i > 0)
            .map(|(key, _, json, _)| ("audit", key, Some(json)))
            .collect();
        if !audit.is_empty() {
            run(write(self.uri.clone(), self.keys[0], audit))?;
        }
        Ok(())
    }

    /// Only admins invite, change or remove people.
    fn may_manage(&self, by: &str) -> Result<()> {
        use crate::backends::AccountStore;
        anyhow::ensure!(
            self.member(by)?.is_some_and(|w| w.role == Role::Admin),
            "only admins can do this"
        );
        Ok(())
    }

    fn write_lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.writing.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Tokens of whoever `whose` accepts that expired more than `EXPIRED_TOKEN_DAYS` ago.
fn long_expired(tokens: &HashMap<String, Token>, whose: impl Fn(&str) -> bool) -> Vec<Change> {
    let gone = (Utc::now() - Duration::days(EXPIRED_TOKEN_DAYS)).to_rfc3339();
    tokens
        .iter()
        .filter(|(_, t)| whose(&t.email) && t.expired(&gone))
        .map(|(k, _)| del("token", k))
        .collect()
}

fn sealed(key: &str, name: &str, value: &str, by: &str) -> Result<Secret> {
    let (s, n) = seal(key, name, value)?;
    Ok(Secret {
        sealed: hex(&s),
        nonce: hex(&n),
        hint: hint(value),
        updated: now(),
        updated_by: by.to_string(),
    })
}

fn unseal(key: &str, name: &str, row: &Secret) -> Option<String> {
    open(key, name, &unhex(&row.sealed)?, &unhex(&row.nonce)?)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Equal without revealing where or how long the values differ.
fn constant_eq(a: &str, b: &str) -> bool {
    let (a, b) = (blake3::hash(a.as_bytes()), blake3::hash(b.as_bytes()));
    a.as_bytes()
        .iter()
        .zip(b.as_bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

fn valid_email(email: &str) -> bool {
    email.contains('@') && !email.contains(|c: char| c.is_whitespace() || c == '|')
}

/// Secret names for a git host's token and for the LLM key.
pub fn git_secret(host: &str) -> String {
    format!("git:{}", host.to_lowercase())
}

pub const LLM_SECRET: &str = "llm_api_key";

impl crate::backends::AccountStore for Store {
    /// The member `email`, with deployment admins always present and admin.
    fn member(&self, email: &str) -> Result<Option<Who>> {
        let email = normalize(email);
        if self.admins.contains(&email) {
            return Ok(Some(Who {
                email,
                role: Role::Admin,
            }));
        }
        let role = self.state().users.get(&email).map(|u| u.role);
        Ok(role.map(|role| Who { email, role }))
    }

    fn members(&self) -> Result<Vec<Member>> {
        let admins = self.admins.clone();
        let st = self.state();
        let mut out: Vec<Member> = st
            .users
            .iter()
            .map(|(email, u)| Member {
                email: email.clone(),
                role: u.role,
                fixed: false,
                created: u.created.clone(),
                last_seen: u.last_seen.clone(),
            })
            .collect();
        for m in &mut out {
            if admins.contains(&m.email) {
                m.role = Role::Admin;
                m.fixed = true;
            }
        }
        for a in admins {
            if !out.iter().any(|m| m.email == a) {
                out.push(Member {
                    last_seen: st.seen.get(&a).cloned(),
                    email: a,
                    role: Role::Admin,
                    fixed: true,
                    created: String::new(),
                });
            }
        }
        Ok(out)
    }

    fn set_role(&self, by: &str, email: &str, role: Role) -> Result<()> {
        let email = normalize(email);
        if self.admins.contains(&email) {
            bail!("{email} is an admin by the deployment's settings");
        }
        let _w = self.write_lock();
        let user = self.state().users.get(&email).cloned();
        let mut user = user.with_context(|| format!("{email} is not a member"))?;
        self.may_manage(by)?;
        user.role = role;
        self.commit(vec![put("user", &email, &user)])?;
        self.audit(by, "role", &format!("{email} {}", role.as_str()));
        Ok(())
    }

    /// Remove a member with their sessions, tokens and invitations.
    fn remove(&self, by: &str, email: &str) -> Result<()> {
        let email = normalize(email);
        if self.admins.contains(&email) {
            bail!("{email} is an admin by the deployment's settings");
        }
        self.may_manage(by)?;
        let _w = self.write_lock();
        let mut changes = vec![del("user", &email), del("code", &email)];
        {
            let st = self.state();
            changes.extend(
                st.sessions
                    .iter()
                    .filter(|(_, s)| s.email == email)
                    .map(|(k, _)| del("session", k)),
            );
            changes.extend(
                st.tokens
                    .iter()
                    .filter(|(_, t)| t.email == email)
                    .map(|(k, _)| del("token", k)),
            );
            changes.extend(
                st.invites
                    .iter()
                    .filter(|(_, i)| i.email == email)
                    .map(|(k, _)| del("invite", k)),
            );
            changes.extend(
                st.links
                    .iter()
                    .filter(|(_, l)| l.email == email)
                    .map(|(k, _)| del("link", k)),
            );
        }
        self.commit(changes)?;
        let prefix = format!("code|{email}|");
        self.lock()
            .failures
            .retain(|(k, _)| !k.starts_with(&prefix));
        self.audit(by, "remove", &email);
        Ok(())
    }

    /// An invitation for `email` only: a link token, or with `code` a code to hand over.
    fn invite(&self, by: &str, email: &str, role: Role, code: bool) -> Result<String> {
        let email = normalize(email);
        self.may_manage(by)?;
        anyhow::ensure!(
            email.contains('@') && !email.contains(char::is_whitespace),
            "that is not an email address"
        );
        let (token, h) = if code {
            let c = new_code();
            let h = code_hash(&c);
            (c, h)
        } else {
            let t = new_token("inv_");
            let h = hash(&t);
            (t, h)
        };
        let _w = self.write_lock();
        let mut changes: Vec<Change> = self
            .state()
            .invites
            .iter()
            .filter(|(_, i)| i.email == email)
            .map(|(k, _)| del("invite", k))
            .collect();
        changes.push(put(
            "invite",
            &h,
            &Invitation {
                email: email.clone(),
                role,
                created_by: by.to_string(),
                created: now(),
                expires: (Utc::now() + Duration::days(INVITE_DAYS)).to_rfc3339(),
            },
        ));
        self.commit(changes)?;
        self.audit(by, "invite", &format!("{email} {}", role.as_str()));
        Ok(token)
    }

    fn invites(&self) -> Result<Vec<Invite>> {
        let t = now();
        let mut out: Vec<(String, Invite)> = self
            .state()
            .invites
            .values()
            .filter(|i| i.expires > t)
            .map(|i| {
                (
                    i.created.clone(),
                    Invite {
                        email: i.email.clone(),
                        role: i.role,
                        created_by: i.created_by.clone(),
                        expires: i.expires.clone(),
                    },
                )
            })
            .collect();
        out.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(out.into_iter().map(|(_, i)| i).collect())
    }

    fn revoke_invite(&self, by: &str, email: &str) -> Result<()> {
        let email = normalize(email);
        self.may_manage(by)?;
        let _w = self.write_lock();
        let changes = self
            .state()
            .invites
            .iter()
            .filter(|(_, i)| i.email == email)
            .map(|(k, _)| del("invite", k))
            .collect();
        self.commit(changes)?;
        self.audit(by, "revoke-invite", &email);
        Ok(())
    }

    /// A sign-in link token for a member, or `None` when `email` is not one.
    fn sign_in_link(&self, email: &str) -> Result<Option<String>> {
        let Some(who) = self.member(email)? else {
            return Ok(None);
        };
        let hour = (Utc::now() - Duration::hours(1)).to_rfc3339();
        let recent = self
            .state()
            .links
            .values()
            .filter(|l| l.email == who.email && l.created > hour)
            .count() as i64;
        // Answered like any other address, so the limit does not reveal who is a member.
        if recent >= MAX_LINKS_PER_HOUR {
            crate::ui::warn(&format!(
                "a member asked for more than {MAX_LINKS_PER_HOUR} sign-in links in an hour"
            ));
            return Ok(None);
        }
        let token = new_token("lnk_");
        let link = Link {
            email: who.email,
            created: now(),
            expires: (Utc::now() + Duration::minutes(LINK_MINUTES)).to_rfc3339(),
        };
        self.commit(vec![put("link", &hash(&token), &link)])?;
        Ok(Some(token))
    }

    /// Exchange a sign-in link, an invitation or the admin token for a new session; `client`
    /// limits how often one place may guess.
    fn redeem(&self, token: &str, client: &str) -> Result<(String, Who)> {
        let token = token.trim();
        let from = format!("redeem|{client}");
        anyhow::ensure!(
            self.failures(&from) < MAX_REDEEM_FAILED,
            "Too many attempts from here; try again in {LOCK_MINUTES} minutes"
        );
        if let Some(admin) = &self.admin_token {
            if constant_eq(token, admin) {
                let email = self
                    .admins
                    .first()
                    .cloned()
                    .context("set COMINDLY_ADMIN_EMAILS to sign in with the admin token")?;
                self.audit(&email, "sign-in", "admin token");
                return self.session_for(&email);
            }
        }
        let h = hash(token);
        let t = now();
        let _w = self.write_lock();
        let link = self
            .state()
            .links
            .get(&h)
            .filter(|l| l.expires > t)
            .map(|l| l.email.clone());
        if let Some(email) = link {
            self.commit(vec![del("link", &h)])?;
            drop(_w);
            self.audit(&email, "sign-in", "email link");
            return self.session_for(&email);
        }
        let invite = self
            .state()
            .invites
            .get(&h)
            .filter(|i| i.expires > t)
            .cloned();
        if let Some(inv) = invite {
            let mut changes = vec![del("invite", &h)];
            if !self.state().users.contains_key(&inv.email) {
                changes.push(put(
                    "user",
                    &inv.email,
                    &User {
                        role: inv.role,
                        created: t,
                        last_seen: None,
                    },
                ));
            }
            self.commit(changes)?;
            drop(_w);
            self.audit(&inv.email, "join", inv.role.as_str());
            return self.session_for(&inv.email);
        }
        drop(_w);
        self.fail(&from);
        bail!("this link has expired or was already used; ask for a new one")
    }

    /// Sign in with an email address and a code: an invitation code, which also issues the
    /// person's own sign-in code (returned once), or that sign-in code.
    /// `client` identifies where the attempt comes from: wrong codes pause that address for that
    /// client only, so nobody can lock another person out.
    fn sign_in_code(
        &self,
        email: &str,
        code: &str,
        client: &str,
    ) -> Result<(String, Who, Option<String>)> {
        let email = normalize(email);
        anyhow::ensure!(valid_email(&email), "Wrong email or code");
        let from = format!("code|{email}|{client}");
        anyhow::ensure!(
            self.failures(&from) < MAX_FAILED,
            "Too many wrong codes for this address; try again in {LOCK_MINUTES} minutes"
        );
        let h = code_hash(code);
        let t = now();
        let _w = self.write_lock();
        let invite = self
            .state()
            .invites
            .get(&h)
            .filter(|i| i.email == email && i.expires > t)
            .cloned();
        if let Some(inv) = invite {
            let personal = new_code();
            let mut changes = vec![
                del("invite", &h),
                put(
                    "code",
                    &email,
                    &Code {
                        hash: code_hash(&personal),
                        created: t.clone(),
                    },
                ),
            ];
            if !self.state().users.contains_key(&email) {
                changes.push(put(
                    "user",
                    &email,
                    &User {
                        role: inv.role,
                        created: t,
                        last_seen: None,
                    },
                ));
            }
            self.commit(changes)?;
            drop(_w);
            self.forgive(&from);
            self.audit(&email, "join", inv.role.as_str());
            let (session, who) = self.session_for(&email)?;
            return Ok((session, who, Some(personal)));
        }
        drop(_w);
        let known = self
            .state()
            .codes
            .get(&email)
            .is_some_and(|c| constant_eq(&c.hash, &h));
        if known && self.member(&email)?.is_some() {
            self.forgive(&from);
            self.audit(&email, "sign-in", "code");
            let (session, who) = self.session_for(&email)?;
            return Ok((session, who, None));
        }
        self.fail(&from);
        bail!("Wrong email or code")
    }

    /// Give `email` a new sign-in code; their sessions end, except `keep` (the caller's own).
    fn reset_code(&self, by: &str, email: &str, keep: Option<&str>) -> Result<String> {
        let email = normalize(email);
        anyhow::ensure!(self.member(&email)?.is_some(), "{email} is not a member");
        if by != email {
            anyhow::ensure!(
                !self.admins.contains(&email),
                "{email} is an admin by the deployment's settings and resets their own code"
            );
            self.may_manage(by)?;
        }
        let code = new_code();
        let keep = keep.map(hash).unwrap_or_default();
        let mut changes = vec![put(
            "code",
            &email,
            &Code {
                hash: code_hash(&code),
                created: now(),
            },
        )];
        changes.extend(
            self.state()
                .sessions
                .iter()
                .filter(|(k, s)| s.email == email && **k != keep)
                .map(|(k, _)| del("session", k)),
        );
        self.commit(changes)?;
        let prefix = format!("code|{email}|");
        self.lock()
            .failures
            .retain(|(k, _)| !k.starts_with(&prefix));
        self.audit(by, "reset-code", &email);
        Ok(code)
    }

    fn sign_out(&self, session: &str, everywhere: bool) -> Result<()> {
        let h = hash(session);
        let (changes, email) = {
            let st = self.state();
            let email = st.sessions.get(&h).map(|s| s.email.clone());
            let changes = match (everywhere, st.sessions.get(&h)) {
                (true, Some(s)) => st
                    .sessions
                    .iter()
                    .filter(|(_, x)| x.email == s.email)
                    .map(|(k, _)| del("session", k))
                    .collect(),
                _ => vec![del("session", &h)],
            };
            (changes, email)
        };
        self.commit(changes)?;
        if let Some(email) = email {
            self.audit(
                &email,
                "sign-out",
                if everywhere { "everywhere" } else { "" },
            );
        }
        Ok(())
    }

    /// A personal access token for agents; its value is returned once.
    fn create_token(&self, who: &Who, name: &str, expires_in_days: Option<u32>) -> Result<String> {
        let name = name.trim();
        anyhow::ensure!(!name.is_empty(), "give the token a name");
        if let Some(d) = expires_in_days {
            anyhow::ensure!((1..=3650).contains(&d), "a token lasts 1 to 3650 days");
        }
        let token = new_token("cly_");
        let id = uuid::Uuid::new_v4().simple().to_string();
        let row = Token {
            hash: hash(&token),
            email: who.email.clone(),
            name: name.to_string(),
            created: now(),
            last_used: None,
            expires: expires_in_days.map(|d| (Utc::now() + Duration::days(d.into())).to_rfc3339()),
        };
        let mut changes = long_expired(&self.state().tokens, |e| e == who.email);
        changes.push(put("token", &id, &row));
        self.commit(changes)?;
        let lasts = match expires_in_days {
            Some(d) => format!("{name} ({d} days)"),
            None => format!("{name} (no expiry)"),
        };
        self.audit(&who.email, "token", &lasts);
        Ok(token)
    }

    fn tokens(&self, who: &Who) -> Result<Vec<AccessToken>> {
        let t = now();
        let mut out: Vec<AccessToken> = self
            .state()
            .tokens
            .iter()
            .filter(|(_, tok)| tok.email == who.email)
            .map(|(id, tok)| AccessToken {
                id: id.clone(),
                name: tok.name.clone(),
                created: tok.created.clone(),
                last_used: tok.last_used.clone(),
                expires: tok.expires.clone(),
                expired: tok.expired(&t),
            })
            .collect();
        out.sort_by(|a, b| b.created.cmp(&a.created));
        Ok(out)
    }

    fn revoke_token(&self, who: &Who, id: &str) -> Result<()> {
        let mine = self
            .state()
            .tokens
            .get(id)
            .filter(|t| t.email == who.email)
            .map(|t| t.name.clone());
        if let Some(name) = mine {
            self.commit(vec![del("token", id)])?;
            self.audit(&who.email, "revoke-token", &name);
        }
        Ok(())
    }

    fn revoke_own_token(&self, token: &str) -> Result<bool> {
        let h = hash(token);
        let found = self
            .state()
            .tokens
            .iter()
            .find(|(_, t)| t.hash == h)
            .map(|(id, t)| (id.clone(), t.email.clone()));
        let Some((id, email)) = found else {
            return Ok(false);
        };
        self.commit(vec![del("token", &id)])?;
        self.audit(&email, "revoke-token", &id);
        Ok(true)
    }
}

impl crate::backends::AuditLog for Store {
    fn audit_entries(&self, query: &AuditQuery) -> Result<AuditPage> {
        let limit = query.limit.unwrap_or(AUDIT_PAGE).clamp(1, AUDIT_PAGE_MAX);
        let before = query.before.as_deref().and_then(|b| b.split_once('|'));
        let actor = query
            .actor
            .as_deref()
            .map(normalize)
            .filter(|a| !a.is_empty());
        let action = query
            .action
            .as_deref()
            .map(str::trim)
            .filter(|a| !a.is_empty());
        let mut rows: Vec<(String, AuditEntry)> = self
            .audit_rows()?
            .into_iter()
            .map(|(key, e, ..)| (key, e))
            .filter(|(key, e)| {
                before.is_none_or(|(at, k)| (e.at.as_str(), key.as_str()) < (at, k))
                    && actor.as_ref().is_none_or(|a| normalize(&e.actor) == *a)
                    && action.is_none_or(|a| e.action == a)
            })
            .collect();
        rows.sort_unstable_by(|a, b| (&b.1.at, &b.0).cmp(&(&a.1.at, &a.0)));
        let next = (rows.len() > limit).then(|| {
            let (key, e) = &rows[limit - 1];
            format!("{}|{key}", e.at)
        });
        rows.truncate(limit);
        Ok(AuditPage {
            entries: rows.into_iter().map(|(_, e)| e).collect(),
            next,
        })
    }
}

impl crate::backends::DeviceSignIn for Store {
    fn device_start(&self, client: &str, from: &str) -> Result<(String, String)> {
        let key = format!("device|{from}");
        anyhow::ensure!(
            self.failures(&key) < MAX_DEVICE_STARTS,
            "Too many sign-ins started from here; try again in {LOCK_MINUTES} minutes"
        );
        self.fail(&key);
        let client: String = client.trim().chars().take(80).collect();
        anyhow::ensure!(!client.is_empty(), "name the client signing in");
        let device_code = new_token("dev_");
        let user_code = new_code()[..9].to_string();
        let row = Device {
            user_code: code_hash(&user_code),
            client,
            created: now(),
            expires: (Utc::now() + Duration::minutes(DEVICE_MINUTES)).to_rfc3339(),
            approved_by: None,
            denied: false,
            token: None,
        };
        self.commit(vec![put("device", &hash(&device_code), &row)])?;
        Ok((device_code, user_code))
    }

    fn device_info(&self, user_code: &str) -> Option<DeviceRequest> {
        let (h, t) = (code_hash(user_code), now());
        self.state()
            .devices
            .values()
            .find(|d| d.user_code == h && d.expires > t && d.approved_by.is_none() && !d.denied)
            .map(|d| DeviceRequest {
                client: d.client.clone(),
                expires: d.expires.clone(),
            })
    }

    fn device_approve(&self, who: &Who, user_code: &str, approve: bool) -> Result<()> {
        let key = format!("approve|{}", who.email);
        anyhow::ensure!(
            self.failures(&key) < MAX_FAILED,
            "Too many wrong codes; try again in {LOCK_MINUTES} minutes"
        );
        let (h, t) = (code_hash(user_code), now());
        let _w = self.write_lock();
        let found = self
            .state()
            .devices
            .iter()
            .find(|(_, d)| {
                d.user_code == h && d.expires > t && d.approved_by.is_none() && !d.denied
            })
            .map(|(k, d)| (k.clone(), d.clone()));
        let Some((id, mut row)) = found else {
            self.fail(&key);
            bail!("That code is not waiting for approval; start the sign-in again");
        };
        if approve {
            let token = self.create_token(who, &row.client, Some(TOKEN_DAYS))?;
            let (sealed, nonce) = seal(&self.key, &format!("device:{id}"), &token)?;
            row.token = Some((hex(&sealed), hex(&nonce)));
            row.approved_by = Some(who.email.clone());
        } else {
            row.denied = true;
        }
        self.commit(vec![put("device", &id, &row)])?;
        self.audit(
            &who.email,
            if approve {
                "device-approved"
            } else {
                "device-denied"
            },
            &row.client,
        );
        Ok(())
    }

    fn device_poll(&self, device_code: &str) -> Result<DevicePoll> {
        let id = hash(device_code.trim());
        let _w = self.write_lock();
        let Some(row) = self.state().devices.get(&id).cloned() else {
            return Ok(DevicePoll::Expired);
        };
        if row.denied {
            self.commit(vec![del("device", &id)])?;
            return Ok(DevicePoll::Denied);
        }
        if let (Some(email), Some((sealed, nonce))) = (&row.approved_by, &row.token) {
            let token = unhex(sealed)
                .zip(unhex(nonce))
                .and_then(|(s, n)| open(&self.key, &format!("device:{id}"), &s, &n))
                .context("the issued token cannot be read; start the sign-in again")?;
            self.commit(vec![del("device", &id)])?;
            let h = hash(&token);
            let expires = self
                .state()
                .tokens
                .values()
                .find(|t| t.hash == h)
                .and_then(|t| t.expires.clone());
            return Ok(DevicePoll::Approved {
                token,
                email: email.clone(),
                expires,
            });
        }
        Ok(if row.expires < now() {
            DevicePoll::Expired
        } else {
            DevicePoll::Pending
        })
    }
}

impl crate::backends::Authenticator for Store {
    /// The member behind a session cookie or, failing that, a bearer token, and which it was.
    fn authenticate(
        &self,
        session: Option<&str>,
        bearer: Option<&str>,
    ) -> Result<Option<(Who, Via)>> {
        let t = now();
        let mut found: Option<(String, Via)> = None;
        let mut used: Option<(String, Token)> = None;
        if let Some(s) = session {
            found = self
                .state()
                .sessions
                .get(&hash(s))
                .filter(|s| s.expires > t)
                .map(|s| (s.email.clone(), Via::Session));
        }
        if found.is_none() {
            if let Some(b) = bearer {
                if self
                    .admin_token
                    .as_deref()
                    .is_some_and(|a| constant_eq(b, a))
                {
                    found = self.admins.first().map(|e| (e.clone(), Via::AdminToken));
                } else {
                    let h = hash(b);
                    let st = self.state();
                    let live = st
                        .tokens
                        .iter()
                        .find(|(_, k)| k.hash == h && !k.expired(&t));
                    if let Some((id, tok)) = live {
                        found = Some((tok.email.clone(), Via::Token));
                        used = Some((id.clone(), tok.clone()));
                    }
                }
            }
        }
        let Some((email, via)) = found else {
            return Ok(None);
        };
        let who = self.member(&email)?;
        if who.is_some() {
            // Recorded at most every few minutes, so requests do not each write to storage.
            let since = (Utc::now() - Duration::minutes(5)).to_rfc3339();
            let mut changes = Vec::new();
            if let Some((id, mut tok)) =
                used.filter(|(_, t)| t.last_used.as_ref().is_none_or(|u| *u < since))
            {
                tok.last_used = Some(t.clone());
                changes.push(put("token", &id, &tok));
            }
            let (user, seen) = {
                let st = self.state();
                (st.users.get(&email).cloned(), st.seen.get(&email).cloned())
            };
            if let Some(mut u) = user {
                if u.last_seen.as_ref().is_none_or(|s| *s < since) {
                    u.last_seen = Some(t);
                    changes.push(put("user", &email, &u));
                }
            } else if seen.is_none_or(|s| s < since) {
                changes.push(put("seen", &email, &t));
            }
            if let Err(e) = self.commit(changes) {
                crate::ui::warn(&format!("recording last use: {e:#}"));
            }
        }
        Ok(who.map(|w| (w, via)))
    }
}

impl crate::backends::SecretStore for Store {
    /// Store `value` under `name` (an empty value removes it), sealed with `COMINDLY_SECRET_KEY`.
    fn set_secret(&self, by: &str, name: &str, value: &str) -> Result<()> {
        let value = value.trim();
        if value.is_empty() {
            self.commit(vec![del("secret", name)])?;
            self.audit(by, "secret-removed", name);
            return Ok(());
        }
        let row = sealed(&self.key, name, value, by)?;
        self.commit(vec![put("secret", name, &row)])?;
        self.audit(by, "secret-set", name);
        Ok(())
    }

    fn secret(&self, name: &str) -> Option<String> {
        let row = self.state().secrets.get(name).cloned()?;
        unseal(&self.key, name, &row)
    }

    fn secret_info(&self, name: &str) -> Option<SecretInfo> {
        self.state().secrets.get(name).map(|s| SecretInfo {
            name: name.to_string(),
            source: "stored",
            hint: Some(s.hint.clone()),
            updated: Some(s.updated.clone()),
            updated_by: Some(s.updated_by.clone()),
        })
    }

    /// Names of stored secrets.
    fn secret_names(&self) -> Vec<String> {
        self.state().secrets.keys().cloned().collect()
    }
}

impl crate::backends::SettingsStore for Store {
    fn repos(&self) -> Vec<crate::config::RepoEntry> {
        self.state()
            .config
            .get(REPOS)
            .and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default()
    }

    fn set_repos(&self, by: &str, repos: &[crate::config::RepoEntry]) -> Result<()> {
        self.commit(vec![put("config", REPOS, &serde_json::to_string(repos)?)])?;
        self.audit(by, "repos", &repos.len().to_string());
        Ok(())
    }

    fn legacy_config(&self) -> Option<String> {
        self.state().config.get(CONFIG).cloned()
    }

    fn drop_legacy_config(&self) -> Result<()> {
        self.commit(vec![del("config", CONFIG)])
    }
}

impl crate::jobs::JobStore for Store {
    fn save(&self, job: &crate::jobs::JobRecord, events: &[crate::events::Event]) -> Result<()> {
        let mut changes = vec![
            put("job", &job.id, job),
            put("job-events", &job.id, &events),
        ];
        {
            let st = self.state();
            let mut ids: Vec<(&String, &String)> = st
                .jobs
                .iter()
                .filter(|(id, _)| **id != job.id)
                .map(|(id, j)| (&j.created, id))
                .collect();
            ids.sort_unstable_by(|a, b| b.cmp(a));
            for (_, id) in ids.into_iter().skip(crate::jobs::MAX_JOBS - 1) {
                changes.extend([del("job", id), del("job-events", id)]);
            }
        }
        let _w = self.write_lock();
        self.commit(changes)
    }

    fn get(&self, id: &str) -> Option<(crate::jobs::JobRecord, Vec<crate::events::Event>)> {
        let st = self.state();
        let job = st.jobs.get(id)?.clone();
        Some((job, st.job_events.get(id).cloned().unwrap_or_default()))
    }

    fn list(&self, limit: usize) -> Vec<crate::jobs::JobRecord> {
        let mut jobs: Vec<_> = self.state().jobs.values().cloned().collect();
        jobs.sort_unstable_by(|a, b| b.created.cmp(&a.created));
        jobs.truncate(limit);
        jobs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> ((), Store) {
        let dir = std::env::temp_dir().join(format!("comindly-server-{}", uuid::Uuid::new_v4()));
        ((), {
            let mut s = Store::open_keyed(dir.to_str().unwrap(), "test key", None).unwrap();
            s.admins = vec!["a".into(), "a@x.org".into(), "admin@x.org".into()];
            s
        })
    }

    #[test]
    fn invitations_are_single_use_and_tied_to_their_email() {
        let (_d, s) = temp_store();
        let token = s
            .invite("admin@x.org", "Dev@X.org ", Role::Member, false)
            .unwrap();
        let (session, who) = s.redeem(&token, "10.0.0.1").unwrap();
        assert_eq!(who.email, "dev@x.org");
        assert_eq!(who.role, Role::Member);
        assert!(s.redeem(&token, "10.0.0.1").is_err());
        let (back, _) = s.authenticate(Some(&session), None).unwrap().unwrap();
        assert_eq!(back.email, "dev@x.org");
        s.sign_out(&session, false).unwrap();
        assert!(s.authenticate(Some(&session), None).unwrap().is_none());
    }

    #[test]
    fn a_command_line_sign_in_is_approved_once_and_collected_once() {
        use crate::backends::DeviceSignIn;
        let (_d, s) = temp_store();
        let invite = s.invite("a@x.org", "m@x.org", Role::Member, false).unwrap();
        let (_, who) = s.redeem(&invite, "1.2.3.4").unwrap();
        let (device, user) = s.device_start("comindly CLI on laptop", "1.2.3.4").unwrap();
        assert_eq!(s.device_poll(&device).unwrap(), DevicePoll::Pending);
        assert_eq!(
            s.device_info(&user.to_lowercase()).unwrap().client,
            "comindly CLI on laptop"
        );
        assert!(s.device_approve(&who, "AAAA-AAAA", true).is_err());
        s.device_approve(&who, &user, true).unwrap();
        assert!(s.device_info(&user).is_none(), "approved once");
        let DevicePoll::Approved { token, email, .. } = s.device_poll(&device).unwrap() else {
            panic!("not approved");
        };
        assert_eq!(email, "m@x.org");
        assert!(s.authenticate(None, Some(&token)).unwrap().is_some());
        assert_eq!(
            s.device_poll(&device).unwrap(),
            DevicePoll::Expired,
            "collected once"
        );

        let (device, user) = s.device_start("other", "1.2.3.4").unwrap();
        s.device_approve(&who, &user, false).unwrap();
        assert_eq!(s.device_poll(&device).unwrap(), DevicePoll::Denied);
    }

    #[test]
    fn only_admins_manage_people() {
        let (_d, s) = temp_store();
        let join = |email: &str, role| {
            let inv = s.invite("a", email, role, false).unwrap();
            s.redeem(&inv, "1.2.3.4").unwrap().1
        };
        let admin = join("adm@x.org", Role::Admin);
        join("mem@x.org", Role::Member);
        join("view@x.org", Role::Viewer);
        assert!(s
            .invite(&admin.email, "x@x.org", Role::Admin, false)
            .is_ok());
        assert!(s.set_role(&admin.email, "mem@x.org", Role::Viewer).is_ok());
        assert!(s.set_role("mem@x.org", "view@x.org", Role::Member).is_err());
        assert!(s
            .invite("view@x.org", "y@x.org", Role::Viewer, false)
            .is_err());
        assert!(s.remove("mem@x.org", "adm@x.org").is_err());
        assert!(
            s.remove(&admin.email, "a").is_err(),
            "deployment admins stay"
        );
        assert!(s.remove(&admin.email, "view@x.org").is_ok());
    }

    #[test]
    fn sign_in_links_only_for_members_and_once() {
        let (_d, s) = temp_store();
        assert!(s.sign_in_link("stranger@x.org").unwrap().is_none());
        let inv = s.invite("a", "m@x.org", Role::Member, false).unwrap();
        s.redeem(&inv, "10.0.0.1").unwrap();
        let link = s.sign_in_link("m@x.org").unwrap().unwrap();
        assert!(s.redeem(&link, "10.0.0.1").is_ok());
        assert!(s.redeem(&link, "10.0.0.1").is_err());
    }

    #[test]
    fn removing_a_member_ends_their_tokens() {
        let (_d, s) = temp_store();
        let inv = s.invite("a", "m@x.org", Role::Member, false).unwrap();
        let (_, who) = s.redeem(&inv, "10.0.0.1").unwrap();
        let token = s.create_token(&who, "laptop", None).unwrap();
        assert!(s.authenticate(None, Some(&token)).unwrap().is_some());
        s.remove("a", "m@x.org").unwrap();
        assert!(s.authenticate(None, Some(&token)).unwrap().is_none());
    }

    #[test]
    fn a_token_revokes_itself_and_no_other() {
        let (_d, s) = temp_store();
        let inv = s.invite("a", "m@x.org", Role::Member, false).unwrap();
        let (_, who) = s.redeem(&inv, "10.0.0.1").unwrap();
        let laptop = s.create_token(&who, "laptop").unwrap();
        let ci = s.create_token(&who, "ci").unwrap();
        assert!(s.revoke_own_token(&laptop).unwrap());
        assert!(s.authenticate(None, Some(&laptop)).unwrap().is_none());
        assert!(s.authenticate(None, Some(&ci)).unwrap().is_some());
        assert!(!s.revoke_own_token(&laptop).unwrap(), "already gone");
        assert!(!s.revoke_own_token("cly_unknown").unwrap());
    }

    #[test]
    fn invitation_codes_need_their_own_address_and_give_a_sign_in_code() {
        let (_d, s) = temp_store();
        let invite = s.invite("a", "m@x.org", Role::Member, true).unwrap();
        assert!(s.sign_in_code("other@x.org", &invite, "10.0.0.1").is_err());
        let typed = invite.to_lowercase().replace('-', " ");
        let (_, who, personal) = s.sign_in_code("M@x.org", &typed, "10.0.0.1").unwrap();
        assert_eq!(who.email, "m@x.org");
        let personal = personal.unwrap();
        assert!(s.sign_in_code("m@x.org", &invite, "10.0.0.1").is_err());
        let (_, _, again) = s.sign_in_code("m@x.org", &personal, "10.0.0.1").unwrap();
        assert!(again.is_none());
    }

    #[test]
    fn a_reset_code_replaces_the_old_one_and_ends_other_sessions() {
        let (_d, s) = temp_store();
        let invite = s.invite("a", "m@x.org", Role::Member, true).unwrap();
        let (mine, _, old) = s.sign_in_code("m@x.org", &invite, "10.0.0.1").unwrap();
        let (elsewhere, _, _) = s
            .sign_in_code("m@x.org", old.as_ref().unwrap(), "10.0.0.1")
            .unwrap();
        let new = s.reset_code("m@x.org", "m@x.org", Some(&mine)).unwrap();
        assert!(s.authenticate(Some(&mine), None).unwrap().is_some());
        assert!(s.authenticate(Some(&elsewhere), None).unwrap().is_none());
        assert!(s
            .sign_in_code("m@x.org", old.as_ref().unwrap(), "10.0.0.1")
            .is_err());
        assert!(s.sign_in_code("m@x.org", &new, "10.0.0.1").is_ok());
    }

    #[test]
    fn wrong_codes_lock_the_address_for_that_client() {
        let (_d, s) = temp_store();
        let invite = s.invite("a", "m@x.org", Role::Member, true).unwrap();
        for _ in 0..MAX_FAILED {
            assert!(s.sign_in_code("m@x.org", "AAAA-AAAA", "10.0.0.1").is_err());
        }
        let e = s
            .sign_in_code("m@x.org", &invite, "10.0.0.1")
            .unwrap_err()
            .to_string();
        assert!(e.contains("Too many"), "{e}");
        assert!(s.sign_in_code("m@x.org", &invite, "10.0.0.2").is_ok());
    }

    #[test]
    fn accounts_survive_a_restart_and_rows_without_the_key_are_ignored() {
        let dir = std::env::temp_dir().join(format!("comindly-server-{}", uuid::Uuid::new_v4()));
        let root = dir.to_str().unwrap();
        let mut s = Store::open_keyed(root, "key one", None).unwrap();
        s.admins = vec!["admin@x.org".into()];
        let code = s
            .invite("admin@x.org", "dev@x.org", Role::Member, true)
            .unwrap();
        let (_, _, personal) = s.sign_in_code("dev@x.org", &code, "1.2.3.4").unwrap();
        let token = s
            .create_token(&s.member("dev@x.org").unwrap().unwrap(), "agent", None)
            .unwrap();
        drop(s);

        let again = Store::open_keyed(root, "key one", None).unwrap();
        assert!(again
            .sign_in_code("dev@x.org", &personal.unwrap(), "1.2.3.4")
            .is_ok());
        assert!(again.authenticate(None, Some(&token)).unwrap().is_some());

        let forged = vec![put(
            "user",
            "eve@x.org",
            &User {
                role: Role::Admin,
                created: now(),
                last_seen: None,
            },
        )];
        run(write(
            again.uri.clone(),
            mac_key("someone else's key"),
            forged,
        ))
        .unwrap();
        again.reload().unwrap();
        assert!(again.member("eve@x.org").unwrap().is_none());

        let rotated = Store::open_keyed(root, "key two", Some("key one")).unwrap();
        assert!(rotated.authenticate(None, Some(&token)).unwrap().is_some());
        let only_new = Store::open_keyed(root, "key two", None).unwrap();
        assert!(only_new.authenticate(None, Some(&token)).unwrap().is_some());
    }

    /// Move token `id`'s expiry to `days` from now.
    fn expire(s: &Store, id: &str, days: i64) {
        let mut tok = s.state().tokens.get(id).cloned().unwrap();
        tok.expires = Some((Utc::now() + Duration::days(days)).to_rfc3339());
        s.commit(vec![put("token", id, &tok)]).unwrap();
    }

    #[test]
    fn expired_tokens_stop_working_stay_listed_then_go() {
        let (_d, s) = temp_store();
        let who = s.member("a@x.org").unwrap().unwrap();
        let token = s.create_token(&who, "agent", Some(30)).unwrap();
        let forever = s.create_token(&who, "forever", None).unwrap();
        assert!(s.create_token(&who, "zero", Some(0)).is_err());
        let list = s.tokens(&who).unwrap();
        let agent = list.iter().find(|t| t.name == "agent").unwrap();
        assert!(agent.expires.is_some() && !agent.expired);
        assert!(list
            .iter()
            .find(|t| t.name == "forever")
            .unwrap()
            .expires
            .is_none());
        let id = agent.id.clone();

        expire(&s, &id, -1);
        assert!(s.authenticate(None, Some(&token)).unwrap().is_none());
        assert!(s.authenticate(None, Some(&forever)).unwrap().is_some());
        assert!(s
            .tokens(&who)
            .unwrap()
            .iter()
            .any(|t| t.id == id && t.expired));
        s.prune();
        assert!(
            s.tokens(&who).unwrap().iter().any(|t| t.id == id),
            "kept a while"
        );

        expire(&s, &id, -(EXPIRED_TOKEN_DAYS + 1));
        s.prune();
        let left = s.tokens(&who).unwrap();
        assert!(!left.iter().any(|t| t.id == id), "pruned later");
        assert_eq!(left.len(), 1);
    }

    #[test]
    fn tokens_stored_without_an_expiry_never_expire() {
        let (_d, s) = temp_store();
        let row = r#"{"hash":"h","email":"a@x.org","name":"old","created":"2025-01-01T00:00:00+00:00","last_used":null}"#;
        let tok: Token = serde_json::from_str(row).unwrap();
        assert!(tok.expires.is_none() && !tok.expired(&now()));
        let who = s.member("a@x.org").unwrap().unwrap();
        s.commit(vec![put("token", "old", &tok)]).unwrap();
        let t = s.tokens(&who).unwrap();
        assert!(t[0].expires.is_none() && !t[0].expired);
    }

    #[test]
    fn a_command_line_token_expires_by_default() {
        use crate::backends::DeviceSignIn;
        let (_d, s) = temp_store();
        let who = s.member("a@x.org").unwrap().unwrap();
        let (device, user) = s.device_start("comindly CLI on laptop", "1.2.3.4").unwrap();
        s.device_approve(&who, &user, true).unwrap();
        let DevicePoll::Approved { expires, .. } = s.device_poll(&device).unwrap() else {
            panic!("not approved");
        };
        let expires = expires.expect("an expiry");
        let in_days = chrono::DateTime::parse_from_rfc3339(&expires)
            .unwrap()
            .to_utc()
            - Utc::now();
        assert_eq!(in_days.num_days(), i64::from(TOKEN_DAYS) - 1);
    }

    #[test]
    fn the_audit_log_lists_newest_first_in_pages_and_filters() {
        use crate::backends::AuditLog;
        let (_d, s) = temp_store();
        for i in 0..5 {
            s.audit("a@x.org", "secret-set", &format!("n{i}"));
        }
        s.audit("b@x.org", "repos", "2");
        let token = s
            .create_token(&s.member("a@x.org").unwrap().unwrap(), "agent", Some(30))
            .unwrap();
        let all = s.audit_entries(&AuditQuery::default()).unwrap();
        assert_eq!(all.entries.len(), 7);
        assert!(all.next.is_none());
        assert!(all.entries.windows(2).all(|w| w[0].at >= w[1].at));
        assert_eq!(all.entries[0].action, "token");
        assert!(all.entries.iter().all(|e| !e.detail.contains(&token)));

        let first = s
            .audit_entries(&AuditQuery {
                limit: Some(3),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(first.entries.len(), 3);
        let second = s
            .audit_entries(&AuditQuery {
                limit: Some(3),
                before: first.next.clone(),
                ..Default::default()
            })
            .unwrap();
        let third = s
            .audit_entries(&AuditQuery {
                limit: Some(3),
                before: second.next.clone(),
                ..Default::default()
            })
            .unwrap();
        assert!(third.next.is_none());
        let paged: Vec<_> = [first, second, third]
            .into_iter()
            .flat_map(|p| p.entries)
            .map(|e| (e.at, e.detail))
            .collect();
        let whole: Vec<_> = all.entries.into_iter().map(|e| (e.at, e.detail)).collect();
        assert_eq!(paged, whole);

        let by = |actor: Option<&str>, action: Option<&str>| {
            s.audit_entries(&AuditQuery {
                actor: actor.map(str::to_string),
                action: action.map(str::to_string),
                ..Default::default()
            })
            .unwrap()
            .entries
            .len()
        };
        assert_eq!(by(Some("B@x.org"), None), 1);
        assert_eq!(by(None, Some("secret-set")), 5);
        assert_eq!(by(Some("a@x.org"), Some("repos")), 0);
    }

    #[test]
    fn audit_rows_survive_a_key_rotation() {
        use crate::backends::AuditLog;
        let dir = std::env::temp_dir().join(format!("comindly-server-{}", uuid::Uuid::new_v4()));
        let root = dir.to_str().unwrap();
        Store::open_keyed(root, "key one", None)
            .unwrap()
            .audit("a@x.org", "repos", "1");
        drop(Store::open_keyed(root, "key two", Some("key one")).unwrap());
        let only_new = Store::open_keyed(root, "key two", None).unwrap();
        assert_eq!(
            only_new
                .audit_entries(&AuditQuery::default())
                .unwrap()
                .entries
                .len(),
            1
        );
    }

    #[test]
    fn codes_are_twenty_characters_in_groups() {
        let c = new_code();
        assert_eq!(c.len(), 24);
        assert!(c.split('-').all(|g| g.len() == 4));
        assert_ne!(c, new_code());
    }

    #[test]
    fn sealed_values_open_only_with_their_key() {
        let (sealed, nonce) = seal("key-one", "git:gitlab.com", "glpat-example").unwrap();
        assert_eq!(
            open("key-one", "git:gitlab.com", &sealed, &nonce).as_deref(),
            Some("glpat-example")
        );
        assert!(open("key-two", "git:gitlab.com", &sealed, &nonce).is_none());
        assert!(open("key-one", "llm_api_key", &sealed, &nonce).is_none());
        assert_eq!(hint("glpat-example"), "…mple");
    }

    #[test]
    fn values_sealed_by_earlier_versions_still_open() {
        // Sealed with chacha20poly1305 0.10.
        let unhex = |s: &str| {
            (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
                .collect::<Vec<u8>>()
        };
        let sealed = unhex("04558269f76990598e21e0fa73bff46e3b8778dc6671e982ac0d0096b4");
        let nonce = unhex("c7063968b8df63bf60ecae5b3deba9969d937912cc1b4726");
        assert_eq!(
            open("fixture-key", "git:example.com", &sealed, &nonce).as_deref(),
            Some("example-value")
        );
    }
}
