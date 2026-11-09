//! What core reports while it works: steps, results, warnings and progress, as structured events.
//! The CLI renders them in the terminal; a server keeps them with the job, streams them to the app
//! and writes them to its log.

use std::sync::Arc;

pub use comindly_api::events::{Event, Kind, Level};

/// Where events go. Implementations decide how often to show progress.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: Event);
}

/// Events dropped, for callers that do not need them.
pub struct Discard;

impl EventSink for Discard {
    fn emit(&self, _: Event) {}
}

/// Sends events to a sink, tagging each with the step in progress.
#[derive(Clone)]
pub struct Reporter {
    sink: Arc<dyn EventSink>,
    step: Arc<std::sync::Mutex<Option<String>>>,
}

impl Reporter {
    pub fn new(sink: Arc<dyn EventSink>) -> Self {
        Reporter {
            sink,
            step: Default::default(),
        }
    }

    fn send(&self, mut e: Event) {
        e.step = self.step.lock().ok().and_then(|s| s.clone());
        self.sink.emit(e);
    }

    pub fn step(&self, name: impl Into<String>) {
        let name = name.into();
        if let Ok(mut s) = self.step.lock() {
            *s = Some(name.clone());
        }
        self.send(Event::new(Level::Info, Kind::Step, name));
    }

    pub fn done(&self, message: impl Into<String>) {
        self.send(Event::new(Level::Info, Kind::Done, message));
    }

    pub fn note(&self, message: impl Into<String>) {
        self.send(Event::new(Level::Info, Kind::Note, message));
    }

    pub fn warn(&self, message: impl Into<String>) {
        self.send(Event::new(Level::Warn, Kind::Warning, message));
    }

    pub fn error(&self, message: impl Into<String>) {
        self.send(Event::new(Level::Error, Kind::Error, message));
    }

    /// `done` of `total` for the step in progress, with LLM `usage` so far when there is any.
    pub fn progress(&self, message: &str, done: u64, total: u64, usage: Option<Usage>) {
        let mut e = Event::new(Level::Info, Kind::Progress, message);
        e.done = Some(done);
        e.total = Some(total);
        if let Some(u) = usage {
            e.tokens_in = Some(u.tokens_in);
            e.tokens_out = Some(u.tokens_out);
            e.usd = u.usd;
        }
        self.send(e);
    }
}

/// LLM tokens so far and, when prices are set, their cost.
#[derive(Debug, Clone, Copy, Default)]
pub struct Usage {
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub usd: Option<f64>,
}

impl Usage {
    pub fn priced(tokens_in: u64, tokens_out: u64, prices: Option<(f64, f64)>) -> Self {
        Usage {
            tokens_in,
            tokens_out,
            usd: prices.map(|(pi, po)| (tokens_in as f64 * pi + tokens_out as f64 * po) / 1e6),
        }
    }
}

impl std::fmt::Display for Usage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let k = |n: u64| {
            if n >= 1000 {
                format!("{:.1}k", n as f64 / 1000.0)
            } else {
                n.to_string()
            }
        };
        write!(
            f,
            "{} in / {} out tokens",
            k(self.tokens_in),
            k(self.tokens_out)
        )?;
        match self.usd {
            Some(u) if u < 0.01 => write!(f, " ≈ < $0.01"),
            Some(u) => write!(f, " ≈ ${u:.2}"),
            None => Ok(()),
        }
    }
}
