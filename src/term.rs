//! Terminal output: styled lines on stderr, tables, and an engine's job events as progress.

use std::time::Duration;

use comindly_api::events::{Event, Kind};
use console::style;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

/// Bold section header.
pub fn header(title: &str) {
    eprintln!("\n{}", style(title).bold().cyan());
}

/// Success line (`✓`).
pub fn ok(msg: &str) {
    eprintln!("{} {msg}", style("✓").green().bold());
}

/// Warning line (`!`).
pub fn warn(msg: &str) {
    eprintln!("{} {msg}", style("!").yellow().bold());
}

/// Error line (`✗`).
pub fn err(msg: &str) {
    eprintln!("{} {msg}", style("✗").red().bold());
}

/// Dim, indented detail line.
pub fn note(msg: &str) {
    eprintln!("  {}", style(msg).dim());
}

/// Dim `label:` followed by a value.
pub fn field(label: &str, value: &str) {
    eprintln!("  {} {value}", style(format!("{label}:")).dim());
}

/// A preset unicode table that wraps to the terminal width. Add rows, then print it.
pub fn table(headers: &[&str]) -> comfy_table::Table {
    let mut t = comfy_table::Table::new();
    t.load_style(comfy_table::presets::UTF8_FULL)
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

/// Prints a job's events as they arrive, with one progress bar per step at a terminal.
#[derive(Default)]
pub struct Events {
    bar: Option<(String, ProgressBar)>,
}

impl Events {
    pub fn show(&mut self, e: Event) {
        let m = &e.message;
        if e.kind != Kind::Progress {
            if let Some((_, pb)) = self.bar.take() {
                pb.finish_and_clear();
            }
        }
        match e.kind {
            Kind::Step => header(m),
            Kind::Done => ok(m),
            Kind::Note => note(m),
            Kind::Warning => warn(m),
            Kind::Error => err(m),
            Kind::Progress => {
                if !std::io::IsTerminal::is_terminal(&std::io::stderr()) {
                    return;
                }
                let total = e.total.unwrap_or(0);
                if self.bar.as_ref().is_none_or(|(s, _)| s != m) {
                    if let Some((_, old)) = self.bar.take() {
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
                    self.bar = Some((m.clone(), pb));
                }
                if let Some((_, pb)) = &self.bar {
                    pb.set_length(total);
                    pb.set_position(e.done.unwrap_or(0));
                    pb.set_message(m.clone());
                }
            }
        }
    }
}
