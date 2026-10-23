//! Terminal UI helpers — styled status lines, spinners, and progress bars; with
//! `COMINDLY_LOG_FORMAT=json`, the same messages as one JSON event per line.
//!
//! Everything writes to **stderr** so `comindly serve`'s stdout stays pure JSON-RPC. Styling and
//! animation degrade automatically when stderr is not a TTY (pipes, CI), keeping logs clean.

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use console::style;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

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

/// A structured event as one JSON line: `level` (info, warn, error), `kind` (step, done, note,
/// warning, error, progress), the current step, the message and any `fields`.
pub fn event(level: &str, kind: &str, message: &str, fields: serde_json::Value) {
    let mut e = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "level": level,
        "kind": kind,
        "message": message,
    });
    let step = current_step().lock().map(|s| s.clone()).unwrap_or_default();
    if !step.is_empty() {
        e["step"] = step.into();
    }
    if let serde_json::Value::Object(f) = fields {
        for (k, v) in f {
            e[k] = v;
        }
    }
    eprintln!("{e}");
}

/// Bold section header.
pub fn header(title: &str) {
    if json_logs() {
        if let Ok(mut s) = current_step().lock() {
            *s = title.to_string();
        }
        return event("info", "step", title, serde_json::Value::Null);
    }
    eprintln!("\n{}", style(title).bold().cyan());
}

/// Success line (`✓`).
pub fn ok(msg: &str) {
    if json_logs() {
        return event("info", "done", msg, serde_json::Value::Null);
    }
    eprintln!("{} {msg}", style("✓").green().bold());
}

/// Warning line (`!`).
pub fn warn(msg: &str) {
    if json_logs() {
        return event("warn", "warning", msg, serde_json::Value::Null);
    }
    eprintln!("{} {msg}", style("!").yellow().bold());
}

/// Error line (`✗`).
pub fn err(msg: &str) {
    if json_logs() {
        return event("error", "error", msg, serde_json::Value::Null);
    }
    eprintln!("{} {msg}", style("✗").red().bold());
}

/// In-progress step (`→`).
pub fn step(msg: &str) {
    if json_logs() {
        return event("info", "note", msg, serde_json::Value::Null);
    }
    eprintln!("{} {msg}", style("→").cyan().bold());
}

/// Dim, indented detail line.
pub fn note(msg: &str) {
    if json_logs() {
        return event("info", "note", msg, serde_json::Value::Null);
    }
    eprintln!("  {}", style(msg).dim());
}

/// Dim `label:` followed by a value.
pub fn field(label: &str, value: &str) {
    if json_logs() {
        return event(
            "info",
            "note",
            &format!("{label}: {value}"),
            serde_json::Value::Null,
        );
    }
    eprintln!("  {} {value}", style(format!("{label}:")).dim());
}

/// Indeterminate spinner on stderr with a steady tick.
pub fn spinner(msg: &str) -> ProgressBar {
    if json_logs() {
        event("info", "note", msg, serde_json::Value::Null);
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
            if json_logs() {
                let mut f = serde_json::json!({"done": done, "total": total});
                if let Some((i, o, usd)) = *usage_now().lock().unwrap_or_else(|e| e.into_inner()) {
                    f["tokens_in"] = i.into();
                    f["tokens_out"] = o.into();
                    if let Some(u) = usd {
                        f["usd"] = ((u * 10_000.0).round() / 10_000.0).into();
                    }
                }
                event("info", "progress", &msg, f);
            } else {
                eprintln!("  {msg}: {done}/{total} · {}", p.message());
            }
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
        if json_logs() {
            if e.kind != Kind::Progress || self.due() {
                if let Ok(line) = serde_json::to_string(&e) {
                    eprintln!("{line}");
                }
            }
            return;
        }
        let usage = e.tokens_in.map(|i| {
            crate::events::Usage {
                tokens_in: i,
                tokens_out: e.tokens_out.unwrap_or(0),
                usd: e.usd,
            }
            .to_string()
        });
        match e.kind {
            Kind::Step => eprintln!("\n{}", style(&e.message).bold().cyan()),
            Kind::Done => eprintln!("{} {}", style("✓").green().bold(), e.message),
            Kind::Note => eprintln!("  {}", style(&e.message).dim()),
            Kind::Warning => eprintln!("{} {}", style("!").yellow().bold(), e.message),
            Kind::Error => eprintln!("{} {}", style("✗").red().bold(), e.message),
            Kind::Progress => {
                let (done, total) = (e.done.unwrap_or(0), e.total.unwrap_or(0));
                if !std::io::IsTerminal::is_terminal(&std::io::stderr()) {
                    if self.due() {
                        eprintln!(
                            "  {}: {done}/{total}{}",
                            e.message,
                            usage.map(|u| format!(" · {u}")).unwrap_or_default()
                        );
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
