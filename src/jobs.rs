//! Work the server runs in the background, indexing for now: each job has a record, a status and
//! its events, kept in a [`JobStore`]; a running job streams its events to whoever follows it.

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
    pub finished: Option<String>,
    pub error: Option<String>,
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

/// A job in progress: its record, its events so far and the stream of new ones.
pub struct Running {
    record: Mutex<JobRecord>,
    events: Mutex<Vec<Event>>,
    live: broadcast::Sender<Event>,
    pub cancel: CancellationToken,
    /// Where events also go: the terminal or the server's log.
    out: Arc<dyn EventSink>,
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
        self.out.emit(e);
    }
}

/// Starts jobs one at a time and keeps their records.
pub struct Jobs {
    store: Arc<dyn JobStore>,
    current: Mutex<Option<Arc<Running>>>,
    out: Arc<dyn EventSink>,
}

impl Jobs {
    /// Jobs left running by a stopped process are marked interrupted.
    pub fn new(store: Arc<dyn JobStore>, out: Arc<dyn EventSink>) -> Jobs {
        for job in store.list(MAX_JOBS) {
            if !job.status.finished() {
                if let Some((mut job, events)) = store.get(&job.id) {
                    job.status = Status::Interrupted;
                    job.finished = Some(now());
                    let _ = store.save(&job, &events);
                }
            }
        }
        Jobs {
            store,
            current: Mutex::new(None),
            out,
        }
    }

    /// The running job, if any.
    pub fn running(&self) -> Option<Arc<Running>> {
        self.current
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .filter(|r| !r.record().status.finished())
    }

    /// The most recent job: the running one, else the last kept.
    pub fn latest(&self) -> Option<(JobRecord, Vec<Event>)> {
        if let Some(r) = self
            .current
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            return Some((r.record(), r.events()));
        }
        let id = self.store.list(1).into_iter().next()?.id;
        self.store.get(&id)
    }

    pub fn get(&self, id: &str) -> Option<(JobRecord, Vec<Event>)> {
        match self
            .current
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            Some(r) if r.record().id == id => Some((r.record(), r.events())),
            _ => self.store.get(id),
        }
    }

    /// The running job `id`, to follow its events.
    pub fn live(&self, id: &str) -> Option<Arc<Running>> {
        self.running().filter(|r| r.record().id == id)
    }

    pub fn list(&self, limit: usize) -> Vec<JobRecord> {
        self.store.list(limit)
    }

    /// Run `work` as a job of `kind`; refused while another job runs.
    pub fn start(
        &self,
        kind: &str,
        started_by: Option<String>,
        work: impl FnOnce(Arc<Running>) -> BoxFuture<'static, Result<()>>,
    ) -> Result<Arc<Running>> {
        let job = {
            let mut current = self.current.lock().unwrap_or_else(|e| e.into_inner());
            anyhow::ensure!(
                current
                    .as_ref()
                    .is_none_or(|r| r.record().status.finished()),
                "a job is already running"
            );
            let job = Arc::new(Running {
                record: Mutex::new(JobRecord {
                    id: uuid::Uuid::new_v4().simple().to_string(),
                    kind: kind.to_string(),
                    started_by,
                    status: Status::Running,
                    created: now(),
                    finished: None,
                    error: None,
                }),
                events: Default::default(),
                live: broadcast::channel(256).0,
                cancel: CancellationToken::new(),
                out: self.out.clone(),
            });
            *current = Some(job.clone());
            job
        };
        let store = self.store.clone();
        save(&store, &job);
        let run = work(job.clone());
        let (j, s) = (job.clone(), store.clone());
        tokio::spawn(async move {
            let ticker = tokio::spawn({
                let (j, s) = (j.clone(), s.clone());
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
                let mut r = j.record.lock().unwrap_or_else(|e| e.into_inner());
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
            let _ = tokio::task::spawn_blocking(move || save(&s, &j)).await;
        });
        Ok(job)
    }
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

    #[tokio::test(flavor = "multi_thread")]
    async fn a_job_runs_once_at_a_time_and_ends_cancelled() {
        let jobs = Jobs::new(
            Arc::new(MemoryJobs::default()),
            Arc::new(crate::events::Discard),
        );
        let job = jobs
            .start("index", None, |j| {
                Box::pin(async move {
                    j.cancel.cancelled().await;
                    Err(crate::context::Cancelled.into())
                })
            })
            .unwrap();
        assert!(jobs
            .start("index", None, |_| Box::pin(async { Ok(()) }))
            .is_err());
        job.cancel.cancel();
        for _ in 0..50 {
            if jobs.running().is_none() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let (rec, _) = jobs.get(&job.record().id).unwrap();
        assert_eq!(rec.status, Status::Cancelled);
    }

    #[test]
    fn jobs_left_running_are_interrupted() {
        let store = Arc::new(MemoryJobs::default());
        let rec = JobRecord {
            id: "a".into(),
            kind: "index".into(),
            started_by: None,
            status: Status::Running,
            created: now(),
            finished: None,
            error: None,
        };
        store.save(&rec, &[]).unwrap();
        let jobs = Jobs::new(store, Arc::new(crate::events::Discard));
        assert_eq!(jobs.get("a").unwrap().0.status, Status::Interrupted);
    }
}
