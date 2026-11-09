//! Incremental indexing re-resolves from stored facts: it must match a full index exactly.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

fn run(dir: &Path, cmd: &str, args: &[&str]) {
    let ok = Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .status()
        .unwrap()
        .success();
    assert!(ok, "{cmd} {args:?} failed");
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dst = to.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &dst);
        } else {
            std::fs::copy(e.path(), dst).unwrap();
        }
    }
}

fn index(repo: &Path, dir: &Path, incremental: bool) {
    let mut settings = comindly_engine::context::Settings::resolve(&Default::default(), |_| None);
    settings.embed = false;
    settings.enrich = false;
    settings.flows = false;
    settings.guide = false;
    let ctx = comindly_engine::context::IndexContext::new(
        dir.to_str().unwrap(),
        settings,
        std::sync::Arc::new(comindly_engine::events::Discard),
        std::env::temp_dir(),
    );
    let source = comindly_engine::pipeline::Source::at(repo.to_str().unwrap());
    comindly_engine::rt::shared()
        .block_on(comindly_engine::pipeline::index_repo(
            &ctx,
            source,
            incremental,
            None,
            comindly_engine::scip::Mode::Off,
        ))
        .unwrap();
}

fn edges(dir: &Path) -> BTreeSet<(String, String, String)> {
    let (_, edges) =
        comindly_engine::index::read_graph_blocking(&format!("{}/_graph", dir.display())).unwrap();
    edges
        .into_iter()
        .map(|e| (e.src.render(), e.dst.render(), e.kind.to_string()))
        .collect()
}

#[test]
fn incremental_matches_full_after_cross_file_change() {
    let tmp = std::env::temp_dir().join(format!("comindly-incr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let repo = tmp.join("service-a");
    copy_dir(Path::new("tests/fixtures/org/service-a"), &repo);
    run(&repo, "git", &["init", "-q"]);
    run(&repo, "git", &["add", "."]);
    run(&repo, "git", &["commit", "-qm", "base"]);

    let (incr, full) = (tmp.join("incr"), tmp.join("full"));
    index(&repo, &incr, false);
    let before = edges(&incr);
    assert!(before
        .iter()
        .any(|(_, d, k)| k == "calls" && d.ends_with("jobs/run().")));

    // Only jobs.py changes; main.py still imports and calls the old name.
    std::fs::write(
        repo.join("service_a/jobs.py"),
        "def execute():\n    return \"jobs\"\n",
    )
    .unwrap();
    run(&repo, "git", &["commit", "-qam", "rename run"]);

    index(&repo, &incr, true);
    index(&repo, &full, false);
    let (after, expected) = (edges(&incr), edges(&full));
    std::fs::remove_dir_all(&tmp).ok();

    assert!(
        !after.iter().any(|(_, d, _)| d.ends_with("jobs/run().")),
        "stale edge kept"
    );
    assert_eq!(after, expected);
}
