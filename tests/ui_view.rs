//! `comindly ui` against a server shows the same data as the index read locally.

use std::path::Path;

use comindly::ledger::{Anchor, Kind, Provenance, Record};
use comindly::view::{Data, View};

/// An index of a few fixture repositories, with one ledger record.
fn build(root: &str) {
    let (mut symbols, mut edges, mut facts, mut packages) = (vec![], vec![], vec![], vec![]);
    for repo in ["courses-svc", "profile-svc", "ui-kit", "web-app"] {
        let dir = Path::new("tests/fixtures/org").join(repo);
        let out = comindly::parse::parse_repo(&dir, repo).unwrap();
        symbols.extend(out.symbols);
        edges.extend(out.edges);
        facts.extend(out.facts);
        packages.extend(comindly::manifests::scan(&dir, repo));
    }
    let r = comindly::resolve::resolve(&symbols, &edges, &facts);
    symbols.extend(r.symbols);
    let graph = format!("{root}/_graph");
    comindly::index::write_graph_blocking(&graph, &symbols, &r.edges).unwrap();
    comindly::index::write_facts_blocking(&graph, &facts).unwrap();
    comindly::index::write_packages_blocking(&graph, &packages).unwrap();
    let on = symbols.iter().find(|s| s.name == "get_course").unwrap();
    let prov = Provenance::human("test", &comindly::config::local_identity());
    let rec = Record::new(
        Kind::Decision,
        "courses are read-only",
        vec![Anchor::of(on)],
        prov,
    );
    comindly::ledger::put_blocking(root, &rec, None).unwrap();
}

fn same(local: &Data, remote: &Data) {
    let json = |v: &[comindly::ledger::Entry]| serde_json::to_value(v).unwrap();
    assert_eq!(local.symbols, remote.symbols);
    assert_eq!(local.edges, remote.edges);
    assert_eq!(json(&local.entries), json(&remote.entries));
    assert_eq!(local.conventions, remote.conventions);
    assert_eq!(local.manual, remote.manual);
    assert_eq!(local.deps, remote.deps);
    assert_eq!(local.graph.repos(), remote.graph.repos());
    let names = |d: &Data| -> Vec<String> {
        d.contracts
            .iter()
            .map(|c| c.contract.name.clone())
            .collect()
    };
    assert_eq!(names(local), names(remote));
}

#[test]
fn a_server_sends_the_terminal_ui_what_the_index_holds() {
    let tmp = std::env::temp_dir().join(format!("comindly-ui-view-{}", std::process::id()));
    std::env::set_var("XDG_CONFIG_HOME", tmp.join("config"));
    std::env::set_var("XDG_DATA_HOME", tmp.join("data"));
    let root = tmp.join("index").to_string_lossy().into_owned();
    build(&root);
    let verifier = comindly::config::local_verifier();
    let local = Data::load(&root, &verifier).unwrap();
    assert!(!local.entries.is_empty() && !local.deps.is_empty() && !local.contracts.is_empty());

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let token = comindly::web::new_token();
    let served = Data::load(&root, &verifier).unwrap();
    let access = comindly::web::Access::Local(token.clone());
    std::thread::spawn(move || comindly::web::serve(served, listener, access));

    let cookie = format!("comindly_token={token}");
    let fetch = || {
        comindly::rt::block_on(async {
            let r = reqwest::Client::new()
                .get(format!("{base}/api/v1/view"))
                .header("cookie", &cookie)
                .send()
                .await?
                .error_for_status()?;
            anyhow::Ok(Data::from_view(r.json::<View>().await?))
        })
        .unwrap()
    };
    let encoding = comindly::rt::block_on(async {
        reqwest::Client::builder()
            .no_gzip()
            .build()?
            .get(format!("{base}/api/v1/view"))
            .header("cookie", &cookie)
            .header("accept-encoding", "gzip")
            .send()
            .await
    })
    .unwrap()
    .headers()
    .get("content-encoding")
    .cloned();
    assert_eq!(encoding.as_ref().map(|v| v.as_bytes()), Some(&b"gzip"[..]));
    let remote = fetch();
    assert!(remote.root.is_none(), "a server's data is read-only here");
    same(&local, &remote);

    // A client holding the current tag gets 304 instead of the graph again, until the view changes.
    let tagged = |tag: &str| {
        comindly::rt::block_on(async {
            let r = reqwest::Client::new()
                .get(format!("{base}/api/v1/view"))
                .header("cookie", &cookie)
                .header("if-none-match", tag)
                .send()
                .await?;
            let etag = r
                .headers()
                .get("etag")
                .map(|v| v.to_str().unwrap().to_string());
            anyhow::Ok((r.status().as_u16(), etag.unwrap_or_default()))
        })
        .unwrap()
    };
    let (status, tag) = tagged("\"none\"");
    assert_eq!(status, 200);
    assert!(!tag.is_empty());
    assert_eq!(tagged(&tag), (304, tag.clone()));

    let id = local.entries[0].record.id.clone();
    let confirmed: serde_json::Value = comindly::rt::block_on(async {
        reqwest::Client::new()
            .post(format!("{base}/api/v1/review"))
            .header("cookie", &cookie)
            .json(&serde_json::json!({"id": id, "action": "confirm"}))
            .send()
            .await?
            .json()
            .await
    })
    .unwrap();
    assert_eq!(confirmed["ok"], true, "{confirmed}");
    let local = Data::load(&root, &verifier).unwrap();
    assert_eq!(local.entries[0].trust, comindly::ledger::Trust::T3);
    same(&local, &fetch());
    assert_eq!(
        tagged(&tag).0,
        200,
        "a review changes the view, so the old tag no longer matches"
    );
    std::fs::remove_dir_all(&tmp).ok();
}
