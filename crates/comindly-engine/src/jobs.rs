//! Work the server runs in the background, indexing for now: jobs run one at a time in order, each
//! with a record, a status and its events kept in a [`JobStore`]; followers get them live.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use futures::future::BoxFuture;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::events::{Event, EventSink, Kind};

pub use comindly_api::jobs::{JobRecord, Status};

/// A running job without a sign of life for this long belongs to a server that is gone.
const STALE_SECONDS: i64 = 60;

/// Running, and its server wrote a sign of life recently.
fn alive(job: &JobRecord) -> bool {
    job.status == Status::Running
        && job
            .beat
            .as_deref()
            .and_then(|b| chrono::DateTime::parse_from_rfc3339(b).ok())
            .is_some_and(|b| {
                chrono::Utc::now() - b.with_timezone(&chrono::Utc)
                    < chrono::Duration::seconds(STALE_SECONDS)
            })
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
    /// Jobs a stopped server left are paused when they can continue, else interrupted; while
    /// another server still runs one (a deploy), they are its to pause.
    pub fn new(store: Arc<dyn JobStore>, out: Arc<dyn EventSink>) -> Jobs {
        let left: Vec<JobRecord> = store
            .list(MAX_JOBS)
            .into_iter()
            .filter(|j| matches!(j.status, Status::Queued | Status::Running))
            .collect();
        if !left.iter().any(alive) {
            for job in left {
                if let Some((mut job, events)) = store.get(&job.id) {
                    if job.request.is_some() {
                        job.status = Status::Paused;
                        job.beat = None;
                    } else {
                        if job.status == Status::Queued {
                            job.error = Some(NOT_RUN.into());
                        }
                        job.status = Status::Interrupted;
                        job.finished = Some(now());
                    }
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
        self.live_in(&self.line(), id)
    }

    fn live_in(&self, line: &Line, id: &str) -> Option<Arc<Running>> {
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
        self.submit_resumable(kind, key, started_by, request_id, None, work)
    }

    /// [`Jobs::submit`], keeping `request` so a restart can continue the job.
    pub fn submit_resumable(
        &self,
        kind: &str,
        key: &str,
        started_by: Option<String>,
        request_id: Option<String>,
        request: Option<serde_json::Value>,
        work: impl FnOnce(Arc<Running>) -> BoxFuture<'static, Result<()>> + Send + 'static,
    ) -> Result<Submitted> {
        let line = self.line();
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
        anyhow::ensure!(
            line.current.is_none() || line.queue.len() < MAX_QUEUED,
            "{MAX_QUEUED} jobs are already waiting; try again when one has run"
        );
        let record = JobRecord {
            id: uuid::Uuid::new_v4().simple().to_string(),
            kind: kind.to_string(),
            started_by,
            status: Status::Queued,
            created: now(),
            started: None,
            finished: None,
            error: None,
            request_id,
            request,
            beat: None,
        };
        Ok(self.add(line, key, record, Vec::new(), Box::new(work)))
    }

    /// Continue `job`, which a restart paused, under its own id: it runs now or waits its turn.
    pub fn resume(
        &self,
        key: &str,
        (mut job, mut events): (JobRecord, Vec<Event>),
        work: impl FnOnce(Arc<Running>) -> BoxFuture<'static, Result<()>> + Send + 'static,
    ) -> Result<Submitted> {
        let line = self.line();
        anyhow::ensure!(!line.closed, "the server is stopping");
        anyhow::ensure!(
            self.live_in(&line, &job.id).is_none(),
            "job {} already runs here",
            job.id
        );
        (job.finished, job.error, job.beat) = (None, None, None);
        events.push(Event::new(
            crate::events::Level::Info,
            Kind::Note,
            "Continues after a restart",
        ));
        Ok(self.add(line, key, job, events, Box::new(work)))
    }

    /// Jobs a restart paused, and running ones whose server went quiet, oldest first: the ones
    /// this server should continue.
    pub fn resumable(&self) -> Vec<(JobRecord, Vec<Event>)> {
        let line = self.line();
        if line.closed {
            return Vec::new();
        }
        let mut jobs: Vec<JobRecord> = self
            .inner
            .store
            .list(MAX_JOBS)
            .into_iter()
            .filter(|j| j.request.is_some() && self.live_in(&line, &j.id).is_none())
            .filter(|j| j.status == Status::Paused || (j.status == Status::Running && !alive(j)))
            .collect();
        drop(line);
        jobs.sort_by(|a, b| a.created.cmp(&b.created));
        jobs.into_iter()
            .filter_map(|j| self.inner.store.get(&j.id))
            .collect()
    }

    /// Queue `record` or start it now; the line is held so nothing slips in between.
    fn add(
        &self,
        mut line: std::sync::MutexGuard<'_, Line>,
        key: &str,
        mut record: JobRecord,
        events: Vec<Event>,
        work: Work,
    ) -> Submitted {
        let busy = line.current.is_some();
        if !busy {
            record.status = Status::Running;
            record.started = Some(now());
        } else {
            record.status = Status::Queued;
        }
        let (id, kind, request_id) = (
            record.id.clone(),
            record.kind.clone(),
            record.request_id.clone(),
        );
        let kind = kind.as_str();
        let request_id = request_id.as_deref();
        // Its own trace, linked to the request that started it.
        let span = tracing::info_span!(
            parent: None,
            "job",
            otel.name = %format!("job {kind}"),
            job_id = %id,
            kind,
            request_id,
            status = tracing::field::Empty,
            otel.status_code = tracing::field::Empty,
        );
        span.follows_from(tracing::Span::current());
        let job = Arc::new(Running {
            record: Mutex::new(record),
            events: Mutex::new(events),
            live: broadcast::channel(256).0,
            cancel: CancellationToken::new(),
            done: CancellationToken::new(),
            out: self.inner.out.clone(),
            span: Mutex::new(span),
        });
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
        Submitted {
            job,
            position,
            coalesced: false,
        }
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

    /// Stop starting jobs; the waiting ones are paused, or interrupted when they cannot continue.
    /// Returns the running one, which pauses too when it stops at its next check.
    pub fn shutdown(&self) -> Option<Arc<Running>> {
        let waiting: Vec<Waiting> = {
            let mut line = self.line();
            line.closed = true;
            line.queue.drain(..).collect()
        };
        for w in waiting {
            if w.job.record().request.is_some() {
                pause(&self.inner.store, &w.job);
            } else {
                end(
                    &self.inner.store,
                    &w.job,
                    Status::Interrupted,
                    Some(NOT_RUN.into()),
                );
            }
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
        beat(&job);
        save(&s, &job);
        let ticker = tokio::spawn({
            let (j, s) = (job.clone(), s.clone());
            async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                    beat(&j);
                    let (j, s) = (j.clone(), s.clone());
                    let _ = tokio::task::spawn_blocking(move || save(&s, &j)).await;
                }
            }
        });
        let result = run.await;
        ticker.abort();
        let stopping = inner.line.lock().unwrap_or_else(|e| e.into_inner()).closed;
        let paused = {
            let mut r = job.record.lock().unwrap_or_else(|e| e.into_inner());
            r.finished = Some(now());
            r.status = match &result {
                Ok(()) => Status::Succeeded,
                // Stopped by the server going down: it continues after the restart.
                Err(e)
                    if e.is::<crate::context::Cancelled>() && stopping && r.request.is_some() =>
                {
                    (r.finished, r.beat) = (None, None);
                    Status::Paused
                }
                Err(e) if e.is::<crate::context::Cancelled>() => Status::Cancelled,
                Err(e) => {
                    r.error = Some(crate::storage::explain(&format!("{e:#}")));
                    Status::Failed
                }
            };
            r.status == Status::Paused
        };
        if paused {
            job.emit(Event::new(crate::events::Level::Info, Kind::Note, PAUSED));
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

/// Pause a waiting job until a server is back.
fn pause(store: &Arc<dyn JobStore>, job: &Running) {
    {
        let mut r = job.record.lock().unwrap_or_else(|e| e.into_inner());
        (r.status, r.beat) = (Status::Paused, None);
    }
    job.emit(Event::new(crate::events::Level::Info, Kind::Note, PAUSED));
    close(job);
    save(store, job);
    job.done.cancel();
}

const PAUSED: &str = "Paused: the server is restarting; this continues once it is back";

fn beat(job: &Running) {
    job.record.lock().unwrap_or_else(|e| e.into_inner()).beat = Some(now());
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
                request: None,
                beat: None,
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

    fn left(id: &str, status: Status, beat: Option<String>) -> JobRecord {
        JobRecord {
            id: id.into(),
            kind: "index".into(),
            started_by: None,
            status,
            created: format!("2026-01-01T00:00:0{}Z", id.len()),
            started: None,
            finished: None,
            error: None,
            request_id: None,
            request: Some(serde_json::json!({ "llm": true })),
            beat,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_restart_pauses_jobs_that_can_continue_and_the_next_server_resumes_them() {
        let store = Arc::new(MemoryJobs::default());
        let jobs = jobs(store.clone());
        let ran = Arc::new(Mutex::new(Vec::new()));
        let request = Some(serde_json::json!({ "llm": true }));
        let a = jobs
            .submit_resumable(
                "index",
                "a",
                None,
                None,
                request.clone(),
                blocking(ran.clone(), "a"),
            )
            .unwrap();
        let b = jobs
            .submit_resumable(
                "index",
                "b",
                None,
                None,
                request,
                blocking(ran.clone(), "b"),
            )
            .unwrap();
        let plain = jobs
            .submit("index", "c", None, None, blocking(ran.clone(), "c"))
            .unwrap();
        until(|| ran.lock().unwrap().len() == 1).await;
        let (a, b, plain) = (a.job.record().id, b.job.record().id, plain.job.record().id);
        if let Some(r) = jobs.shutdown() {
            r.cancel.cancel();
            r.wait().await;
        }
        assert_eq!(
            (status(&jobs, &a), status(&jobs, &b), status(&jobs, &plain)),
            (Status::Paused, Status::Paused, Status::Interrupted)
        );
        assert!(jobs.get(&a).unwrap().1.iter().any(|e| e.message == PAUSED));
        assert!(
            jobs.resumable().is_empty(),
            "a stopping server continues nothing"
        );

        let next = Jobs::new(store, Arc::new(crate::events::Discard));
        let waiting: Vec<String> = next.resumable().iter().map(|(j, _)| j.id.clone()).collect();
        assert_eq!(waiting, [a.clone(), b.clone()]);
        let again = Arc::new(Mutex::new(Vec::new()));
        for job in next.resumable() {
            let name = job.0.id.clone();
            next.resume("x", job, blocking(again.clone(), &name))
                .unwrap();
        }
        assert!(
            next.resumable().is_empty(),
            "a resumed job is not offered twice"
        );
        until(|| again.lock().unwrap().len() == 1).await;
        assert_eq!(status(&next, &a), Status::Running);
        assert_eq!(status(&next, &b), Status::Queued);
        assert!(next
            .get(&a)
            .unwrap()
            .1
            .iter()
            .any(|e| e.message == "Continues after a restart"));
        next.cancel(&a);
        until(|| status(&next, &b) == Status::Running).await;
        next.cancel(&b);
        until(|| next.running().is_none()).await;
        assert_eq!(*again.lock().unwrap(), [a, b]);
    }

    #[test]
    fn a_starting_server_leaves_another_servers_live_job_alone() {
        let store = Arc::new(MemoryJobs::default());
        store
            .save(&left("a", Status::Running, Some(now())), &[])
            .unwrap();
        store.save(&left("bb", Status::Queued, None), &[]).unwrap();
        let jobs = jobs(store);
        assert_eq!(
            (status(&jobs, "a"), status(&jobs, "bb")),
            (Status::Running, Status::Queued)
        );
        assert!(jobs.resumable().is_empty(), "its server is still alive");
    }

    #[test]
    fn jobs_of_a_server_that_went_quiet_are_paused_and_offered() {
        let store = Arc::new(MemoryJobs::default());
        let old = (chrono::Utc::now() - chrono::Duration::seconds(STALE_SECONDS + 5)).to_rfc3339();
        store
            .save(&left("a", Status::Running, Some(old)), &[])
            .unwrap();
        store.save(&left("bb", Status::Queued, None), &[]).unwrap();
        let jobs = jobs(store);
        assert_eq!(
            (status(&jobs, "a"), status(&jobs, "bb")),
            (Status::Paused, Status::Paused)
        );
        let ids: Vec<String> = jobs.resumable().into_iter().map(|(j, _)| j.id).collect();
        assert_eq!(ids, ["a", "bb"]);
    }
}
