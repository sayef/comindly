//! An open discussion shows in `why` and `preflight` on the code its posts are about, on a real index.

use std::collections::HashMap;
use std::path::Path;

use comindly_engine::discussion::{self, Discussion, Discussions};
use comindly_engine::graph::CodeGraph;
use comindly_engine::ledger::Provenance;
use comindly_engine::preflight::Change;

#[tokio::test]
async fn why_and_preflight_name_open_discussions_on_the_code() {
    let dir = Path::new("tests/fixtures/org/courses-svc");
    let out = comindly_engine::parse::parse_repo(dir, "courses-svc").unwrap();
    let graph = CodeGraph::build(&out.symbols, &out.edges);
    let on = out.symbols.iter().find(|s| s.name == "get_course").unwrap();

    let root = std::env::temp_dir().join(format!("comindly-discussions-it-{}", std::process::id()));
    let store = Discussions::open(root.to_str().unwrap()).unwrap();
    let by = Provenance::agent("test", Some("claude-code".into()), &Default::default());
    let d = Discussion::new(
        "courses-svc",
        Some("LIN-7"),
        None,
        "cache course reads",
        by.clone(),
    );
    let (id, _) = store.start(&d).await.unwrap();
    let anchors = discussion::anchors(
        &graph,
        out.symbols.iter(),
        &[on.id.render()],
        &HashMap::new(),
    )
    .unwrap();
    let post =
        discussion::new_post(&id, Some("plan"), "cache by course id", vec![], anchors, by).unwrap();
    store.post(&post).await.unwrap();
    let all = store.all().await.unwrap();
    std::fs::remove_dir_all(&root).ok();

    let why = comindly_engine::answers::why(&graph, &[], &all, "get_course").unwrap();
    assert!(
        why.markdown.contains("### Open discussions on this code"),
        "{}",
        why.markdown
    );
    assert!(why.markdown.contains(&id));

    let change = Change {
        repo: on.repo.0.clone(),
        path: on.file_path.clone(),
        lines: vec![(on.range.start.line, on.range.end.line)],
    };
    let pre = comindly_engine::preflight::discussions(&graph, &out.symbols, &all, &[change]);
    assert!(pre.contains(&id), "{pre}");

    let elsewhere = out
        .symbols
        .iter()
        .find(|s| s.name != "get_course" && s.file_path != on.file_path);
    if let Some(other) = elsewhere {
        let why = comindly_engine::answers::why(&graph, &[], &all, &other.id.render()).unwrap();
        assert!(
            !why.markdown.contains(&id),
            "unrelated code must not name it"
        );
    }
}
