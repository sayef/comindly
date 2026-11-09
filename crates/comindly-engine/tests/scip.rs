//! SCIP indexes (scip-typescript 0.4.0 output for the ui-kit/web-app fixtures) answer references.

use std::path::Path;

#[test]
fn scip_answers_are_used_and_exact() {
    let only = ["ui-kit", "web-app"];
    let r = comindly_engine::eval::eval_with(
        Path::new("tests/fixtures/org"),
        Some(&only),
        Some(Path::new("tests/fixtures/scip")),
    )
    .unwrap();
    assert!(r.scip_answers > 0, "SCIP was not consulted");
    assert!(r.missed.is_empty(), "missed: {:?}", r.missed);
    assert!(r.spurious.is_empty(), "spurious: {:?}", r.spurious);
}
