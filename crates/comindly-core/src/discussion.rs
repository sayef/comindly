//! Discussions store: `<index-dir>/_discussions/<id>/discussion.json` and `<id>/posts/<post-id>.json`,
//! written create-only like the ledger: a discussion grows by new posts, never by edits.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use futures::TryStreamExt;
use object_store::path::Path as ObjPath;
use object_store::{ObjectStore, ObjectStoreExt, PutMode, PutOptions, PutPayload};

pub use comindly_api::discussion::*;

use crate::graph::CodeGraph;
use crate::ledger::{sub_store, Anchor, Provenance};
use crate::model::{Symbol, SymbolKind};

/// The discussions under an index root (local directory or a bucket prefix).
pub struct Discussions {
    store: Arc<dyn ObjectStore>,
    root: ObjPath,
}

impl Discussions {
    pub fn open(index_root: &str) -> Result<Discussions> {
        let (store, root) = sub_store(index_root, "_discussions")?;
        Ok(Discussions { store, root })
    }

    /// Write `json` at `key`, create-only: an existing key is an error, never an overwrite.
    async fn put_new(&self, key: ObjPath, json: Vec<u8>) -> Result<()> {
        let opts = PutOptions {
            mode: PutMode::Create,
            ..Default::default()
        };
        self.store
            .put_opts(&key, PutPayload::from(json), opts)
            .await
            .with_context(|| format!("write discussion {key}"))?;
        Ok(())
    }

    pub async fn create(&self, d: &Discussion) -> Result<()> {
        d.validate()?;
        let key = self
            .root
            .clone()
            .join(d.id.as_str())
            .join("discussion.json");
        self.put_new(key, serde_json::to_vec_pretty(d)?).await
    }

    /// Open `d`, or find the open discussion for the same repository and ticket or branch:
    /// its id, and whether it already existed.
    // shortcut: two writers opening the same ticket at the same moment get two discussions; list shows both.
    pub async fn start(&self, d: &Discussion) -> Result<(String, bool)> {
        let all = self.all().await?;
        if let Some(t) = find_open(&all, &d.repo, d.ticket.as_deref(), d.branch.as_deref()) {
            return Ok((t.discussion.id.clone(), true));
        }
        self.create(d).await?;
        Ok((d.id.clone(), false))
    }

    pub async fn post(&self, p: &Post) -> Result<()> {
        p.validate()?;
        anyhow::ensure!(
            self.get(&p.discussion).await?.is_some(),
            "no discussion `{}`",
            p.discussion
        );
        let key = self
            .root
            .clone()
            .join(p.discussion.as_str())
            .join("posts")
            .join(format!("{}.json", p.id));
        self.put_new(key, serde_json::to_vec_pretty(p)?).await
    }

    /// The discussions under `prefix` with their posts, latest activity first.
    async fn read(&self, prefix: &ObjPath) -> Result<Vec<Transcript>> {
        let mut found: Vec<Discussion> = Vec::new();
        let mut posts: HashMap<String, Vec<Post>> = HashMap::new();
        let metas: Vec<_> = self.store.list(Some(prefix)).try_collect().await?;
        for meta in metas {
            let key = meta.location;
            if !key.as_ref().ends_with(".json") {
                continue;
            }
            let bytes = self.store.get(&key).await?.bytes().await?;
            // Like the ledger: a malformed file is skipped with a warning, so it cannot hide the rest.
            if key.as_ref().ends_with("/discussion.json") {
                match serde_json::from_slice::<Discussion>(&bytes) {
                    Ok(d) => found.push(d),
                    Err(e) => eprintln!("comindly: skipping unreadable discussion file {key}: {e}"),
                }
            } else {
                match serde_json::from_slice::<Post>(&bytes) {
                    Ok(p) => posts.entry(p.discussion.clone()).or_default().push(p),
                    Err(e) => eprintln!("comindly: skipping unreadable discussion file {key}: {e}"),
                }
            }
        }
        let mut out: Vec<Transcript> = found
            .into_iter()
            .map(|discussion| {
                let mut posts = posts.remove(&discussion.id).unwrap_or_default();
                posts.sort_by(|a, b| a.created.cmp(&b.created).then_with(|| a.id.cmp(&b.id)));
                Transcript { discussion, posts }
            })
            .collect();
        out.sort_by(|a, b| {
            b.last()
                .cmp(a.last())
                .then_with(|| a.discussion.id.cmp(&b.discussion.id))
        });
        Ok(out)
    }

    pub async fn all(&self) -> Result<Vec<Transcript>> {
        self.read(&self.root).await
    }

    pub async fn get(&self, id: &str) -> Result<Option<Transcript>> {
        let id = id.trim();
        if id.is_empty() {
            return Ok(None);
        }
        let prefix = self.root.clone().join(id);
        Ok(self
            .read(&prefix)
            .await?
            .into_iter()
            .find(|t| t.discussion.id == id))
    }
}

/// What `on` names, as anchors: a file by its repo-relative path, else the symbol `lookup` finds.
pub fn anchors<'a>(
    g: &CodeGraph,
    symbols: impl Iterator<Item = &'a Symbol> + Clone,
    on: &[String],
    commits: &HashMap<String, String>,
) -> Result<Vec<Anchor>> {
    let mut out = Vec::new();
    for focus in on.iter().map(|f| f.trim()).filter(|f| !f.is_empty()) {
        let file = symbols.clone().find(|s| {
            s.kind == SymbolKind::File
                && (s.file_path == focus || focus.ends_with(&format!("/{}", s.file_path)))
        });
        let s = match file {
            Some(s) => s,
            None => {
                let id = g.lookup(focus).map(|i| g.node(i).id).with_context(|| {
                    format!("no symbol or file matches `{focus}`; use `find` first")
                })?;
                symbols
                    .clone()
                    .find(|s| s.id.render() == id)
                    .with_context(|| format!("symbol `{id}` not loaded"))?
            }
        };
        out.push(Anchor::of(s).at(commits));
    }
    Ok(out)
}

/// A post from a request's fields; `kind` is checked here.
pub fn new_post(
    discussion: &str,
    kind: Option<&str>,
    body: &str,
    evidence: Vec<String>,
    anchors: Vec<Anchor>,
    by: Provenance,
) -> Result<Post> {
    let kind = match kind.map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => Some(PostKind::parse(k).with_context(|| {
            format!("unknown kind `{k}`: plan, decision, change, question or concern")
        })?),
        None => None,
    };
    let mut post = Post::new(discussion, kind, body, by);
    post.evidence = evidence
        .into_iter()
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty())
        .collect();
    post.anchors = anchors;
    Ok(post)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::AuthorKind;

    fn prov(author: AuthorKind) -> Provenance {
        Provenance {
            author,
            actor: "actor-test".into(),
            source: "test".into(),
            agent: None,
            session: None,
            name: None,
            teams: Vec::new(),
        }
    }

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "comindly-discussions-{}-{name}",
            std::process::id()
        ))
    }

    fn anchor(symbol: &str) -> Anchor {
        Anchor {
            symbol: symbol.into(),
            repo: "r".into(),
            path: "a.rs".into(),
            name: "f".into(),
            line: 1,
            fingerprint: None,
            commit: None,
        }
    }

    #[tokio::test]
    async fn posts_append_create_only_and_read_back_oldest_first() {
        let dir = temp("roundtrip");
        let store = Discussions::open(dir.to_str().unwrap()).unwrap();
        let d = Discussion::new(
            "r",
            Some("LIN-1"),
            None,
            "paginate orders",
            prov(AuthorKind::Agent),
        );
        store.create(&d).await.unwrap();
        assert!(
            store.create(&d).await.is_err(),
            "the same discussion twice must not overwrite"
        );
        let first = new_post(
            &d.id,
            Some("plan"),
            "offset pagination",
            vec![],
            vec![],
            prov(AuthorKind::Agent),
        )
        .unwrap();
        store.post(&first).await.unwrap();
        let second = new_post(
            &d.id,
            Some("change"),
            "cursor on (created_at, id)",
            vec!["pr:#91".into()],
            vec![anchor("s1")],
            prov(AuthorKind::Human),
        )
        .unwrap();
        store.post(&second).await.unwrap();
        let all = store.all().await.unwrap();
        let got = store.get(&d.id).await.unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].posts, vec![first, second]);
        assert_eq!(got.as_ref(), all.first());
    }

    #[tokio::test]
    async fn opening_again_returns_the_open_discussion() {
        let dir = temp("start");
        let store = Discussions::open(dir.to_str().unwrap()).unwrap();
        let d = Discussion::new(
            "r",
            Some("LIN-1"),
            Some("feat/x"),
            "t",
            prov(AuthorKind::Agent),
        );
        let (id, existed) = store.start(&d).await.unwrap();
        let again = Discussion::new("r", None, Some("feat/x"), "t", prov(AuthorKind::Agent));
        let (same, existed_now) = store.start(&again).await.unwrap();
        let other = Discussion::new("r2", Some("LIN-1"), None, "t", prov(AuthorKind::Agent));
        let (elsewhere, _) = store.start(&other).await.unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert!(!existed && existed_now);
        assert_eq!(id, same);
        assert_ne!(id, elsewhere, "another repository gets its own discussion");
    }

    #[tokio::test]
    async fn posting_to_an_unknown_discussion_fails() {
        let dir = temp("unknown");
        let store = Discussions::open(dir.to_str().unwrap()).unwrap();
        let d = Discussion::new("r", Some("LIN-1"), None, "t", prov(AuthorKind::Agent));
        store.create(&d).await.unwrap();
        let unknown = Post::new("no-such-id", None, "body", prov(AuthorKind::Human));
        let err = store.post(&unknown).await.unwrap_err().to_string();
        let empty = Post::new(" ", None, "body", prov(AuthorKind::Human));
        let empty_err = store.post(&empty).await.is_err();
        let prefix = store.get(&d.id[..8]).await.unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert!(err.contains("no discussion"), "{err}");
        assert!(empty_err, "an empty id names no discussion");
        assert!(prefix.is_none(), "only the exact id finds a discussion");
    }

    #[test]
    fn a_post_kind_must_be_known() {
        let by = prov(AuthorKind::Agent);
        assert!(new_post("d", Some("Decision"), "b", vec![], vec![], by.clone()).is_ok());
        assert!(new_post("d", Some("rant"), "b", vec![], vec![], by).is_err());
    }

    #[test]
    fn why_and_preflight_see_discussions_through_post_anchors() {
        let d = Discussion::new("r", Some("LIN-1"), None, "t", prov(AuthorKind::Agent));
        let mut p = Post::new(&d.id, None, "b", prov(AuthorKind::Agent));
        p.anchors = vec![anchor("s1")];
        let all = vec![Transcript {
            discussion: d,
            posts: vec![p],
        }];
        assert_eq!(touching(&all, &["s1".into()]).len(), 1);
        assert!(touching(&all, &["s2".into()]).is_empty());
        assert!(
            mention(&touching(&all, &["s1".into()])).contains("### Open discussions on this code")
        );
        assert_eq!(mention(&[]), "");
    }

    #[test]
    fn posts_read_as_quoted_unreviewed_notes() {
        let d = Discussion::new("r", Some("LIN-1"), None, "t", prov(AuthorKind::Agent));
        let p = Post::new(
            &d.id,
            Some(PostKind::Plan),
            "one\ntwo",
            prov(AuthorKind::Agent),
        );
        let md = markdown(&Transcript {
            discussion: d,
            posts: vec![p],
        });
        assert!(md.contains("_Unreviewed working notes"));
        assert!(md.contains("> one\n> two\n"));
        assert!(md.contains(" · plan · an agent"));
    }
}
