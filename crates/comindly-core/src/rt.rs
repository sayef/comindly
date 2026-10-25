//! Running async code from synchronous callers (CLI commands, the terminal UI, blocking-pool
//! work): one shared runtime for the process, never one per call.

use std::future::Future;
use std::sync::OnceLock;

use tokio::runtime::{Handle, Runtime, RuntimeFlavor};

/// The process's shared runtime, built on first use.
pub fn shared() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .thread_name("comindly")
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

/// Run `f` to completion from synchronous code. Inside a multi-threaded runtime the current
/// thread hands its work over while it waits; elsewhere the shared runtime runs it.
pub fn block_on<F>(f: F) -> F::Output
where
    F: Future + Send,
    F::Output: Send,
{
    match Handle::try_current() {
        Ok(h) if h.runtime_flavor() == RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(|| h.block_on(f))
        }
        Ok(_) => std::thread::scope(|s| {
            s.spawn(|| shared().block_on(f))
                .join()
                .expect("a worker thread failed")
        }),
        Err(_) => shared().block_on(f),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocking_calls_nest_inside_the_runtime_and_its_workers() {
        let from_main = shared().block_on(async { block_on(async { block_on(async { 1 }) }) });
        let on_worker = shared()
            .block_on(shared().spawn(async { block_on(async { block_on(async { 2 }) }) }))
            .unwrap();
        assert_eq!((from_main, on_worker), (1, 2));
    }
}
