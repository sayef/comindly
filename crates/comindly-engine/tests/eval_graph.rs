//! Ratchet: graph quality on the fixture org may only go up. Raise the floors as M1 lands.

use std::path::Path;

const MIN_PRECISION: f64 = 1.0;
const MIN_RECALL: f64 = 1.0;

#[test]
fn fixture_org_quality_does_not_regress() {
    let r = comindly_engine::eval::eval_fixture(Path::new("tests/fixtures/org")).unwrap();
    let (p, rc) = (r.total.precision(), r.total.recall());
    assert!(p >= MIN_PRECISION, "precision {p:.2} < {MIN_PRECISION}");
    assert!(rc >= MIN_RECALL, "recall {rc:.2} < {MIN_RECALL}");
}
