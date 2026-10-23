//! The process's logs and traces: `tracing` events rendered as readable lines or, with
//! `COMINDLY_LOG_FORMAT=json`, one JSON object per line; spans sent over OTLP when configured.

use std::fmt::Debug;
use std::sync::Arc;

use console::style;
use serde_json::{Map, Value};
use tracing::field::{Field, Visit};
use tracing::{span, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;

/// Set up logging (and OTLP export when `OTEL_EXPORTER_OTLP_ENDPOINT` is set) for this process.
/// Keep the guard until the process ends; dropping it sends the spans still queued.
pub fn init() -> Guard {
    let level = match std::env::var("COMINDLY_LOG_LEVEL").as_deref() {
        Ok("error") => Level::ERROR,
        Ok("warn") => Level::WARN,
        Ok("debug") => Level::DEBUG,
        Ok("trace") => Level::TRACE,
        _ => Level::INFO,
    };
    let lines =
        Lines::stderr(crate::ui::json_logs()).with_filter(tracing_subscriber::filter::filter_fn(
            move |m| ours(m.target()) && (m.is_span() || *m.level() <= level),
        ));
    let registry = tracing_subscriber::registry().with(lines);
    #[cfg(feature = "otel")]
    let registry = registry.with(otel::layer());
    let _ = tracing::subscriber::set_global_default(registry);
    Guard(())
}

/// Sends what is queued for export; for when the process is about to exit.
pub fn flush() {
    #[cfg(feature = "otel")]
    otel::flush();
}

pub struct Guard(());

impl Drop for Guard {
    fn drop(&mut self) {
        #[cfg(feature = "otel")]
        otel::shutdown();
    }
}

fn ours(target: &str) -> bool {
    target.starts_with("comindly")
}

/// What a span tells the lines logged inside it.
#[derive(Default)]
struct Scope {
    request_id: Option<String>,
    job_id: Option<String>,
}

impl Visit for Scope {
    fn record_str(&mut self, f: &Field, v: &str) {
        match f.name() {
            "request_id" => self.request_id = Some(v.to_string()),
            "job_id" => self.job_id = Some(v.to_string()),
            _ => {}
        }
    }

    fn record_debug(&mut self, f: &Field, v: &dyn Debug) {
        self.record_str(f, &format!("{v:?}"))
    }
}

struct Fields(Map<String, Value>);

impl Visit for Fields {
    fn record_str(&mut self, f: &Field, v: &str) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_u64(&mut self, f: &Field, v: u64) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_i64(&mut self, f: &Field, v: i64) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_f64(&mut self, f: &Field, v: f64) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_bool(&mut self, f: &Field, v: bool) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_debug(&mut self, f: &Field, v: &dyn Debug) {
        self.0.insert(f.name().into(), format!("{v:?}").into());
    }
}

/// Renders events as comindly's log lines, with the request and job they happened in.
pub struct Lines {
    json: bool,
    out: Arc<dyn Fn(&str) + Send + Sync>,
}

impl Lines {
    pub fn stderr(json: bool) -> Self {
        Lines {
            json,
            out: Arc::new(|l| eprintln!("{l}")),
        }
    }

    /// Lines handed to `out` instead of stderr.
    pub fn to(json: bool, out: impl Fn(&str) + Send + Sync + 'static) -> Self {
        Lines {
            json,
            out: Arc::new(out),
        }
    }
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Lines {
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let mut scope = Scope::default();
        attrs.record(&mut scope);
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(scope);
        }
    }

    fn on_record(&self, id: &span::Id, values: &span::Record<'_>, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            if let Some(scope) = span.extensions_mut().get_mut::<Scope>() {
                values.record(scope);
            }
        }
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        let mut fields = Fields(Map::new());
        event.record(&mut fields);
        let mut fields = fields.0;
        for span in ctx.event_scope(event).into_iter().flatten() {
            if let Some(s) = span.extensions().get::<Scope>() {
                for (k, v) in [("request_id", &s.request_id), ("job_id", &s.job_id)] {
                    if let Some(v) = v {
                        fields.entry(k).or_insert_with(|| v.clone().into());
                    }
                }
            }
        }
        (self.out)(&render(*event.metadata().level(), fields, self.json));
    }
}

/// One log line: a JSON object (`timestamp`, `level`, `kind`, `message`, then the other fields),
/// or the readable `line` when given, else one styled by `kind`.
pub(crate) fn render(level: Level, mut f: Map<String, Value>, json: bool) -> String {
    let readable = f.shift_remove("line");
    let text = |v: Option<Value>| match v {
        Some(Value::String(s)) => s,
        Some(v) => v.to_string(),
        None => String::new(),
    };
    let message = text(f.shift_remove("message"));
    let kind = match f.shift_remove("kind") {
        Some(k) => text(Some(k)),
        None if level == Level::ERROR => "error".into(),
        None if level == Level::WARN => "warning".into(),
        None => "note".into(),
    };
    if json {
        let mut out = Map::new();
        out.insert(
            "timestamp".into(),
            chrono::Utc::now()
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
                .into(),
        );
        out.insert("level".into(), level_name(level).into());
        out.insert("kind".into(), kind.into());
        out.insert("message".into(), message.into());
        for k in ["step", "job_id", "request_id"] {
            if let Some(v) = f.shift_remove(k) {
                out.insert(k.into(), v);
            }
        }
        out.extend(f);
        return Value::Object(out).to_string();
    }
    let mut line = match readable {
        Some(r) => text(Some(r)),
        None => match kind.as_str() {
            "step" => format!("\n{}", style(&message).bold().cyan()),
            "done" => format!("{} {message}", style("✓").green().bold()),
            "warning" => format!("{} {message}", style("!").yellow().bold()),
            "error" => format!("{} {message}", style("✗").red().bold()),
            "access" => format!("{} {message}", style("·").dim()),
            _ => format!("  {}", style(&message).dim()),
        },
    };
    let tagged = level <= Level::WARN || kind == "access";
    if let Some(id) = f
        .get("request_id")
        .and_then(Value::as_str)
        .filter(|_| tagged)
    {
        let who = f.get("email").and_then(Value::as_str);
        let suffix = who.map_or(format!("request {id}"), |w| format!("{w} · request {id}"));
        line.push_str(&format!(" {}", style(format!("({suffix})")).dim()));
    }
    line
}

fn level_name(l: Level) -> &'static str {
    match l {
        Level::ERROR => "error",
        Level::WARN => "warn",
        Level::INFO => "info",
        Level::DEBUG => "debug",
        Level::TRACE => "trace",
    }
}

#[cfg(feature = "otel")]
mod otel {
    use std::sync::OnceLock;

    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use tracing::Subscriber;
    use tracing_subscriber::registry::LookupSpan;
    use tracing_subscriber::Layer;

    static PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

    /// Spans of comindly's own requests and jobs, to the OTLP/HTTP endpoint in the environment.
    pub(super) fn layer<S>() -> Option<impl Layer<S>>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        let set = |k: &str| std::env::var(k).is_ok_and(|v| !v.trim().is_empty());
        if !set("OTEL_EXPORTER_OTLP_ENDPOINT") && !set("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT") {
            return None;
        }
        let exporter = match opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .build()
        {
            Ok(e) => e,
            Err(e) => {
                crate::ui::warn(&format!("traces will not be exported: {e}"));
                return None;
            }
        };
        let mut resource = opentelemetry_sdk::Resource::builder().with_attribute(
            opentelemetry::KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
        );
        if !set("OTEL_SERVICE_NAME") {
            resource = resource.with_service_name("comindly");
        }
        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource.build())
            .build();
        let tracer = provider.tracer("comindly");
        let _ = PROVIDER.set(provider);
        let only_ours = tracing_subscriber::filter::filter_fn(|m| {
            super::ours(m.target()) && *m.level() <= tracing::Level::INFO
        });
        Some(
            tracing_opentelemetry::layer()
                .with_tracer(tracer)
                .with_filter(only_ours),
        )
    }

    pub(super) fn flush() {
        if let Some(p) = PROVIDER.get() {
            let _ = p.force_flush();
        }
    }

    pub(super) fn shutdown() {
        if let Some(p) = PROVIDER.get() {
            let _ = p.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_lines_keep_their_field_names() {
        let mut f = Map::new();
        f.insert("message".into(), "hello".into());
        f.insert("kind".into(), "done".into());
        f.insert("job_id".into(), "j1".into());
        f.insert("request_id".into(), "r1".into());
        let v: Value = serde_json::from_str(&render(Level::INFO, f, true)).unwrap();
        assert_eq!(v["level"], "info");
        assert_eq!(v["kind"], "done");
        assert_eq!(v["message"], "hello");
        assert_eq!(v["job_id"], "j1");
        assert_eq!(v["request_id"], "r1");
        assert!(v["timestamp"].is_string());
    }
}
