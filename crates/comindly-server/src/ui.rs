//! Terminal UI helpers — styled status lines, spinners, and progress bars; with
//! `COMINDLY_LOG_FORMAT=json`, the same messages as one JSON event per line.
//!
//! Everything writes to **stderr** so `comindly serve`'s stdout stays pure JSON-RPC. Styling and
//! animation degrade automatically when stderr is not a TTY (pipes, CI), keeping logs clean.

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use console::style;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use tracing::Level;

/// One JSON object per line instead of styled text, for platform logs (`COMINDLY_LOG_FORMAT=json`).
pub fn json_logs() -> bool {
    static JSON: OnceLock<bool> = OnceLock::new();
    *JSON.get_or_init(|| std::env::var("COMINDLY_LOG_FORMAT").is_ok_and(|v| v == "json"))
}

fn current_step() -> &'static Mutex<String> {
    static STEP: OnceLock<Mutex<String>> = OnceLock::new();
    STEP.get_or_init(Default::default)
}

/// LLM tokens and cost so far, attached to progress events.
pub fn usage(tokens_in: u64, tokens_out: u64, usd: Option<f64>) {
    *usage_now().lock().unwrap_or_else(|e| e.into_inner()) = Some((tokens_in, tokens_out, usd));
}

/// Tokens in, tokens out and their cost in USD, when prices are set.
type Usage = (u64, u64, Option<f64>);

fn usage_now() -> &'static Mutex<Option<Usage>> {
    static USAGE: OnceLock<Mutex<Option<Usage>>> = OnceLock::new();
    USAGE.get_or_init(Default::default)
}

/// The fields a log line may carry besides its level, kind and message.
#[derive(Default)]
struct Extra {
    step: Option<String>,
    job_id: Option<String>,
    done: Option<u64>,
    total: Option<u64>,
    tokens_in: Option<u64>,
    tokens_out: Option<u64>,
    usd: Option<f64>,
}

impl Extra {
    fn here() -> Self {
        let step = current_step().lock().map(|s| s.clone()).unwrap_or_default();
        Extra {
            step: (!step.is_empty()).then_some(step),
            ..Default::default()
        }
    }
}

/// One log line as a `tracing` event, rendered by [`crate::telemetry`]; `line` is its readable
/// form. Without a subscriber (tests, embedders) the line is written to stderr directly.
fn emit(level: Level, kind: &str, message: &str, x: Extra, line: impl FnOnce() -> String) {
    let line = (!json_logs()).then(line);
    let routed = tracing::dispatcher::get_default(|d| !d.is::<tracing::subscriber::NoSubscriber>());
    if !routed {
        let mut f = serde_json::Map::new();
        f.insert("kind".into(), kind.into());
        f.insert("message".into(), message.into());
        let opt = |f: &mut serde_json::Map<_, _>, k: &str, v: Option<serde_json::Value>| {
            if let Some(v) = v {
                f.insert(k.to_string(), v);
            }
        };
        opt(&mut f, "step", x.step.map(Into::into));
        opt(&mut f, "job_id", x.job_id.map(Into::into));
        opt(&mut f, "done", x.done.map(Into::into));
        opt(&mut f, "total", x.total.map(Into::into));
        opt(&mut f, "tokens_in", x.tokens_in.map(Into::into));
        opt(&mut f, "tokens_out", x.tokens_out.map(Into::into));
        opt(&mut f, "usd", x.usd.map(Into::into));
        opt(&mut f, "line", line.map(Into::into));
        eprintln!("{}", crate::telemetry::render(level, f, json_logs()));
        return;
    }
    macro_rules! at {
        ($l:expr) => {
            tracing::event!(
                $l,
                kind,
                step = x.step.as_deref(),
                job_id = x.job_id.as_deref(),
                done = x.done,
                total = x.total,
                tokens_in = x.tokens_in,
                tokens_out = x.tokens_out,
                usd = x.usd,
                line = line.as_deref(),
                "{message}"
            )
        };
    }
    match level {
        Level::ERROR => at!(Level::ERROR),
        Level::WARN => at!(Level::WARN),
        _ => at!(Level::INFO),
    }
}

/// Bold section header.
pub fn header(title: &str) {
    if json_logs() {
        if let Ok(mut s) = current_step().lock() {
            *s = title.to_string();
        }
    }
    emit(Level::INFO, "step", title, Extra::here(), || {
        format!("\n{}", style(title).bold().cyan())
    });
}

/// Success line (`✓`).
pub fn ok(msg: &str) {
    emit(Level::INFO, "done", msg, Extra::here(), || {
        format!("{} {msg}", style("✓").green().bold())
    });
}

/// Warning line (`!`).
pub fn warn(msg: &str) {
    emit(Level::WARN, "warning", msg, Extra::here(), || {
        format!("{} {msg}", style("!").yellow().bold())
    });
}

/// Error line (`✗`).
pub fn err(msg: &str) {
    emit(Level::ERROR, "error", msg, Extra::here(), || {
        format!("{} {msg}", style("✗").red().bold())
    });
}

/// In-progress step (`→`).
pub fn step(msg: &str) {
    emit(Level::INFO, "note", msg, Extra::here(), || {
        format!("{} {msg}", style("→").cyan().bold())
    });
}

/// Dim, indented detail line.
pub fn note(msg: &str) {
    emit(Level::INFO, "note", msg, Extra::here(), || {
        format!("  {}", style(msg).dim())
    });
}

/// Dim `label:` followed by a value.
pub fn field(label: &str, value: &str) {
    emit(
        Level::INFO,
        "note",
        &format!("{label}: {value}"),
        Extra::here(),
        || format!("  {} {value}", style(format!("{label}:")).dim()),
    );
}

/// Indeterminate spinner on stderr with a steady tick.
pub fn spinner(msg: &str) -> ProgressBar {
    if json_logs() {
        note(msg);
        return ProgressBar::hidden();
    }
    let pb = ProgressBar::with_draw_target(None, ProgressDrawTarget::stderr());
    pb.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg} {elapsed:.dim}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(90));
    pb
}

/// A preset unicode table that wraps to the terminal width. Add rows, then print it.
pub fn table(headers: &[&str]) -> comfy_table::Table {
    let mut t = comfy_table::Table::new();
    t.load_preset(comfy_table::presets::UTF8_FULL)
        .set_content_arrangement(comfy_table::ContentArrangement::Dynamic)
        .set_header(headers.to_vec());
    t
}

/// Right-align the given (0-based) columns — for numbers, which read best flush-right.
pub fn right_align(t: &mut comfy_table::Table, cols: &[usize]) {
    for &c in cols {
        if let Some(col) = t.column_mut(c) {
            col.set_cell_alignment(comfy_table::CellAlignment::Right);
        }
    }
}

/// Determinate `pos/len` progress bar on stderr.
pub fn progress(len: u64, msg: &str) -> ProgressBar {
    let target = if json_logs() {
        ProgressDrawTarget::hidden()
    } else {
        ProgressDrawTarget::stderr()
    };
    let pb = ProgressBar::with_draw_target(Some(len), target);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.cyan} [{elapsed_precise}] [{bar:32.cyan/blue}] {pos}/{len} {msg} {eta:.dim}",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("=>-"),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(90));
    // A log (the app, a container) gets progress every 30 s instead of a bar it cannot draw.
    if json_logs() || !std::io::IsTerminal::is_terminal(&std::io::stderr()) {
        let (weak, msg) = (pb.downgrade(), msg.to_string());
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(30));
            let Some(p) = weak.upgrade().filter(|p| !p.is_finished()) else {
                break;
            };
            let (done, total) = (p.position(), p.length().unwrap_or(0));
            let mut x = Extra {
                done: Some(done),
                total: Some(total),
                ..Extra::here()
            };
            if let Some((i, o, usd)) = *usage_now().lock().unwrap_or_else(|e| e.into_inner()) {
                x.tokens_in = Some(i);
                x.tokens_out = Some(o);
                x.usd = usd.map(|u| (u * 10_000.0).round() / 10_000.0);
            }
            emit(Level::INFO, "progress", &msg, x, || {
                format!("  {msg}: {done}/{total} · {}", p.message())
            });
        });
    }
    pb
}

/// Renders core's events: styled lines and progress bars at a terminal, one JSON object per line
/// with `COMINDLY_LOG_FORMAT=json`, plain progress lines every 30 s otherwise.
pub struct Terminal {
    bar: Mutex<Option<(String, ProgressBar)>>,
    last_progress: Mutex<Option<std::time::Instant>>,
}

impl Terminal {
    pub fn new() -> Self {
        Terminal {
            bar: Mutex::new(None),
            last_progress: Mutex::new(None),
        }
    }

    fn finish_bar(&self) {
        if let Some((_, pb)) = self.bar.lock().ok().and_then(|mut b| b.take()) {
            pb.finish_and_clear();
        }
    }

    fn due(&self) -> bool {
        let mut last = self.last_progress.lock().unwrap_or_else(|e| e.into_inner());
        let now = std::time::Instant::now();
        let due = last.is_none_or(|t| now.duration_since(t) >= Duration::from_secs(30));
        if due {
            *last = Some(now);
        }
        due
    }
}

impl Default for Terminal {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::events::EventSink for Terminal {
    fn emit(&self, e: crate::events::Event) {
        use crate::events::Kind;
        if e.kind != Kind::Progress {
            self.finish_bar();
        }
        let usage = e.tokens_in.map(|i| {
            crate::events::Usage {
                tokens_in: i,
                tokens_out: e.tokens_out.unwrap_or(0),
                usd: e.usd,
            }
            .to_string()
        });
        let level = match e.level {
            crate::events::Level::Info => Level::INFO,
            crate::events::Level::Warn => Level::WARN,
            crate::events::Level::Error => Level::ERROR,
        };
        let kind = serde_json::to_value(e.kind)
            .ok()
            .and_then(|k| k.as_str().map(str::to_string))
            .unwrap_or_default();
        let x = Extra {
            step: e.step.clone(),
            job_id: e.job_id.clone(),
            done: e.done,
            total: e.total,
            tokens_in: e.tokens_in,
            tokens_out: e.tokens_out,
            usd: e.usd,
        };
        let m = &e.message;
        let tty = std::io::IsTerminal::is_terminal(&std::io::stderr());
        match e.kind {
            Kind::Step => emit(level, &kind, m, x, || {
                format!("\n{}", style(m).bold().cyan())
            }),
            Kind::Done => emit(level, &kind, m, x, || {
                format!("{} {m}", style("✓").green().bold())
            }),
            Kind::Note => emit(level, &kind, m, x, || format!("  {}", style(m).dim())),
            Kind::Warning => emit(level, &kind, m, x, || {
                format!("{} {m}", style("!").yellow().bold())
            }),
            Kind::Error => emit(level, &kind, m, x, || {
                format!("{} {m}", style("✗").red().bold())
            }),
            Kind::Progress => {
                let (done, total) = (e.done.unwrap_or(0), e.total.unwrap_or(0));
                if json_logs() || !tty {
                    if self.due() {
                        emit(level, &kind, m, x, || {
                            format!(
                                "  {m}: {done}/{total}{}",
                                usage.map(|u| format!(" · {u}")).unwrap_or_default()
                            )
                        });
                    }
                    return;
                }
                let mut bar = self.bar.lock().unwrap_or_else(|p| p.into_inner());
                if bar.as_ref().is_none_or(|(m, _)| *m != e.message) {
                    if let Some((_, old)) = bar.take() {
                        old.finish_and_clear();
                    }
                    let pb =
                        ProgressBar::with_draw_target(Some(total), ProgressDrawTarget::stderr());
                    pb.set_style(
                        ProgressStyle::with_template(
                            "{spinner:.cyan} [{elapsed_precise}] [{bar:32.cyan/blue}] {pos}/{len} {msg} {eta:.dim}",
                        )
                        .unwrap_or_else(|_| ProgressStyle::default_bar())
                        .progress_chars("=>-"),
                    );
                    pb.enable_steady_tick(Duration::from_millis(90));
                    *bar = Some((e.message.clone(), pb));
                }
                if let Some((_, pb)) = bar.as_ref() {
                    pb.set_length(total);
                    pb.set_position(done);
                    pb.set_message(usage.unwrap_or_else(|| e.message.clone()));
                }
            }
        }
    }
}
