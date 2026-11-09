//! The Lance symbols table round-trips every field, including fingerprints.

use std::path::Path;

#[test]
fn symbols_roundtrip_with_fingerprints() {
    let out = comindly_engine::parse::parse_repo(
        Path::new("tests/fixtures/org/pkg-common"),
        "pkg-common",
    )
    .unwrap();
    let dir = std::env::temp_dir().join(format!("comindly-roundtrip-{}", std::process::id()));
    let uri = dir.to_string_lossy().to_string();
    comindly_engine::index::write_graph_blocking(&uri, &out.symbols, &out.edges).unwrap();
    let (mut back, _) = comindly_engine::index::read_graph_blocking(&uri).unwrap();
    std::fs::remove_dir_all(&dir).ok();

    let mut orig = out.symbols.clone();
    orig.sort_by_key(|s| s.id.render());
    back.sort_by_key(|s| s.id.render());
    assert!(orig.iter().all(|s| s.fingerprint.is_some()));
    assert_eq!(orig, back);
}
