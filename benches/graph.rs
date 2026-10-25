//! Resolution, graph queries and ledger derivation on the fixture org.

use std::path::Path;

use comindly::graph::CodeGraph;
use comindly::ledger::{Anchor, AuthorKind, Kind, Provenance, Record};
use criterion::{criterion_group, criterion_main, Criterion};

fn corpus() -> (
    Vec<comindly::model::Symbol>,
    Vec<comindly::model::Edge>,
    Vec<comindly::facts::FileFacts>,
) {
    let root = Path::new("tests/fixtures/org");
    let mut out = (Vec::new(), Vec::new(), Vec::new());
    let mut repos: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .collect();
    repos.sort_by_key(|e| e.file_name());
    for e in repos {
        let name = e.file_name().to_string_lossy().to_string();
        let o = comindly::parse::parse_repo(&e.path(), &name).unwrap();
        out.0.extend(o.symbols);
        out.1.extend(o.edges);
        out.2.extend(o.facts);
    }
    out
}

fn bench(c: &mut Criterion) {
    let (symbols, edges, facts) = corpus();
    c.bench_function("resolve fixture org", |b| {
        b.iter(|| comindly::resolve::resolve(&symbols, &edges, &facts))
    });

    let r = comindly::resolve::resolve(&symbols, &edges, &facts);
    let mut all = symbols.clone();
    all.extend(r.symbols);
    let graph = CodeGraph::build(&all, &r.edges);
    let focus = graph.lookup("get_logger").expect("fixture symbol");
    c.bench_function("ripple", |b| b.iter(|| graph.ripple(focus, 4)));
    c.bench_function("zoom", |b| b.iter(|| graph.zoom(focus)));
    c.bench_function("context_pack", |b| {
        b.iter(|| graph.context_pack(focus, 1500))
    });

    let anchors: Vec<Anchor> = all.iter().take(200).map(Anchor::of).collect();
    let records: Vec<Record> = (0..1000)
        .map(|i| {
            let mut rec = Record::new(
                Kind::Constraint,
                &format!("constraint {i}"),
                vec![anchors[i % anchors.len()].clone()],
                Provenance {
                    author: AuthorKind::Human,
                    actor: "bench".into(),
                    source: "bench".into(),
                    agent: None,
                    session: None,
                    name: None,
                },
            );
            rec.body = "Lookups must stay scoped to the customer.".into();
            rec
        })
        .collect();
    c.bench_function("ledger derive 1000 records", |b| {
        b.iter(|| comindly::ledger::derive(&records, &all, &Default::default()))
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
