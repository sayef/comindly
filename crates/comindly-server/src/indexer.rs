//! Index jobs run in a child `comindly index-job`, so the memory a run takes goes back when it ends.

use std::collections::BTreeMap;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use crate::context::{Cancelled, IndexContext, Settings};
use crate::events::{Event, EventSink, Kind, Level};

/// The hidden subcommand a server starts for each index job.
pub const COMMAND: &str = "index-job";

/// What the child indexes, sent on its stdin so no secret shows in its arguments or environment.
#[derive(Serialize, Deserialize)]
pub struct Spec {
    pub root: String,
    pub repos: Vec<crate::config::RepoEntry>,
    pub llm: bool,
    pub llm_key: Option<String>,
    /// Stored git tokens by host.
    pub git: BTreeMap<String, String>,
}

/// Run `spec` in a child process, its events going to `job`; cancelling `job` stops it.
pub async fn run(spec: &Spec, job: Arc<crate::jobs::Running>) -> Result<()> {
    let mut cmd = tokio::process::Command::new(std::env::current_exe()?);
    cmd.arg(COMMAND)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    let mut child = cmd.spawn().context("start the indexer")?;
    let pid = child.id();
    // Kept open until the end: the child stops when it closes.
    let mut stdin = child.stdin.take().context("indexer stdin")?;
    stdin.write_all(&serde_json::to_vec(spec)?).await?;
    stdin.write_all(b"\n").await?;
    let out = child.stdout.take().context("indexer stdout")?;
    let err = child.stderr.take().context("indexer stderr")?;
    let events = async {
        let mut failure = None;
        let mut lines = BufReader::new(out).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            match serde_json::from_str::<Event>(&l) {
                Ok(e) => {
                    if e.level == Level::Error {
                        failure = Some(e.message.clone());
                    }
                    job.emit(e);
                }
                Err(_) if !l.trim().is_empty() => job.emit(Event::new(Level::Info, Kind::Note, l)),
                Err(_) => {}
            }
        }
        failure
    };
    let logs = async {
        let mut lines = BufReader::new(err).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            crate::telemetry::write(&l);
        }
    };
    let exit = async {
        tokio::select! {
            s = child.wait() => s,
            _ = job.cancel.cancelled() => {
                signal(pid, Signal::Term);
                match tokio::time::timeout(std::time::Duration::from_secs(60), child.wait()).await {
                    Ok(s) => s,
                    Err(_) => {
                        signal(pid, Signal::Kill);
                        child.wait().await
                    }
                }
            }
        }
    };
    let (failure, (), status) = tokio::join!(events, logs, exit);
    drop(stdin);
    let status = status.context("wait for the indexer")?;
    if status.success() {
        return Ok(());
    }
    if job.cancel.is_cancelled() {
        return Err(Cancelled.into());
    }
    Err(anyhow::anyhow!(failure.unwrap_or_else(|| stopped(&status))))
}

fn stopped(status: &std::process::ExitStatus) -> String {
    #[cfg(unix)]
    if std::os::unix::process::ExitStatusExt::signal(status) == Some(9) {
        return "the indexer was killed, most likely for running out of memory".into();
    }
    format!("the indexer stopped ({status})")
}

enum Signal {
    Term,
    Kill,
}

/// Ask the child to stop between steps, or kill its whole group, git and SCIP indexers included.
fn signal(pid: Option<u32>, sig: Signal) {
    #[cfg(unix)]
    if let Some(pid) = pid.and_then(|p| i32::try_from(p).ok()) {
        let (to, sig) = match sig {
            Signal::Term => (pid, libc::SIGTERM),
            Signal::Kill => (-pid, libc::SIGKILL),
        };
        // SAFETY: kill(2) only sends a signal; a negative pid addresses the group.
        unsafe {
            libc::kill(to, sig);
        }
    }
    #[cfg(not(unix))]
    let _ = (pid, sig);
}

/// Events as JSON lines on stdout, for the server that started this process.
struct Stdout;

impl EventSink for Stdout {
    fn emit(&self, e: Event) {
        use std::io::Write;
        if let Ok(line) = serde_json::to_string(&e) {
            let mut out = std::io::stdout().lock();
            let _ = writeln!(out, "{line}");
            let _ = out.flush();
        }
    }
}

/// The child's side: read the job from stdin, index, and report on stdout.
pub fn child() -> Result<()> {
    crate::rt::shared().block_on(async {
        let mut stdin = BufReader::new(tokio::io::stdin());
        let mut line = String::new();
        stdin.read_line(&mut line).await?;
        let spec: Spec = serde_json::from_str(&line).context("read the index job")?;
        let team = crate::team::load(&spec.root)
            .await
            .map(|t| t.settings)
            .unwrap_or_default();
        let mut settings = Settings::resolve(&team, crate::config::env);
        if !spec.llm {
            settings.enrich = false;
            settings.flows = false;
            settings.guide = false;
        }
        let client = if settings.enrich || settings.flows || settings.guide {
            crate::llm::LlmClient::new(
                settings.llm_base_url.as_deref(),
                &settings.llm_model,
                spec.llm_key.as_deref(),
            )
            .ok()
        } else {
            None
        };
        let scip = settings.scip;
        let git = spec.git;
        let creds = move |host: &str| {
            crate::config::env_token(host).or_else(|| git.get(&host.to_lowercase()).cloned())
        };
        let ctx = IndexContext::new(
            spec.root,
            settings,
            Arc::new(Stdout),
            crate::config::cache_dir(),
        )
        .with_llm(client)
        .with_git(Arc::new(creds));
        let cancel = ctx.cancel.clone();
        tokio::spawn(async move {
            let mut rest = Vec::new();
            #[cfg(unix)]
            {
                use tokio::signal::unix::{signal, SignalKind};
                let mut term = signal(SignalKind::terminate()).ok();
                tokio::select! {
                    _ = async { match term.as_mut() { Some(t) => { t.recv().await; } None => std::future::pending().await } } => {}
                    _ = stdin.read_to_end(&mut rest) => {}
                }
            }
            #[cfg(not(unix))]
            let _ = stdin.read_to_end(&mut rest).await;
            cancel.cancel();
        });
        let run = async {
            let sources =
                crate::pipeline::checkout(&ctx, &crate::config::expanded(&spec.repos)).await?;
            crate::pipeline::link(&ctx, sources, true, scip).await
        };
        match run.await {
            Ok(_) => Ok(()),
            Err(e) if e.is::<Cancelled>() => {
                ctx.report.warn("Cancelled; what was already saved stays.");
                Err(e)
            }
            Err(e) => {
                ctx.report.error(crate::storage::explain(&format!("{e:#}")));
                Err(e)
            }
        }
    })
}
