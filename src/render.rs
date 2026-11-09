//! How the command line prints the engine's answers.

use std::collections::{BTreeMap, BTreeSet};
use std::process::ExitCode;

/// `n word` / `n words` — naive English pluralization for count phrases.
pub fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("{n} {word}")
    } else {
        format!(
            "{n} {word}{}",
            if word.ends_with("ch") || word.ends_with('s') {
                "es"
            } else {
                "s"
            }
        )
    }
}

pub fn print_group(label: &str, nodes: &[comindly_api::graph::Node], limit: usize) {
    if nodes.is_empty() {
        return;
    }
    let names: Vec<String> = nodes.iter().take(limit).map(|n| n.name.clone()).collect();
    let more = nodes.len().saturating_sub(limit);
    let suffix = if more > 0 {
        format!(" (+{more})")
    } else {
        String::new()
    };
    crate::term::field(label, &format!("{}{suffix}", names.join(", ")));
}

pub fn render_contracts(c: &comindly_api::answers::Contracts) -> ExitCode {
    let list = |ns: &[comindly_api::graph::Node]| {
        let set: BTreeSet<String> = ns
            .iter()
            .map(|n| {
                if n.repo == comindly_api::graph::CONTRACTS {
                    n.name.clone()
                } else {
                    format!("{}:{}", n.repo, n.name)
                }
            })
            .collect();
        if set.is_empty() {
            "—".to_string()
        } else {
            set.into_iter().collect::<Vec<_>>().join("\n")
        }
    };
    let mut table = comfy_table::Table::new();
    table
        .load_style(comfy_table::presets::UTF8_FULL)
        .set_header(vec!["contract", "provided by", "used by"]);
    for v in &c.views {
        table.add_row(vec![
            v.contract.name.clone(),
            list(&v.providers),
            list(&v.consumers),
        ]);
    }
    crate::term::header(&format!(
        "Contracts — {} shown of {}",
        c.views.len(),
        c.total
    ));
    println!("{table}");
    ExitCode::SUCCESS
}

pub fn render_conventions(c: &comindly_api::answers::Conventions) -> ExitCode {
    use comindly_api::conventions::{Origin, ORG};
    let mut by_repo: BTreeMap<&str, Vec<&comindly_api::conventions::Convention>> = BTreeMap::new();
    for c in &c.conventions {
        by_repo.entry(c.repo.as_str()).or_default().push(c);
    }
    for (r, cs) in by_repo {
        crate::term::header(if r == ORG { "Org baseline" } else { r });
        for c in cs {
            let tag = match c.origin {
                Origin::Measured => "",
                Origin::Edited => ", edited",
                Origin::Manual => ", team",
            };
            let how = c.strength().map(|s| format!("{s}: ")).unwrap_or_default();
            crate::term::field(&c.dimension, &format!("{how}{}  [{}{tag}]", c.rule, c.id()));
            if let Some(e) = c.evidence() {
                crate::term::note(&e);
            }
            if !c.exemplars.is_empty() {
                crate::term::note(&format!("e.g. {}", c.exemplars.join(", ")));
            }
            if !c.counterexamples.is_empty() {
                crate::term::note(&format!("except {}", c.counterexamples.join(", ")));
            }
        }
    }
    if !c.hidden.is_empty() {
        crate::term::header("Hidden (comindly conventions restore <id>)");
        for m in &c.hidden {
            crate::term::field(&m.dimension, &format!("{}  [{}, {}]", m.rule, m.id, m.repo));
        }
    }
    ExitCode::SUCCESS
}

pub fn render_explore(focus: &str, e: &comindly_api::answers::Explore) -> ExitCode {
    let dangling = if e.dangling > 0 {
        format!(", {} dangling", e.dangling)
    } else {
        String::new()
    };
    crate::term::note(&format!(
        "graph: {} symbols, {} edges{dangling}",
        e.symbols, e.edges,
    ));
    let Some(v) = &e.view else {
        crate::term::err(&format!("no symbol matching `{focus}`"));
        return ExitCode::FAILURE;
    };

    // zoom
    if let Some(f) = &v.focus {
        crate::term::header(&format!(
            "● {} [{}]  {}  {}",
            f.name, f.kind, f.repo, f.location
        ));
        if let Some(sig) = &f.signature {
            crate::term::note(sig);
        }
    }
    if let Some(c) = &v.container {
        // Drop the redundant location when the container is the file itself.
        if c.kind.eq_ignore_ascii_case("file") {
            crate::term::field("in", &c.name);
        } else {
            crate::term::field("in", &format!("{}  ({})", c.name, c.location));
        }
    }
    print_group("calls", &v.callees, 8);
    print_group("called by", &v.callers, 8);
    print_group("imported by", &v.importers, 8);
    print_group("members", &v.members, 8);

    // ripple — blast radius grouped by repo
    crate::term::header(&format!(
        "ripple (blast radius, ≤{} hops): {} dependents",
        comindly_api::answers::RIPPLE_DEPTH,
        v.dependents
    ));
    if !v.by_repo.is_empty() {
        let mut t = crate::term::table(&["repo", "dependents"]);
        for (repo, n) in &v.by_repo {
            t.add_row(vec![repo.clone(), n.to_string()]);
        }
        crate::term::right_align(&mut t, &[1]); // dependents
        println!("{t}");
    }

    // context pack — the minimal correct read-set within a token budget
    let total: usize = v.pack.iter().map(|(_, t)| t).sum();
    crate::term::header(&format!(
        "context pack for changing `{}` (~{total} tokens, budget {}): {}",
        v.focus.as_ref().map_or("?", |f| f.name.as_str()),
        v.budget,
        plural(v.pack.len(), "symbol")
    ));
    if !v.pack.is_empty() {
        let mut t = crate::term::table(&["symbol", "location"]);
        for (node, _) in v.pack.iter().take(15) {
            t.add_row(vec![node.name.clone(), node.location.clone()]);
        }
        println!("{t}");
    }
    ExitCode::SUCCESS
}

pub fn render_find(query: &str, hits: &[comindly_api::graph::Node]) -> ExitCode {
    crate::term::header(&format!(
        "find: \"{query}\" ({})",
        plural(hits.len(), "match")
    ));
    if hits.is_empty() {
        crate::term::note("no symbols matched");
        return ExitCode::SUCCESS;
    }
    let mut t = crate::term::table(&["symbol", "kind", "repo", "location", "id"]);
    for n in hits {
        t.add_row(vec![
            n.name.clone(),
            n.kind.clone(),
            n.repo.clone(),
            n.location.clone(),
            n.id.clone(),
        ]);
    }
    println!("{t}");
    ExitCode::SUCCESS
}

pub fn render_flow(f: &comindly_api::answers::Flow) -> ExitCode {
    let focus = &f.focus;
    match &f.narration {
        Some((narr, queries)) => {
            crate::term::header(&format!("flow: {focus}"));
            println!("{narr}\n");
            if !queries.is_empty() {
                crate::term::field("Ask", &queries.join(" · "));
            }
        }
        None => crate::term::note(&format!(
            "no pre-generated narration for `{focus}` — showing the raw trace; run `index/link --flows` to narrate"
        )),
    }
    crate::term::header(&format!(
        "flow trace from `{focus}` ({} steps)",
        f.steps.len()
    ));
    if f.steps.is_empty() {
        crate::term::note("no outgoing calls from here");
    } else {
        let mut t = crate::term::table(&["depth", "via", "symbol", "location"]);
        for h in &f.steps {
            t.add_row(vec![
                format!("d{}", h.depth),
                h.via.clone(),
                h.name.clone(),
                h.location.clone(),
            ]);
        }
        crate::term::right_align(&mut t, &[0]);
        println!("{t}");
    }
    ExitCode::SUCCESS
}

pub fn render_ledger(l: &comindly_api::answers::Ledger) -> ExitCode {
    if let Some(note) = &l.note {
        crate::term::ok(note);
    } else {
        print!("{}", l.markdown);
    }
    if l.failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

pub fn render_packages(deps: &[comindly_api::manifests::InternalDep]) -> ExitCode {
    let mut table = comfy_table::Table::new();
    table
        .load_style(comfy_table::presets::UTF8_FULL)
        .set_header(vec![
            "package",
            "published by",
            "consumer",
            "requires",
            "locked",
        ]);
    let opt = |v: &Option<String>| v.clone().unwrap_or_else(|| "—".into());
    for d in deps {
        let locked = match (&d.locked, d.skewed()) {
            (Some(l), true) => format!("{l}  ≠"),
            _ => opt(&d.locked),
        };
        table.add_row(vec![
            format!("{} ({})", d.name, d.ecosystem),
            format!("{} @ {}", d.publisher, opt(&d.published)),
            d.consumer.clone(),
            opt(&d.requirement),
            locked,
        ]);
    }
    crate::term::header(&format!("Internal dependencies — {}", deps.len()));
    println!("{table}");
    ExitCode::SUCCESS
}

pub fn render_preflight(p: &comindly_api::answers::Preflight, strict: bool) -> ExitCode {
    println!("{}", p.markdown);
    if strict && p.blocking {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

pub fn render_search(query: &str, s: &comindly_api::answers::Search, markdown: bool) -> ExitCode {
    if markdown {
        print!("{}", s.markdown);
        return ExitCode::SUCCESS;
    }
    crate::term::header(&format!("search: \"{query}\""));
    if s.hits.is_empty() {
        crate::term::note(
            "no matches — try a broader query (or the index was built with --no-embed)",
        );
        return ExitCode::SUCCESS;
    }
    let mut t = crate::term::table(&["score", "symbol", "kind", "deps", "location", "summary"]);
    for h in &s.hits {
        t.add_row(vec![
            format!("{:.3}", h.score),
            h.name.clone(),
            h.kind.to_string(),
            h.deps.to_string(),
            h.location.clone(),
            h.summary.clone().unwrap_or_default(),
        ]);
    }
    crate::term::right_align(&mut t, &[0, 3]); // score, deps
    println!("{t}");
    ExitCode::SUCCESS
}

pub fn render_stats(st: &comindly_api::answers::Stats) -> ExitCode {
    for r in &st.unknown {
        crate::term::warn(&format!("no such repo in the index: {r}"));
    }
    if st.repos.is_empty() {
        crate::term::err("no matching repos");
        return ExitCode::FAILURE;
    }

    crate::term::header(&format!(
        "Index stats — {} across {}",
        plural(st.symbols, "symbol"),
        plural(st.indexed, "repo")
    ));

    for r in &st.repos {
        crate::term::header(&format!("● {}", r.repo));
        crate::term::field("symbols", &r.symbols.to_string());
        crate::term::field("edges", &r.edges.to_string());
        crate::term::field("enriched", &format!("{} / {}", r.enriched, r.symbols));

        let mut kt = crate::term::table(&["kind", "count"]);
        for (k, n) in &r.kinds {
            kt.add_row(vec![k.clone(), n.to_string()]);
        }
        crate::term::right_align(&mut kt, &[1]);
        println!("{kt}");
    }
    ExitCode::SUCCESS
}

pub fn render_why(w: &comindly_api::answers::Why) -> ExitCode {
    println!("{}", w.markdown);
    ExitCode::SUCCESS
}
