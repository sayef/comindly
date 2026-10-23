//! Work the server runs in the background, indexing for now: jobs run one at a time in order, each
//! with a record, a status and its events kept in a [`JobStore`]; followers get them live.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::events::{Event, EventSink, Kind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    /// The process stopped while it ran; what was saved stays, and it can run again.
    Interrupted,
}

impl Status {
    pub fn finished(self) -> bool {
        !matches!(self, Status::Queued | Status::Running)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: String,
    pub kind: String,
    pub started_by: Option<String>,
    pub status: Status,
    pub created: String,
    #[serde(default)]
    pub started: Option<String>,
    pub finished: Option<String>,
    pub error: Option<String>,
    /// The request that started it, as in its `X-Request-Id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

/// Events kept per job.
pub const MAX_EVENTS: usize = 400;
/// Jobs kept in the store.
pub const MAX_JOBS: usize = 50;

/// Job records and their events.
pub trait JobStore: Send + Sync {
    fn save(&self, job: &JobRecord, events: &[Event]) -> Result<()>;
    fn get(&self, id: &str) -> Option<(JobRecord, Vec<Event>)>;
    /// Most recent first.
    fn list(&self, limit: usize) -> Vec<JobRecord>;
}

/// Jobs kept in memory, for the local app.
#[derive(Default)]
pub struct MemoryJobs {
    jobs: Mutex<Vec<(JobRecord, Vec<Event>)>>,
}

impl JobStore for MemoryJobs {
    fn save(&self, job: &JobRecord, events: &[Event]) -> Result<()> {
        let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        jobs.retain(|(j, _)| j.id != job.id);
        jobs.insert(0, (job.clone(), events.to_vec()));
        jobs.truncate(MAX_JOBS);
        Ok(())
    }

    fn get(&self, id: &str) -> Option<(JobRecord, Vec<Event>)> {
        let jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        jobs.iter().find(|(j, _)| j.id == id).cloned()
    }

    fn list(&self, limit: usize) -> Vec<JobRecord> {
        let jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        jobs.iter().take(limit).map(|(j, _)| j.clone()).collect()
    }
}

/// A job waiting or in progress: its record, its events so far and the stream of new ones.
pub struct Running {
    record: Mutex<JobRecord>,
    events: Mutex<Vec<Event>>,
    live: broadcast::Sender<Event>,
    pub cancel: CancellationToken,
    /// Set once the job has ended and its record is saved.
    done: CancellationToken,
    /// Where events also go: the terminal or the server's log.
    out: Arc<dyn EventSink>,
    /// The job's span, for its log lines and trace; closed when the job ends.
    span: Mutex<tracing::Span>,
}

impl Running {
    pub fn record(&self) -> JobRecord {
        self.record
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn events(&self) -> Vec<Event> {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Wait until the job has ended and its record is saved.
    pub async fn wait(&self) {
        self.done.cancelled().await
    }

    /// The events so far and the ones to come.
    pub fn follow(&self) -> (Vec<Event>, broadcast::Receiver<Event>) {
        let events = self.events.lock().unwrap_or_else(|e| e.into_inner());
        (events.clone(), self.live.subscribe())
    }
}

impl EventSink for Running {
    fn emit(&self, mut e: Event) {
        e.job_id = Some(self.record().id);
        let mut events = self.events.lock().unwrap_or_else(|p| p.into_inner());
        // Progress replaces the previous progress of the same step, so the record stays short.
        let replace = e.kind == Kind::Progress
            && events
                .last()
                .is_some_and(|l| l.kind == Kind::Progress && l.message == e.message);
        if replace {
            if let Some(last) = events.last_mut() {
                *last = e.clone();
            }
        } else {
            events.push(e.clone());
            let over = events.len().saturating_sub(MAX_EVENTS);
            events.drain(..over);
        }
        drop(events);
        let _ = self.live.send(e.clone());
        let span = self.span.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let _in = span.enter();
        self.out.emit(e);
    }
}

/// What a job runs, built when it starts.
type Work = Box<dyn FnOnce(Arc<Running>) -> BoxFuture<'static, Result<()>> + Send>;

/// A job waiting its turn; `key` says which requests are the same.
struct Waiting {
    job: Arc<Running>,
    key: String,
    work: Work,
}

#[derive(Default)]
struct Line {
    /// The job that runs, until the next one takes its place.
    current: Option<Arc<Running>>,
    queue: VecDeque<Waiting>,
    /// Set when the server stops: nothing new starts.
    closed: bool,
}

struct Inner {
    store: Arc<dyn JobStore>,
    out: Arc<dyn EventSink>,
    line: Mutex<Line>,
}

/// What [`Jobs::submit`] did with a request.
pub struct Submitted {
    pub job: Arc<Running>,
    /// Place in the queue, 1 for the next to run; `None` when it runs now.
    pub position: Option<usize>,
    /// An identical request was already waiting; this is that job.
    pub coalesced: bool,
}

/// Runs jobs one at a time in the order they came, and keeps their records.
pub struct Jobs {
    inner: Arc<Inner>,
}

impl Jobs {
    /// Jobs left running or queued by a stopped process are marked interrupted.
    pub fn new(store: Arc<dyn JobStore>, out: Arc<dyn EventSink>) -> Jobs {
        for job in store.list(MAX_JOBS) {
            if !job.status.finished() {
                if let Some((mut job, events)) = store.get(&job.id) {
                    if job.status == Status::Queued {
                        job.error = Some(NOT_RUN.into());
                    }
                    job.status = Status::Interrupted;
                    job.finished = Some(now());
                    let _ = store.save(&job, &events);
                }
            }
        }
        Jobs {
            inner: Arc::new(Inner {
                store,
                out,
                line: Mutex::default(),
            }),
        }
    }

    fn line(&self) -> std::sync::MutexGuard<'_, Line> {
        self.inner.line.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The running job, if any.
    pub fn running(&self) -> Option<Arc<Running>> {
        self.line()
            .current
            .clone()
            .filter(|r| !r.record().status.finished())
    }

    /// Waiting jobs, the next to run first.
    pub fn queued(&self) -> Vec<JobRecord> {
        self.line().queue.iter().map(|w| w.job.record()).collect()
    }

    /// Place of job `id` in the queue, 1 for the next to run.
    pub fn position(&self, id: &str) -> Option<usize> {
        self.line()
            .queue
            .iter()
            .position(|w| w.job.record().id == id)
            .map(|i| i + 1)
    }

    /// The most recent job that ran: the running one, else the last kept that is not waiting.
    pub fn latest(&self) -> Option<(JobRecord, Vec<Event>)> {
        if let Some(r) = self.line().current.clone() {
            return Some((r.record(), r.events()));
        }
        let id = self
            .inner
            .store
            .list(MAX_JOBS)
            .into_iter()
            .find(|j| j.status != Status::Queued)?
            .id;
        self.inner.store.get(&id)
    }

    pub fn get(&self, id: &str) -> Option<(JobRecord, Vec<Event>)> {
        match self.live(id) {
            Some(r) => Some((r.record(), r.events())),
            None => self.inner.store.get(id),
        }
    }

    /// The running or waiting job `id`, to follow its events.
    pub fn live(&self, id: &str) -> Option<Arc<Running>> {
        let line = self.line();
        line.current
            .iter()
            .chain(line.queue.iter().map(|w| &w.job))
            .find(|r| r.record().id == id && !r.record().status.finished())
            .cloned()
    }

    pub fn list(&self, limit: usize) -> Vec<JobRecord> {
        self.inner.store.list(limit)
    }

    /// Run `work` as a job of `kind` now, or queue it while another runs. A request with the
    /// same `kind` and `key` as a waiting one returns that job instead of queueing another.
    pub fn submit(
        &self,
        kind: &str,
        key: &str,
        started_by: Option<String>,
        request_id: Option<String>,
        work: impl FnOnce(Arc<Running>) -> BoxFuture<'static, Result<()>> + Send + 'static,
    ) -> Result<Submitted> {
        let mut line = self.line();
        anyhow::ensure!(!line.closed, "the server is stopping");
        if let Some(i) = line
            .queue
            .iter()
            .position(|w| w.key == key && w.job.record().kind == kind)
        {
            return Ok(Submitted {
                job: line.queue[i].job.clone(),
                position: Some(i + 1),
                coalesced: true,
            });
        }
        let busy = line.current.is_some();
        anyhow::ensure!(
            !busy || line.queue.len() < MAX_QUEUED,
            "{MAX_QUEUED} jobs are already waiting; try again when one has run"
        );
        let id = uuid::Uuid::new_v4().simple().to_string();
        // Its own trace, linked to the request that started it.
        let span = tracing::info_span!(
            parent: None,
            "job",
            otel.name = %format!("job {kind}"),
            job_id = %id,
            kind,
            request_id = request_id.as_deref(),
            status = tracing::field::Empty,
            otel.status_code = tracing::field::Empty,
        );
        span.follows_from(tracing::Span::current());
        let job = Arc::new(Running {
            record: Mutex::new(JobRecord {
                id,
                kind: kind.to_string(),
                started_by,
                status: if busy {
                    Status::Queued
                } else {
                    Status::Running
                },
                created: now(),
                started: (!busy).then(now),
                finished: None,
                error: None,
                request_id,
            }),
            events: Default::default(),
            live: broadcast::channel(256).0,
            cancel: CancellationToken::new(),
            done: CancellationToken::new(),
            out: self.inner.out.clone(),
            span: Mutex::new(span),
        });
        let work: Work = Box::new(work);
        let position = if busy {
            line.queue.push_back(Waiting {
                job: job.clone(),
                key: key.to_string(),
                work,
            });
            drop(line);
            save(&self.inner.store, &job);
            Some(self.position(&job.record().id).unwrap_or(1))
        } else {
            line.current = Some(job.clone());
            drop(line);
            launch(self.inner.clone(), job.clone(), work);
            None
        };
        Ok(Submitted {
            job,
            position,
            coalesced: false,
        })
    }

    /// Cancel job `id`: the running one stops at its next check, a waiting one leaves the queue.
    /// Returns the status it had, `None` when it is neither.
    pub fn cancel(&self, id: &str) -> Option<Status> {
        let mut line = self.line();
        if let Some(r) = line
            .current
            .clone()
            .filter(|r| r.record().id == id && !r.record().status.finished())
        {
            r.cancel.cancel();
            return Some(Status::Running);
        }
        let i = line.queue.iter().position(|w| w.job.record().id == id)?;
        let w = line.queue.remove(i)?;
        drop(line);
        end(&self.inner.store, &w.job, Status::Cancelled, None);
        Some(Status::Queued)
    }

    /// Stop starting jobs; the waiting ones are marked interrupted. Returns the running one.
    pub fn shutdown(&self) -> Option<Arc<Running>> {
        let waiting: Vec<Waiting> = {
            let mut line = self.line();
            line.closed = true;
            line.queue.drain(..).collect()
        };
        for w in waiting {
            end(
                &self.inner.store,
                &w.job,
                Status::Interrupted,
                Some(NOT_RUN.into()),
            );
        }
        self.running()
    }
}

/// Jobs that wait while one runs.
pub const MAX_QUEUED: usize = 5;

const NOT_RUN: &str = "the server stopped before this job ran; start it again";

/// Run `job` in the background; when it ends, the next waiting job starts.
fn launch(inner: Arc<Inner>, job: Arc<Running>, work: Work) {
    let span = job.span.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let run = span.in_scope(|| work(job.clone()));
    let task = async move {
        let s = inner.store.clone();
        save(&s, &job);
        let ticker = tokio::spawn({
            let (j, s) = (job.clone(), s.clone());
            async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                    let (j, s) = (j.clone(), s.clone());
                    let _ = tokio::task::spawn_blocking(move || save(&s, &j)).await;
                }
            }
        });
        let result = run.await;
        ticker.abort();
        {
            let mut r = job.record.lock().unwrap_or_else(|e| e.into_inner());
            r.finished = Some(now());
            r.status = match &result {
                Ok(()) => Status::Succeeded,
                Err(e) if e.is::<crate::context::Cancelled>() => Status::Cancelled,
                Err(e) => {
                    r.error = Some(crate::storage::explain(&format!("{e:#}")));
                    Status::Failed
                }
            };
        }
        close(&job);
        let saved = job.clone();
        let _ = tokio::task::spawn_blocking(move || save(&s, &saved)).await;
        job.done.cancel();
        next(inner);
    };
    tokio::spawn(tracing::Instrument::instrument(task, span));
}

/// Record how the job ended on its span and close it.
fn close(job: &Running) {
    let status = job.record().status;
    let span = std::mem::replace(
        &mut *job.span.lock().unwrap_or_else(|e| e.into_inner()),
        tracing::Span::none(),
    );
    span.record("status", format!("{status:?}").to_lowercase());
    if status == Status::Failed {
        span.record("otel.status_code", "ERROR");
    }
}

/// Start the next waiting job, if any.
fn next(inner: Arc<Inner>) {
    let mut line = inner.line.lock().unwrap_or_else(|e| e.into_inner());
    line.current = None;
    if line.closed {
        return;
    }
    let Some(w) = line.queue.pop_front() else {
        return;
    };
    {
        let mut r = w.job.record.lock().unwrap_or_else(|e| e.into_inner());
        r.status = Status::Running;
        r.started = Some(now());
    }
    line.current = Some(w.job.clone());
    drop(line);
    launch(inner.clone(), w.job, w.work);
}

/// Finish a job that never ran.
fn end(store: &Arc<dyn JobStore>, job: &Running, status: Status, error: Option<String>) {
    {
        let mut r = job.record.lock().unwrap_or_else(|e| e.into_inner());
        r.status = status;
        r.finished = Some(now());
        r.error = error;
    }
    close(job);
    save(store, job);
    job.done.cancel();
}

fn save(store: &Arc<dyn JobStore>, job: &Running) {
    if let Err(e) = store.save(&job.record(), &job.events()) {
        crate::ui::warn(&format!("keeping the job record: {e:#}"));
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jobs(store: Arc<MemoryJobs>) -> Jobs {
        Jobs::new(store, Arc::new(crate::events::Discard))
    }

    /// A job that waits for its cancel token, then logs `name` into `ran`.
    fn blocking(
        ran: Arc<Mutex<Vec<String>>>,
        name: &str,
    ) -> impl FnOnce(Arc<Running>) -> BoxFuture<'static, Result<()>> + Send + 'static {
        let name = name.to_string();
        move |j| {
            Box::pin(async move {
                ran.lock().unwrap().push(name);
                j.cancel.cancelled().await;
                Err(crate::context::Cancelled.into())
            })
        }
    }

    async fn until(f: impl Fn() -> bool) {
        for _ in 0..100 {
            if f() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("timed out");
    }

    fn status(jobs: &Jobs, id: &str) -> Status {
        jobs.get(id).unwrap().0.status
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn jobs_run_in_order_one_at_a_time() {
        let jobs = jobs(Default::default());
        let ran = Arc::new(Mutex::new(Vec::new()));
        let a = jobs
            .submit("index", "a", None, None, blocking(ran.clone(), "a"))
            .unwrap();
        let b = jobs
            .submit("index", "b", None, None, blocking(ran.clone(), "b"))
            .unwrap();
        let c = jobs
            .submit("index", "c", None, None, blocking(ran.clone(), "c"))
            .unwrap();
        assert_eq!(
            (a.position, b.position, c.position),
            (None, Some(1), Some(2))
        );
        let (a, b, c) = (a.job.record().id, b.job.record().id, c.job.record().id);
        assert_eq!(status(&jobs, &b), Status::Queued);
        assert_eq!(
            jobs.list(10)
                .iter()
                .filter(|j| j.status == Status::Queued)
                .count(),
            2
        );
        until(|| ran.lock().unwrap().len() == 1).await;
        jobs.cancel(&a);
        until(|| status(&jobs, &b) == Status::Running).await;
        assert_eq!(status(&jobs, &a), Status::Cancelled);
        assert_eq!(jobs.position(&c), Some(1));
        jobs.cancel(&b);
        until(|| status(&jobs, &c) == Status::Running).await;
        jobs.cancel(&c);
        until(|| jobs.running().is_none()).await;
        assert_eq!(*ran.lock().unwrap(), ["a", "b", "c"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn identical_waiting_requests_coalesce_and_the_queue_is_bounded() {
        let jobs = jobs(Default::default());
        let ran = Arc::new(Mutex::new(Vec::new()));
        let a = jobs
            .submit("index", "x", None, None, blocking(ran.clone(), "a"))
            .unwrap();
        // The running job does not absorb a new request: its input may already be stale.
        let b = jobs
            .submit("index", "x", None, None, blocking(ran.clone(), "b"))
            .unwrap();
        let again = jobs
            .submit("index", "x", None, None, blocking(ran.clone(), "b2"))
            .unwrap();
        assert!(!b.coalesced && again.coalesced);
        assert_eq!(again.job.record().id, b.job.record().id);
        for k in 1..MAX_QUEUED {
            jobs.submit(
                "index",
                &k.to_string(),
                None,
                None,
                blocking(ran.clone(), "k"),
            )
            .unwrap();
        }
        assert!(jobs
            .submit("index", "full", None, None, blocking(ran.clone(), "f"))
            .is_err());
        assert_eq!(jobs.queued().len(), MAX_QUEUED);
        jobs.shutdown();
        jobs.cancel(&a.job.record().id);
        until(|| jobs.running().is_none()).await;
        let b = jobs.get(&b.job.record().id).unwrap().0;
        assert_eq!(b.status, Status::Interrupted);
        assert_eq!(*ran.lock().unwrap(), ["a"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cancelling_a_queued_job_never_starts_it() {
        let jobs = jobs(Default::default());
        let ran = Arc::new(Mutex::new(Vec::new()));
        let a = jobs
            .submit("index", "a", None, None, blocking(ran.clone(), "a"))
            .unwrap();
        let b = jobs
            .submit("index", "b", None, None, blocking(ran.clone(), "b"))
            .unwrap();
        let (a, b) = (a.job.record().id, b.job.record().id);
        assert_eq!(jobs.cancel(&b), Some(Status::Queued));
        assert_eq!(status(&jobs, &b), Status::Cancelled);
        assert!(jobs.live(&b).is_none() && jobs.queued().is_empty());
        assert_eq!(jobs.cancel(&a), Some(Status::Running));
        until(|| jobs.running().is_none()).await;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(*ran.lock().unwrap(), ["a"]);
        assert_eq!(jobs.cancel(&a), None);
    }

    #[test]
    fn records_from_before_request_ids_still_read() {
        let old = r#"{"id":"a","kind":"index","started_by":null,"status":"succeeded",
            "created":"2026-01-01T00:00:00Z","finished":null,"error":null}"#;
        let rec: JobRecord = serde_json::from_str(old).unwrap();
        assert_eq!(rec.request_id, None);
        assert!(!serde_json::to_string(&rec).unwrap().contains("request_id"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_job_keeps_the_request_that_started_it() {
        let jobs = Jobs::new(
            Arc::new(MemoryJobs::default()),
            Arc::new(crate::events::Discard),
        );
        let s = jobs
            .submit("index", "k", None, Some("req-1".into()), |_| {
                Box::pin(async { Ok(()) })
            })
            .unwrap();
        s.job.wait().await;
        assert_eq!(s.job.record().request_id.as_deref(), Some("req-1"));
    }

    #[test]
    fn jobs_left_running_or_queued_are_interrupted() {
        let store = Arc::new(MemoryJobs::default());
        for (id, status) in [("a", Status::Running), ("b", Status::Queued)] {
            let rec = JobRecord {
                id: id.into(),
                kind: "index".into(),
                started_by: None,
                status,
                created: now(),
                started: None,
                finished: None,
                error: None,
                request_id: None,
            };
            store.save(&rec, &[]).unwrap();
        }
        let jobs = jobs(store);
        let (a, b) = (jobs.get("a").unwrap().0, jobs.get("b").unwrap().0);
        assert_eq!(
            (a.status, b.status),
            (Status::Interrupted, Status::Interrupted)
        );
        assert!(a.error.is_none() && b.error.as_deref() == Some(NOT_RUN));
        assert!(jobs.queued().is_empty() && jobs.running().is_none());
    }
}
