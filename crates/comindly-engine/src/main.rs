//! `comindly-engine`: serves an index for the `comindly` command line, the app and agents.

use std::path::Path;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// comindly's engine. With `COMINDLY_PUBLIC_URL` set it is a team server with sign-in; otherwise it
/// serves this machine on 127.0.0.1. `comindly use local` starts it for you.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Index folder or bucket location; defaults to the configured one
    #[arg(long)]
    index_dir: Option<String>,
    /// Address to listen on; 127.0.0.1 locally, 0.0.0.0 as a team server
    #[arg(long)]
    host: Option<String>,
    #[arg(long)]
    port: Option<u16>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// The app in its own window, for an engine at `url` (macOS)
    Window { url: String },
    /// Measure graph quality against a gold set
    Eval {
        #[arg(default_value = "tests/fixtures/org")]
        fixture: String,
        #[arg(long, short)]
        verbose: bool,
        /// Folder of SCIP indexes to answer references with
        #[arg(long)]
        scip: Option<String>,
        /// Only these fixture repositories
        #[arg(long)]
        repo: Vec<String>,
    },
}

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some(comindly_engine::indexer::COMMAND) {
        return match comindly_engine::indexer::child() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("index job: {e:#}");
                ExitCode::FAILURE
            }
        };
    }
    let args = Args::parse();
    let run = match args.cmd {
        Some(Cmd::Window { url }) => window(url),
        Some(Cmd::Eval {
            fixture,
            verbose,
            scip,
            repo,
        }) => return eval_graph(&fixture, verbose, scip.as_deref(), &repo),
        None => {
            let team =
                comindly_engine::config::env("COMINDLY_PUBLIC_URL").is_some_and(|u| !u.is_empty());
            let _telemetry = comindly_engine::telemetry::init(team);
            let host = args
                .host
                .unwrap_or_else(|| if team { "0.0.0.0" } else { "127.0.0.1" }.into());
            // Locally the usual port, else a free one: the command line reads where it went.
            let free = |p: u16| std::net::TcpListener::bind((host.as_str(), p)).is_ok();
            let port = args.port.unwrap_or(match team {
                true => 7878,
                false if free(7879) => 7879,
                false => 0,
            });
            comindly_engine::run::serve(args.index_dir.as_deref(), &host, port)
        }
    };
    match run {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            comindly_engine::ui::err(&comindly_engine::storage::explain(&format!("{e:#}")));
            ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "macos")]
fn window(url: String) -> anyhow::Result<()> {
    comindly_engine::app::run(comindly_engine::app::Open::Server(url))
}

#[cfg(not(target_os = "macos"))]
fn window(_url: String) -> anyhow::Result<()> {
    anyhow::bail!("the app window is macOS only; `comindly app` opens the browser elsewhere")
}

fn eval_graph(fixture: &str, verbose: bool, scip: Option<&str>, repos: &[String]) -> ExitCode {
    use comindly_engine::ui;
    let only: Vec<&str> = repos.iter().map(String::as_str).collect();
    let r = match comindly_engine::eval::eval_with(
        Path::new(fixture),
        (!only.is_empty()).then_some(only.as_slice()),
        scip.map(Path::new),
    ) {
        Ok(r) => r,
        Err(e) => {
            ui::err(&format!("eval failed: {e:#}"));
            return ExitCode::FAILURE;
        }
    };
    let pct = |x: f64| format!("{:.0}%", x * 100.0);
    let mut kinds = ui::table(&["kind", "found", "missed", "spurious", "precision", "recall"]);
    for (k, c) in r.by_kind.iter().chain([(&"total".to_string(), &r.total)]) {
        kinds.add_row(vec![
            k.clone(),
            c.tp.to_string(),
            c.fn_.to_string(),
            c.fp.to_string(),
            pct(c.precision()),
            pct(c.recall()),
        ]);
    }
    ui::header(&format!("Graph eval — {fixture}"));
    println!("{kinds}");
    let mut cases = ui::table(&["case", "found / expected"]);
    for (case, (found, expected)) in &r.by_case {
        cases.add_row(vec![case.clone(), format!("{found} / {expected}")]);
    }
    println!("{cases}");
    if scip.is_some() {
        ui::note(&format!("references answered by SCIP: {}", r.scip_answers));
    }
    if verbose {
        ui::header("Missed");
        for g in &r.missed {
            ui::note(&format!("[{}] {} {} -> {}", g.case, g.kind, g.src, g.dst));
        }
        ui::header("Spurious");
        for (k, s, d) in &r.spurious {
            ui::note(&format!("{k} {s} -> {d}"));
        }
    }
    ExitCode::SUCCESS
}
